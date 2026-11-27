use tauri::{AppHandle, State};
use tw_core::UpdateState;

use crate::error::CommandResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_update_state(state: State<'_, AppState>) -> UpdateState {
    state.updates.state()
}

/// Starts a check; the result arrives as `updates:state`.
#[tauri::command]
pub fn check_for_updates(app: AppHandle, state: State<'_, AppState>) {
    state.updates.check(&app);
}

/// Starts downloading the release the last check found.
#[tauri::command]
pub fn download_update(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    state.updates.download(&app)
}

/// Installs the downloaded update and restarts the app.
#[tauri::command]
pub fn install_update(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    state.updates.install(&app)
}
