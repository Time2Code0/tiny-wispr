//! `settings.json` in the app data dir: loaded once, written atomically.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tw_core::Settings;

use crate::state::lock;
use crate::store::write_atomic;

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Reads `path`, falling back to defaults when it is missing. A corrupt
    /// file is moved aside to `settings.json.bad` rather than overwritten.
    pub fn load(path: PathBuf) -> Self {
        let settings = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(settings) => settings,
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "settings file is invalid, using defaults");
                    let _ = fs::rename(&path, path.with_extension("json.bad"));
                    Settings::default()
                }
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "could not read settings, using defaults");
                Settings::default()
            }
        };
        Self {
            path,
            current: Mutex::new(settings),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self) -> Settings {
        lock(&self.current).clone()
    }

    /// Applies `change`, saves, and returns the new settings. Nothing changes
    /// in memory if the save fails.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> io::Result<Settings> {
        let mut current = lock(&self.current);
        let mut next = current.clone();
        change(&mut next);
        write_atomic(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        *current = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use tw_core::settings::Theme;

    use super::*;

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("tiny-wispr-test-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_and_reloads() {
        let dir = temp_dir();
        let path = dir.join("nested").join("settings.json");
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());

        let saved = store.update(|s| s.theme = Theme::Dark).unwrap();
        assert_eq!(saved.theme, Theme::Dark);
        assert!(!path.with_extension("json.tmp").exists());

        let reloaded = SettingsStore::load(path);
        assert_eq!(reloaded.get().theme, Theme::Dark);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = temp_dir();
        let path = dir.join("settings.json");
        fs::write(&path, b"{ not json").unwrap();
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get(), Settings::default());
        assert!(path.with_extension("json.bad").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
