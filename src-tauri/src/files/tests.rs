use std::collections::HashMap;
use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use tw_core::Settings;
use tw_core::text::polish::PolishRequest;
use tw_models::Inventory;
use tw_stt::{Result as SttResult, TranscribeOptions, Transcriber, Transcript};

use super::*;
use crate::dictation::engine::WhisperModels;
use crate::state::AppPaths;
use crate::store::tests::temp_dir;

const WAIT: Duration = Duration::from_secs(20);

/// Answers "Part N of the call." for the Nth call. With a gate, each call first waits
/// for one message on it, then honours cancellation like Whisper does.
#[derive(Default)]
struct FakeTranscriber {
    calls: AtomicUsize,
    gate: Option<Mutex<Receiver<()>>>,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(
        &self,
        samples: &[f32],
        _options: &TranscribeOptions,
        cancel: &AtomicBool,
    ) -> SttResult<Transcript> {
        if let Some(gate) = &self.gate {
            let _ = lock(gate).recv_timeout(WAIT);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        let n = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(Transcript {
            text: format!(" Part {n} of the call."),
            language: Some("en".into()),
            audio_ms: samples.len() as u64 / 16,
            processing_ms: 1,
        })
    }
}

struct FakeTranscribers(Option<Arc<FakeTranscriber>>);

impl Transcribers for FakeTranscribers {
    fn has_model(&self, _settings: &Settings) -> bool {
        self.0.is_some()
    }

    fn load(&self, _settings: &Settings) -> Result<Arc<dyn Transcriber>, Problem> {
        self.0
            .clone()
            .map(|m| m as Arc<dyn Transcriber>)
            .ok_or(Problem::NoModel)
    }
}

/// Polishes to upper case, except that every second call fails.
struct FakePolisher {
    active: bool,
    calls: AtomicUsize,
}

impl Polisher for FakePolisher {
    fn active(&self, _settings: &Settings) -> bool {
        self.active
    }

    fn polish(&self, request: &PolishRequest) -> Option<String> {
        let n = self.calls.fetch_add(1, Ordering::Relaxed);
        n.is_multiple_of(2).then(|| request.text.to_uppercase())
    }
}

struct FakeSink {
    tx: Mutex<Sender<FileProgress>>,
    busy: AtomicBool,
    history_changed: AtomicUsize,
}

impl FileSink for FakeSink {
    fn progress(&self, progress: &FileProgress) {
        let _ = lock(&self.tx).send(progress.clone());
    }

    fn history_changed(&self) {
        self.history_changed.fetch_add(1, Ordering::Relaxed);
    }

    fn dictation_busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }
}

struct Harness {
    jobs: FileJobs,
    data: Arc<Data>,
    sink: Arc<FakeSink>,
    rx: Receiver<FileProgress>,
    model: Option<Arc<FakeTranscriber>>,
    dir: PathBuf,
}

impl Harness {
    fn new(model: Option<FakeTranscriber>, polish: bool) -> Self {
        Self::with(
            model.map(|m| Arc::new(m) as Arc<FakeTranscriber>),
            None,
            polish,
        )
    }

    fn with(
        model: Option<Arc<FakeTranscriber>>,
        transcribers: Option<Arc<dyn Transcribers>>,
        polish: bool,
    ) -> Self {
        let dir = temp_dir("files");
        let data = Arc::new(Data::load(AppPaths::new(&dir)));
        let (tx, rx) = channel();
        let sink = Arc::new(FakeSink {
            tx: Mutex::new(tx),
            busy: AtomicBool::new(false),
            history_changed: AtomicUsize::new(0),
        });
        let transcribers =
            transcribers.unwrap_or_else(|| Arc::new(FakeTranscribers(model.clone())));
        let jobs = FileJobs::new(JobDeps {
            data: Arc::clone(&data),
            transcribers,
            polisher: Arc::new(FakePolisher {
                active: polish,
                calls: AtomicUsize::new(0),
            }),
            sink: Arc::clone(&sink) as Arc<dyn FileSink>,
        });
        Self {
            jobs,
            data,
            sink,
            rx,
            model,
            dir,
        }
    }

    fn next(&self) -> FileProgress {
        self.rx.recv_timeout(WAIT).expect("a progress event")
    }

    /// Every event up to and including the one that ends the job.
    fn until_finished(&self) -> Vec<FileProgress> {
        let mut seen = Vec::new();
        loop {
            let progress = self.next();
            let end = matches!(
                progress.stage,
                FileStage::Done | FileStage::Failed | FileStage::Cancelled
            );
            seen.push(progress);
            if end {
                return seen;
            }
        }
    }

    fn calls(&self) -> usize {
        self.model
            .as_ref()
            .map_or(0, |m| m.calls.load(Ordering::Relaxed))
    }

    fn history(&self) -> Vec<HistoryEntry> {
        self.data.history.read(|h| h.list(None, None)).entries
    }

    fn file(&self, name: &str, samples: Vec<f32>) -> PathBuf {
        let path = self.dir.join(name);
        tw_audio::wav::write(
            &path,
            &tw_audio::Recording {
                samples,
                ..tw_audio::Recording::default()
            },
        )
        .unwrap();
        path
    }

    /// Waits for the job slot to free up after the last event.
    fn idle(&self) {
        let deadline = std::time::Instant::now() + WAIT;
        while lock(&self.jobs.running).is_some() {
            assert!(std::time::Instant::now() < deadline, "the job never ended");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// `sentences` loud tones of `seconds` each, a second of near silence after
/// each, so the splitter finds pauses between them.
fn talk(sentences: usize, seconds: usize) -> Vec<f32> {
    let quiet = |n: usize| (0..n).map(|i| if i % 2 == 0 { 0.001 } else { -0.001 });
    let mut samples = Vec::new();
    for _ in 0..sentences {
        samples.extend((0..seconds * 16_000).map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 / 16_000.0;
            (TAU * 220.0 * t).sin() * 0.4
        }));
        samples.extend(quiet(16_000));
    }
    samples
}

fn stages(events: &[FileProgress]) -> Vec<FileStage> {
    let mut stages: Vec<FileStage> = events.iter().map(|e| e.stage).collect();
    stages.dedup();
    stages
}

#[test]
fn a_long_file_is_transcribed_in_chunks_and_saved_to_history() {
    let h = Harness::new(Some(FakeTranscriber::default()), false);
    let path = h.file("Team call.wav", talk(4, 9));
    let id = h.jobs.start(path).unwrap();
    let events = h.until_finished();

    assert_eq!(
        stages(&events),
        [
            FileStage::Decoding,
            FileStage::Transcribing,
            FileStage::Done
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.job_id == id && e.file_name == "Team call.wav")
    );
    let fractions: Vec<f32> = events
        .iter()
        .filter(|e| e.stage == FileStage::Transcribing)
        .filter_map(|e| e.fraction)
        .collect();
    assert!(fractions.len() >= 3, "{fractions:?}");
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
    assert!(h.calls() >= 2, "one call per chunk, got {}", h.calls());

    let done = events.last().unwrap();
    let text = done.text.clone().unwrap();
    assert!(text.starts_with("Part 1 of the call."), "{text:?}");
    assert!(
        text.ends_with(&format!("Part {} of the call.", h.calls())),
        "{text:?}"
    );
    assert!(
        text.contains("\n\n"),
        "chunks cut at pauses are paragraphs: {text:?}"
    );
    assert_eq!(done.duration_ms, Some(40_000));
    assert_eq!(done.word_count, Some(tw_core::text::count_words(&text)));

    let history = h.history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].final_text, text);
    assert_eq!(
        history[0].app.as_ref().map(|a| a.name.as_str()),
        Some("File: Team call.wav")
    );
    assert_eq!(history[0].duration_ms, 40_000);
    assert_eq!(h.sink.history_changed.load(Ordering::Relaxed), 1);
    let stats = h.data.history.read(|s| s.stats(now_ms()));
    assert_eq!(stats.total_dictations, 1);
    h.idle();
    assert!(h.jobs.current().is_none());
}

#[test]
fn history_off_keeps_no_entry_but_counts_the_words() {
    let h = Harness::new(Some(FakeTranscriber::default()), false);
    h.data
        .settings
        .update(|s| s.history_retention = tw_core::history::HistoryRetention::Never)
        .unwrap();
    h.jobs.start(h.file("memo.wav", talk(1, 3))).unwrap();
    assert_eq!(h.until_finished().last().unwrap().stage, FileStage::Done);
    assert!(h.history().is_empty());
    assert_eq!(
        h.data.history.read(|s| s.stats(now_ms())).total_dictations,
        1
    );
}

#[test]
fn cancelling_mid_way_stops_without_saving() {
    let (gate, rx) = channel();
    let model = FakeTranscriber {
        gate: Some(Mutex::new(rx)),
        ..FakeTranscriber::default()
    };
    let h = Harness::new(Some(model), false);
    let id = h.jobs.start(h.file("call.wav", talk(4, 9))).unwrap();
    gate.send(()).unwrap();
    // Wait for the first chunk to finish, then cancel during the second.
    loop {
        let event = h.next();
        if event.stage == FileStage::Transcribing && event.fraction.unwrap_or(0.0) > 0.0 {
            break;
        }
    }
    assert!(h.jobs.current().is_some());
    assert!(!h.jobs.cancel("some other job"));
    assert!(h.jobs.cancel(&id));
    gate.send(()).unwrap();
    let events = h.until_finished();
    assert_eq!(events.last().unwrap().stage, FileStage::Cancelled);
    assert_eq!(h.calls(), 1);
    assert!(h.history().is_empty());
    h.idle();
    assert!(!h.jobs.cancel(&id));
}

#[test]
fn a_dictation_makes_the_job_wait_between_chunks() {
    let h = Harness::new(Some(FakeTranscriber::default()), false);
    h.sink.busy.store(true, Ordering::Relaxed);
    h.jobs.start(h.file("call.wav", talk(4, 9))).unwrap();
    assert_eq!(h.next().stage, FileStage::Decoding);
    assert_eq!(h.next().fraction, Some(0.0));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(h.calls(), 0, "the model is the dictation's while it runs");
    h.sink.busy.store(false, Ordering::Relaxed);
    assert_eq!(h.until_finished().last().unwrap().stage, FileStage::Done);
    assert!(h.calls() >= 2);
}

#[test]
fn without_a_voice_model_it_fails_before_decoding() {
    let h = Harness::new(None, false);
    h.jobs.start(h.file("memo.wav", talk(1, 3))).unwrap();
    let events = h.until_finished();
    assert_eq!(stages(&events), [FileStage::Failed]);
    let error = events[0].error.clone().unwrap();
    assert_eq!(error.kind, FileErrorKind::NoModel);
    assert_eq!(error.message, "Download a voice model first");
}

#[test]
fn one_job_at_a_time_and_only_supported_files() {
    let (gate, rx) = channel();
    let model = FakeTranscriber {
        gate: Some(Mutex::new(rx)),
        ..FakeTranscriber::default()
    };
    let h = Harness::new(Some(model), false);
    let first = h.file("one.wav", talk(1, 3));
    assert_eq!(
        h.jobs.start(h.dir.join("notes.txt")),
        Err(StartError::Unsupported)
    );
    h.jobs.start(first.clone()).unwrap();
    assert_eq!(h.jobs.start(first.clone()), Err(StartError::Busy));
    gate.send(()).unwrap();
    assert_eq!(h.until_finished().last().unwrap().stage, FileStage::Done);
    h.idle();
    h.jobs.start(first).unwrap();
    gate.send(()).unwrap();
    assert_eq!(h.until_finished().last().unwrap().stage, FileStage::Done);
}

#[test]
fn files_that_are_not_audio_or_silent_fail_with_a_reason() {
    let h = Harness::new(Some(FakeTranscriber::default()), false);
    let fake = h.dir.join("notes.mp3");
    fs::write(&fake, "not audio at all ".repeat(100)).unwrap();
    h.jobs.start(fake).unwrap();
    let failed = h.until_finished().pop().unwrap();
    assert_eq!(failed.error.map(|e| e.kind), Some(FileErrorKind::Corrupt));
    h.idle();

    h.jobs
        .start(h.file("silence.wav", vec![0.0; 16_000 * 5]))
        .unwrap();
    let failed = h.until_finished().pop().unwrap();
    assert_eq!(failed.error.map(|e| e.kind), Some(FileErrorKind::NoSpeech));
    assert_eq!(h.calls(), 0);
}

#[test]
fn polish_runs_per_paragraph_and_falls_back_to_the_rule_based_text() {
    let h = Harness::new(Some(FakeTranscriber::default()), true);
    h.jobs.start(h.file("call.wav", talk(4, 9))).unwrap();
    let events = h.until_finished();
    assert_eq!(
        stages(&events),
        [
            FileStage::Decoding,
            FileStage::Transcribing,
            FileStage::Polishing,
            FileStage::Done
        ]
    );
    let text = events.last().unwrap().text.clone().unwrap();
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    assert!(paragraphs.len() >= 2, "{text:?}");
    assert_eq!(paragraphs[0], "PART 1 OF THE CALL.", "polished");
    assert_eq!(
        paragraphs[1], "Part 2 of the call.",
        "polish failed: the rule-based text is kept"
    );
}

#[test]
fn transcript_file_names_end_in_txt() {
    assert_eq!(text_file_name("Team call.m4a"), "Team call.txt");
    assert_eq!(text_file_name("memo"), "memo.txt");
}

const SPEECH: &str = "Good morning everyone. Thanks for joining the weekly planning call. \
    First, the garden project is on schedule, and the new benches arrive on Tuesday. \
    Second, the library wants volunteers for the reading hour on Saturday mornings. \
    Please send me your name if you can help for an hour or two. \
    The reading hour starts at ten and runs until noon, and children of every age are welcome. \
    We will need people to choose books, to read aloud, and to tidy the room afterwards. \
    Last year more than forty families came, so we expect a busy morning again. \
    Third, the budget for the summer festival is almost final. \
    We still need quotes for the stage, the lights and the sound system. \
    Maria will collect those by the end of the month. \
    The festival committee meets again in two weeks to agree on the final plan and the opening times. \
    Finally, remember that the office is closed next Friday for maintenance. \
    If you need anything before then, write to the team inbox and someone will answer the same day. \
    That is everything for today. See you all at the lighthouse.";

/// A ~60 s spoken MP3, made with `say` and `ffmpeg`.
fn spoken_mp3(dir: &Path) -> Option<PathBuf> {
    let wav = dir.join("speech.wav");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/make-speech-fixture.sh");
    let made = Command::new(script)
        .args([SPEECH, wav.to_str()?, "Samantha"])
        .output()
        .ok()?;
    let mp3 = dir.join("planning call.mp3");
    let encoded = made.status.success()
        && Command::new("ffmpeg")
            .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
            .arg(&wav)
            .args(["-c:a", "libmp3lame", "-b:a", "64k"])
            .arg(&mp3)
            .status()
            .is_ok_and(|s| s.success());
    if !encoded {
        eprintln!("skipping: could not make the spoken MP3");
        return None;
    }
    Some(mp3)
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// `TINY_WISPR_TEST_MODELS` holds `whisper/ggml-tiny.en.bin`.
#[test]
fn real_whisper_model_transcribes_a_minute_long_mp3() {
    let Some(models) = std::env::var_os("TINY_WISPR_TEST_MODELS").map(PathBuf::from) else {
        eprintln!("skipping: TINY_WISPR_TEST_MODELS is not set");
        return;
    };
    if !models.join("whisper/ggml-tiny.en.bin").is_file() {
        eprintln!("skipping: ggml-tiny.en.bin is missing");
        return;
    }
    let whisper = Arc::new(WhisperModels::new(Inventory::new(models)));
    let h = Harness::with(
        None,
        Some(Arc::clone(&whisper) as Arc<dyn Transcribers>),
        false,
    );
    let Some(mp3) = spoken_mp3(&h.dir) else {
        return;
    };
    h.data
        .settings
        .update(|s| s.stt_model = Some("tiny.en".into()))
        .unwrap();
    h.jobs.start(mp3).unwrap();
    let done = h.until_finished().pop().unwrap();
    assert_eq!(done.stage, FileStage::Done, "{:?}", done.error);
    let text = done.text.unwrap();
    let duration = done.duration_ms.unwrap();
    eprintln!("{duration} ms of audio:\n{text}");
    assert!(duration > 45_000, "{duration} ms");

    let mut heard: HashMap<String, usize> = HashMap::new();
    for word in words(&text) {
        *heard.entry(word).or_default() += 1;
    }
    let expected = words(SPEECH);
    let found = expected
        .iter()
        .filter(|w| {
            heard.get_mut(*w).is_some_and(|n| {
                let present = *n > 0;
                *n = n.saturating_sub(1);
                present
            })
        })
        .count();
    let recall = found as f32 / expected.len() as f32;
    eprintln!("{found} of {} words ({recall:.3})", expected.len());
    assert!(recall >= 0.9, "recall {recall}");
    assert_eq!(words(&text).last().map(String::as_str), Some("lighthouse"));
    h.idle();
    drop(h);
    whisper.unload();
}
