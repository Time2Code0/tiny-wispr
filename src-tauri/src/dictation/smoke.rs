//! Debug builds only: `TINY_WISPR_SMOKE=<wav>` runs one push-to-talk
//! dictation through the real runtime with the WAV as the microphone, writes
//! the inserted text to `TINY_WISPR_SMOKE_OUT`, and exits 0 (non-zero on
//! failure). This is how the app is checked end to end without a person.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tw_core::dictation::machine::{Event, Problem, Sound};
use tw_core::hotkey::HotkeyAction;
use tw_core::text::AppInfo;
use tw_core::{DictationPhase, Settings};

use super::{AudioSource, Desktop};
use crate::state::AppState;

const START_DELAY: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(120);

pub struct Smoke {
    out: PathBuf,
    samples: Arc<[f32]>,
}

impl Smoke {
    /// `None` unless `TINY_WISPR_SMOKE` is set; an error when it is set but
    /// unusable.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Some(wav) = std::env::var_os("TINY_WISPR_SMOKE").map(PathBuf::from) else {
            return Ok(None);
        };
        let out = std::env::var_os("TINY_WISPR_SMOKE_OUT")
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("TINY_WISPR_SMOKE_OUT is not set"))?;
        let recording = tw_audio::wav::read(&wav)?;
        tracing::info!(wav = %wav.display(), ms = recording.duration_ms(), "smoke: audio loaded");
        let _ = fs::remove_file(&out);
        Ok(Some(Self {
            out,
            samples: recording.samples.into(),
        }))
    }

    pub fn source(&self) -> Box<dyn AudioSource> {
        Box::new(WavSource {
            samples: Arc::clone(&self.samples),
        })
    }

    pub fn desktop(&self) -> Arc<dyn Desktop> {
        Arc::new(FileDesktop {
            out: self.out.clone(),
        })
    }

    /// Holds push to talk for the length of the audio, then waits for the
    /// text and exits.
    pub fn run(self, app: AppHandle) {
        std::thread::spawn(move || {
            std::thread::sleep(START_DELAY);
            let state = app.state::<AppState>();
            let hold = Duration::from_millis(self.samples.len() as u64 * 1_000 / 16_000)
                .max(Duration::from_millis(800));
            tracing::info!(hold_ms = hold.as_millis(), "smoke: pressing push to talk");
            state
                .dictation
                .event(Event::HotkeyPressed(HotkeyAction::PushToTalk));
            std::thread::sleep(hold);
            tracing::info!("smoke: releasing push to talk");
            state
                .dictation
                .event(Event::HotkeyReleased(HotkeyAction::PushToTalk));

            let started = Instant::now();
            let code = loop {
                if let Ok(text) = fs::read_to_string(&self.out) {
                    tracing::info!(%text, "smoke: inserted");
                    break 0;
                }
                let view = state.dictation.view();
                if view.phase == DictationPhase::Notice {
                    tracing::error!(notice = ?view.notice, "smoke: dictation failed");
                    break 1;
                }
                if started.elapsed() > TIMEOUT {
                    tracing::error!("smoke: timed out");
                    break 2;
                }
                std::thread::sleep(Duration::from_millis(50));
            };
            tracing::info!(code, "smoke: done");
            app.exit(code);
        });
    }
}

struct WavSource {
    samples: Arc<[f32]>,
}

impl AudioSource for WavSource {
    fn start(&mut self, _device: Option<&str>) -> Result<(), Problem> {
        Ok(())
    }

    fn stop(&mut self) -> tw_audio::Recording {
        tw_audio::Recording {
            samples: self.samples.to_vec(),
            ..tw_audio::Recording::default()
        }
    }

    fn cancel(&mut self) {}

    fn start_monitor(&mut self, _device: Option<&str>) -> tw_audio::Result<()> {
        Ok(())
    }

    fn stop_monitor(&mut self) {}
}

struct FileDesktop {
    out: PathBuf,
}

impl Desktop for FileDesktop {
    fn frontmost_app(&self) -> Option<AppInfo> {
        None
    }

    fn insert(&self, text: &str, _settings: &Settings) -> Result<(), String> {
        fs::write(&self.out, text).map_err(|err| err.to_string())
    }

    fn play(&self, sound: Sound) {
        tracing::info!(?sound, "smoke: sound");
    }

    fn microphone_denied(&self) -> bool {
        false
    }
}
