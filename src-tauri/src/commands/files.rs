use std::path::PathBuf;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::{CommandError, CommandResult};
use crate::files::{FileJobs, FileProgress, text_file_name};

/// Starts transcribing the audio file at `path`; progress arrives as
/// `files:progress` events. Returns the job id.
#[tauri::command]
pub fn transcribe_file(jobs: State<'_, FileJobs>, path: String) -> CommandResult<String> {
    jobs.start(PathBuf::from(path))
        .map_err(|err| CommandError::invalid(err.to_string()))
}

#[tauri::command]
pub fn cancel_file_transcription(jobs: State<'_, FileJobs>, job_id: String) {
    jobs.cancel(&job_id);
}

/// The running job's latest progress; `None` when no file is being
/// transcribed.
#[tauri::command]
pub fn get_file_transcription(jobs: State<'_, FileJobs>) -> Option<FileProgress> {
    jobs.current()
}

/// Asks where to save `text` and writes it there. False when the person
/// cancels the dialog.
#[tauri::command]
pub async fn save_file_transcript(
    app: AppHandle,
    text: String,
    file_name: String,
) -> CommandResult<bool> {
    let Some(target) = app
        .dialog()
        .file()
        .set_title("Save transcript")
        .set_file_name(text_file_name(&file_name))
        .add_filter("Text", &["txt"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = target
        .into_path()
        .map_err(|err| CommandError::invalid(format!("Couldn't save there: {err}")))?;
    std::fs::write(&path, text)?;
    Ok(true)
}
