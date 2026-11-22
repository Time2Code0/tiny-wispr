//! Polish with the selected local language model, run by `tw-llm-worker`.

use std::path::PathBuf;
use std::time::Duration;

use tw_core::Settings;
use tw_core::models::{self, ModelKind, ModelStatus};
use tw_core::text::polish::PolishRequest;
use tw_llm::{LlmPolisher, ModelSpec};
use tw_models::Inventory;

use super::Polisher;

/// A loaded language model holds gigabytes of memory, so it is let go after
/// this long without a dictation.
const IDLE_UNLOAD: Duration = Duration::from_secs(300);

pub struct LlmPolish {
    inventory: Inventory,
    worker: PathBuf,
    polisher: LlmPolisher,
}

impl LlmPolish {
    pub fn new(inventory: Inventory) -> Self {
        let worker = worker_path();
        if !worker.is_file() {
            tracing::warn!(path = %worker.display(), "the language model worker is missing, polish is off");
        }
        Self {
            inventory,
            polisher: LlmPolisher::new(worker.clone(), Some(IDLE_UNLOAD)),
            worker,
        }
    }

    /// The selected language model, if it is on disk. Unlike the voice model
    /// there is no fallback: no selection means polish is off.
    fn selected(&self, settings: &Settings) -> Option<ModelSpec> {
        let id = settings.llm_model.as_deref()?;
        let models = match self.inventory.models(&models::catalog()) {
            Ok(models) => models,
            Err(err) => {
                tracing::warn!(%err, "could not read the models folder");
                return None;
            }
        };
        let model = models.iter().find(|m| {
            m.id == id && m.kind == ModelKind::Language && m.status == ModelStatus::Downloaded
        })?;
        let path = self.inventory.path(model.kind, &model.file).ok()?;
        Some(ModelSpec::new(path, model.reasoning))
    }

    /// Ends the worker process so it never outlives the app.
    pub fn shutdown(&self) {
        self.polisher.set_model(None);
    }
}

impl Polisher for LlmPolish {
    fn active(&self, settings: &Settings) -> bool {
        if !self.worker.is_file() {
            return false;
        }
        let model = self.selected(settings);
        let active = model.is_some();
        self.polisher.set_model(model);
        active
    }

    fn polish(&self, request: &PolishRequest) -> Option<String> {
        match self.polisher.polish(request) {
            Ok(text) => Some(text),
            Err(err) => {
                tracing::info!(%err, "kept the rule-based text");
                None
            }
        }
    }
}

/// The worker ships next to the app binary: Tauri places the bundled
/// sidecar there, and a workspace build puts both in the same target folder.
fn worker_path() -> PathBuf {
    let name = if cfg!(windows) {
        "tw-llm-worker.exe"
    } else {
        "tw-llm-worker"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}
