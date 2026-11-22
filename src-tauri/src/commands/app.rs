use std::time::Duration;

use tauri::{AppHandle, Manager, State};
use tw_core::Settings;

use crate::commands::settings::save;
use crate::error::CommandResult;
use crate::network_log::NetworkRequest;
use crate::pill::{self, PillRect};
use crate::state::AppState;
use crate::windows::{self, MainRoute, now_ms};

/// Marks onboarding done, closes its window and opens the hub.
#[tauri::command]
pub async fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<Settings> {
    let settings = save(&app, &state, |s| s.onboarding_completed = true)?;
    windows::show_main(&app, None)?;
    if let Some(onboarding) = app.get_webview_window(windows::ONBOARDING) {
        onboarding.hide()?;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn open_main_window(app: AppHandle, route: Option<MainRoute>) -> CommandResult<()> {
    windows::show_main(&app, route)?;
    Ok(())
}

/// "Hide for 1 hour" from the pill's context menu.
#[tauri::command]
pub async fn hide_pill_for(
    app: AppHandle,
    state: State<'_, AppState>,
    minutes: u32,
) -> CommandResult<Settings> {
    hide_pill(&app, &state, minutes)
}

/// Hides the pill for `minutes` and shows it again afterwards. Also the
/// tray's "Hide the pill for 1 hour", since the pill can't take focus.
pub fn hide_pill(app: &AppHandle, state: &AppState, minutes: u32) -> CommandResult<Settings> {
    let until = now_ms() + i64::from(minutes) * 60_000;
    let settings = save(app, state, |s| s.pill_hidden_until = Some(until))?;
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(u64::from(minutes) * 60));
        pill::sync_visibility(&handle);
    });
    Ok(settings)
}

/// The pill's interactive areas; clicks anywhere else pass through.
#[tauri::command]
pub fn set_pill_hit_regions(state: State<'_, AppState>, rects: Vec<PillRect>) {
    state.pill.set_regions(rects);
}

/// Every network request the app has made: model downloads only.
#[tauri::command]
pub fn list_network_log(state: State<'_, AppState>) -> Vec<NetworkRequest> {
    state.data.network_log.get()
}
