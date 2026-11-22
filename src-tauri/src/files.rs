//! Transcribing an audio file with the dictation pipeline: decode, split at
//! pauses, transcribe chunk by chunk, format and polish per paragraph, save
//! to history. One job at a time, on its own thread. A dictation always goes
//! first: while one is listening or processing, the job waits between chunks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tw_audio::DecodeError;
use tw_audio::vad;
use tw_core::dictation::machine::Problem;
use tw_core::text::pipeline::PipelineContext;
use tw_core::text::{AppCategory, AppInfo, StyleCategory};
use tw_core::{DictationPhase, HistoryEntry};
use tw_stt::SttError;

use crate::dictation::{Polisher, Transcribers, pipeline};
use crate::events;
use crate::state::{AppState, Data, lock};
use crate::windows::now_ms;

/// `FileProgress`
pub const FILES_PROGRESS: &str = "files:progress";

/// How often a waiting job checks whether the dictation has finished.
const WAIT_STEP: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStage {
    Decoding,
    Transcribing,
    Polishing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileErrorKind {
    NoModel,
    NoAudioTrack,
    NoSpeech,
    Unsupported,
    Corrupt,
    Empty,
    TooLong,
    Unreadable,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileError {
    pub kind: FileErrorKind,
    pub message: String,
}

impl FileError {
    fn new(kind: FileErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn no_speech() -> Self {
        Self::new(FileErrorKind::NoSpeech, "No speech found in this file")
    }
}

/// Payload of `files:progress`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
    pub job_id: String,
    pub file_name: String,
    pub stage: FileStage,
    /// 0..1 while transcribing or polishing.
    pub fraction: Option<f32>,
    /// The transcript, when done.
    pub text: Option<String>,
    pub word_count: Option<u32>,
    /// Length of the audio, once decoded.
    pub duration_ms: Option<u64>,
    pub error: Option<FileError>,
}

/// Where a job reports to: the windows, in the app.
pub trait FileSink: Send + Sync {
    fn progress(&self, progress: &FileProgress);
    fn history_changed(&self);
    /// A dictation is listening or processing, so the job should wait.
    fn dictation_busy(&self) -> bool;
}

pub struct AppFileSink {
    pub app: AppHandle,
}

impl FileSink for AppFileSink {
    fn progress(&self, progress: &FileProgress) {
        events::emit(&self.app, FILES_PROGRESS, progress);
    }

    fn history_changed(&self) {
        events::emit(&self.app, events::HISTORY_CHANGED, ());
    }

    fn dictation_busy(&self) -> bool {
        self.app.try_state::<AppState>().is_some_and(|state| {
            matches!(
                state.dictation.view().phase,
                DictationPhase::Listening | DictationPhase::Processing
            )
        })
    }
}

#[derive(Clone)]
pub struct JobDeps {
    pub data: Arc<Data>,
    pub transcribers: Arc<dyn Transcribers>,
    pub polisher: Arc<dyn Polisher>,
    pub sink: Arc<dyn FileSink>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    #[error("A file is already being transcribed")]
    Busy,
    #[error(
        "Tiny Wispr can transcribe WAV, MP3, M4A, AAC, FLAC and Ogg files, and the sound of MP4 and MOV videos"
    )]
    Unsupported,
}

struct Running {
    id: String,
    cancel: Arc<AtomicBool>,
    last: Arc<Mutex<Option<FileProgress>>>,
}

/// The file job, if one runs. Managed as Tauri state.
pub struct FileJobs {
    deps: JobDeps,
    running: Arc<Mutex<Option<Running>>>,
}

impl FileJobs {
    pub fn new(deps: JobDeps) -> Self {
        Self {
            deps,
            running: Arc::new(Mutex::new(None)),
        }
    }

    /// Starts transcribing `path` and returns the job id.
    pub fn start(&self, path: PathBuf) -> Result<String, StartError> {
        if !tw_audio::decode::is_supported(&path) {
            return Err(StartError::Unsupported);
        }
        let mut running = lock(&self.running);
        if running.is_some() {
            return Err(StartError::Busy);
        }
        let id = crate::store::new_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let last = Arc::new(Mutex::new(None));
        let job = Job {
            id: id.clone(),
            file_name: path
                .file_name()
                .map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned()),
            path,
            cancel: Arc::clone(&cancel),
            last: Arc::clone(&last),
            deps: self.deps.clone(),
        };
        let slot = Arc::clone(&self.running);
        let finished_id = id.clone();
        std::thread::Builder::new()
            .name("tw-file-job".into())
            .spawn(move || {
                job.run();
                let mut running = lock(&slot);
                if running.as_ref().is_some_and(|r| r.id == finished_id) {
                    *running = None;
                }
            })
            .map_err(|err| {
                tracing::error!(%err, "could not start the file job");
                StartError::Busy
            })?;
        *running = Some(Running {
            id: id.clone(),
            cancel,
            last,
        });
        Ok(id)
    }

    /// Cancels the job `id`; false when it is not running.
    pub fn cancel(&self, id: &str) -> bool {
        match lock(&self.running).as_ref() {
            Some(running) if running.id == id => {
                running.cancel.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    /// Cancels whatever runs, so it lets go of the voice model before exit.
    pub fn cancel_all(&self) {
        if let Some(running) = lock(&self.running).as_ref() {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The running job's latest progress, for a window opened mid-job.
    pub fn current(&self) -> Option<FileProgress> {
        lock(&self.running)
            .as_ref()
            .and_then(|running| lock(&running.last).clone())
    }
}

struct Job {
    id: String,
    path: PathBuf,
    file_name: String,
    cancel: Arc<AtomicBool>,
    last: Arc<Mutex<Option<FileProgress>>>,
    deps: JobDeps,
}

/// Why a job stopped early.
enum Stop {
    Cancelled,
    Failed(FileError),
}

impl From<FileError> for Stop {
    fn from(err: FileError) -> Self {
        Self::Failed(err)
    }
}

impl Job {
    fn run(&self) {
        match self.transcribe() {
            Ok(()) => {}
            Err(Stop::Cancelled) => self.report(FileStage::Cancelled, |_| {}),
            Err(Stop::Failed(error)) => {
                tracing::info!(kind = ?error.kind, message = %error.message, "file transcription failed");
                self.report(FileStage::Failed, |p| p.error = Some(error));
            }
        }
    }

    fn transcribe(&self) -> Result<(), Stop> {
        let settings = self.deps.data.settings.get();
        if !self.deps.transcribers.has_model(&settings) {
            return Err(no_model().into());
        }
        self.report(FileStage::Decoding, |_| {});
        let recording = tw_audio::decode_file(&self.path, &self.cancel).map_err(decode_error)?;
        let duration_ms = recording.duration_ms();
        let samples = recording.samples;
        let chunks = vad::split_at_pauses(&samples);
        if !chunks.iter().any(|c| c.has_speech) {
            return Err(FileError::no_speech().into());
        }
        self.report(FileStage::Transcribing, |p| {
            p.fraction = Some(0.0);
            p.duration_ms = Some(duration_ms);
        });

        let engine = self
            .deps
            .transcribers
            .load(&settings)
            .map_err(|problem| match problem {
                Problem::NoModel => no_model(),
                Problem::Failed(message) => FileError::new(
                    FileErrorKind::Failed,
                    format!("Couldn't load the voice model: {message}"),
                ),
                Problem::NoMicrophone | Problem::MicrophoneDenied => {
                    FileError::new(FileErrorKind::Failed, "Couldn't load the voice model")
                }
            })?;
        let dictionary = self.deps.data.dictionary.get();
        let snippets = self.deps.data.snippets.get();
        let vocabulary: Vec<String> = dictionary.iter().map(|e| e.word.clone()).collect();
        let options = pipeline::transcribe_options(&settings, &vocabulary);
        let mut paragraphs = Vec::new();
        let mut paragraph = String::new();
        for chunk in &chunks {
            self.wait_for_dictation()?;
            if chunk.has_speech {
                let audio = &samples[chunk.range.clone()];
                let text = match engine.transcribe(audio, &options, &self.cancel) {
                    Ok(transcript) => tw_stt::clean_transcript(&transcript.text, true),
                    Err(SttError::Cancelled) => return Err(Stop::Cancelled),
                    Err(err) => {
                        return Err(FileError::new(
                            FileErrorKind::Failed,
                            format!("Transcription failed: {err}"),
                        )
                        .into());
                    }
                };
                if !text.is_empty() {
                    if !paragraph.is_empty() {
                        paragraph.push(' ');
                    }
                    paragraph.push_str(&text);
                }
            }
            // A cut in the middle of speech joins the next chunk's text to
            // this paragraph.
            if chunk.ends_at_pause && !paragraph.is_empty() {
                paragraphs.push(std::mem::take(&mut paragraph));
            }
            let fraction = chunk.range.end as f32 / samples.len().max(1) as f32;
            self.report(FileStage::Transcribing, |p| {
                p.fraction = Some(fraction);
                p.duration_ms = Some(duration_ms);
            });
        }
        drop(engine);
        drop(samples);
        if paragraphs.is_empty() {
            return Err(FileError::no_speech().into());
        }

        let category = StyleCategory::Other;
        let ctx = PipelineContext {
            level: settings.cleanup_level,
            style: settings.styles.get(category),
            category,
            dictionary: &dictionary,
            snippets: &snippets,
        };
        let mut texts = Vec::with_capacity(paragraphs.len());
        let mut fixes = 0;
        let count = paragraphs.len();
        for (i, raw) in paragraphs.iter().enumerate() {
            self.wait_for_dictation()?;
            let polish_started = || {
                self.report(FileStage::Polishing, |p| {
                    p.fraction = Some(i as f32 / count as f32);
                    p.duration_ms = Some(duration_ms);
                });
            };
            let (text, paragraph_fixes) = pipeline::format(
                self.deps.polisher.as_ref(),
                &settings,
                &ctx,
                raw,
                vocabulary.clone(),
                &polish_started,
            );
            texts.push(text);
            fixes += paragraph_fixes;
        }
        self.check_cancel()?;

        let text = texts.join("\n\n");
        let word_count = tw_core::text::count_words(&text);
        self.save(paragraphs.join("\n\n"), text.clone(), duration_ms, fixes);
        self.report(FileStage::Done, |p| {
            p.fraction = Some(1.0);
            p.text = Some(text);
            p.word_count = Some(word_count);
            p.duration_ms = Some(duration_ms);
        });
        Ok(())
    }

    fn save(&self, raw_text: String, final_text: String, duration_ms: u64, fixes: u32) {
        let entry = HistoryEntry {
            id: crate::store::new_id(),
            created_at: now_ms(),
            word_count: tw_core::text::count_words(&final_text),
            raw_text,
            final_text,
            app: Some(AppInfo {
                name: format!("File: {}", self.file_name),
                id: None,
                category: AppCategory::Other,
            }),
            duration_ms,
            language: None,
        };
        match self.deps.data.add_history(entry, fixes) {
            Ok(()) => self.deps.sink.history_changed(),
            Err(err) => tracing::warn!(%err, "could not save the file transcript to history"),
        }
    }

    /// Dictation has priority over the voice and language models.
    fn wait_for_dictation(&self) -> Result<(), Stop> {
        loop {
            self.check_cancel()?;
            if !self.deps.sink.dictation_busy() {
                return Ok(());
            }
            std::thread::sleep(WAIT_STEP);
        }
    }

    fn check_cancel(&self) -> Result<(), Stop> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }

    fn report(&self, stage: FileStage, fill: impl FnOnce(&mut FileProgress)) {
        let mut progress = FileProgress {
            job_id: self.id.clone(),
            file_name: self.file_name.clone(),
            stage,
            fraction: None,
            text: None,
            word_count: None,
            duration_ms: None,
            error: None,
        };
        fill(&mut progress);
        *lock(&self.last) = Some(progress.clone());
        self.deps.sink.progress(&progress);
    }
}

fn no_model() -> FileError {
    FileError::new(FileErrorKind::NoModel, "Download a voice model first")
}

fn decode_error(err: DecodeError) -> Stop {
    let kind = match err {
        DecodeError::Cancelled => return Stop::Cancelled,
        DecodeError::Unsupported => FileErrorKind::Unsupported,
        DecodeError::NoAudioTrack => FileErrorKind::NoAudioTrack,
        DecodeError::Corrupt => FileErrorKind::Corrupt,
        DecodeError::Empty => FileErrorKind::Empty,
        DecodeError::TooLong => FileErrorKind::TooLong,
        DecodeError::Io(_) => FileErrorKind::Unreadable,
    };
    Stop::Failed(FileError::new(kind, err.to_string()))
}

/// The name the save dialog suggests for a transcript of `file_name`.
pub fn text_file_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map_or_else(|| "Transcript".into(), |s| s.to_string_lossy().into_owned());
    format!("{stem}.txt")
}

#[cfg(test)]
mod tests;
