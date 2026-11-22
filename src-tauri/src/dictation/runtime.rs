//! The runtime thread: owns the machine state, turns key events and commands
//! into machine events, and performs the effects that come back.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant as StdInstant};

use tw_core::dictation::machine::{
    Effect, Event, Instant, JobId, Outcome, Problem, State, TimerId, Transcript, step,
};
use tw_core::hotkey::{self, Hotkey, KeyTransition, Matcher, Platform};
use tw_core::{DictationState, HistoryEntry};
use tw_os::keys::KeyEvent;

use super::audio::{AudioThread, MicTestStart};
use super::{AudioSource, Desktop, Polisher, Sink, Transcribers, pipeline};
use crate::events::HotkeyCapture;
use crate::listener::Keys;
use crate::state::{Data, lock};
use crate::windows::now_ms;

/// Everything the runtime reacts to.
pub enum Msg {
    Event(Event),
    Key(KeyEvent),
    /// The audio of a finished or cancelled capture.
    Captured {
        job: JobId,
        recording: tw_audio::Recording,
    },
    /// The microphone went away mid-capture.
    CaptureLost,
    /// The pipeline finished `job`; `None` when it was cancelled.
    Processed {
        job: JobId,
        outcome: Option<Outcome>,
    },
    /// The shortcut recorder opened or closed.
    HotkeyCapture(bool),
    /// Start the microphone test and reply whether it opened, or stop it.
    MicTest(Option<Sender<tw_audio::Result<()>>>),
    /// Shortcuts may have changed: refresh what the listener swallows.
    SettingsChanged,
    Shutdown,
}

#[derive(Clone)]
pub struct Deps {
    pub data: Arc<Data>,
    pub transcribers: Arc<dyn Transcribers>,
    pub desktop: Arc<dyn Desktop>,
    pub polisher: Arc<dyn Polisher>,
    pub sink: Arc<dyn Sink>,
    pub keys: Arc<Keys>,
}

/// What to do with a capture's audio once the audio thread hands it over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Process,
    Keep,
    ProcessAs(JobId),
}

struct Running {
    cancel: Arc<AtomicBool>,
    audio: Arc<[f32]>,
}

/// Shared with the commands: what the pill shows and the last text inserted.
#[derive(Default)]
pub struct Shared {
    pub view: Mutex<DictationState>,
    pub last_text: Mutex<Option<String>>,
}

pub struct Runtime {
    state: State,
    deps: Deps,
    audio: AudioThread,
    tx: Sender<Msg>,
    shared: Arc<Shared>,
    timers: Vec<(Instant, TimerId)>,
    pending: HashMap<JobId, Pending>,
    kept: HashMap<JobId, Arc<[f32]>>,
    running: HashMap<JobId, Running>,
    capturing: Option<JobId>,
    /// Dropped when the capture ends; until then the voice model stays
    /// loaded, however long the user speaks.
    model_hold: Option<Sender<()>>,
    matcher: Matcher,
    recorder: Option<ShortcutRecorder>,
    swallowed: Option<Vec<Hotkey>>,
}

impl Runtime {
    fn new(deps: Deps, source: Box<dyn AudioSource>, tx: Sender<Msg>, shared: Arc<Shared>) -> Self {
        let mut runtime = Self {
            state: State::new(),
            audio: AudioThread::spawn(source, tx.clone()),
            deps,
            tx,
            shared,
            timers: Vec::new(),
            pending: HashMap::new(),
            kept: HashMap::new(),
            running: HashMap::new(),
            capturing: None,
            model_hold: None,
            matcher: Matcher::new(),
            recorder: None,
            swallowed: None,
        };
        runtime.sync_swallowed();
        runtime
    }

    pub fn handle(&mut self, msg: Msg, now: Instant) {
        match msg {
            Msg::Event(event) => self.dispatch(event, now),
            Msg::Key(event) => self.on_key(&event, now),
            Msg::Captured { job, recording } => self.on_captured(job, recording),
            Msg::CaptureLost => {
                if let Some(job) = self.capturing {
                    self.dispatch(Event::CaptureLost { job }, now);
                }
            }
            Msg::Processed { job, outcome } => {
                self.running.remove(&job);
                if let Some(outcome) = outcome {
                    self.dispatch(Event::Processed { job, outcome }, now);
                }
            }
            Msg::HotkeyCapture(on) => {
                self.recorder = on.then(ShortcutRecorder::default);
                self.deps.keys.set_capture(on);
                self.dispatch(Event::HotkeysSuspended(on), now);
            }
            Msg::MicTest(reply) => {
                self.audio.monitor(reply.map(|reply| MicTestStart {
                    device: self.deps.data.settings.get().microphone,
                    reply,
                }));
            }
            Msg::SettingsChanged => self.sync_swallowed(),
            Msg::Shutdown => {}
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.timers.iter().map(|(due, _)| *due).min()
    }

    pub fn fire_due(&mut self, now: Instant) {
        while let Some(index) = self
            .timers
            .iter()
            .enumerate()
            .filter(|(_, (due, _))| *due <= now)
            .min_by_key(|(_, (due, _))| *due)
            .map(|(index, _)| index)
        {
            let (_, id) = self.timers.remove(index);
            self.dispatch(Event::TimerFired(id), now);
        }
    }

    fn on_key(&mut self, event: &KeyEvent, now: Instant) {
        if let Some(recorder) = &mut self.recorder {
            if let Some(capture) = recorder.on_key(event) {
                self.deps.sink.hotkey_captured(capture);
            }
            return;
        }
        let shortcuts = self.deps.data.settings.get().shortcuts;
        for signal in self.matcher.handle(&event.clone().into(), &shortcuts) {
            self.dispatch(signal.into(), now);
        }
    }

    fn on_captured(&mut self, job: JobId, recording: tw_audio::Recording) {
        let audio: Arc<[f32]> = recording.samples.into();
        match self.pending.remove(&job) {
            Some(Pending::Process) => self.spawn_pipeline(job, audio),
            Some(Pending::ProcessAs(next)) => self.spawn_pipeline(next, audio),
            Some(Pending::Keep) => {
                self.kept.insert(job, audio);
            }
            None => {}
        }
    }

    fn dispatch(&mut self, event: Event, now: Instant) {
        let mut queue = VecDeque::from([event]);
        while let Some(event) = queue.pop_front() {
            tracing::debug!(?event, "dictation event");
            let (next, effects) = step(&self.state, event, now);
            self.state = next;
            for effect in effects {
                self.perform(effect, now, &mut queue);
            }
        }
        self.sync_swallowed();
    }

    fn perform(&mut self, effect: Effect, now: Instant, queue: &mut VecDeque<Event>) {
        match effect {
            Effect::StartCapture { job } => {
                let settings = self.deps.data.settings.get();
                if !self.deps.transcribers.has_model(&settings) {
                    queue.push_back(Event::CaptureFailed {
                        job,
                        problem: Problem::NoModel,
                    });
                    return;
                }
                self.capturing = Some(job);
                self.audio.start(job, settings.microphone.clone());
                // Load the model while the user speaks.
                let transcribers = Arc::clone(&self.deps.transcribers);
                let (hold, released) = channel();
                self.model_hold = Some(hold);
                std::thread::spawn(move || match transcribers.load(&settings) {
                    Ok(_model) => {
                        let _ = released.recv();
                    }
                    Err(problem) => tracing::warn!(?problem, "voice model could not be loaded"),
                });
            }
            Effect::FinishCapture { job } => {
                self.capturing = None;
                self.model_hold = None;
                self.pending.insert(job, Pending::Process);
                self.audio.finish(job);
            }
            Effect::CancelCapture { job, keep_for_undo } => {
                self.capturing = None;
                self.model_hold = None;
                if keep_for_undo {
                    self.pending.insert(job, Pending::Keep);
                }
                self.audio.cancel(job, keep_for_undo);
            }
            Effect::CancelProcessing { job } => {
                if let Some(running) = self.running.remove(&job) {
                    running.cancel.store(true, Ordering::Relaxed);
                    self.kept.insert(job, running.audio);
                } else if let Some(pending) = self.pending.get_mut(&job) {
                    *pending = Pending::Keep;
                }
            }
            Effect::ProcessKeptAudio { kept, job } => {
                if let Some(audio) = self.kept.remove(&kept) {
                    self.spawn_pipeline(job, audio);
                } else if let Some(pending) = self.pending.get_mut(&kept) {
                    *pending = Pending::ProcessAs(job);
                } else {
                    queue.push_back(Event::Processed {
                        job,
                        outcome: Outcome::NoSpeech,
                    });
                }
            }
            Effect::DiscardKeptAudio { job } => {
                self.kept.remove(&job);
                if self.pending.get(&job) == Some(&Pending::Keep) {
                    self.pending.remove(&job);
                }
            }
            Effect::Insert { job, text } => self.spawn_insert(job, text),
            Effect::RecordHistory {
                transcript,
                duration_ms,
            } => self.record_history(transcript, duration_ms),
            Effect::PlaySound(sound) => {
                if self.deps.data.settings.get().sounds {
                    self.deps.desktop.play(sound);
                }
            }
            Effect::StartTimer { id, after_ms } => {
                self.timers.retain(|(_, timer)| *timer != id);
                self.timers.push((now.saturating_add(after_ms), id));
            }
            Effect::CancelTimer(id) => self.timers.retain(|(_, timer)| *timer != id),
            Effect::OpenSettings => self.deps.sink.open_settings(),
            Effect::Emit(state) => {
                tracing::info!(
                    phase = ?state.phase,
                    mode = ?state.mode,
                    stage = ?state.stage,
                    notice = state.notice.as_ref().map(|n| n.text.as_str()),
                    "dictation state"
                );
                *lock(&self.shared.view) = state.clone();
                self.deps.sink.state(&state);
            }
        }
    }

    fn spawn_pipeline(&mut self, job: JobId, audio: Arc<[f32]>) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.running.insert(
            job,
            Running {
                cancel: Arc::clone(&cancel),
                audio: Arc::clone(&audio),
            },
        );
        let deps = self.deps.clone();
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("tw-pipeline".into())
            .spawn(move || {
                let polish_tx = tx.clone();
                let polish_started = move || {
                    let _ = polish_tx.send(Msg::Event(Event::PolishStarted { job }));
                };
                let outcome = pipeline::process(&deps, &audio, &cancel, &polish_started);
                let _ = tx.send(Msg::Processed { job, outcome });
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not start the pipeline");
            let _ = self.tx.send(Msg::Processed {
                job,
                outcome: Some(Outcome::Failed(Problem::Failed(err.to_string()))),
            });
        }
    }

    fn spawn_insert(&self, job: JobId, text: String) {
        let desktop = Arc::clone(&self.deps.desktop);
        let settings = self.deps.data.settings.get();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if let Err(err) = desktop.insert(&text, &settings) {
                tracing::warn!(%err, "insertion failed");
                let shortcut = settings.shortcuts.paste_last.display(Platform::current());
                let _ = tx.send(Msg::Event(Event::InsertFailed {
                    job,
                    paste_last_shortcut: shortcut,
                }));
            }
        });
    }

    fn record_history(&self, transcript: Transcript, duration_ms: u64) {
        *lock(&self.shared.last_text) = Some(transcript.text.clone());
        let entry = HistoryEntry {
            id: crate::store::new_id(),
            created_at: now_ms(),
            word_count: tw_core::text::count_words(&transcript.text),
            raw_text: transcript.raw,
            final_text: transcript.text,
            app: transcript.app,
            duration_ms,
            language: None,
        };
        match self.deps.data.add_history(entry, transcript.fixes) {
            Ok(()) => self.deps.sink.history_changed(),
            Err(err) => tracing::warn!(%err, "could not save history"),
        }
    }

    /// Escape is only swallowed while dictating, so the set follows the state.
    fn sync_swallowed(&mut self) {
        let shortcuts = self.deps.data.settings.get().shortcuts;
        let mut combos = hotkey::swallow_set(&shortcuts, self.state.is_dictating());
        combos.extend(
            shortcuts
                .iter()
                .filter(|(_, hotkey)| hotkey.key.is_none() && !hotkey.is_empty())
                .map(|(_, hotkey)| hotkey.clone()),
        );
        if self.swallowed.as_ref() != Some(&combos) {
            self.deps.keys.set_swallowed(combos.clone());
            self.swallowed = Some(combos);
        }
    }

    fn shutdown(&mut self) {
        for running in self.running.values() {
            running.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(job) = self.capturing.take() {
            self.audio.cancel(job, false);
        }
        self.model_hold = None;
    }
}

/// The shortcut recorder: reports every combination as it is pressed, and
/// the largest one once every key is up.
#[derive(Default)]
struct ShortcutRecorder {
    best: Option<Hotkey>,
}

impl ShortcutRecorder {
    fn on_key(&mut self, event: &KeyEvent) -> Option<HotkeyCapture> {
        if event.repeat {
            return None;
        }
        match event.transition {
            KeyTransition::Down => {
                let current = Hotkey::new(
                    event.pressed.modifiers.iter().copied(),
                    event.pressed.keys.iter().next().cloned(),
                );
                if self
                    .best
                    .as_ref()
                    .is_none_or(|best| current.key_count() >= best.key_count())
                {
                    self.best = Some(current.clone());
                }
                Some(HotkeyCapture {
                    hotkey: current,
                    done: false,
                })
            }
            KeyTransition::Up => {
                let all_up = event.pressed.modifiers.is_empty() && event.pressed.keys.is_empty();
                let best = if all_up { self.best.take() } else { None };
                best.map(|hotkey| HotkeyCapture { hotkey, done: true })
            }
        }
    }
}

/// The handle the app keeps: sends to the runtime thread and reads what it
/// shares.
pub struct Dictation {
    tx: Sender<Msg>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Dictation {
    /// `source` gets the runtime's sender, for reporting a lost microphone.
    pub fn spawn(deps: Deps, source: impl FnOnce(Sender<Msg>) -> Box<dyn AudioSource>) -> Self {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::default());
        let runtime = Runtime::new(deps, source(tx.clone()), tx.clone(), Arc::clone(&shared));
        let thread = std::thread::Builder::new()
            .name("tw-dictation".into())
            .spawn(move || run(runtime, &rx));
        let thread = match thread {
            Ok(thread) => Some(thread),
            Err(err) => {
                tracing::error!(%err, "could not start the dictation runtime");
                None
            }
        };
        Self {
            tx,
            shared,
            thread: Mutex::new(thread),
        }
    }

    pub fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_err() {
            tracing::warn!("the dictation runtime is not running");
        }
    }

    pub fn event(&self, event: Event) {
        self.send(Msg::Event(event));
    }

    /// For the key listener's callback, which runs inside the OS hook.
    pub fn sender(&self) -> Sender<Msg> {
        self.tx.clone()
    }

    pub fn view(&self) -> DictationState {
        lock(&self.shared.view).clone()
    }

    pub fn last_text(&self) -> Option<String> {
        lock(&self.shared.last_text).clone()
    }

    /// Stops the runtime and waits for it, so no pipeline holds a model.
    pub fn shutdown(&self) {
        self.send(Msg::Shutdown);
        if let Some(thread) = lock(&self.thread).take()
            && thread.join().is_err()
        {
            tracing::error!("the dictation runtime panicked");
        }
    }
}

fn run(mut runtime: Runtime, rx: &Receiver<Msg>) {
    let started = StdInstant::now();
    let now = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    loop {
        let received = match runtime.next_deadline() {
            Some(due) => match due.checked_sub(now()) {
                Some(wait) if wait > 0 => rx.recv_timeout(Duration::from_millis(wait)),
                _ => Err(RecvTimeoutError::Timeout),
            },
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(msg) => runtime.handle(msg, now()),
            Err(RecvTimeoutError::Timeout) => runtime.fire_due(now()),
        }
    }
    runtime.shutdown();
}

#[cfg(test)]
mod tests;
