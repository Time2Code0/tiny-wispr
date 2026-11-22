//! Tiny Wispr desktop app: windows, tray, commands, settings persistence and
//! the dictation runtime that wires the crates together.

#[cfg(target_os = "macos")]
mod app_menu;
mod commands;
mod dictation;
mod error;
pub mod events;
mod files;
mod listener;
mod models;
mod network_log;
mod pill;
mod settings_store;
mod sink;
mod sound;
mod state;
mod store;
mod tray;
mod updates;
mod windows;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use tauri::{App, AppHandle, Manager, RunEvent};
use tracing_subscriber::EnvFilter;
use tw_core::history::HistoryRetention;
use tw_models::{Downloader, Inventory};

pub use error::{CommandError, ErrorKind};
pub use state::{AppPaths, AppState};
pub use windows::MainRoute;

use crate::dictation::desktop::{MicSource, SystemDesktop};
use crate::dictation::engine::WhisperModels;
use crate::dictation::polish::LlmPolish;
use crate::dictation::{Deps, Desktop, Dictation, Msg, Sink};
use crate::listener::Keys;
use crate::models::Models;
use crate::pill::Pill;
use crate::sink::AppSink;
use crate::state::Data;
use crate::updates::Updates;

#[cfg(debug_assertions)]
type Smoke = dictation::smoke::Smoke;
#[cfg(not(debug_assertions))]
type Smoke = std::convert::Infallible;

#[cfg(debug_assertions)]
fn smoke() -> anyhow::Result<Option<Smoke>> {
    Smoke::from_env()
}

#[cfg(not(debug_assertions))]
fn smoke() -> anyhow::Result<Option<Smoke>> {
    Ok(None)
}

/// Debug builds may point a folder elsewhere, so smoke runs leave the real
/// data alone.
fn debug_override(var: &str) -> Option<PathBuf> {
    cfg!(debug_assertions)
        .then(|| std::env::var_os(var).map(PathBuf::from))
        .flatten()
}

fn setup(app: &mut App) -> anyhow::Result<()> {
    let data_dir = match debug_override("TINY_WISPR_DATA_DIR") {
        Some(dir) => dir,
        None => app.path().app_data_dir().context("no app data directory")?,
    };
    let paths = AppPaths::new(&data_dir);
    let inventory = Inventory::new(
        debug_override("TINY_WISPR_MODELS_DIR").unwrap_or_else(|| paths.models_dir.clone()),
    );
    let data = Arc::new(Data::load(paths));
    tracing::info!(path = %data.paths.data_dir.display(), models = %inventory.root().display(), "data loaded");
    if data.settings.get().history_retention != HistoryRetention::Forever {
        data.change_history(|_| ())?;
    }

    let log_data = Arc::clone(&data);
    let downloader =
        Downloader::new().with_request_log(move |record| network_log::record(&log_data, record));
    let models = Arc::new(Models::new(inventory.clone(), downloader));
    let log_data = Arc::clone(&data);
    let updates = Arc::new(Updates::new(
        Downloader::new().with_request_log(move |record| network_log::record(&log_data, record)),
    ));
    let polisher = Arc::new(LlmPolish::new(inventory.clone()));
    let transcribers = Arc::new(WhisperModels::new(inventory));
    transcribers.spawn_idle_check();
    let keys = Arc::new(Keys::default());
    let handle = app.handle().clone();
    let sink: Arc<dyn Sink> = Arc::new(AppSink {
        app: handle.clone(),
    });
    let smoke = smoke()?;

    let desktop: Arc<dyn Desktop> = match &smoke {
        #[cfg(debug_assertions)]
        Some(smoke) => smoke.desktop(),
        _ => Arc::new(SystemDesktop),
    };
    let deps = Deps {
        data: Arc::clone(&data),
        transcribers: Arc::clone(&transcribers) as Arc<dyn dictation::Transcribers>,
        desktop,
        polisher: Arc::clone(&polisher) as Arc<dyn dictation::Polisher>,
        sink: Arc::clone(&sink),
        keys: Arc::clone(&keys),
    };
    let dictation = Dictation::spawn(deps, |tx| match &smoke {
        #[cfg(debug_assertions)]
        Some(smoke) => smoke.source(),
        _ => {
            let level_sink = Arc::clone(&sink);
            Box::new(MicSource::new(
                Arc::new(move |level| level_sink.level(level)),
                Arc::new(move || {
                    let _ = tx.send(Msg::CaptureLost);
                }),
            ))
        }
    });
    let runtime = dictation.sender();
    let file_jobs = files::FileJobs::new(files::JobDeps {
        data: Arc::clone(&data),
        transcribers: Arc::clone(&transcribers) as Arc<dyn dictation::Transcribers>,
        polisher: Arc::clone(&polisher) as Arc<dyn dictation::Polisher>,
        sink: Arc::new(files::AppFileSink {
            app: handle.clone(),
        }),
    });
    app.manage(file_jobs);
    let settings = data.settings.get();
    app.manage(AppState {
        data,
        dictation,
        keys: Arc::clone(&keys),
        models,
        transcribers,
        polisher,
        pill: Arc::new(Pill::default()),
        updates: Arc::clone(&updates),
    });

    if let Err(err) = tw_os::insert::refresh_keyboard_layout() {
        tracing::warn!(%err, "could not read the keyboard layout");
    }
    if !settings.show_in_dock {
        commands::settings::show_in_dock(&handle, false);
    }
    listener::spawn_supervisor(handle.clone(), keys, runtime);
    tray::create(&handle).context("creating tray icon")?;
    match smoke {
        #[cfg(debug_assertions)]
        Some(smoke) => {
            windows::create_pill(&handle)?;
            pill::sync_visibility(&handle);
            smoke.run(handle.clone());
        }
        _ => windows::open_initial(&handle, &settings).context("creating windows")?,
    }
    updates.spawn_schedule(handle.clone());
    pill::spawn_tracker(handle);
    Ok(())
}

/// Stops dictation and drops the voice model before the process exits.
pub(crate) fn shutdown(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Some(files) = app.try_state::<files::FileJobs>() {
        files.cancel_all();
    }
    state.dictation.shutdown();
    state.keys.remove();
    state.polisher.shutdown();
    state.transcribers.unload();
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,whisper_rs=warn")),
        )
        .init();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(err) = windows::show_main(app, None) {
                tracing::warn!(%err, "could not show the main window");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu::build);
    let app = builder
        .invoke_handler(commands::handler())
        .on_window_event(windows::on_window_event)
        .setup(|app| setup(app).map_err(Into::into))
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(%err, "failed to start Tiny Wispr");
            std::process::exit(1);
        }
    };

    app.run(|app, event| match event {
        // Closing the last window must not quit: the tray keeps the app alive.
        // `app.exit(code)` from the tray or the smoke run is let through.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => shutdown(app),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => windows::reopen(app),
        _ => {}
    });
}
