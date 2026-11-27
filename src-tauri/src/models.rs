//! Voice and language models: the catalog joined with what is on disk and
//! what is downloading, plus downloads on worker threads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::AppHandle;
use tw_core::models::{self, HardwareInfo, ModelProgress, Recommendation};
use tw_core::{ModelInfo, ModelKind, ModelStatus};
use tw_models::{DownloadRequest, Downloader, Inventory, ModelsError};

use crate::error::{CommandError, CommandResult, ErrorKind};
use crate::events;
use crate::state::lock;

enum Transfer {
    Running {
        cancel: Arc<AtomicBool>,
        received: u64,
        total: u64,
    },
    Failed(String),
}

/// Result of `get_hardware_info`: what the model catalog needs to know about
/// this computer, plus the processor's name to show.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    #[serde(flatten)]
    pub info: HardwareInfo,
    /// For example "Apple M3 Max"; `None` when the OS doesn't say.
    pub cpu_name: Option<String>,
}

fn cpu_name() -> Option<String> {
    let mut system = sysinfo::System::new().ok()?;
    system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    let brand = system.cpus().first()?.brand().trim();
    (!brand.is_empty()).then(|| brand.to_owned())
}

pub struct Models {
    inventory: Inventory,
    downloader: Downloader,
    transfers: Mutex<HashMap<String, Transfer>>,
    hardware: OnceLock<HardwareInfo>,
    cpu_name: OnceLock<Option<String>>,
}

impl Models {
    pub fn new(inventory: Inventory, downloader: Downloader) -> Self {
        Self {
            inventory,
            downloader,
            transfers: Mutex::new(HashMap::new()),
            hardware: OnceLock::new(),
            cpu_name: OnceLock::new(),
        }
    }

    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    pub fn hardware(&self) -> HardwareInfo {
        self.hardware.get_or_init(tw_models::hardware).clone()
    }

    pub fn hardware_with_cpu(&self) -> Hardware {
        Hardware {
            info: self.hardware(),
            cpu_name: self.cpu_name.get_or_init(cpu_name).clone(),
        }
    }

    pub fn recommendation(&self) -> Recommendation {
        models::recommend(&self.hardware(), &models::catalog())
    }

    /// The catalog and imported models, with live download status and the
    /// recommendation marked.
    pub fn list(&self) -> CommandResult<Vec<ModelInfo>> {
        let mut models = self.inventory.models(&models::catalog())?;
        let transfers = lock(&self.transfers);
        for model in &mut models {
            match transfers.get(&model.id) {
                Some(Transfer::Running {
                    received, total, ..
                }) => {
                    model.status = ModelStatus::Downloading {
                        received: *received,
                        total: *total,
                    };
                }
                Some(Transfer::Failed(message)) if model.status != ModelStatus::Downloaded => {
                    model.status = ModelStatus::Failed {
                        message: message.clone(),
                    };
                }
                _ => {}
            }
        }
        drop(transfers);
        self.recommendation().mark(&mut models);
        Ok(models)
    }

    fn find(&self, id: &str) -> CommandResult<ModelInfo> {
        self.inventory
            .models(&models::catalog())?
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| CommandError::new(ErrorKind::NotFound, format!("No model {id}")))
    }

    /// Starts or resumes a download; progress arrives as `models:progress`.
    pub fn download(self: &Arc<Self>, app: &AppHandle, id: &str) -> CommandResult<()> {
        let Some((request, cancel)) = self.begin(id)? else {
            return Ok(());
        };
        let this = Arc::clone(self);
        let app = app.clone();
        let spawned = std::thread::Builder::new()
            .name("tw-download".into())
            .spawn(move || this.run_download(&app, &request, &cancel));
        if let Err(err) = spawned {
            lock(&self.transfers).remove(id);
            return Err(CommandError::new(ErrorKind::Internal, err.to_string()));
        }
        Ok(())
    }

    /// Registers a download that may start, or `None` when it is already
    /// downloaded or running.
    fn begin(&self, id: &str) -> CommandResult<Option<(DownloadRequest, Arc<AtomicBool>)>> {
        let model = self.find(id)?;
        if model.status == ModelStatus::Downloaded {
            return Ok(None);
        }
        let request = DownloadRequest::for_model(&model)
            .ok_or_else(|| CommandError::invalid(format!("{} can't be downloaded", model.label)))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut transfers = lock(&self.transfers);
        if matches!(transfers.get(id), Some(Transfer::Running { .. })) {
            return Ok(None);
        }
        transfers.insert(
            id.to_owned(),
            Transfer::Running {
                cancel: Arc::clone(&cancel),
                received: 0,
                total: model.size_bytes,
            },
        );
        Ok(Some((request, cancel)))
    }

    fn run_download(&self, app: &AppHandle, request: &DownloadRequest, cancel: &AtomicBool) {
        tracing::info!(id = %request.id, url = %request.url, "download started");
        let mut on_progress = |progress: ModelProgress| {
            if let Some(Transfer::Running {
                received, total, ..
            }) = lock(&self.transfers).get_mut(&progress.id)
            {
                *received = progress.received;
                *total = progress.total;
            }
            events::emit(app, events::MODELS_PROGRESS, progress);
        };
        let result = self
            .downloader
            .download(&self.inventory, request, &mut on_progress, cancel);
        let mut transfers = lock(&self.transfers);
        match result {
            Ok(path) => {
                tracing::info!(id = %request.id, path = %path.display(), "download finished");
                transfers.remove(&request.id);
            }
            Err(ModelsError::Cancelled) => {
                tracing::info!(id = %request.id, "download paused");
                transfers.remove(&request.id);
            }
            Err(err) => {
                tracing::warn!(id = %request.id, %err, "download failed");
                transfers.insert(request.id.clone(), Transfer::Failed(err.to_string()));
            }
        }
        drop(transfers);
        events::emit(app, events::MODELS_CHANGED, ());
    }

    /// Pauses: the partial file is kept and resumed by the next download.
    pub fn cancel(&self, id: &str) {
        if let Some(Transfer::Running { cancel, .. }) = lock(&self.transfers).get(id) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn delete(&self, id: &str) -> CommandResult<ModelInfo> {
        if matches!(
            lock(&self.transfers).get(id),
            Some(Transfer::Running { .. })
        ) {
            return Err(CommandError::invalid(
                "Pause the download before deleting it",
            ));
        }
        let model = self.find(id)?;
        self.inventory.delete(model.kind, &model.file)?;
        lock(&self.transfers).remove(id);
        Ok(model)
    }

    pub fn import(&self, path: &Path, kind: ModelKind) -> CommandResult<ModelInfo> {
        Ok(self.inventory.import(path, kind)?)
    }
}

/// The downloaded voice model to use: the selected one, or any downloaded
/// one when the selection is missing or gone.
pub fn voice_model_path(inventory: &Inventory, selected: Option<&str>) -> Option<PathBuf> {
    let models = match inventory.models(&models::catalog()) {
        Ok(models) => models,
        Err(err) => {
            tracing::warn!(%err, "could not read the models folder");
            return None;
        }
    };
    let usable = |m: &&ModelInfo| m.kind == ModelKind::Voice && m.status == ModelStatus::Downloaded;
    let model = selected
        .and_then(|id| models.iter().filter(usable).find(|m| m.id == id))
        .or_else(|| models.iter().find(usable))?;
    inventory.path(model.kind, &model.file).ok()
}

impl From<ModelsError> for CommandError {
    fn from(err: ModelsError) -> Self {
        let kind = match err {
            ModelsError::NotFound { .. } => ErrorKind::NotFound,
            ModelsError::WrongFormat { .. }
            | ModelsError::InvalidFileName(_)
            | ModelsError::AlreadyInstalled { .. } => ErrorKind::InvalidInput,
            ModelsError::Io(_) | ModelsError::DiskFull | ModelsError::NotEnoughSpace { .. } => {
                ErrorKind::Io
            }
            _ => ErrorKind::Internal,
        };
        Self::new(kind, err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::store::tests::temp_dir;

    fn place(inventory: &Inventory, id: &str) {
        let model = models::catalog().into_iter().find(|m| m.id == id).unwrap();
        let path = inventory.path(model.kind, &model.file).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = fs::File::create(path).unwrap();
        file.set_len(model.size_bytes).unwrap();
    }

    #[test]
    fn picks_the_selected_voice_model_or_falls_back() {
        let dir = temp_dir("models");
        let inventory = Inventory::new(dir.join("models"));
        assert_eq!(voice_model_path(&inventory, Some("base")), None);

        place(&inventory, "tiny.en");
        place(&inventory, "base");
        let base = voice_model_path(&inventory, Some("base")).unwrap();
        assert!(base.ends_with("whisper/ggml-base.bin"));
        let fallback = voice_model_path(&inventory, Some("large-v3")).unwrap();
        assert!(fallback.ends_with("whisper/ggml-tiny.en.bin"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_overlays_running_and_failed_downloads() {
        let dir = temp_dir("models-list");
        let models = Models::new(Inventory::new(dir.join("models")), Downloader::new());
        lock(&models.transfers).insert(
            "base".into(),
            Transfer::Running {
                cancel: Arc::default(),
                received: 10,
                total: 20,
            },
        );
        lock(&models.transfers).insert("tiny".into(), Transfer::Failed("No".into()));
        let list = models.list().unwrap();
        let status = |id: &str| list.iter().find(|m| m.id == id).unwrap().status.clone();
        assert_eq!(
            status("base"),
            ModelStatus::Downloading {
                received: 10,
                total: 20
            }
        );
        assert_eq!(
            status("tiny"),
            ModelStatus::Failed {
                message: "No".into()
            }
        );
        assert_eq!(status("small"), ModelStatus::NotDownloaded);
        fs::remove_dir_all(dir).unwrap();
    }
}
