use tauri::{AppHandle, State};
use tw_core::Stats;
use tw_core::history::HistoryPage;

use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::events;
use crate::state::AppState;
use crate::windows::now_ms;

/// Newest first. `query` filters by text; `cursor` comes from the previous page.
#[tauri::command]
pub fn list_history(
    state: State<'_, AppState>,
    query: Option<String>,
    cursor: Option<String>,
) -> HistoryPage {
    state
        .data
        .history
        .read(|history| history.list(query.as_deref(), cursor.as_deref()))
}

#[tauri::command]
pub fn delete_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CommandResult<()> {
    if !state.data.change_history(|history| history.remove(&id))? {
        return Err(CommandError::new(
            ErrorKind::NotFound,
            "That dictation is already gone",
        ));
    }
    events::emit(&app, events::HISTORY_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle, state: State<'_, AppState>) -> CommandResult<()> {
    state.data.change_history(|history| history.clear())?;
    events::emit(&app, events::HISTORY_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Stats {
    let offset_minutes = chrono::Local::now().offset().local_minus_utc() / 60;
    state
        .data
        .history
        .read(|history| history.stats_with_offset(now_ms(), offset_minutes))
}
