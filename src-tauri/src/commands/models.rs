use std::path::PathBuf;

use tauri::{AppHandle, State};
use tw_core::models::Recommendation;
use tw_core::{ModelInfo, ModelKind};

use crate::error::CommandResult;
use crate::events;
use crate::models::Hardware;
use crate::state::AppState;

/// Catalog plus imported models, with download status and the
/// recommendation for this computer.
#[tauri::command]
pub async fn list_models(state: State<'_, AppState>) -> CommandResult<Vec<ModelInfo>> {
    state.models.list()
}

/// Starts or resumes a download in the background; progress arrives as
/// `models:progress`, the end as `models:changed`. Refused while "Stay
/// offline" is on.
#[tauri::command]
pub fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CommandResult<()> {
    let offline = state.data.settings.get().offline;
    state.models.download(&app, &id, offline)
}

/// Pauses the download; the next `download_model` resumes it.
#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) {
    state.models.cancel(&id);
}

#[tauri::command]
pub fn delete_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CommandResult<()> {
    state.models.delete(&id)?;
    events::emit(&app, events::MODELS_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub async fn import_model(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    kind: ModelKind,
) -> CommandResult<ModelInfo> {
    let model = state.models.import(&PathBuf::from(path), kind)?;
    events::emit(&app, events::MODELS_CHANGED, ());
    Ok(model)
}

#[tauri::command]
pub async fn get_hardware_info(state: State<'_, AppState>) -> CommandResult<Hardware> {
    Ok(state.models.hardware_with_cpu())
}

#[tauri::command]
pub async fn get_recommendation(state: State<'_, AppState>) -> CommandResult<Recommendation> {
    Ok(state.models.recommendation())
}
