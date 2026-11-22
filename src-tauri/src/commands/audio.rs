use std::sync::mpsc::channel;
use std::time::Duration;

use tauri::State;
use tw_core::MicrophoneInfo;

use crate::dictation::Msg;
use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::state::AppState;

/// Opening a Bluetooth microphone can take a few seconds.
const MIC_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

#[tauri::command]
pub fn list_microphones() -> CommandResult<Vec<MicrophoneInfo>> {
    Ok(tw_audio::list_input_devices()?)
}

/// Streams `audio:level` from the selected microphone without recording.
/// Rejects with `noMicrophone`, `microphoneBusy` or `permissionDenied` when
/// the microphone can't be opened.
#[tauri::command]
pub async fn start_mic_test(state: State<'_, AppState>) -> CommandResult<()> {
    let (reply, opened) = channel();
    state.dictation.send(Msg::MicTest(Some(reply)));
    let result =
        tauri::async_runtime::spawn_blocking(move || opened.recv_timeout(MIC_OPEN_TIMEOUT))
            .await
            .map_err(|err| CommandError::new(ErrorKind::Internal, err.to_string()))?;
    match result {
        Ok(opened) => Ok(opened?),
        Err(_) => Err(CommandError::new(
            ErrorKind::Internal,
            "The microphone didn't respond",
        )),
    }
}

#[tauri::command]
pub fn stop_mic_test(state: State<'_, AppState>) {
    state.dictation.send(Msg::MicTest(None));
}
