use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tw_core::text::count_words;
use tw_core::text::polish::{
    MAX_INPUT_WORDS, PolishRequest, RejectReason, build_prompt, is_acceptable, sanitize_output,
    should_polish, split_for_polish,
};

use crate::client::{CancelHandle, LlmClient, LlmError};
use crate::protocol::{Completion, Done, ErrorKind, Message, Role};

/// Enough for the prompt, [`MAX_INPUT_WORDS`] of input and its output, while
/// keeping the context's memory small.
pub const DEFAULT_CONTEXT: u32 = 4096;
const TEMPERATURE: f32 = 0.0;
/// Editing repeats the input's words, which a repeat penalty fights.
const REPEAT_PENALTY: f32 = 1.05;

/// The language model to polish with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub path: PathBuf,
    pub context_length: u32,
    /// The catalog's reasoning flag: the model thinks before answering.
    pub reasoning: bool,
}

impl ModelSpec {
    pub fn new(path: impl Into<PathBuf>, reasoning: bool) -> Self {
        Self {
            path: path.into(),
            context_length: DEFAULT_CONTEXT,
            reasoning,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PolishError {
    /// Too short or already clean to be worth the latency.
    #[error("nothing to polish")]
    Skipped,
    #[error("no language model selected")]
    NoModel,
    #[error("the text is too long to polish")]
    TooLong,
    #[error("cancelled")]
    Cancelled,
    #[error("the model took too long")]
    Timeout,
    /// `output` is what the model returned, before sanitising.
    #[error("polish rejected: {reason}")]
    Rejected {
        reason: RejectReason,
        output: String,
    },
    #[error(transparent)]
    Llm(LlmError),
}

impl From<LlmError> for PolishError {
    fn from(err: LlmError) -> Self {
        match (err.kind(), err) {
            (Some(ErrorKind::TooLong), _) => Self::TooLong,
            (Some(ErrorKind::Cancelled), _) => Self::Cancelled,
            (_, LlmError::Timeout) => Self::Timeout,
            (_, err) => Self::Llm(err),
        }
    }
}

/// A polish with its measurements.
#[derive(Debug, Clone)]
pub struct Polished {
    pub text: String,
    /// Set when this call had to load the model first.
    pub load_ms: Option<u64>,
    /// The model's raw output and timings.
    pub done: Done,
}

struct State {
    client: LlmClient,
    model: Option<ModelSpec>,
    last_used: Instant,
}

/// Polishes dictated text with a local model in `tw-llm-worker`: one call
/// per dictation, safe to share between threads. Every failure means "keep
/// the rule-based text".
pub struct LlmPolisher {
    state: Arc<Mutex<State>>,
    cancel: CancelHandle,
    /// Set by [`Self::cancel`], so a cancel between two pieces of a long text
    /// isn't lost.
    cancelled: AtomicBool,
    /// Dropping it stops the idle-unload thread.
    _idle: Option<mpsc::Sender<()>>,
}

impl LlmPolisher {
    /// `idle_unload` ends the worker (freeing the model's memory) after that
    /// long without a polish; the next polish loads it again.
    pub fn new(worker_path: impl Into<PathBuf>, idle_unload: Option<Duration>) -> Self {
        let client = LlmClient::new(worker_path);
        let cancel = client.cancel_handle();
        let state = Arc::new(Mutex::new(State {
            client,
            model: None,
            last_used: Instant::now(),
        }));
        let idle = idle_unload.map(|after| spawn_idle_unloader(&state, after));
        Self {
            state,
            cancel,
            cancelled: AtomicBool::new(false),
            _idle: idle,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Selects the model; `None` turns polishing off. Doesn't load anything:
    /// the next [`Self::warm_up`] or [`Self::polish`] does.
    pub fn set_model(&self, model: Option<ModelSpec>) {
        let mut state = self.lock();
        if state.model != model {
            state.client.shutdown();
            state.model = model;
        }
    }

    pub fn model(&self) -> Option<ModelSpec> {
        self.lock().model.clone()
    }

    /// Loads the selected model now if it isn't, e.g. when recording starts,
    /// so the polish after it doesn't wait for the load. Returns the load
    /// time when it loaded.
    pub fn warm_up(&self) -> Result<Option<u64>, PolishError> {
        let mut state = self.lock();
        let model = state.model.clone().ok_or(PolishError::NoModel)?;
        state.last_used = Instant::now();
        ensure_loaded(&mut state.client, &model)
    }

    /// Cleans up `request.text`. Blocks for up to the model's run time, so
    /// call it off the main thread.
    pub fn polish(&self, request: &PolishRequest) -> Result<String, PolishError> {
        self.polish_timed(request).map(|polished| polished.text)
    }

    pub fn polish_timed(&self, request: &PolishRequest) -> Result<Polished, PolishError> {
        if !should_polish(&request.text, request.level) {
            return Err(PolishError::Skipped);
        }
        if count_words(&request.text) > MAX_INPUT_WORDS {
            return Err(PolishError::TooLong);
        }
        let mut state = self.lock();
        self.cancelled.store(false, Ordering::SeqCst);
        let model = state.model.clone().ok_or(PolishError::NoModel)?;
        let load_ms = ensure_loaded(&mut state.client, &model)?;
        let mut request = request.clone();
        request.reasoning_model |= model.reasoning;

        let mut parts = Vec::new();
        let mut raw = Vec::new();
        let mut rejected = None;
        let mut stats: Option<Done> = None;
        for chunk in split_for_polish(&request.text) {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(PolishError::Cancelled);
            }
            let piece = PolishRequest {
                text: chunk.to_owned(),
                ..request.clone()
            };
            let done = complete(&mut state.client, &piece);
            state.last_used = Instant::now();
            let done = done?;
            let text = sanitize_output(&done.text);
            match is_acceptable(chunk, &text, request.level) {
                Ok(()) => parts.push(text),
                Err(reason) => {
                    rejected.get_or_insert((reason, done.text.clone()));
                    parts.push(chunk.to_owned());
                }
            }
            raw.push(done.text.clone());
            stats = Some(match stats {
                None => done,
                Some(total) => Done {
                    prompt_tokens: total.prompt_tokens + done.prompt_tokens,
                    tokens: total.tokens + done.tokens,
                    total_ms: total.total_ms + done.total_ms,
                    ..total
                },
            });
        }
        drop(state);
        let Some(mut done) = stats else {
            return Err(PolishError::Skipped);
        };
        if let Some((reason, output)) = rejected.filter(|_| raw.len() == 1) {
            return Err(PolishError::Rejected { reason, output });
        }
        done.text = raw.join("\n\n");
        Ok(Polished {
            text: parts.join("\n\n"),
            load_ms,
            done,
        })
    }

    /// Ends the running polish promptly with [`PolishError::Cancelled`].
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.cancel.cancel();
    }

    /// The worker is running with the model in memory.
    pub fn is_loaded(&self) -> bool {
        self.lock().client.is_running()
    }

    /// Ends the worker now, freeing the model's memory.
    pub fn unload(&self) {
        self.lock().client.shutdown();
    }
}

/// One model call for one piece of text.
fn complete(client: &mut LlmClient, request: &PolishRequest) -> Result<Done, PolishError> {
    let prompt = build_prompt(request);
    let mut messages = vec![Message::new(Role::System, prompt.system)];
    for example in prompt.examples {
        messages.push(Message::new(Role::User, example.user));
        messages.push(Message::new(Role::Assistant, example.assistant));
    }
    messages.push(Message::new(Role::User, prompt.user));
    let completion = Completion {
        messages,
        max_tokens: prompt.max_tokens,
        temperature: TEMPERATURE,
        repeat_penalty: REPEAT_PENALTY,
        stop: prompt.stop,
        disable_thinking: true,
        ..Completion::default()
    };
    Ok(client.complete(completion, timeout_for(prompt.max_tokens), &mut |_| {})?)
}

/// Generous for a small model on a slow machine (~15 tokens/s), so it only
/// catches a stuck worker.
fn timeout_for(max_tokens: u32) -> Duration {
    Duration::from_secs(10) + Duration::from_millis(u64::from(max_tokens) * 70)
}

fn ensure_loaded(client: &mut LlmClient, model: &ModelSpec) -> Result<Option<u64>, PolishError> {
    let current = client.model_path() == Some(model.path.as_path())
        && client.context_length() == Some(model.context_length);
    if current && client.is_running() {
        return Ok(None);
    }
    let loaded = client.load(&model.path, model.context_length)?;
    Ok(Some(loaded.load_ms))
}

fn spawn_idle_unloader(state: &Arc<Mutex<State>>, after: Duration) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    let state = Arc::downgrade(state);
    let tick = (after / 4).clamp(Duration::from_millis(50), Duration::from_secs(5));
    let spawned = thread::Builder::new()
        .name("tw-llm-idle".into())
        .spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = rx.recv_timeout(tick) {
                let Some(state) = state.upgrade() else { return };
                // A busy lock means a polish is running: not idle.
                let Ok(mut state) = state.try_lock() else {
                    continue;
                };
                if state.last_used.elapsed() >= after && state.client.is_running() {
                    state.client.shutdown();
                }
            }
        });
    // Without the thread the model just stays loaded until the app quits.
    drop(spawned);
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tw_core::text::{CleanupLevel, StyleCategory, WritingStyle};

    fn request(text: &str) -> PolishRequest {
        PolishRequest {
            text: text.into(),
            level: CleanupLevel::Light,
            style: WritingStyle::Formal,
            category: StyleCategory::Other,
            vocabulary: vec![],
            reasoning_model: false,
        }
    }

    #[test]
    fn skips_before_touching_the_worker() {
        let polisher = LlmPolisher::new("/nonexistent/tw-llm-worker", None);
        assert!(matches!(
            polisher.polish(&request("ok")),
            Err(PolishError::Skipped)
        ));
        assert!(matches!(
            polisher.polish(&request(&"so like ".repeat(400))),
            Err(PolishError::TooLong)
        ));
        assert!(matches!(
            polisher.polish(&request("so like we should meet at two")),
            Err(PolishError::NoModel)
        ));
        polisher.set_model(Some(ModelSpec::new("/nonexistent/model.gguf", false)));
        assert!(matches!(
            polisher.polish(&request("so like we should meet at two")),
            Err(PolishError::Llm(LlmError::WorkerNotFound(_)))
        ));
    }

    #[test]
    fn worker_errors_map_to_fallback_reasons() {
        use crate::protocol::WorkerError;
        let too_long = LlmError::Worker(WorkerError::new(ErrorKind::TooLong, "x"));
        assert!(matches!(PolishError::from(too_long), PolishError::TooLong));
        let cancelled = LlmError::Worker(WorkerError::new(ErrorKind::Cancelled, "x"));
        assert!(matches!(
            PolishError::from(cancelled),
            PolishError::Cancelled
        ));
        assert!(matches!(
            PolishError::from(LlmError::Timeout),
            PolishError::Timeout
        ));
    }
}
