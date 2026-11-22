//! Speech to text with whisper.cpp (`whisper-rs`; Metal on macOS).
//!
//! [`WhisperEngine::load`] and [`Transcriber::transcribe`] block for as long
//! as whisper.cpp works (loading: up to seconds for large models;
//! transcribing: a fraction of the audio length), so call them from a worker
//! thread, never the UI thread. A loaded engine is `Send + Sync`: share it
//! with `Arc`. Calls are serialised internally, since one engine keeps one
//! set of whisper.cpp buffers and reuses it for every call.
//!
//! Drop every engine before the process exits (not in a `static`, not leaked):
//! ggml's Metal backend asserts at exit that all its buffers were freed and
//! aborts the process otherwise.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

mod clean;

pub use clean::clean_transcript;

/// Whisper's input rate; samples must be mono at this rate.
pub const SAMPLE_RATE: u32 = tw_audio::SAMPLE_RATE;
/// whisper.cpp returns nothing for input shorter than a second, so shorter
/// audio is padded with silence up to this length.
const MIN_INPUT_MS: u32 = 1_250;
/// whisper.cpp stops decoding once it predicts a timestamp within 100 ms of
/// the end of the input, which drops a last word spoken right before the key
/// was released. Silence after it keeps that timestamp away from the end.
const TRAILING_SILENCE_MS: u32 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),
    #[error("could not load the model: {0}")]
    Load(String),
    #[error("transcription failed: {0}")]
    Transcribe(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T, E = SttError> = std::result::Result<T, E>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscribeOptions {
    /// ISO 639-1 code to force, or `None` to detect.
    pub language: Option<String>,
    /// When detecting, pick only among these ISO 639-1 codes (the user's
    /// chosen languages). Empty means any language.
    pub detect_among: Vec<String>,
    /// Text Whisper takes as what came before the audio, which biases it
    /// towards its spellings. Build it from dictionary words with
    /// [`vocabulary_prompt`].
    pub initial_prompt: Option<String>,
    /// Translate to English instead of transcribing.
    pub translate: bool,
    /// CPU threads; `None` picks a sensible number for this machine.
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
    /// Detected or forced language.
    pub language: Option<String>,
    /// Length of the audio that was passed in.
    pub audio_ms: u64,
    /// Wall-clock time spent transcribing.
    pub processing_ms: u64,
}

/// Anything that turns 16 kHz mono samples into text. Lets the app pipeline be
/// tested without a model.
pub trait Transcriber: Send + Sync {
    fn transcribe(
        &self,
        samples: &[f32],
        options: &TranscribeOptions,
        cancel: &AtomicBool,
    ) -> Result<Transcript>;
}

/// A loaded whisper.cpp model.
pub struct WhisperEngine {
    model_path: PathBuf,
    context: WhisperContext,
    state: Mutex<WhisperState>,
}

impl WhisperEngine {
    pub fn load(model_path: &Path) -> Result<Self> {
        if !model_path.is_file() {
            return Err(SttError::ModelNotFound(model_path.to_path_buf()));
        }
        whisper_rs::install_logging_hooks();
        let started = Instant::now();
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .map_err(|err| SttError::Load(err.to_string()))?;
        let state = context
            .create_state()
            .map_err(|err| SttError::Load(err.to_string()))?;
        tracing::info!(
            model = %model_path.display(),
            multilingual = context.is_multilingual(),
            ms = started.elapsed().as_millis(),
            "whisper model loaded"
        );
        Ok(Self {
            model_path: model_path.to_path_buf(),
            context,
            state: Mutex::new(state),
        })
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Whether the model knows languages other than English.
    pub fn is_multilingual(&self) -> bool {
        self.context.is_multilingual()
    }

    fn choose_language(
        &self,
        state: &mut WhisperState,
        samples: &[f32],
        options: &TranscribeOptions,
        threads: usize,
    ) -> Result<Option<String>> {
        if !self.is_multilingual() {
            return Ok(Some("en".to_owned()));
        }
        if let Some(code) = &options.language {
            return known_language(code).map(Some);
        }
        let candidates = options
            .detect_among
            .iter()
            .map(|code| known_language(code))
            .collect::<Result<Vec<_>>>()?;
        match candidates.as_slice() {
            [] => Ok(None),
            [only] => Ok(Some(only.clone())),
            _ => {
                state
                    .pcm_to_mel(samples, threads)
                    .map_err(|err| SttError::Transcribe(err.to_string()))?;
                let (_, probabilities) = state
                    .lang_detect(0, threads)
                    .map_err(|err| SttError::Transcribe(err.to_string()))?;
                Ok(most_likely(&candidates, |code| {
                    whisper_rs::get_lang_id(code)
                        .and_then(|id| probabilities.get(usize::try_from(id).ok()?).copied())
                }))
            }
        }
    }
}

impl Transcriber for WhisperEngine {
    fn transcribe(
        &self,
        samples: &[f32],
        options: &TranscribeOptions,
        cancel: &AtomicBool,
    ) -> Result<Transcript> {
        let started = Instant::now();
        let audio_ms = samples.len() as u64 * 1_000 / u64::from(SAMPLE_RATE);
        let finish = |text: String, language: Option<String>| Transcript {
            text,
            language,
            audio_ms,
            processing_ms: started.elapsed().as_millis() as u64,
        };
        if samples.iter().all(|&s| s == 0.0) {
            return Ok(finish(String::new(), None));
        }

        let padded = pad(samples);
        let threads = options
            .threads
            .map_or_else(default_threads, |t| t.max(1) as usize);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // Checked after the lock because waiting for another call can take
        // as long as that call's whole transcription.
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        let language = self.choose_language(&mut state, &padded, options, threads)?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads as i32);
        params.set_language(Some(language.as_deref().unwrap_or("auto")));
        params.set_translate(options.translate && self.is_multilingual());
        params.set_no_context(true);
        params.set_no_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let prompt = options
            .initial_prompt
            .as_deref()
            .map(|p| p.replace('\0', ""))
            .filter(|p| !p.trim().is_empty());
        if let Some(prompt) = &prompt {
            params.set_initial_prompt(prompt);
        }
        // SAFETY: whisper.cpp only reads the flag through this pointer while
        // `full` below runs, and `cancel` outlives that call.
        unsafe {
            params.set_abort_callback(Some(abort_requested));
            params.set_abort_callback_user_data(std::ptr::from_ref(cancel).cast_mut().cast());
        }

        let result = state.full(params, &padded);
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        result.map_err(|err| SttError::Transcribe(err.to_string()))?;

        let mut raw = String::new();
        for segment in state.as_iter() {
            let text = segment
                .to_str_lossy()
                .map_err(|err| SttError::Transcribe(err.to_string()))?;
            raw.push_str(&text);
        }
        let detected = language.or_else(|| {
            whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_owned)
        });
        let text = clean_transcript(&raw, tw_audio::vad::has_speech(samples));
        tracing::debug!(audio_ms, ms = started.elapsed().as_millis(), "transcribed");
        Ok(finish(text, detected))
    }
}

unsafe extern "C" fn abort_requested(user_data: *mut c_void) -> bool {
    // SAFETY: `user_data` is the `&AtomicBool` set in `transcribe`, alive for
    // the whole whisper.cpp call that invokes this.
    let cancel = unsafe { &*user_data.cast::<AtomicBool>() };
    cancel.load(Ordering::Relaxed)
}

/// An initial prompt that teaches Whisper the spelling of `words` (names,
/// jargon). A bare list such as "Amelia Lindqvist, Tiny Wispr" makes small models
/// treat speech that repeats it as already transcribed and drop it; framed as
/// a labelled sentence it only changes the spelling.
pub fn vocabulary_prompt<S: AsRef<str>>(words: &[S]) -> Option<String> {
    let words: Vec<&str> = words
        .iter()
        .map(|word| word.as_ref().trim())
        .filter(|word| !word.is_empty())
        .collect();
    (!words.is_empty()).then(|| format!("Vocabulary: {}.", words.join(", ")))
}

fn known_language(code: &str) -> Result<String> {
    let code = code.trim().to_lowercase();
    if whisper_rs::get_lang_id(&code).is_some() {
        Ok(code)
    } else {
        Err(SttError::Transcribe(format!("unknown language: {code}")))
    }
}

/// The candidate with the highest probability.
fn most_likely(candidates: &[String], probability: impl Fn(&str) -> Option<f32>) -> Option<String> {
    candidates
        .iter()
        .filter_map(|code| Some((code, probability(code)?)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(code, _)| code.clone())
}

fn pad(samples: &[f32]) -> Vec<f32> {
    let len = (samples.len() + ms_to_samples(TRAILING_SILENCE_MS)).max(ms_to_samples(MIN_INPUT_MS));
    let mut padded = samples.to_vec();
    padded.resize(len, 0.0);
    padded
}

fn ms_to_samples(ms: u32) -> usize {
    (SAMPLE_RATE * ms / 1_000) as usize
}

/// Half the logical cores (roughly the performance or physical cores), at
/// most 8: more threads than that slow ggml down on hybrid CPUs.
fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, |n| n.get() / 2)
        .clamp(1, 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_is_reported() {
        let err = WhisperEngine::load(Path::new("/nonexistent/ggml-base.bin"))
            .err()
            .unwrap();
        assert!(matches!(err, SttError::ModelNotFound(_)));
    }

    #[test]
    fn a_file_that_is_not_a_model_fails_to_load() {
        let path = std::env::temp_dir().join(format!("tw-stt-{}-bad.bin", std::process::id()));
        std::fs::write(&path, b"not a ggml model").unwrap();
        let err = WhisperEngine::load(&path).err().unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(err, SttError::Load(_)));
    }

    #[test]
    fn audio_is_padded_to_the_minimum_and_after_the_last_word() {
        assert_eq!(pad(&[0.5; 10]).len(), 20_000);
        assert_eq!(pad(&[0.5; 10])[..10], [0.5; 10]);
        let long = pad(&[0.5; 30_000]);
        assert_eq!(long.len(), 46_000);
        assert!(long[30_000..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn vocabulary_becomes_a_labelled_sentence() {
        assert_eq!(
            vocabulary_prompt(&["Amelia Lindqvist", " Tiny Wispr ", ""]).as_deref(),
            Some("Vocabulary: Amelia Lindqvist, Tiny Wispr.")
        );
        assert_eq!(vocabulary_prompt::<&str>(&[]), None);
        assert_eq!(vocabulary_prompt(&[" "]), None);
    }

    #[test]
    fn language_codes_are_checked() {
        assert_eq!(known_language(" DE ").unwrap(), "de");
        assert!(known_language("xx").is_err());
    }

    #[test]
    fn detection_picks_the_likeliest_allowed_language() {
        let candidates = vec!["en".to_owned(), "de".to_owned()];
        let probability = |code: &str| match code {
            "en" => Some(0.2),
            "de" => Some(0.7),
            _ => None,
        };
        assert_eq!(most_likely(&candidates, probability).as_deref(), Some("de"));
        assert_eq!(most_likely(&[], probability), None);
    }

    #[test]
    fn thread_count_is_sane() {
        assert!((1..=8).contains(&default_threads()));
    }
}
