use tauri::State;
use tw_core::hotkey::{self, HotkeyIssue, Platform};
use tw_core::{Hotkey, HotkeyAction};

use crate::dictation::Msg;
use crate::listener::ListenerStatus;
use crate::state::AppState;

/// `null` when `hotkey` can be used for `action`.
#[tauri::command]
pub fn validate_hotkey(
    state: State<'_, AppState>,
    action: HotkeyAction,
    hotkey: Hotkey,
) -> Option<HotkeyIssue> {
    let settings = state.data.settings.get();
    hotkey::validate(action, &hotkey, Platform::current(), &settings.shortcuts).err()
}

/// Starts emitting `hotkey:captured` for every combination pressed, and
/// pauses the dictation hotkeys, until `stop_hotkey_capture`.
#[tauri::command]
pub fn start_hotkey_capture(state: State<'_, AppState>) {
    state.dictation.send(Msg::HotkeyCapture(true));
}

#[tauri::command]
pub fn stop_hotkey_capture(state: State<'_, AppState>) {
    state.dictation.send(Msg::HotkeyCapture(false));
}

/// Whether global shortcuts work right now, and why not.
#[tauri::command]
pub fn get_listener_status(state: State<'_, AppState>) -> ListenerStatus {
    state.keys.status()
}
