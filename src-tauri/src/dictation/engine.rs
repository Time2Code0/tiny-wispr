//! The Whisper model, loaded on first use and let go of after a while without
//! a dictation, when the selection changes, or when the app quits.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use tw_core::Settings;
use tw_core::dictation::machine::Problem;
use tw_models::Inventory;
use tw_stt::{Transcriber, WhisperEngine};

use super::Transcribers;
use crate::models::voice_model_path;
use crate::state::lock;

/// A loaded model holds 0.3–3 GB; reloading takes up to a second.
const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);
const IDLE_CHECK_EVERY: Duration = Duration::from_secs(60);

pub struct WhisperModels {
    inventory: Inventory,
    cache: IdleCache<WhisperEngine>,
}

impl WhisperModels {
    pub fn new(inventory: Inventory) -> Self {
        Self {
            inventory,
            cache: IdleCache::new(IDLE_LIMIT, Instant::now),
        }
    }

    fn path(&self, settings: &Settings) -> Option<PathBuf> {
        voice_model_path(&self.inventory, settings.stt_model.as_deref())
    }

    /// Checks every minute, for as long as the models exist, whether the
    /// voice model has been idle long enough to drop.
    pub fn spawn_idle_check(self: &Arc<Self>) {
        let models = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(IDLE_CHECK_EVERY);
                let Some(models) = Weak::upgrade(&models) else {
                    return;
                };
                if models.cache.unload_if_idle() {
                    tracing::info!("voice model unloaded after being idle");
                }
            }
        });
    }

    /// ggml's Metal backend aborts the process if a model is still loaded
    /// when it exits, so drop it first, waiting briefly for a cancelled
    /// transcription to let go of it.
    pub fn unload(&self) {
        let Some(engine) = self.cache.take() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while Arc::strong_count(&engine) > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if Arc::strong_count(&engine) > 1 {
            tracing::warn!("the voice model is still in use at exit");
        }
        drop(engine);
        tracing::info!("voice model unloaded");
    }
}

impl Transcribers for WhisperModels {
    fn has_model(&self, settings: &Settings) -> bool {
        self.path(settings).is_some()
    }

    fn load(&self, settings: &Settings) -> Result<Arc<dyn Transcriber>, Problem> {
        let path = self.path(settings).ok_or(Problem::NoModel)?;
        let engine = self.cache.get_or_load(&path, |path| {
            WhisperEngine::load(path).map_err(|err| {
                tracing::warn!(%err, path = %path.display(), "could not load the voice model");
                Problem::Failed(err.to_string())
            })
        })?;
        Ok(engine)
    }
}

/// One loaded model, dropped once nobody has used it for `limit`. Whoever
/// holds an `Arc` from [`IdleCache::get_or_load`] counts as using it.
struct IdleCache<T> {
    limit: Duration,
    clock: Box<dyn Fn() -> Instant + Send + Sync>,
    slot: Mutex<Slot<T>>,
}

struct Slot<T> {
    loaded: Option<(PathBuf, Arc<T>)>,
    used: Instant,
}

impl<T> IdleCache<T> {
    fn new(limit: Duration, clock: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        let used = clock();
        Self {
            limit,
            clock: Box::new(clock),
            slot: Mutex::new(Slot { loaded: None, used }),
        }
    }

    /// Returns the model at `path`, loading it (and dropping any other) if it
    /// isn't the one loaded. Blocks while loading.
    fn get_or_load<E>(
        &self,
        path: &Path,
        load: impl FnOnce(&Path) -> Result<T, E>,
    ) -> Result<Arc<T>, E> {
        let mut slot = lock(&self.slot);
        slot.used = (self.clock)();
        if let Some((loaded_path, value)) = &slot.loaded
            && loaded_path == path
        {
            return Ok(Arc::clone(value));
        }
        slot.loaded = None;
        let value = Arc::new(load(path)?);
        slot.loaded = Some((path.to_owned(), Arc::clone(&value)));
        Ok(value)
    }

    /// Drops the model if nobody holds it and it hasn't been used for the
    /// limit. A holder restarts the idle time, so a long dictation is never
    /// cut short.
    fn unload_if_idle(&self) -> bool {
        let mut slot = lock(&self.slot);
        let now = (self.clock)();
        let Some((_, value)) = &slot.loaded else {
            return false;
        };
        // Only `get_or_load` hands out clones, under this same lock.
        if Arc::strong_count(value) > 1 {
            slot.used = now;
            return false;
        }
        if now.saturating_duration_since(slot.used) < self.limit {
            return false;
        }
        let dropped = slot.loaded.take();
        drop(slot);
        drop(dropped);
        true
    }

    fn take(&self) -> Option<Arc<T>> {
        lock(&self.slot).loaded.take().map(|(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const LIMIT: Duration = Duration::from_secs(600);

    struct Model;

    struct Harness {
        cache: IdleCache<Model>,
        now: Arc<Mutex<Instant>>,
        loads: AtomicUsize,
    }

    impl Harness {
        fn new() -> Self {
            let now = Arc::new(Mutex::new(Instant::now()));
            let clock = Arc::clone(&now);
            Self {
                cache: IdleCache::new(LIMIT, move || *lock(&clock)),
                now,
                loads: AtomicUsize::new(0),
            }
        }

        fn load(&self, path: &str) -> Arc<Model> {
            let loaded: Result<_, ()> = self.cache.get_or_load(Path::new(path), |_| {
                self.loads.fetch_add(1, Ordering::Relaxed);
                Ok(Model)
            });
            loaded.expect("the fake loader never fails")
        }

        fn wait(&self, seconds: u64) {
            *lock(&self.now) += Duration::from_secs(seconds);
        }

        fn loads(&self) -> usize {
            self.loads.load(Ordering::Relaxed)
        }

        fn is_loaded(&self) -> bool {
            lock(&self.cache.slot).loaded.is_some()
        }
    }

    #[test]
    fn an_idle_model_is_unloaded_and_reloaded_on_next_use() {
        let h = Harness::new();
        drop(h.load("base.bin"));
        h.wait(599);
        assert!(!h.cache.unload_if_idle());
        assert!(h.is_loaded());

        drop(h.load("base.bin"));
        assert_eq!(h.loads(), 1);
        h.wait(599);
        assert!(!h.cache.unload_if_idle(), "using it restarts the idle time");

        h.wait(1);
        assert!(h.cache.unload_if_idle());
        assert!(!h.is_loaded());
        assert!(!h.cache.unload_if_idle());

        drop(h.load("base.bin"));
        assert_eq!(h.loads(), 2);
        assert!(h.is_loaded());
    }

    #[test]
    fn a_model_in_use_is_never_unloaded() {
        let h = Harness::new();
        let held = h.load("base.bin");
        h.wait(3_600);
        assert!(!h.cache.unload_if_idle());
        assert!(h.is_loaded());

        drop(held);
        h.wait(599);
        assert!(
            !h.cache.unload_if_idle(),
            "the idle time starts when it was last seen in use"
        );
        h.wait(1);
        assert!(h.cache.unload_if_idle());
    }

    #[test]
    fn another_model_replaces_the_loaded_one() {
        let h = Harness::new();
        drop(h.load("base.bin"));
        drop(h.load("small.bin"));
        drop(h.load("small.bin"));
        assert_eq!(h.loads(), 2);
        assert!(h.cache.take().is_some());
        assert!(h.cache.take().is_none());
    }
}
