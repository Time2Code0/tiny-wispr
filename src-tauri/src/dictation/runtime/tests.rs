//! The whole runtime driven by fakes: a scripted microphone, a fake
//! transcriber, a recording desktop and sink. Time is a fake clock; the
//! audio, pipeline and insertion threads are real.

use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant as StdInstant};

use tw_core::dictation::machine::{Event, Instant, Problem, Sound};
use tw_core::dictation::{NoticeActionKind, NoticeKind};
use tw_core::history::{HistoryRetention, HistoryStore};
use tw_core::hotkey::HotkeyAction::{Cancel, HandsFree, PushToTalk};
use tw_core::text::AppInfo;
use tw_core::{DictationPhase, DictationState, Settings};
use tw_models::Inventory;
use tw_stt::{Result as SttResult, SttError, TranscribeOptions, Transcriber, Transcript};

use super::*;
use crate::dictation::engine::WhisperModels;
use crate::dictation::{AudioSource, Desktop, NoPolish, Sink, Transcribers};
use crate::events::HotkeyCapture;
use crate::state::{AppPaths, Data};
use crate::store::HistoryFile;
use crate::store::tests::temp_dir;

const WAIT: Duration = Duration::from_secs(10);

/// Half a second of near silence around a second of a loud tone, so the
/// speech detector finds speech.
fn speech() -> Vec<f32> {
    let quiet = |n: usize| (0..n).map(|i| if i % 2 == 0 { 0.001 } else { -0.001 });
    let tone = (0..16_000).map(|i| {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f32 / 16_000.0;
        (TAU * 220.0 * t).sin() * 0.4
    });
    quiet(8_000).chain(tone).chain(quiet(8_000)).collect()
}

struct FakeSource {
    samples: Vec<f32>,
    fail: Option<Problem>,
}

impl AudioSource for FakeSource {
    fn start(&mut self, _device: Option<&str>) -> Result<(), Problem> {
        self.fail.clone().map_or(Ok(()), Err)
    }

    fn stop(&mut self) -> tw_audio::Recording {
        tw_audio::Recording {
            samples: self.samples.clone(),
            ..tw_audio::Recording::default()
        }
    }

    fn cancel(&mut self) {}

    fn start_monitor(&mut self, _device: Option<&str>) -> tw_audio::Result<()> {
        match self.fail {
            Some(Problem::NoMicrophone) => Err(tw_audio::AudioError::NoDevice),
            _ => Ok(()),
        }
    }

    fn stop_monitor(&mut self) {}
}

/// Returns `text`. While `gate` is set, each call waits for one message on
/// it first and ignores cancellation, like a model that answers late.
struct FakeTranscriber {
    text: String,
    gate: Option<Mutex<Receiver<()>>>,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(
        &self,
        samples: &[f32],
        _options: &TranscribeOptions,
        _cancel: &AtomicBool,
    ) -> SttResult<Transcript> {
        if let Some(gate) = &self.gate {
            lock(gate)
                .recv_timeout(WAIT)
                .map_err(|_| SttError::Cancelled)?;
        }
        Ok(Transcript {
            text: self.text.clone(),
            language: Some("en".into()),
            audio_ms: samples.len() as u64 / 16,
            processing_ms: 1,
        })
    }
}

struct FakeTranscribers {
    model: Option<Arc<FakeTranscriber>>,
}

impl Transcribers for FakeTranscribers {
    fn has_model(&self, _settings: &Settings) -> bool {
        self.model.is_some()
    }

    fn load(&self, _settings: &Settings) -> Result<Arc<dyn Transcriber>, Problem> {
        self.model
            .clone()
            .map(|m| m as Arc<dyn Transcriber>)
            .ok_or(Problem::NoModel)
    }
}

#[derive(Default)]
struct FakeDesktop {
    inserted: Mutex<Vec<String>>,
    sounds: Mutex<Vec<Sound>>,
    fail_insert: AtomicBool,
}

impl Desktop for FakeDesktop {
    fn frontmost_app(&self) -> Option<AppInfo> {
        None
    }

    fn insert(&self, text: &str, _settings: &Settings) -> Result<(), String> {
        if self.fail_insert.load(Ordering::Relaxed) {
            return Err("nothing has focus".into());
        }
        lock(&self.inserted).push(text.to_owned());
        Ok(())
    }

    fn play(&self, sound: Sound) {
        lock(&self.sounds).push(sound);
    }

    fn microphone_denied(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct FakeSink {
    states: Mutex<Vec<DictationState>>,
    history: Mutex<usize>,
    settings_opened: Mutex<usize>,
}

impl Sink for FakeSink {
    fn state(&self, state: &DictationState) {
        lock(&self.states).push(state.clone());
    }

    fn level(&self, _level: f32) {}

    fn history_changed(&self) {
        *lock(&self.history) += 1;
    }

    fn hotkey_captured(&self, _capture: HotkeyCapture) {}

    fn open_settings(&self) {
        *lock(&self.settings_opened) += 1;
    }
}

struct Setup {
    samples: Vec<f32>,
    mic_fails: Option<Problem>,
    transcribers: Option<Arc<dyn Transcribers>>,
    text: &'static str,
    gate: Option<Receiver<()>>,
    fail_insert: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            samples: speech(),
            mic_fails: None,
            transcribers: None,
            text: " Hello world.",
            gate: None,
            fail_insert: false,
        }
    }
}

struct Harness {
    runtime: Runtime,
    rx: Receiver<Msg>,
    now: Instant,
    desktop: Arc<FakeDesktop>,
    sink: Arc<FakeSink>,
    data: Arc<Data>,
    dir: PathBuf,
}

impl Harness {
    fn new(setup: Setup) -> Self {
        let dir = temp_dir("runtime");
        let data = Arc::new(Data::load(AppPaths::new(&dir)));
        let desktop = Arc::new(FakeDesktop {
            fail_insert: AtomicBool::new(setup.fail_insert),
            ..FakeDesktop::default()
        });
        let sink = Arc::new(FakeSink::default());
        let transcribers = setup.transcribers.unwrap_or_else(|| {
            Arc::new(FakeTranscribers {
                model: Some(Arc::new(FakeTranscriber {
                    text: setup.text.into(),
                    gate: setup.gate.map(Mutex::new),
                })),
            })
        });
        let deps = Deps {
            data: Arc::clone(&data),
            transcribers,
            desktop: Arc::clone(&desktop) as Arc<dyn Desktop>,
            polisher: Arc::new(NoPolish),
            sink: Arc::clone(&sink) as Arc<dyn Sink>,
            keys: Arc::new(Keys::default()),
        };
        let (tx, rx) = channel();
        let source = Box::new(FakeSource {
            samples: setup.samples,
            fail: setup.mic_fails,
        });
        Self {
            runtime: Runtime::new(deps, source, tx, Arc::new(Shared::default())),
            rx,
            now: 1_000,
            desktop,
            sink,
            data,
            dir,
        }
    }

    fn send(&mut self, event: Event) {
        self.runtime.handle(Msg::Event(event), self.now);
    }

    fn advance(&mut self, ms: u64) {
        self.now += ms;
        self.runtime.fire_due(self.now);
    }

    fn view(&self) -> DictationState {
        lock(&self.runtime.shared.view).clone()
    }

    fn phase(&self) -> DictationPhase {
        self.view().phase
    }

    /// Feeds what the worker threads send back until `done` holds.
    fn pump_until(&mut self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = StdInstant::now() + WAIT;
        while !done(self) {
            assert!(StdInstant::now() < deadline, "timed out waiting for {what}");
            if let Ok(msg) = self.rx.recv_timeout(Duration::from_millis(20)) {
                self.runtime.handle(msg, self.now);
            }
        }
    }

    fn inserted(&self) -> Vec<String> {
        lock(&self.desktop.inserted).clone()
    }

    fn notice(&self) -> Option<NoticeKind> {
        self.view().notice.map(|n| n.kind)
    }

    /// Feeds messages until the audio thread reports the microphone open.
    fn listening(&mut self) {
        loop {
            let msg = self
                .rx
                .recv_timeout(WAIT)
                .expect("timed out waiting for the microphone");
            let started = matches!(msg, Msg::Event(Event::CaptureStarted { .. }));
            self.runtime.handle(msg, self.now);
            if started {
                break;
            }
        }
        assert_eq!(self.phase(), DictationPhase::Listening);
    }

    /// Push to talk held long enough to count as a hold.
    fn hold_and_release(&mut self) {
        self.send(Event::HotkeyPressed(PushToTalk));
        self.listening();
        self.advance(1_200);
        self.send(Event::HotkeyReleased(PushToTalk));
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn hold_and_release_inserts_the_transcript_and_records_history() {
    let mut h = Harness::new(Setup::default());
    h.hold_and_release();
    assert_eq!(h.phase(), DictationPhase::Processing);
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    h.pump_until("idle", |h| h.phase() == DictationPhase::Idle);

    assert_eq!(h.inserted(), vec!["Hello world."]);
    let history = h.data.history.get().entries().to_vec();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].raw_text, "Hello world.");
    assert_eq!(history[0].word_count, 2);
    assert_eq!(*lock(&h.sink.history), 1);
    assert_eq!(
        lock(&h.runtime.shared.last_text).as_deref(),
        Some("Hello world.")
    );
    assert_eq!(*lock(&h.desktop.sounds), vec![Sound::Start, Sound::Stop]);
    let phases: Vec<_> = lock(&h.sink.states).iter().map(|s| s.phase).collect();
    assert_eq!(
        phases,
        vec![
            DictationPhase::Listening,
            DictationPhase::Processing,
            DictationPhase::Idle
        ]
    );
}

#[test]
fn history_off_keeps_nothing_but_counts_stats_and_clearing_keeps_them() {
    let mut h = Harness::new(Setup {
        text: " Um, hello world.",
        ..Setup::default()
    });
    h.hold_and_release();
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    h.pump_until("history", |h| *lock(&h.sink.history) == 1);
    h.data.change_history(HistoryStore::clear).unwrap();
    let stats = h.data.history.get().stats(now_ms());
    assert_eq!((stats.total_dictations, stats.total_words), (1, 2));
    assert_eq!(stats.fixes_made, 1, "the filler counts as a fix");

    h.data
        .settings
        .update(|s| s.history_retention = HistoryRetention::Never)
        .unwrap();
    h.pump_until("idle", |h| h.phase() == DictationPhase::Idle);
    h.hold_and_release();
    h.pump_until("history", |h| *lock(&h.sink.history) == 2);

    let reloaded = HistoryFile::load(h.data.paths.history_file.clone()).get();
    assert!(reloaded.entries().is_empty(), "history off stores nothing");
    let stats = reloaded.stats(now_ms());
    assert_eq!((stats.total_dictations, stats.total_words), (2, 4));
}

#[test]
fn the_microphone_test_reports_whether_it_opened() {
    let mut h = Harness::new(Setup::default());
    let (reply, opened) = channel();
    h.runtime.handle(Msg::MicTest(Some(reply)), h.now);
    assert!(opened.recv_timeout(WAIT).unwrap().is_ok());

    let mut h = Harness::new(Setup {
        mic_fails: Some(Problem::NoMicrophone),
        ..Setup::default()
    });
    let (reply, opened) = channel();
    h.runtime.handle(Msg::MicTest(Some(reply)), h.now);
    let err = opened.recv_timeout(WAIT).unwrap().unwrap_err();
    assert_eq!(
        crate::error::CommandError::from(err).kind,
        crate::error::ErrorKind::NoMicrophone
    );
}

#[test]
fn sounds_follow_the_setting() {
    let mut h = Harness::new(Setup::default());
    h.data.settings.update(|s| s.sounds = false).unwrap();
    h.hold_and_release();
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert!(lock(&h.desktop.sounds).is_empty());
}

#[test]
fn double_tap_starts_hands_free_until_the_next_press() {
    let mut h = Harness::new(Setup::default());
    h.send(Event::HotkeyPressed(PushToTalk));
    h.listening();
    h.advance(150);
    h.send(Event::HotkeyReleased(PushToTalk));
    h.advance(150);
    h.send(Event::HotkeyPressed(PushToTalk));
    h.send(Event::HotkeyReleased(PushToTalk));
    // Long past the double-tap window: still listening, hands-free.
    h.advance(5_000);
    assert_eq!(h.phase(), DictationPhase::Listening);
    assert_eq!(
        h.view().mode,
        Some(tw_core::DictationMode::HandsFree),
        "a double tap locks dictation on"
    );
    assert!(h.inserted().is_empty());

    h.send(Event::HotkeyPressed(HandsFree));
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert_eq!(h.inserted(), vec!["Hello world."]);
}

#[test]
fn the_voice_model_is_held_for_as_long_as_capture_runs() {
    let model = Arc::new(FakeTranscriber {
        text: "Hello.".into(),
        gate: None,
    });
    let mut h = Harness::new(Setup {
        transcribers: Some(Arc::new(FakeTranscribers {
            model: Some(Arc::clone(&model)),
        })),
        ..Setup::default()
    });
    let idle = Arc::strong_count(&model);
    h.send(Event::StartRequested);
    h.listening();
    let deadline = StdInstant::now() + WAIT;
    while Arc::strong_count(&model) == idle {
        assert!(
            StdInstant::now() < deadline,
            "not loaded when capture starts"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    h.advance(60_000);
    assert_eq!(Arc::strong_count(&model), idle + 1);

    h.send(Event::HotkeyPressed(Cancel));
    let deadline = StdInstant::now() + WAIT;
    while Arc::strong_count(&model) > idle {
        assert!(StdInstant::now() < deadline, "the hold was never released");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_single_short_tap_is_dropped_silently() {
    let mut h = Harness::new(Setup::default());
    h.send(Event::HotkeyPressed(PushToTalk));
    h.listening();
    h.advance(150);
    h.send(Event::HotkeyReleased(PushToTalk));
    h.advance(1_000);
    assert_eq!(h.phase(), DictationPhase::Idle);
    assert_eq!(h.notice(), None);
    std::thread::sleep(Duration::from_millis(100));
    while let Ok(msg) = h.rx.try_recv() {
        h.runtime.handle(msg, h.now);
    }
    assert!(h.inserted().is_empty());
    assert!(h.data.history.get().entries().is_empty());
}

#[test]
fn cancel_while_listening_then_undo_inserts_the_kept_audio() {
    let mut h = Harness::new(Setup::default());
    h.send(Event::StartRequested);
    h.listening();
    h.advance(2_000);
    h.send(Event::HotkeyPressed(Cancel));
    assert_eq!(h.notice(), Some(NoticeKind::Cancelled));
    let action = h.view().notice.and_then(|n| n.action).map(|a| a.kind);
    assert_eq!(action, Some(NoticeActionKind::Undo));
    // The cancelled audio arrives from the audio thread and is kept.
    h.pump_until("kept audio", |h| !h.runtime.kept.is_empty());
    assert!(h.inserted().is_empty());

    h.send(Event::UndoRequested);
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert_eq!(h.inserted(), vec!["Hello world."]);
    assert!(h.runtime.kept.is_empty());
}

#[test]
fn undo_right_after_cancel_waits_for_the_audio() {
    let mut h = Harness::new(Setup::default());
    h.send(Event::StartRequested);
    h.listening();
    h.advance(2_000);
    h.send(Event::CancelRequested);
    // Undo before the audio thread has handed the samples over.
    h.send(Event::UndoRequested);
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert_eq!(h.inserted(), vec!["Hello world."]);
}

#[test]
fn the_cancelled_notice_expiring_drops_the_kept_audio() {
    let mut h = Harness::new(Setup::default());
    h.send(Event::StartRequested);
    h.listening();
    h.advance(2_000);
    h.send(Event::CancelRequested);
    h.pump_until("kept audio", |h| !h.runtime.kept.is_empty());
    h.advance(10_000);
    assert_eq!(h.phase(), DictationPhase::Idle);
    assert!(h.runtime.kept.is_empty());
}

#[test]
fn silence_shows_no_speech_and_inserts_nothing() {
    let mut h = Harness::new(Setup {
        samples: vec![0.0; 32_000],
        ..Setup::default()
    });
    h.hold_and_release();
    h.pump_until("a notice", |h| h.notice().is_some());
    assert_eq!(h.notice(), Some(NoticeKind::NoSpeech));
    assert!(h.inserted().is_empty());
    assert!(h.data.history.get().entries().is_empty());
}

#[test]
fn no_model_shows_a_notice_that_opens_settings() {
    let mut h = Harness::new(Setup {
        transcribers: Some(Arc::new(FakeTranscribers { model: None })),
        ..Setup::default()
    });
    h.send(Event::HotkeyPressed(PushToTalk));
    assert_eq!(h.notice(), Some(NoticeKind::NoModel));
    assert_ne!(h.phase(), DictationPhase::Listening);
    let action = h.view().notice.and_then(|n| n.action).map(|a| a.kind);
    assert_eq!(action, Some(NoticeActionKind::OpenSettings));
    h.send(Event::OpenSettingsRequested);
    assert_eq!(*lock(&h.sink.settings_opened), 1);
}

#[test]
fn a_missing_microphone_shows_a_notice() {
    let mut h = Harness::new(Setup {
        mic_fails: Some(Problem::NoMicrophone),
        ..Setup::default()
    });
    h.send(Event::HotkeyPressed(PushToTalk));
    h.pump_until("a notice", |h| h.notice().is_some());
    assert_eq!(h.notice(), Some(NoticeKind::NoMicrophone));
}

#[test]
fn failed_insertion_offers_paste_last_which_retries() {
    let mut h = Harness::new(Setup {
        fail_insert: true,
        ..Setup::default()
    });
    h.hold_and_release();
    h.pump_until("a notice", |h| h.notice().is_some());
    assert_eq!(h.notice(), Some(NoticeKind::PasteFailed));
    // The text is kept in history and as the last transcript.
    assert_eq!(h.data.history.get().entries().len(), 1);
    assert_eq!(
        lock(&h.runtime.shared.last_text).as_deref(),
        Some("Hello world.")
    );
    let action = h.view().notice.and_then(|n| n.action).map(|a| a.kind);
    assert_eq!(action, Some(NoticeActionKind::CopyLast));

    h.desktop.fail_insert.store(false, Ordering::Relaxed);
    h.send(Event::PasteLastRequested);
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert_eq!(h.inserted(), vec!["Hello world."]);
    assert_eq!(
        h.data.history.get().entries().len(),
        1,
        "a paste is not a new dictation"
    );
}

#[test]
fn a_late_result_after_cancel_is_not_inserted() {
    let (open, gate) = channel::<()>();
    let mut h = Harness::new(Setup {
        gate: Some(gate),
        ..Setup::default()
    });
    h.hold_and_release();
    h.pump_until("the pipeline", |h| !h.runtime.running.is_empty());
    h.send(Event::CancelRequested);
    assert_eq!(h.notice(), Some(NoticeKind::Cancelled));
    // The stale transcription finishes after the cancel.
    open.send(()).unwrap();
    h.pump_until("the stale job to end", |h| h.runtime.running.is_empty());
    std::thread::sleep(Duration::from_millis(100));
    while let Ok(msg) = h.rx.try_recv() {
        h.runtime.handle(msg, h.now);
    }
    assert!(h.inserted().is_empty());
    assert!(h.data.history.get().entries().is_empty());

    // Undo transcribes the kept audio under a new job.
    h.send(Event::UndoRequested);
    open.send(()).unwrap();
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    assert_eq!(h.inserted(), vec!["Hello world."]);
}

#[test]
fn shortcut_recorder_suspends_dictation() {
    let mut h = Harness::new(Setup::default());
    h.runtime.handle(Msg::HotkeyCapture(true), h.now);
    h.send(Event::HotkeyPressed(PushToTalk));
    assert_eq!(h.phase(), DictationPhase::Idle);
    h.runtime.handle(Msg::HotkeyCapture(false), h.now);
    h.send(Event::HotkeyPressed(PushToTalk));
    h.listening();
}

/// The real Whisper model on a `say` recording. Skipped unless
/// `TINY_WISPR_TEST_MODELS` holds `whisper/ggml-tiny.en.bin`.
#[test]
fn real_whisper_model_transcribes_a_recording() {
    let Some(models) = std::env::var_os("TINY_WISPR_TEST_MODELS").map(PathBuf::from) else {
        eprintln!("skipping: TINY_WISPR_TEST_MODELS is not set");
        return;
    };
    if !models.join("whisper/ggml-tiny.en.bin").is_file() {
        eprintln!("skipping: ggml-tiny.en.bin is missing");
        return;
    }
    let Some(samples) = fixture("The quick brown fox jumps over the lazy dog.") else {
        return;
    };
    let whisper = Arc::new(WhisperModels::new(Inventory::new(models)));
    let mut h = Harness::new(Setup {
        samples,
        transcribers: Some(Arc::clone(&whisper) as Arc<dyn Transcribers>),
        ..Setup::default()
    });
    h.data
        .settings
        .update(|s| s.stt_model = Some("tiny.en".into()))
        .unwrap();
    h.hold_and_release();
    h.pump_until("insertion", |h| !h.inserted().is_empty());
    let text = h.inserted().remove(0).to_lowercase();
    assert!(
        text.contains("quick brown fox") && text.contains("lazy dog"),
        "{text}"
    );
    drop(h);
    whisper.unload();
}

fn fixture(text: &str) -> Option<Vec<f32>> {
    let out = std::env::temp_dir().join("tiny-wispr-app-fox.wav");
    if !out.exists() {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/make-speech-fixture.sh");
        let made = Command::new(script).arg(text).arg(&out).output().ok()?;
        if !made.status.success() {
            eprintln!(
                "skipping: could not make a speech fixture: {}",
                String::from_utf8_lossy(&made.stderr)
            );
            return None;
        }
    }
    Some(tw_audio::wav::read(&out).ok()?.samples)
}
