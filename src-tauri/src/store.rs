//! JSON files in the app data dir: loaded once, written atomically.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tw_core::history::{HistoryLoadError, HistoryStore};

use crate::state::lock;
use crate::windows::now_ms;

pub struct JsonStore<T> {
    path: PathBuf,
    current: Mutex<T>,
}

impl<T: Serialize + DeserializeOwned + Default + Clone> JsonStore<T> {
    /// Reads `path`, falling back to the default when it is missing. A corrupt
    /// file is moved aside to `<name>.bad` rather than overwritten.
    pub fn load(path: PathBuf) -> Self {
        let value = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<T>(&bytes) {
                Ok(value) => value,
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "file is invalid, starting empty");
                    let _ = fs::rename(&path, path.with_extension("json.bad"));
                    T::default()
                }
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => T::default(),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "could not read file, starting empty");
                T::default()
            }
        };
        Self {
            path,
            current: Mutex::new(value),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> T {
        lock(&self.current).clone()
    }

    /// Reads without cloning everything.
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&lock(&self.current))
    }

    /// Applies `change` and saves. Nothing changes in memory when `change`
    /// fails or the save does.
    pub fn try_update<R, E: From<io::Error>>(
        &self,
        change: impl FnOnce(&mut T) -> Result<R, E>,
    ) -> Result<R, E> {
        let mut current = lock(&self.current);
        let mut next = current.clone();
        let result = change(&mut next)?;
        let bytes = serde_json::to_vec_pretty(&next).map_err(io::Error::from)?;
        write_atomic(&self.path, &bytes)?;
        *current = next;
        Ok(result)
    }

    pub fn update<R>(&self, change: impl FnOnce(&mut T) -> R) -> io::Result<R> {
        self.try_update(|value| Ok::<_, io::Error>(change(value)))
    }
}

/// The history file: entries plus the stats ledger, which outlives deleted
/// entries.
pub struct HistoryFile {
    path: PathBuf,
    current: Mutex<HistoryStore>,
}

impl HistoryFile {
    /// Reads `path`, also the older plain list of entries. What can't be read
    /// is logged; a file that isn't a history at all is moved aside to
    /// `history.json.bad`.
    pub fn load(path: PathBuf) -> Self {
        let store = match fs::read_to_string(&path) {
            Ok(json) => {
                let (store, problem) = HistoryStore::from_json(&json);
                if let Some(problem) = problem {
                    tracing::warn!(%problem, path = %path.display(), "history file");
                    if matches!(problem, HistoryLoadError::Corrupt(_)) {
                        let _ = fs::rename(&path, path.with_extension("json.bad"));
                    }
                }
                store
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => HistoryStore::default(),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "could not read history, starting empty");
                HistoryStore::default()
            }
        };
        Self {
            path,
            current: Mutex::new(store),
        }
    }

    pub fn get(&self) -> HistoryStore {
        lock(&self.current).clone()
    }

    pub fn read<R>(&self, f: impl FnOnce(&HistoryStore) -> R) -> R {
        f(&lock(&self.current))
    }

    /// Applies `change` and saves. Nothing changes in memory when the save
    /// fails.
    pub fn update<R>(&self, change: impl FnOnce(&mut HistoryStore) -> R) -> io::Result<R> {
        let mut current = lock(&self.current);
        let mut next = current.clone();
        let result = change(&mut next);
        let json = next.to_json().map_err(io::Error::from)?;
        write_atomic(&self.path, json.as_bytes())?;
        *current = next;
        Ok(result)
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// A unique id: the time in milliseconds plus a per-process counter.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{n:x}", now_ms(), std::process::id())
}

#[cfg(test)]
pub mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    pub fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("tiny-wispr-{name}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_reloads_and_keeps_failed_changes_out() {
        let dir = temp_dir("store");
        let path = dir.join("list.json");
        let store = JsonStore::<Vec<String>>::load(path.clone());
        store.update(|v| v.push("a".into())).unwrap();
        let failed: Result<(), io::Error> = store.try_update(|v| {
            v.push("b".into());
            Err(io::Error::other("no"))
        });
        assert!(failed.is_err());
        assert_eq!(JsonStore::<Vec<String>>::load(path).get(), vec!["a"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = temp_dir("store-bad");
        let path = dir.join("list.json");
        fs::write(&path, b"[1,").unwrap();
        assert!(JsonStore::<Vec<u32>>::load(path.clone()).get().is_empty());
        assert!(path.with_extension("json.bad").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    fn entry(id: &str) -> tw_core::HistoryEntry {
        tw_core::HistoryEntry {
            id: id.into(),
            created_at: crate::windows::now_ms(),
            raw_text: "hello there".into(),
            final_text: "Hello there.".into(),
            app: None,
            duration_ms: 1_000,
            word_count: 2,
            language: None,
        }
    }

    #[test]
    fn history_keeps_stats_for_deleted_entries_across_reloads() {
        let dir = temp_dir("history");
        let path = dir.join("history.json");
        let file = HistoryFile::load(path.clone());
        file.update(|h| h.push_with_fixes(entry("a"), 3)).unwrap();
        file.update(HistoryStore::clear).unwrap();

        let reloaded = HistoryFile::load(path).get();
        assert!(reloaded.entries().is_empty());
        let stats = reloaded.stats(crate::windows::now_ms());
        assert_eq!(stats.total_words, 2);
        assert_eq!(stats.fixes_made, 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn history_reads_the_old_plain_list() {
        let dir = temp_dir("history-old");
        let path = dir.join("history.json");
        fs::write(&path, serde_json::to_vec(&[entry("a")]).unwrap()).unwrap();
        let history = HistoryFile::load(path.clone()).get();
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.stats(crate::windows::now_ms()).total_dictations, 1);
        assert!(path.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_history_is_moved_aside() {
        let dir = temp_dir("history-bad");
        let path = dir.join("history.json");
        fs::write(&path, b"{nope").unwrap();
        assert!(HistoryFile::load(path.clone()).get().entries().is_empty());
        assert!(path.with_extension("json.bad").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(), new_id());
    }
}
