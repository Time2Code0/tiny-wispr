use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tw_core::history::HistoryStore;
use tw_core::{DictionaryEntry, HistoryEntry, Snippet};

use crate::dictation::Dictation;
use crate::dictation::engine::WhisperModels;
use crate::dictation::polish::LlmPolish;
use crate::listener::Keys;
use crate::models::Models;
use crate::network_log::NetworkRequest;
use crate::pill::Pill;
use crate::settings_store::SettingsStore;
use crate::store::{HistoryFile, JsonStore};
use crate::windows::now_ms;

/// Locks `mutex`, recovering the data if another thread panicked with it held.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Files and folders under the OS app data dir.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub settings_file: PathBuf,
    pub history_file: PathBuf,
    pub dictionary_file: PathBuf,
    pub snippets_file: PathBuf,
    pub network_log_file: PathBuf,
    /// `models/{whisper,llm}/`
    pub models_dir: PathBuf,
}

impl AppPaths {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            settings_file: data_dir.join("settings.json"),
            history_file: data_dir.join("history.json"),
            dictionary_file: data_dir.join("dictionary.json"),
            snippets_file: data_dir.join("snippets.json"),
            network_log_file: data_dir.join("network-log.json"),
            models_dir: data_dir.join("models"),
        }
    }
}

/// Everything persisted in the app data dir.
pub struct Data {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    pub history: HistoryFile,
    pub dictionary: JsonStore<Vec<DictionaryEntry>>,
    pub snippets: JsonStore<Vec<Snippet>>,
    pub network_log: JsonStore<Vec<NetworkRequest>>,
}

impl Data {
    pub fn load(paths: AppPaths) -> Self {
        Self {
            settings: SettingsStore::load(paths.settings_file.clone()),
            history: HistoryFile::load(paths.history_file.clone()),
            dictionary: JsonStore::load(paths.dictionary_file.clone()),
            snippets: JsonStore::load(paths.snippets_file.clone()),
            network_log: JsonStore::load(paths.network_log_file.clone()),
            paths,
        }
    }

    /// Changes the history and applies the retention setting before saving.
    pub fn change_history<R>(&self, change: impl FnOnce(&mut HistoryStore) -> R) -> io::Result<R> {
        let retention = self.settings.get().history_retention;
        self.history.update(|store| {
            let result = change(store);
            store.apply_retention(retention, now_ms());
            result
        })
    }

    /// Counts the dictation in the stats; keeps it only if retention allows.
    pub fn add_history(&self, entry: HistoryEntry, fixes: u32) -> io::Result<()> {
        self.change_history(|store| store.push_with_fixes(entry, fixes))
    }
}

/// Everything the commands share.
pub struct AppState {
    pub data: Arc<Data>,
    pub dictation: Dictation,
    pub keys: Arc<Keys>,
    pub models: Arc<Models>,
    pub transcribers: Arc<WhisperModels>,
    pub polisher: Arc<LlmPolish>,
    pub pill: Arc<Pill>,
}
