//! The in-app updater (`docs/internals/updates.md`). Every request goes
//! through the app's `Downloader`, so it is in the network log and "Stay
//! offline" refuses it. The updater plugin makes no requests: it reads the
//! manifest, checks the archive's signature and swaps the app bundle.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, Url};
use tauri_plugin_updater::{Update, UpdaterExt};
use tw_core::updates::{self, Release, UpdateState};
use tw_core::{DictationPhase, Millis};
use tw_models::{Downloader, ModelsError};

use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::files::FileJobs;
use crate::state::{AppState, lock};
use crate::windows::now_ms;
use crate::{events, tray};

/// `latest.json` is a few hundred bytes.
const MANIFEST_LIMIT: u64 = 1024 * 1024;
/// The app archive is about 30 MB; this only stops a runaway download.
const ARCHIVE_LIMIT: u64 = 300 * 1024 * 1024;
/// How often the automatic check looks at the clock. No request is made
/// unless a check is due.
const SCHEDULE_POLL: Duration = Duration::from_secs(60);

const BAD_MANIFEST: &str = "The update server sent something unexpected. Try again later.";
const BAD_SIGNATURE: &str =
    "The download didn't match its signature, so it wasn't installed. Try again.";

pub struct Updates {
    downloader: Downloader,
    state: Mutex<UpdateState>,
    /// What the last successful check found. Downloads use its address and
    /// signature.
    found: Mutex<Option<Update>>,
    /// The downloaded archive, already verified.
    archive: Mutex<Option<Vec<u8>>>,
    launched_at: Millis,
    last_check: Mutex<Option<Millis>>,
}

impl Updates {
    pub fn new(downloader: Downloader) -> Self {
        Self {
            downloader,
            state: Mutex::new(UpdateState::Idle),
            found: Mutex::new(None),
            archive: Mutex::new(None),
            launched_at: now_ms(),
            last_check: Mutex::new(None),
        }
    }

    pub fn state(&self) -> UpdateState {
        lock(&self.state).clone()
    }

    /// "Check for updates". Runs in the background; the result arrives as
    /// `updates:state`.
    pub fn check(self: &Arc<Self>, app: &AppHandle, offline: bool) -> CommandResult<()> {
        if offline {
            return Err(offline_error("check for updates"));
        }
        self.start_check(app, false);
        Ok(())
    }

    /// Downloads and verifies the release the last check found.
    pub fn download(self: &Arc<Self>, app: &AppHandle, offline: bool) -> CommandResult<()> {
        if offline {
            return Err(offline_error("download the update"));
        }
        let update = lock(&self.found)
            .clone()
            .ok_or_else(|| CommandError::invalid("Check for updates first"))?;
        let release = {
            let mut state = lock(&self.state);
            let Some(release) = state.downloadable().cloned() else {
                return Ok(());
            };
            *state = UpdateState::Downloading {
                release: release.clone(),
                received: 0,
                total: None,
            };
            release
        };
        self.emit(app, false);
        let this = Arc::clone(self);
        let app = app.clone();
        spawn("tw-update-download", move || {
            this.run_download(&app, &update, release);
        });
        Ok(())
    }

    /// Replaces the app with the downloaded update and restarts it.
    pub fn install(self: &Arc<Self>, app: &AppHandle) -> CommandResult<()> {
        if is_busy(app) {
            return Err(CommandError::invalid(
                "Finish the dictation first, then restart to update",
            ));
        }
        let update = lock(&self.found).clone();
        let archive = lock(&self.archive).take();
        let (Some(update), Some(archive)) = (update, archive) else {
            return Err(CommandError::invalid("No update is ready to install"));
        };
        let release = {
            let mut state = lock(&self.state);
            let Some(release) = state.installable().cloned() else {
                return Err(CommandError::invalid("No update is ready to install"));
            };
            *state = UpdateState::Installing {
                release: release.clone(),
            };
            release
        };
        self.emit(app, true);
        let this = Arc::clone(self);
        let app = app.clone();
        // The plugin may ask for an administrator password on the main
        // thread and waits for it, so this can't run there.
        spawn("tw-update-install", move || {
            // The new bundle has its own language model worker; the old one
            // must not be restarted from it.
            if let Some(state) = app.try_state::<AppState>() {
                state.polisher.shutdown();
            }
            match update.install(&archive) {
                Ok(()) => {
                    tracing::info!(version = %release.version, "update installed, restarting");
                    app.restart();
                }
                Err(err) => {
                    tracing::warn!(%err, "could not install the update");
                    this.set(
                        &app,
                        UpdateState::Failed {
                            message: format!("Couldn't install the update: {err}"),
                            release: Some(release),
                        },
                    );
                }
            }
        });
        Ok(())
    }

    /// Checks once a day while "Check for updates automatically" is on.
    pub fn spawn_schedule(self: &Arc<Self>, app: AppHandle) {
        let this = Arc::clone(self);
        spawn("tw-update-schedule", move || {
            loop {
                std::thread::sleep(SCHEDULE_POLL);
                let Some(state) = app.try_state::<AppState>() else {
                    continue;
                };
                let settings = state.data.settings.get();
                let due = updates::auto_check_due(
                    settings.check_for_updates,
                    settings.offline,
                    this.launched_at,
                    *lock(&this.last_check),
                    now_ms(),
                );
                if due {
                    this.start_check(&app, true);
                }
            }
        });
    }

    /// An automatic check that fails leaves the state as it was: nobody
    /// asked, so there is nothing to show.
    fn start_check(self: &Arc<Self>, app: &AppHandle, automatic: bool) {
        let previous = {
            let mut state = lock(&self.state);
            if !state.can_check() {
                return;
            }
            std::mem::replace(&mut *state, UpdateState::Checking)
        };
        self.emit(app, false);
        let this = Arc::clone(self);
        let app = app.clone();
        spawn("tw-update-check", move || {
            let next = match this.fetch_update(&app) {
                Ok(found) => {
                    let release = found.as_ref().map(release_of);
                    *lock(&this.found) = found;
                    UpdateState::checked(release, now_ms())
                }
                Err(message) if automatic => {
                    tracing::warn!(%message, "automatic update check failed");
                    previous
                }
                Err(message) => UpdateState::Failed {
                    message,
                    release: None,
                },
            };
            *lock(&this.last_check) = Some(now_ms());
            this.set(&app, next);
        });
    }

    /// Fetches the manifest; `Some` when it offers a newer version.
    fn fetch_update(&self, app: &AppHandle) -> Result<Option<Update>, String> {
        let url = endpoint(app).ok_or_else(|| "No update address is set".to_owned())?;
        let body = self
            .downloader
            .get(&url, MANIFEST_LIMIT, &mut |_, _| ())
            .map_err(|err| fetch_message(&err))?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| BAD_MANIFEST.to_owned())?;
        let endpoint = Url::parse(&url).map_err(|_| BAD_MANIFEST.to_owned())?;
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .and_then(tauri_plugin_updater::UpdaterBuilder::build)
            .map_err(|err| format!("Updates don't work in this copy of Tiny Wispr: {err}"))?;
        updater.restore_update(manifest).map_err(|err| {
            tracing::warn!(%err, "the update manifest was rejected");
            BAD_MANIFEST.to_owned()
        })
    }

    fn run_download(&self, app: &AppHandle, update: &Update, release: Release) {
        let mut on_progress = |received, total| {
            self.set(
                app,
                UpdateState::Downloading {
                    release: release.clone(),
                    received,
                    total,
                },
            );
        };
        let result = self
            .downloader
            .get(
                update.download_url.as_str(),
                ARCHIVE_LIMIT,
                &mut on_progress,
            )
            .map_err(|err| fetch_message(&err))
            .and_then(|archive| match update.verify(&archive) {
                Ok(()) => Ok(archive),
                Err(err) => {
                    tracing::warn!(%err, "the update's signature didn't match");
                    Err(BAD_SIGNATURE.to_owned())
                }
            });
        let next = match result {
            Ok(archive) => {
                *lock(&self.archive) = Some(archive);
                UpdateState::Ready { release }
            }
            Err(message) => UpdateState::Failed {
                message,
                release: Some(release),
            },
        };
        self.set(app, next);
    }

    fn set(&self, app: &AppHandle, next: UpdateState) {
        let tray_changed = {
            let mut state = lock(&self.state);
            let changed = state.installable().is_some() != next.installable().is_some();
            *state = next;
            changed
        };
        self.emit(app, tray_changed);
    }

    fn emit(&self, app: &AppHandle, refresh_tray: bool) {
        events::emit(app, events::UPDATES_STATE, self.state());
        if refresh_tray {
            tray::refresh(app);
        }
    }
}

/// The manifest's address: `plugins.updater.endpoints[0]` in
/// `tauri.conf.json`, or `TINY_WISPR_UPDATE_URL` to test an update from a
/// prerelease or a local server. The override is safe in release builds:
/// the archive must still carry our signature for the version it claims, and
/// only a newer version is installed.
fn endpoint(app: &AppHandle) -> Option<String> {
    if let Ok(url) = std::env::var("TINY_WISPR_UPDATE_URL") {
        return Some(url);
    }
    app.config()
        .plugins
        .0
        .get("updater")?
        .get("endpoints")?
        .get(0)?
        .as_str()
        .map(str::to_owned)
}

/// A dictation or file transcription would be cut off by the restart.
fn is_busy(app: &AppHandle) -> bool {
    let dictating = app.try_state::<AppState>().is_some_and(|state| {
        matches!(
            state.dictation.view().phase,
            DictationPhase::Listening | DictationPhase::Processing
        )
    });
    let transcribing = app
        .try_state::<FileJobs>()
        .is_some_and(|jobs| jobs.current().is_some());
    dictating || transcribing
}

fn release_of(update: &Update) -> Release {
    Release {
        version: update.version.clone(),
        notes: update
            .body
            .as_deref()
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
            .map(str::to_owned),
        published_at: update
            .date
            .and_then(|date| Millis::try_from(date.unix_timestamp_nanos() / 1_000_000).ok()),
    }
}

fn fetch_message(err: &ModelsError) -> String {
    match err {
        ModelsError::Network(_) => {
            "Couldn't reach the update server. Check your internet connection.".to_owned()
        }
        ModelsError::Http { status: 404 } => {
            "No release was found on the update server (404)".to_owned()
        }
        ModelsError::Http { status } => {
            format!("The update server answered with an error ({status}). Try again later.")
        }
        other => other.to_string(),
    }
}

fn offline_error(what: &str) -> CommandError {
    CommandError::new(
        ErrorKind::Offline,
        format!("Stay offline is on, so Tiny Wispr can't {what}"),
    )
}

fn spawn(name: &str, run: impl FnOnce() + Send + 'static) {
    if let Err(err) = std::thread::Builder::new().name(name.into()).spawn(run) {
        tracing::warn!(%err, thread = name, "could not start the updater thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_errors_name_the_update_server() {
        assert_eq!(
            fetch_message(&ModelsError::Http { status: 404 }),
            "No release was found on the update server (404)"
        );
        assert_eq!(
            fetch_message(&ModelsError::Http { status: 502 }),
            "The update server answered with an error (502). Try again later."
        );
        assert_eq!(
            fetch_message(&ModelsError::Network("reset".into())),
            "Couldn't reach the update server. Check your internet connection."
        );
    }

    #[test]
    fn offline_errors_say_why() {
        let err = offline_error("check for updates");
        assert_eq!(err.kind, ErrorKind::Offline);
        assert_eq!(
            err.message,
            "Stay offline is on, so Tiny Wispr can't check for updates"
        );
    }
}
