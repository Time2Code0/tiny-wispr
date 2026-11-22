//! The three windows: pill, onboarding and main (hub).

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tw_core::Settings;

use crate::state::AppState;
use crate::{events, pill};

pub const PILL: &str = "pill";
pub const ONBOARDING: &str = "onboarding";
pub const MAIN: &str = "main";

/// Logical size of the pill window. Larger than the visible capsule: the
/// hover hint, notices and menus are drawn above it inside the same window,
/// and the page keeps the capsule 8 px above the window's bottom edge.
const PILL_SIZE: (f64, f64) = (360.0, 280.0);

/// Pages of the main window, also its hash routes (`index.html#/settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MainRoute {
    #[default]
    Home,
    Dictionary,
    Snippets,
    Style,
    Models,
    Settings,
}

impl MainRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Dictionary => "dictionary",
            Self::Snippets => "snippets",
            Self::Style => "style",
            Self::Models => "models",
            Self::Settings => "settings",
        }
    }
}

/// Payload of `window:navigate`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigate {
    pub route: MainRoute,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Main and onboarding hide instead of closing; the tray keeps the app alive.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && matches!(window.label(), MAIN | ONBOARDING)
    {
        api.prevent_close();
        if let Err(err) = window.hide() {
            tracing::warn!(%err, label = window.label(), "failed to hide window");
        }
    }
}

pub fn create_pill(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(PILL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, PILL, WebviewUrl::App("pill.html".into()))
        .title("Tiny Wispr")
        .inner_size(PILL_SIZE.0, PILL_SIZE.1)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    pill::configure(&window);
    if pill::can_set_click_through(false) {
        window.set_ignore_cursor_events(true)?;
    }
    let monitor = match app.cursor_position() {
        Ok(cursor) => app.monitor_from_point(cursor.x, cursor.y)?,
        Err(_) => None,
    };
    match monitor.or(window.primary_monitor()?) {
        Some(monitor) => position_pill(&window, &monitor)?,
        None => tracing::warn!("no monitor; pill left at default position"),
    }
    tracing::info!("window created: pill");
    Ok(window)
}

/// Bottom centre of `monitor`'s work area (above the Dock / taskbar).
pub fn position_pill(window: &WebviewWindow, monitor: &Monitor) -> tauri::Result<()> {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = PILL_SIZE.0 * scale;
    let height = PILL_SIZE.1 * scale;
    let x = f64::from(area.position.x) + (f64::from(area.size.width) - width) / 2.0;
    let y = f64::from(area.position.y) + f64::from(area.size.height) - height;
    window.set_position(PhysicalPosition::new(x.round(), y.round()))
}

pub fn show_onboarding(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = match app.get_webview_window(ONBOARDING) {
        Some(window) => window,
        None => {
            let builder = WebviewWindowBuilder::new(
                app,
                ONBOARDING,
                WebviewUrl::App("onboarding.html".into()),
            )
            .title("Welcome to Tiny Wispr")
            .inner_size(720.0, 560.0)
            .resizable(false)
            .maximizable(false)
            .center()
            .visible(false);
            #[cfg(target_os = "macos")]
            let builder = builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
            let window = builder.build()?;
            tracing::info!("window created: onboarding");
            window
        }
    };
    window.show()?;
    window.set_focus()?;
    Ok(window)
}

pub fn show_main(app: &AppHandle, route: Option<MainRoute>) -> tauri::Result<WebviewWindow> {
    let window = match app.get_webview_window(MAIN) {
        Some(window) => {
            if let Some(route) = route {
                window.emit_to(MAIN, events::WINDOW_NAVIGATE, Navigate { route })?;
            }
            window
        }
        None => {
            let route = route.unwrap_or_default();
            let builder =
                WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
                    .title("Tiny Wispr")
                    .inner_size(1040.0, 720.0)
                    .min_inner_size(860.0, 560.0)
                    .center()
                    .visible(false)
                    .initialization_script(format!(
                        "if (!window.location.hash) window.location.hash = '#/{}';",
                        route.as_str()
                    ));
            #[cfg(target_os = "macos")]
            let builder = builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true);
            let window = builder.build()?;
            tracing::info!("window created: main");
            window
        }
    };
    window.show()?;
    window.set_focus()?;
    Ok(window)
}

/// Home, or onboarding while it is unfinished.
pub fn open_app(app: &AppHandle) -> tauri::Result<()> {
    let completed = app
        .try_state::<AppState>()
        .is_some_and(|state| state.data.settings.get().onboarding_completed);
    if completed {
        show_main(app, None).map(drop)
    } else {
        show_onboarding(app).map(drop)
    }
}

/// Clicking the Dock icon. The pill counts as a visible window to macOS, so
/// look at the two real windows instead.
#[cfg(target_os = "macos")]
pub fn reopen(app: &AppHandle) {
    let visible = [MAIN, ONBOARDING].iter().any(|label| {
        app.get_webview_window(label)
            .is_some_and(|window| window.is_visible().unwrap_or(false))
    });
    if !visible && let Err(err) = open_app(app) {
        tracing::warn!(%err, "could not reopen a window");
    }
}

/// Startup: the pill always exists; onboarding on first run. Afterwards the
/// app starts quietly, with only the pill and the tray icon.
pub fn open_initial(app: &AppHandle, settings: &Settings) -> tauri::Result<()> {
    create_pill(app)?;
    pill::sync_visibility(app);
    if !settings.onboarding_completed {
        show_onboarding(app)?;
    }
    // Smoke test for development: create every window at startup.
    if cfg!(debug_assertions) && std::env::var_os("TINY_WISPR_OPEN_ALL").is_some() {
        show_onboarding(app)?;
        show_main(app, None)?;
        tracing::info!("TINY_WISPR_OPEN_ALL: all windows created");
    }
    Ok(())
}
