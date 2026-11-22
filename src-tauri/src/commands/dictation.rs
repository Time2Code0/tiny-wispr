use tauri::State;
use tw_core::dictation::machine::Event;
use tw_core::{DictationMode, DictationState};

use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::state::AppState;

/// Pill click or hands-free start from the UI. Always hands-free: no key is
/// held whose release would end it.
#[tauri::command]
pub fn start_dictation(state: State<'_, AppState>, mode: DictationMode) {
    let _ = mode;
    state.dictation.event(Event::StartRequested);
}

#[tauri::command]
pub fn stop_dictation(state: State<'_, AppState>) {
    state.dictation.event(Event::StopRequested);
}

#[tauri::command]
pub fn cancel_dictation(state: State<'_, AppState>) {
    state.dictation.event(Event::CancelRequested);
}

/// Transcribes the audio of the last cancelled dictation and pastes it.
#[tauri::command]
pub fn undo_cancel(state: State<'_, AppState>) {
    state.dictation.event(Event::UndoRequested);
}

#[tauri::command]
pub fn paste_last_transcript(state: State<'_, AppState>) {
    state.dictation.event(Event::PasteLastRequested);
}

/// Puts the last transcript on the clipboard as an ordinary entry.
#[tauri::command]
pub fn copy_last_transcript(state: State<'_, AppState>) -> CommandResult<()> {
    let text = state
        .dictation
        .last_text()
        .or_else(|| {
            state
                .data
                .history
                .read(|history| history.entries().first().map(|e| e.final_text.clone()))
        })
        .ok_or_else(|| CommandError::new(ErrorKind::NotFound, "Nothing dictated yet"))?;
    Ok(tw_os::insert::copy_to_clipboard(&text)?)
}

#[tauri::command]
pub fn get_dictation_state(state: State<'_, AppState>) -> DictationState {
    state.dictation.view()
}
