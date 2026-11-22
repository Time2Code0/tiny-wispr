//! Event names (spec §4) and a logging emit helper.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// `DictationState`
pub const DICTATION_STATE: &str = "dictation:state";
/// `AudioLevel`
pub const AUDIO_LEVEL: &str = "audio:level";
/// `ModelProgress`
pub const MODELS_PROGRESS: &str = "models:progress";
/// `()`
pub const MODELS_CHANGED: &str = "models:changed";
/// `Settings`
pub const SETTINGS_CHANGED: &str = "settings:changed";
/// `Permissions`
pub const PERMISSIONS_CHANGED: &str = "permissions:changed";
/// `()`
pub const HISTORY_CHANGED: &str = "history:changed";
/// `HotkeyCapture`
pub const HOTKEY_CAPTURED: &str = "hotkey:captured";
/// `ListenerStatus`
pub const LISTENER_STATUS: &str = "listener:status";
/// `UpdateState`
pub const UPDATES_STATE: &str = "updates:state";
/// `Navigate`, sent to the main window only.
pub const WINDOW_NAVIGATE: &str = "window:navigate";

/// Payload of `audio:level`: smoothed RMS, 0..1, about 30 Hz.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioLevel {
    pub level: f32,
}

/// Payload of `hotkey:captured`, sent while the shortcut recorder is open.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyCapture {
    pub hotkey: tw_core::Hotkey,
    /// All keys were released: this is the final combination.
    pub done: bool,
}

pub fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!(%event, %err, "failed to emit event");
    }
}
