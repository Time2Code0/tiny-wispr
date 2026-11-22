//! The dictation reducer: `step(&State, Event, now) -> (State, Vec<Effect>)`.
//!
//! Pure and clock-free: the caller passes the time each event happened, and
//! timers are requested through [`Effect::StartTimer`] and come back as
//! [`Event::TimerFired`]. The app (`src-tauri`) performs the effects and feeds
//! back what happened. Every capture, processing job and notice carries an id,
//! so late results and stale timers are recognised and ignored.
//!
//! ```text
//!               push-to-talk down          ┌──────────────── Listening ─────────────────┐
//!  ┌────────┐ ───────────────────────────► │ Held ──up < 0.5 s──► Tapped                │
//!  │  Idle  │  hands-free key, pill click  │  │                     │ down within 0.5 s │
//!  │   ±    │ ───────────────────────────► │  └──hands-free key──► HandsFree ◄──┘       │
//!  │ notice │                              └────────────────────────────────────────────┘
//!  └────────┘ ◄── silently: tap times out,     │ Esc / ✕       │ up ≥ 0.5 s, stop button,
//!    ▲    ▲       foreign key < 1 s, press     │ (keeps audio) │ ptt or hands-free press,
//!    │    │       < 0.5 s after hands-free     ▼               │ 20 min, mic lost
//!    │    └──── "Dictation cancelled · Undo" ◄─┤               ▼
//!    │                                         │  ┌─────────────────────────────────┐
//!    │                         Esc / ✕ ────────┘◄─│ Processing                      │
//!    └──────── inserted, or a notice ─────────────│ transcribing → polishing        │
//!                     Undo ──────────────────────►└─────────────────────────────────┘
//! ```
//!
//! Notices sit on top of any phase: in Idle they are the whole picture, while
//! listening or processing they show above the pill. A new dictation always
//! replaces the current notice.

use crate::hotkey::{HotkeyAction, HotkeySignal};
use crate::text::AppInfo;

use super::{
    DOUBLE_TAP_MS, DictationMode, DictationPhase, DictationState, FOREIGN_SHORTCUT_WINDOW_MS,
    MIN_HOLD_MS, Notice, NoticeAction, NoticeActionKind, NoticeKind, ProcessingStage,
    SESSION_LIMIT_MS, SESSION_WARN_MS,
};

/// Milliseconds on a monotonic clock owned by the caller.
pub type Instant = u64;

/// Identifies one capture and its processing, or one processing of kept
/// audio after Undo, or one paste of the last transcript.
pub type JobId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimerId {
    /// The double-tap window after a short tap closes.
    SecondTap(JobId),
    /// [`SESSION_WARN_MS`] after the session started.
    SessionWarning(JobId),
    /// [`SESSION_LIMIT_MS`] after the session started.
    SessionLimit(JobId),
    /// Hides the notice with this id.
    NoticeDismiss(u64),
}

/// Why a dictation could not go ahead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    NoModel,
    NoMicrophone,
    MicrophoneDenied,
    Failed(String),
}

/// What the text pipeline produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// Whisper's output, before any formatting or polish.
    pub raw: String,
    /// The text to insert.
    pub text: String,
    /// The app that had focus when the dictation was processed.
    pub app: Option<AppInfo>,
    /// Corrections the text pipeline made, counted in the stats.
    pub fixes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Transcript(Transcript),
    NoSpeech,
    Failed(Problem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// From [`crate::hotkey::Matcher`].
    HotkeyPressed(HotkeyAction),
    HotkeyReleased(HotkeyAction),
    ForeignKey,
    /// The shortcut recorder is open: hotkey presses are ignored meanwhile.
    HotkeysSuspended(bool),
    /// A click on the pill or `start_dictation`. Always hands-free: there is
    /// no key whose release would end it.
    StartRequested,
    /// The pill's stop button or `stop_dictation`.
    StopRequested,
    /// The pill's ✕ or `cancel_dictation`.
    CancelRequested,
    /// The notice's Undo or `undo_cancel`.
    UndoRequested,
    /// `paste_last_transcript` or the notice's Paste.
    PasteLastRequested,
    /// The notice's "Open settings".
    OpenSettingsRequested,
    TimerFired(TimerId),
    /// The recorder is delivering audio.
    CaptureStarted {
        job: JobId,
    },
    /// The recorder could not start (or no voice model is installed).
    CaptureFailed {
        job: JobId,
        problem: Problem,
    },
    /// The microphone went away mid-way. What was captured is transcribed.
    CaptureLost {
        job: JobId,
    },
    /// The language model started polishing the transcript.
    PolishStarted {
        job: JobId,
    },
    /// The pipeline finished: transcription, formatting, polish, dictionary,
    /// snippets and style.
    Processed {
        job: JobId,
        outcome: Outcome,
    },
    /// Inserting the text of this job failed. `paste_last_shortcut` is how
    /// the paste-last shortcut reads on this platform, such as `⌃⌘V`.
    InsertFailed {
        job: JobId,
        paste_last_shortcut: String,
    },
}

impl From<HotkeySignal> for Event {
    fn from(signal: HotkeySignal) -> Self {
        match signal {
            HotkeySignal::Pressed(action) => Self::HotkeyPressed(action),
            HotkeySignal::Released(action) => Self::HotkeyReleased(action),
            HotkeySignal::ForeignKey => Self::ForeignKey,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Start,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Open the microphone (or take over the pre-warmed stream) for this job.
    /// Report back with `CaptureStarted`, `CaptureFailed` or `CaptureLost`.
    StartCapture {
        job: JobId,
    },
    /// Stop capturing and run the audio through the pipeline. Report back
    /// with `PolishStarted` and `Processed`.
    FinishCapture {
        job: JobId,
    },
    /// Stop capturing. Keep the audio under this job id for Undo when asked,
    /// otherwise drop it.
    CancelCapture {
        job: JobId,
        keep_for_undo: bool,
    },
    /// Abort the pipeline for this job and keep its audio for Undo.
    CancelProcessing {
        job: JobId,
    },
    /// Run the audio kept under `kept` through the pipeline as `job`.
    ProcessKeptAudio {
        kept: JobId,
        job: JobId,
    },
    /// The Undo offer is gone: drop the audio kept under this job id.
    DiscardKeptAudio {
        job: JobId,
    },
    /// Insert into whatever has focus now. Report a failure with
    /// `InsertFailed`.
    Insert {
        job: JobId,
        text: String,
    },
    RecordHistory {
        transcript: Transcript,
        duration_ms: u64,
    },
    /// Skip when sounds are turned off in settings.
    PlaySound(Sound),
    /// Fire `TimerFired(id)` after `after_ms`.
    StartTimer {
        id: TimerId,
        after_ms: u64,
    },
    CancelTimer(TimerId),
    OpenSettings,
    /// Push this to the UI as `dictation:state`.
    Emit(DictationState),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    phase: Phase,
    notice: Option<ShownNotice>,
    last_transcript: Option<String>,
    last_insert: Option<JobId>,
    push_to_talk_down: bool,
    suspended: bool,
    next_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    Listening(Session),
    Processing(Job),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Session {
    job: JobId,
    started_at: Instant,
    mic_open: bool,
    press: Press,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
    /// Push to talk is down; whether this is a hold or a tap is decided on
    /// release.
    Held,
    /// A tap shorter than [`MIN_HOLD_MS`] ended. Capture keeps running while
    /// waiting for a second press.
    Tapped,
    HandsFree {
        locked_at: Instant,
        /// Push to talk went down: stop when it comes back up, unless it
        /// turns out to be part of another shortcut.
        stop_on_release: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Job {
    id: JobId,
    mode: DictationMode,
    stage: ProcessingStage,
    recorded_ms: u64,
    told_busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShownNotice {
    id: u64,
    notice: Notice,
    undo: Option<Recording>,
}

/// Audio the integrator kept after a cancel, for Undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Recording {
    job: JobId,
    mode: DictationMode,
    recorded_ms: u64,
}

impl Session {
    fn mode(&self) -> DictationMode {
        match self.press {
            Press::HandsFree { .. } => DictationMode::HandsFree,
            Press::Held | Press::Tapped => DictationMode::Hold,
        }
    }
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    /// What the pill shows for this state at `now`.
    pub fn view(&self, now: Instant) -> DictationState {
        let notice = self.notice.as_ref().map(|shown| shown.notice.clone());
        match &self.phase {
            Phase::Idle => DictationState {
                phase: if notice.is_some() {
                    DictationPhase::Notice
                } else {
                    DictationPhase::Idle
                },
                notice,
                ..DictationState::idle()
            },
            Phase::Listening(session) => DictationState {
                phase: DictationPhase::Listening,
                mode: Some(session.mode()),
                stage: None,
                notice,
                elapsed_ms: now.saturating_sub(session.started_at),
            },
            Phase::Processing(job) => DictationState {
                phase: DictationPhase::Processing,
                mode: Some(job.mode),
                stage: Some(job.stage),
                notice,
                elapsed_ms: job.recorded_ms,
            },
        }
    }

    /// Listening or processing: the cancel shortcut should be swallowed.
    pub fn is_dictating(&self) -> bool {
        self.phase != Phase::Idle
    }
}

/// Advances the machine by one event that happened at `now`.
pub fn step(state: &State, event: Event, now: Instant) -> (State, Vec<Effect>) {
    let mut step = Step {
        state: state.clone(),
        effects: Vec::new(),
        now,
    };
    step.apply(event);
    let before = state.view(now);
    let after = step.state.view(now);
    let changed = (before.phase, before.mode, before.stage, &before.notice)
        != (after.phase, after.mode, after.stage, &after.notice);
    if changed {
        step.effects.push(Effect::Emit(after));
    }
    (step.state, step.effects)
}

struct Step {
    state: State,
    effects: Vec<Effect>,
    now: Instant,
}

impl Step {
    fn apply(&mut self, event: Event) {
        match event {
            Event::HotkeyPressed(action) => {
                if action == HotkeyAction::PushToTalk {
                    self.state.push_to_talk_down = true;
                }
                if !self.state.suspended {
                    self.hotkey_pressed(action);
                }
            }
            // Releases count even while suspended, so a hold can't get stuck.
            Event::HotkeyReleased(HotkeyAction::PushToTalk) => {
                self.state.push_to_talk_down = false;
                self.push_to_talk_released();
            }
            Event::HotkeyReleased(_) => {}
            Event::ForeignKey => {
                if !self.state.suspended {
                    self.foreign_key();
                }
            }
            Event::HotkeysSuspended(suspended) => self.state.suspended = suspended,
            Event::StartRequested => match self.state.phase {
                Phase::Idle => self.start(Press::HandsFree {
                    locked_at: self.now,
                    stop_on_release: false,
                }),
                Phase::Listening(_) => {}
                Phase::Processing(_) => self.tell_busy(),
            },
            Event::StopRequested => {
                if let Phase::Listening(session) = self.state.phase {
                    self.finish(session);
                }
            }
            Event::CancelRequested => self.cancel(),
            Event::UndoRequested => self.undo(),
            Event::PasteLastRequested => self.paste_last(),
            Event::OpenSettingsRequested => {
                self.clear_notice();
                self.effects.push(Effect::OpenSettings);
            }
            Event::TimerFired(id) => self.timer_fired(id),
            Event::CaptureStarted { job } => {
                if let Some(mut session) = self.session(job)
                    && !session.mic_open
                {
                    session.mic_open = true;
                    self.state.phase = Phase::Listening(session);
                    self.effects.push(Effect::PlaySound(Sound::Start));
                }
            }
            Event::CaptureFailed { job, problem } => {
                if let Some(session) = self.session(job) {
                    self.end_session(session);
                    self.effects.push(Effect::CancelCapture {
                        job,
                        keep_for_undo: false,
                    });
                    self.state.phase = Phase::Idle;
                    self.show_problem(problem);
                }
            }
            Event::CaptureLost { job } => {
                if let Some(session) = self.session(job) {
                    self.finish(session);
                }
            }
            Event::PolishStarted { job } => {
                if let Some(mut current) = self.job(job) {
                    current.stage = ProcessingStage::Polishing;
                    self.state.phase = Phase::Processing(current);
                }
            }
            Event::Processed { job, outcome } => {
                if let Some(current) = self.job(job) {
                    self.processed(current, outcome);
                }
            }
            Event::InsertFailed {
                job,
                paste_last_shortcut,
            } => {
                if self.state.last_insert == Some(job) {
                    self.state.last_insert = None;
                    if self.state.phase == Phase::Idle {
                        self.set_notice(
                            NoticeKind::PasteFailed,
                            format!("Couldn't paste. Press {paste_last_shortcut} to paste it"),
                            Some((NoticeActionKind::CopyLast, "Copy")),
                            None,
                        );
                    }
                }
            }
        }
    }

    fn hotkey_pressed(&mut self, action: HotkeyAction) {
        match action {
            HotkeyAction::PushToTalk => self.push_to_talk_pressed(),
            HotkeyAction::HandsFree => self.hands_free_pressed(),
            HotkeyAction::Cancel => self.cancel(),
            HotkeyAction::PasteLast => self.paste_last(),
            // Command mode isn't built yet, so its keys are just another
            // shortcut typed on top of push to talk.
            HotkeyAction::CommandMode => self.foreign_key(),
        }
    }

    fn push_to_talk_pressed(&mut self) {
        match self.state.phase {
            Phase::Idle => self.start(Press::Held),
            Phase::Listening(mut session) => match session.press {
                Press::Held => {}
                Press::Tapped => self.lock_hands_free(session),
                Press::HandsFree { locked_at, .. } => {
                    if self.now.saturating_sub(locked_at) < DOUBLE_TAP_MS {
                        self.cancel_silently(session);
                    } else {
                        session.press = Press::HandsFree {
                            locked_at,
                            stop_on_release: true,
                        };
                        self.state.phase = Phase::Listening(session);
                    }
                }
            },
            Phase::Processing(_) => self.tell_busy(),
        }
    }

    fn hands_free_pressed(&mut self) {
        match self.state.phase {
            // Push to talk is still down from a press this machine already
            // dealt with (it ended a session); this is the rest of that chord.
            Phase::Idle if self.state.push_to_talk_down => {}
            Phase::Idle => self.start(Press::HandsFree {
                locked_at: self.now,
                stop_on_release: false,
            }),
            Phase::Listening(session) => match session.press {
                Press::Held | Press::Tapped => self.lock_hands_free(session),
                Press::HandsFree {
                    stop_on_release: true,
                    ..
                } => self.finish(session),
                // The push-to-talk press that locked hands-free is still down.
                Press::HandsFree { .. } if self.state.push_to_talk_down => {}
                Press::HandsFree { locked_at, .. } => {
                    if self.now.saturating_sub(locked_at) < DOUBLE_TAP_MS {
                        self.cancel_silently(session);
                    } else {
                        self.finish(session);
                    }
                }
            },
            Phase::Processing(_) => self.tell_busy(),
        }
    }

    fn push_to_talk_released(&mut self) {
        let Phase::Listening(mut session) = self.state.phase else {
            return;
        };
        match session.press {
            Press::Held if self.now.saturating_sub(session.started_at) >= MIN_HOLD_MS => {
                self.finish(session);
            }
            Press::Held => {
                session.press = Press::Tapped;
                self.state.phase = Phase::Listening(session);
                self.effects.push(Effect::StartTimer {
                    id: TimerId::SecondTap(session.job),
                    after_ms: DOUBLE_TAP_MS,
                });
            }
            Press::HandsFree {
                stop_on_release: true,
                ..
            } => self.finish(session),
            Press::Tapped | Press::HandsFree { .. } => {}
        }
    }

    fn foreign_key(&mut self) {
        let Phase::Listening(mut session) = self.state.phase else {
            return;
        };
        match session.press {
            Press::Held
                if self.now.saturating_sub(session.started_at) < FOREIGN_SHORTCUT_WINDOW_MS =>
            {
                self.cancel_silently(session);
            }
            Press::HandsFree {
                locked_at,
                stop_on_release: true,
            } => {
                session.press = Press::HandsFree {
                    locked_at,
                    stop_on_release: false,
                };
                self.state.phase = Phase::Listening(session);
            }
            Press::Held | Press::Tapped | Press::HandsFree { .. } => {}
        }
    }

    fn timer_fired(&mut self, id: TimerId) {
        match id {
            TimerId::SecondTap(job) => {
                if let Some(session) = self.session(job)
                    && session.press == Press::Tapped
                {
                    self.cancel_silently(session);
                }
            }
            TimerId::SessionWarning(job) => {
                if self.session(job).is_some() {
                    self.set_notice(
                        NoticeKind::TimeLimitWarning,
                        "Dictation stops in 1 minute".to_owned(),
                        None,
                        None,
                    );
                }
            }
            TimerId::SessionLimit(job) => {
                if let Some(session) = self.session(job) {
                    self.finish(session);
                }
            }
            TimerId::NoticeDismiss(id) => {
                if self
                    .state
                    .notice
                    .as_ref()
                    .is_some_and(|shown| shown.id == id)
                {
                    self.clear_notice();
                }
            }
        }
    }

    fn start(&mut self, press: Press) {
        self.clear_notice();
        let job = self.next_id();
        self.effects.extend([
            Effect::StartCapture { job },
            Effect::StartTimer {
                id: TimerId::SessionWarning(job),
                after_ms: SESSION_WARN_MS,
            },
            Effect::StartTimer {
                id: TimerId::SessionLimit(job),
                after_ms: SESSION_LIMIT_MS,
            },
        ]);
        self.state.phase = Phase::Listening(Session {
            job,
            started_at: self.now,
            mic_open: false,
            press,
        });
    }

    fn lock_hands_free(&mut self, mut session: Session) {
        if session.press == Press::Tapped {
            self.effects
                .push(Effect::CancelTimer(TimerId::SecondTap(session.job)));
        }
        session.press = Press::HandsFree {
            locked_at: self.now,
            stop_on_release: false,
        };
        self.state.phase = Phase::Listening(session);
    }

    fn end_session(&mut self, session: Session) {
        if session.press == Press::Tapped {
            self.effects
                .push(Effect::CancelTimer(TimerId::SecondTap(session.job)));
        }
        self.effects.extend([
            Effect::CancelTimer(TimerId::SessionWarning(session.job)),
            Effect::CancelTimer(TimerId::SessionLimit(session.job)),
        ]);
    }

    fn finish(&mut self, session: Session) {
        self.end_session(session);
        self.clear_notice();
        self.effects
            .push(Effect::FinishCapture { job: session.job });
        if session.mic_open {
            self.effects.push(Effect::PlaySound(Sound::Stop));
        }
        self.state.phase = Phase::Processing(Job {
            id: session.job,
            mode: session.mode(),
            stage: ProcessingStage::Transcribing,
            recorded_ms: self.now.saturating_sub(session.started_at),
            told_busy: false,
        });
    }

    fn cancel_silently(&mut self, session: Session) {
        self.end_session(session);
        self.clear_notice();
        self.effects.push(Effect::CancelCapture {
            job: session.job,
            keep_for_undo: false,
        });
        self.state.phase = Phase::Idle;
    }

    /// Esc or ✕: drop the dictation but keep its audio for Undo.
    fn cancel(&mut self) {
        let undo = match self.state.phase {
            Phase::Idle => return,
            Phase::Listening(session) => {
                self.end_session(session);
                self.clear_notice();
                // Before the microphone opened there is nothing to undo and
                // nothing was heard, so nothing is played either.
                let undo = session.mic_open.then(|| Recording {
                    job: session.job,
                    mode: session.mode(),
                    recorded_ms: self.now.saturating_sub(session.started_at),
                });
                self.effects.push(Effect::CancelCapture {
                    job: session.job,
                    keep_for_undo: undo.is_some(),
                });
                if session.mic_open {
                    self.effects.push(Effect::PlaySound(Sound::Stop));
                }
                undo
            }
            Phase::Processing(job) => {
                self.clear_notice();
                self.effects.push(Effect::CancelProcessing { job: job.id });
                Some(Recording {
                    job: job.id,
                    mode: job.mode,
                    recorded_ms: job.recorded_ms,
                })
            }
        };
        self.state.phase = Phase::Idle;
        let action = undo.map(|_| (NoticeActionKind::Undo, "Undo"));
        self.set_notice(
            NoticeKind::Cancelled,
            "Dictation cancelled".to_owned(),
            action,
            undo,
        );
    }

    fn undo(&mut self) {
        if self.state.phase != Phase::Idle {
            return;
        }
        let Some(ShownNotice {
            id,
            undo: Some(recording),
            ..
        }) = self.state.notice
        else {
            return;
        };
        self.state.notice = None;
        self.effects
            .push(Effect::CancelTimer(TimerId::NoticeDismiss(id)));
        let job = self.next_id();
        self.effects.push(Effect::ProcessKeptAudio {
            kept: recording.job,
            job,
        });
        self.state.phase = Phase::Processing(Job {
            id: job,
            mode: recording.mode,
            stage: ProcessingStage::Transcribing,
            recorded_ms: recording.recorded_ms,
            told_busy: false,
        });
    }

    fn paste_last(&mut self) {
        if self.state.phase != Phase::Idle {
            return;
        }
        match self.state.last_transcript.clone() {
            Some(text) => {
                self.clear_notice();
                let job = self.next_id();
                self.effects.push(Effect::Insert { job, text });
                self.state.last_insert = Some(job);
            }
            None => self.set_notice(
                NoticeKind::Error,
                "Nothing to paste yet".to_owned(),
                None,
                None,
            ),
        }
    }

    fn processed(&mut self, job: Job, outcome: Outcome) {
        self.state.phase = Phase::Idle;
        self.clear_notice();
        match outcome {
            Outcome::Transcript(transcript) if transcript.text.trim().is_empty() => {
                self.show_no_speech();
            }
            Outcome::Transcript(transcript) => {
                self.effects.push(Effect::Insert {
                    job: job.id,
                    text: transcript.text.clone(),
                });
                self.state.last_transcript = Some(transcript.text.clone());
                self.state.last_insert = Some(job.id);
                self.effects.push(Effect::RecordHistory {
                    transcript,
                    duration_ms: job.recorded_ms,
                });
            }
            Outcome::NoSpeech => self.show_no_speech(),
            Outcome::Failed(problem) => self.show_problem(problem),
        }
    }

    /// Shown once per job however often the user presses.
    fn tell_busy(&mut self) {
        let Phase::Processing(mut job) = self.state.phase else {
            return;
        };
        if job.told_busy {
            return;
        }
        job.told_busy = true;
        self.state.phase = Phase::Processing(job);
        self.set_notice(
            NoticeKind::StillProcessing,
            "Still processing your last dictation".to_owned(),
            None,
            None,
        );
    }

    fn show_no_speech(&mut self) {
        self.set_notice(
            NoticeKind::NoSpeech,
            "Didn't catch that".to_owned(),
            None,
            None,
        );
    }

    fn show_problem(&mut self, problem: Problem) {
        let open_settings = Some((NoticeActionKind::OpenSettings, "Open settings"));
        let (kind, text, action) = match problem {
            Problem::NoModel => (
                NoticeKind::NoModel,
                "No voice model yet".to_owned(),
                open_settings,
            ),
            Problem::NoMicrophone => (
                NoticeKind::NoMicrophone,
                "No microphone found".to_owned(),
                None,
            ),
            Problem::MicrophoneDenied => (
                NoticeKind::NoMicrophone,
                "Microphone access is off".to_owned(),
                open_settings,
            ),
            Problem::Failed(message) => (NoticeKind::Error, message, None),
        };
        self.set_notice(kind, text, action, None);
    }

    fn set_notice(
        &mut self,
        kind: NoticeKind,
        text: String,
        action: Option<(NoticeActionKind, &str)>,
        undo: Option<Recording>,
    ) {
        self.clear_notice();
        let id = self.next_id();
        self.effects.push(Effect::StartTimer {
            id: TimerId::NoticeDismiss(id),
            after_ms: notice_duration_ms(kind),
        });
        self.state.notice = Some(ShownNotice {
            id,
            notice: Notice {
                kind,
                text,
                action: action.map(|(kind, label)| NoticeAction {
                    kind,
                    label: label.to_owned(),
                }),
            },
            undo,
        });
    }

    fn clear_notice(&mut self) {
        if let Some(shown) = self.state.notice.take() {
            self.effects
                .push(Effect::CancelTimer(TimerId::NoticeDismiss(shown.id)));
            if let Some(recording) = shown.undo {
                self.effects
                    .push(Effect::DiscardKeptAudio { job: recording.job });
            }
        }
    }

    fn session(&self, job: JobId) -> Option<Session> {
        match self.state.phase {
            Phase::Listening(session) if session.job == job => Some(session),
            _ => None,
        }
    }

    fn job(&self, id: JobId) -> Option<Job> {
        match self.state.phase {
            Phase::Processing(job) if job.id == id => Some(job),
            _ => None,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.state.next_id += 1;
        self.state.next_id
    }
}

/// How long each notice stays up.
pub fn notice_duration_ms(kind: NoticeKind) -> u64 {
    match kind {
        NoticeKind::NoSpeech | NoticeKind::StillProcessing => 2_500,
        NoticeKind::NoMicrophone | NoticeKind::TimeLimitWarning | NoticeKind::Error => 5_000,
        NoticeKind::Cancelled => 6_000,
        NoticeKind::PasteFailed | NoticeKind::NoModel => 8_000,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::hotkey::{
        Key, KeyCode, KeyInput, KeySet, KeyTransition, Matcher, Platform, Shortcuts,
    };
    use HotkeyAction::{Cancel, CommandMode, HandsFree, PasteLast, PushToTalk};

    struct Machine {
        state: State,
    }

    impl Machine {
        fn new() -> Self {
            Self {
                state: State::new(),
            }
        }

        fn send(&mut self, at: Instant, event: Event) -> Vec<Effect> {
            let (state, effects) = step(&self.state, event, at);
            self.state = state;
            effects
        }

        fn press(&mut self, at: Instant, action: HotkeyAction) -> Vec<Effect> {
            self.send(at, Event::HotkeyPressed(action))
        }

        fn release(&mut self, at: Instant, action: HotkeyAction) -> Vec<Effect> {
            self.send(at, Event::HotkeyReleased(action))
        }

        /// Starts a hold at `at` whose microphone opens 50 ms later.
        fn hold(&mut self, at: Instant) -> JobId {
            self.press(at, PushToTalk);
            let job = self.job();
            self.send(at + 50, Event::CaptureStarted { job });
            job
        }

        /// The job of the running capture or processing.
        fn job(&self) -> JobId {
            match self.state.phase {
                Phase::Listening(session) => session.job,
                Phase::Processing(job) => job.id,
                Phase::Idle => panic!("no job while idle"),
            }
        }

        fn view(&self, at: Instant) -> DictationState {
            self.state.view(at)
        }

        fn notice(&self) -> Option<Notice> {
            self.state.notice.as_ref().map(|shown| shown.notice.clone())
        }

        fn notice_kind(&self) -> Option<NoticeKind> {
            self.notice().map(|notice| notice.kind)
        }

        fn notice_action(&self) -> Option<NoticeActionKind> {
            self.notice()
                .and_then(|notice| notice.action)
                .map(|action| action.kind)
        }

        fn notice_timer(&self) -> TimerId {
            let shown = self.state.notice.as_ref().expect("a notice is shown");
            TimerId::NoticeDismiss(shown.id)
        }
    }

    fn transcript(text: &str) -> Outcome {
        Outcome::Transcript(Transcript {
            raw: text.to_owned(),
            text: text.to_owned(),
            app: None,
            fixes: 0,
        })
    }

    fn plays_sound(effects: &[Effect]) -> bool {
        effects.iter().any(|e| matches!(e, Effect::PlaySound(_)))
    }

    fn touches_capture(effects: &[Effect]) -> bool {
        effects.iter().any(|e| {
            matches!(
                e,
                Effect::StartCapture { .. }
                    | Effect::FinishCapture { .. }
                    | Effect::CancelCapture { .. }
            )
        })
    }

    fn inserts(effects: &[Effect]) -> bool {
        effects.iter().any(|e| matches!(e, Effect::Insert { .. }))
    }

    #[test]
    fn hold_to_talk_inserts_on_release() {
        let mut m = Machine::new();
        let effects = m.press(0, PushToTalk);
        assert!(effects.contains(&Effect::StartCapture { job: 1 }));
        assert!(
            !plays_sound(&effects),
            "no ping before the microphone is live"
        );
        assert_eq!(m.view(0).phase, DictationPhase::Listening);
        assert_eq!(m.view(0).mode, Some(DictationMode::Hold));

        let started = Event::CaptureStarted { job: 1 };
        assert_eq!(
            m.send(80, started.clone()),
            [Effect::PlaySound(Sound::Start)]
        );
        assert_eq!(m.send(90, started), [], "the ping plays once");
        assert_eq!(m.view(1_000).elapsed_ms, 1_000);

        let effects = m.release(1_500, PushToTalk);
        assert!(effects.contains(&Effect::FinishCapture { job: 1 }));
        assert!(effects.contains(&Effect::PlaySound(Sound::Stop)));
        assert!(effects.contains(&Effect::CancelTimer(TimerId::SessionLimit(1))));
        let view = m.view(9_000);
        assert_eq!(
            (view.phase, view.stage, view.elapsed_ms),
            (
                DictationPhase::Processing,
                Some(ProcessingStage::Transcribing),
                1_500
            )
        );
        assert!(effects.contains(&Effect::Emit(view)));

        m.send(2_000, Event::PolishStarted { job: 1 });
        assert_eq!(m.view(2_000).stage, Some(ProcessingStage::Polishing));

        let effects = m.send(
            2_500,
            Event::Processed {
                job: 1,
                outcome: transcript("Hello there."),
            },
        );
        assert!(effects.contains(&Effect::Insert {
            job: 1,
            text: "Hello there.".to_owned()
        }));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::RecordHistory {
                duration_ms: 1_500,
                ..
            }
        )));
        assert_eq!(m.view(2_500), DictationState::idle());
        assert!(effects.contains(&Effect::Emit(DictationState::idle())));
    }

    #[test]
    fn a_hold_of_exactly_half_a_second_pastes() {
        let mut m = Machine::new();
        let job = m.hold(0);
        assert!(
            m.release(MIN_HOLD_MS, PushToTalk)
                .contains(&Effect::FinishCapture { job })
        );
    }

    #[test]
    fn short_tap_waits_for_a_second_press_then_cancels_silently() {
        let mut m = Machine::new();
        let job = m.hold(0);
        assert_eq!(
            m.release(300, PushToTalk),
            [Effect::StartTimer {
                id: TimerId::SecondTap(job),
                after_ms: DOUBLE_TAP_MS
            }]
        );
        assert_eq!(m.view(300).phase, DictationPhase::Listening);

        let effects = m.send(800, Event::TimerFired(TimerId::SecondTap(job)));
        assert!(effects.contains(&Effect::CancelCapture {
            job,
            keep_for_undo: false
        }));
        assert!(!plays_sound(&effects));
        assert_eq!(m.view(800), DictationState::idle(), "no notice");
    }

    #[test]
    fn double_tap_locks_hands_free_without_restarting_capture() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.release(200, PushToTalk);
        let effects = m.press(450, PushToTalk);
        assert!(effects.contains(&Effect::CancelTimer(TimerId::SecondTap(job))));
        assert!(
            !touches_capture(&effects),
            "audio continues from the first press"
        );
        assert_eq!(m.view(450).mode, Some(DictationMode::HandsFree));
        assert_eq!(m.release(520, PushToTalk), []);
        assert_eq!(m.send(700, Event::TimerFired(TimerId::SecondTap(job))), []);
        assert_eq!(
            m.view(60_000).elapsed_ms,
            60_000,
            "counted from the first press"
        );

        assert!(!touches_capture(&m.press(5_000, PushToTalk)));
        assert!(
            m.release(5_100, PushToTalk)
                .contains(&Effect::FinishCapture { job })
        );
        assert_eq!(m.view(9_000).elapsed_ms, 5_100);
    }

    #[test]
    fn third_rapid_tap_cancels_silently() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.release(100, PushToTalk);
        m.press(300, PushToTalk);
        m.release(350, PushToTalk);
        let effects = m.press(600, PushToTalk);
        assert!(effects.contains(&Effect::CancelCapture {
            job,
            keep_for_undo: false
        }));
        assert!(!plays_sound(&effects));
        assert_eq!(m.view(600), DictationState::idle());
    }

    #[test]
    fn hands_free_shortcut_starts_and_stops() {
        let mut m = Machine::new();
        assert!(
            m.press(0, HandsFree)
                .contains(&Effect::StartCapture { job: 1 })
        );
        assert_eq!(m.view(0).mode, Some(DictationMode::HandsFree));
        assert_eq!(m.release(100, HandsFree), []);
        assert!(
            m.press(3_000, HandsFree)
                .contains(&Effect::FinishCapture { job: 1 })
        );
    }

    #[test]
    fn pressing_again_right_after_hands_free_started_cancels() {
        let mut m = Machine::new();
        m.press(0, HandsFree);
        m.release(50, HandsFree);
        let effects = m.press(300, HandsFree);
        assert!(effects.contains(&Effect::CancelCapture {
            job: 1,
            keep_for_undo: false
        }));
        assert_eq!(m.view(300), DictationState::idle());

        let mut m = Machine::new();
        m.send(0, Event::StartRequested);
        assert!(m.press(400, PushToTalk).contains(&Effect::CancelCapture {
            job: 1,
            keep_for_undo: false
        }));
    }

    #[test]
    fn pill_click_starts_hands_free_and_the_stop_button_pastes() {
        let mut m = Machine::new();
        assert!(
            m.send(0, Event::StartRequested)
                .contains(&Effect::StartCapture { job: 1 })
        );
        assert_eq!(m.view(0).mode, Some(DictationMode::HandsFree));
        assert_eq!(m.send(10, Event::StartRequested), []);
        assert!(
            m.send(4_000, Event::StopRequested)
                .contains(&Effect::FinishCapture { job: 1 })
        );
    }

    #[test]
    fn push_to_talk_stops_hands_free_on_release_unless_another_key_follows() {
        let mut m = Machine::new();
        m.send(0, Event::StartRequested);
        assert!(!touches_capture(&m.press(2_000, PushToTalk)));
        assert_eq!(m.send(2_050, Event::ForeignKey), []);
        assert!(!touches_capture(&m.release(2_100, PushToTalk)));

        m.press(3_000, PushToTalk);
        assert!(
            m.release(3_100, PushToTalk)
                .contains(&Effect::FinishCapture { job: 1 })
        );
    }

    struct Keyboard {
        matcher: Matcher,
        bindings: Shortcuts,
        held: Vec<Key>,
    }

    impl Keyboard {
        fn mac() -> Self {
            Self {
                matcher: Matcher::new(),
                bindings: Shortcuts::defaults(Platform::Macos),
                held: Vec::new(),
            }
        }

        fn key(name: &str) -> Key {
            match name {
                "fn" => Key::Modifier(crate::hotkey::Modifier::Fn),
                code => Key::Code(KeyCode::new(code)),
            }
        }

        fn send(
            &mut self,
            m: &mut Machine,
            at: Instant,
            transition: KeyTransition,
            name: &str,
        ) -> Vec<Effect> {
            let key = Self::key(name);
            match transition {
                KeyTransition::Down => self.held.push(key.clone()),
                KeyTransition::Up => self.held.retain(|held| *held != key),
            }
            let pressed = KeySet::new(
                self.held.iter().filter_map(|k| match k {
                    Key::Modifier(modifier) => Some(*modifier),
                    Key::Code(_) => None,
                }),
                self.held.iter().filter_map(|k| match k {
                    Key::Code(code) => Some(code.clone()),
                    Key::Modifier(_) => None,
                }),
            );
            let input = KeyInput {
                transition,
                key,
                pressed,
            };
            self.matcher
                .handle(&input, &self.bindings)
                .into_iter()
                .flat_map(|signal| m.send(at, signal.into()))
                .collect()
        }

        fn down(&mut self, m: &mut Machine, at: Instant, name: &str) -> Vec<Effect> {
            self.send(m, at, KeyTransition::Down, name)
        }

        fn up(&mut self, m: &mut Machine, at: Instant, name: &str) -> Vec<Effect> {
            self.send(m, at, KeyTransition::Up, name)
        }
    }

    #[test]
    fn fn_space_hands_over_from_push_to_talk_without_a_stuck_hold() {
        let mut m = Machine::new();
        let mut k = Keyboard::mac();
        assert!(
            k.down(&mut m, 0, "fn")
                .contains(&Effect::StartCapture { job: 1 })
        );
        m.send(40, Event::CaptureStarted { job: 1 });
        assert!(!touches_capture(&k.down(&mut m, 100, "Space")));
        assert!(!touches_capture(&k.up(&mut m, 150, "Space")));
        assert!(!touches_capture(&k.up(&mut m, 160, "fn")));
        assert_eq!(m.view(160).mode, Some(DictationMode::HandsFree));

        assert!(!touches_capture(&k.down(&mut m, 3_000, "fn")));
        assert!(
            k.down(&mut m, 3_050, "Space")
                .contains(&Effect::FinishCapture { job: 1 })
        );
        k.up(&mut m, 3_100, "Space");
        k.up(&mut m, 3_110, "fn");
        m.send(
            3_500,
            Event::Processed {
                job: 1,
                outcome: transcript("Hi."),
            },
        );

        let job = m.state.next_id + 1;
        assert!(
            k.down(&mut m, 5_000, "fn")
                .contains(&Effect::StartCapture { job })
        );
        assert!(
            k.up(&mut m, 6_000, "fn")
                .contains(&Effect::FinishCapture { job })
        );
    }

    #[test]
    fn escape_while_listening_keeps_the_audio_for_undo() {
        let mut m = Machine::new();
        let job = m.hold(0);
        let effects = m.press(2_000, Cancel);
        assert!(effects.contains(&Effect::CancelCapture {
            job,
            keep_for_undo: true
        }));
        assert!(effects.contains(&Effect::PlaySound(Sound::Stop)));
        assert_eq!(m.view(2_000).phase, DictationPhase::Notice);
        assert_eq!(m.notice_kind(), Some(NoticeKind::Cancelled));
        assert_eq!(m.notice_action(), Some(NoticeActionKind::Undo));
        assert_eq!(m.release(2_100, PushToTalk), []);

        let effects = m.send(3_000, Event::UndoRequested);
        let ProcessKept { kept, job: undo } = kept_audio(&effects);
        assert_eq!(kept, job);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::DiscardKeptAudio { .. }))
        );
        let view = m.view(3_000);
        assert_eq!(
            (view.phase, view.mode, view.elapsed_ms, view.notice),
            (
                DictationPhase::Processing,
                Some(DictationMode::Hold),
                2_000,
                None
            )
        );
        let effects = m.send(
            4_000,
            Event::Processed {
                job: undo,
                outcome: transcript("Kept."),
            },
        );
        assert!(effects.contains(&Effect::Insert {
            job: undo,
            text: "Kept.".to_owned()
        }));
    }

    struct ProcessKept {
        kept: JobId,
        job: JobId,
    }

    fn kept_audio(effects: &[Effect]) -> ProcessKept {
        effects
            .iter()
            .find_map(|e| match e {
                Effect::ProcessKeptAudio { kept, job } => Some(ProcessKept {
                    kept: *kept,
                    job: *job,
                }),
                _ => None,
            })
            .expect("kept audio is processed")
    }

    #[test]
    fn the_undo_offer_expires_with_its_notice() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.send(1_000, Event::CancelRequested);
        let effects = m.send(7_000, Event::TimerFired(m.notice_timer()));
        assert!(effects.contains(&Effect::DiscardKeptAudio { job }));
        assert_eq!(m.view(7_000), DictationState::idle());
        assert_eq!(m.send(7_100, Event::UndoRequested), []);
    }

    #[test]
    fn a_new_dictation_replaces_the_undo_offer() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.press(1_000, Cancel);
        m.release(1_100, PushToTalk);
        let effects = m.press(1_500, PushToTalk);
        assert!(effects.contains(&Effect::DiscardKeptAudio { job }));
        assert!(effects.contains(&Effect::StartCapture { job: m.job() }));
        assert_eq!(m.view(1_500).phase, DictationPhase::Listening);
        assert_eq!(m.notice(), None);
    }

    #[test]
    fn cancel_while_processing_keeps_the_audio_and_ignores_the_late_result() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.release(1_000, PushToTalk);
        let effects = m.send(1_200, Event::CancelRequested);
        assert!(effects.contains(&Effect::CancelProcessing { job }));
        assert_eq!(m.notice_action(), Some(NoticeActionKind::Undo));

        let late = Event::Processed {
            job,
            outcome: transcript("Too late."),
        };
        assert_eq!(m.send(1_500, late.clone()), []);

        let undo = kept_audio(&m.send(2_000, Event::UndoRequested));
        assert_eq!(undo.kept, job);
        assert_eq!(m.send(2_100, late), [], "the cancelled job is still stale");
        assert_eq!(m.view(2_100).phase, DictationPhase::Processing);
    }

    #[test]
    fn cancel_before_the_microphone_opened_is_silent() {
        let mut m = Machine::new();
        m.press(0, PushToTalk);
        let effects = m.press(100, Cancel);
        assert!(effects.contains(&Effect::CancelCapture {
            job: 1,
            keep_for_undo: false
        }));
        assert!(!plays_sound(&effects));
        assert_eq!(m.notice_kind(), Some(NoticeKind::Cancelled));
        assert_eq!(m.notice_action(), None, "nothing to undo");
        assert_eq!(m.send(200, Event::CaptureStarted { job: 1 }), []);
    }

    #[test]
    fn cancel_does_nothing_while_idle() {
        let mut m = Machine::new();
        assert_eq!(m.press(0, Cancel), []);
    }

    #[test]
    fn another_shortcut_in_the_first_second_cancels_silently() {
        let mut m = Machine::new();
        let job = m.hold(0);
        let effects = m.send(900, Event::ForeignKey);
        assert!(effects.contains(&Effect::CancelCapture {
            job,
            keep_for_undo: false
        }));
        assert!(!plays_sound(&effects));
        assert_eq!(m.view(900), DictationState::idle());

        let mut m = Machine::new();
        m.hold(0);
        assert!(touches_capture(&m.press(500, CommandMode)));

        let mut m = Machine::new();
        let job = m.hold(0);
        assert_eq!(m.send(FOREIGN_SHORTCUT_WINDOW_MS, Event::ForeignKey), []);
        assert!(
            m.release(3_000, PushToTalk)
                .contains(&Effect::FinishCapture { job })
        );
    }

    #[test]
    fn presses_while_processing_say_so_once() {
        let mut m = Machine::new();
        m.hold(0);
        m.release(1_000, PushToTalk);
        let effects = m.press(1_200, PushToTalk);
        assert!(!touches_capture(&effects));
        assert_eq!(m.notice_kind(), Some(NoticeKind::StillProcessing));
        assert_eq!(
            m.notice().map(|notice| notice.text).as_deref(),
            Some("Still processing your last dictation")
        );
        assert_eq!(m.view(1_200).phase, DictationPhase::Processing);
        assert_eq!(m.release(1_250, PushToTalk), []);
        assert_eq!(m.press(1_300, PushToTalk), []);
        assert_eq!(m.press(1_400, HandsFree), []);
        assert_eq!(m.send(1_500, Event::StartRequested), []);
    }

    #[test]
    fn warns_at_nineteen_minutes_and_transcribes_at_twenty() {
        let mut m = Machine::new();
        let effects = m.send(0, Event::StartRequested);
        let job = m.job();
        assert!(effects.contains(&Effect::StartTimer {
            id: TimerId::SessionWarning(job),
            after_ms: SESSION_WARN_MS
        }));
        assert!(effects.contains(&Effect::StartTimer {
            id: TimerId::SessionLimit(job),
            after_ms: SESSION_LIMIT_MS
        }));

        m.send(
            SESSION_WARN_MS,
            Event::TimerFired(TimerId::SessionWarning(job)),
        );
        assert_eq!(m.view(SESSION_WARN_MS).phase, DictationPhase::Listening);
        assert_eq!(m.notice_kind(), Some(NoticeKind::TimeLimitWarning));

        let effects = m.send(
            SESSION_LIMIT_MS,
            Event::TimerFired(TimerId::SessionLimit(job)),
        );
        assert!(effects.contains(&Effect::FinishCapture { job }));
        let view = m.view(SESSION_LIMIT_MS);
        assert_eq!(
            (view.phase, view.elapsed_ms, view.notice),
            (DictationPhase::Processing, SESSION_LIMIT_MS, None)
        );
    }

    #[test]
    fn no_speech_pastes_nothing() {
        for outcome in [Outcome::NoSpeech, transcript("  ")] {
            let mut m = Machine::new();
            let job = m.hold(0);
            m.release(1_000, PushToTalk);
            let effects = m.send(1_500, Event::Processed { job, outcome });
            assert!(!inserts(&effects));
            assert_eq!(m.notice_kind(), Some(NoticeKind::NoSpeech));
            assert_eq!(m.view(1_500).phase, DictationPhase::Notice);
        }
    }

    #[test]
    fn capture_problems_end_the_dictation_with_a_notice() {
        let cases = [
            (
                Problem::NoModel,
                NoticeKind::NoModel,
                Some(NoticeActionKind::OpenSettings),
            ),
            (Problem::NoMicrophone, NoticeKind::NoMicrophone, None),
            (
                Problem::MicrophoneDenied,
                NoticeKind::NoMicrophone,
                Some(NoticeActionKind::OpenSettings),
            ),
        ];
        for (problem, kind, action) in cases {
            let mut m = Machine::new();
            m.press(0, PushToTalk);
            let effects = m.send(20, Event::CaptureFailed { job: 1, problem });
            assert!(effects.contains(&Effect::CancelCapture {
                job: 1,
                keep_for_undo: false
            }));
            assert!(!plays_sound(&effects));
            assert_eq!((m.notice_kind(), m.notice_action()), (Some(kind), action));
            assert_eq!(m.release(300, PushToTalk), []);
        }
    }

    #[test]
    fn open_settings_from_a_notice() {
        let mut m = Machine::new();
        m.press(0, PushToTalk);
        let job = m.job();
        m.release(800, PushToTalk);
        m.send(
            900,
            Event::Processed {
                job,
                outcome: Outcome::Failed(Problem::NoModel),
            },
        );
        assert_eq!(m.notice_kind(), Some(NoticeKind::NoModel));
        let notice = m
            .view(900)
            .notice
            .map(|n| (n.text, n.action.map(|a| a.label)));
        assert_eq!(
            notice,
            Some((
                "No voice model yet".to_owned(),
                Some("Open settings".to_owned())
            ))
        );
        let effects = m.send(1_000, Event::OpenSettingsRequested);
        assert!(effects.contains(&Effect::OpenSettings));
        assert_eq!(m.view(1_000), DictationState::idle());
    }

    #[test]
    fn a_lost_microphone_transcribes_what_was_captured() {
        let mut m = Machine::new();
        let job = m.hold(0);
        let effects = m.send(3_000, Event::CaptureLost { job });
        assert!(effects.contains(&Effect::FinishCapture { job }));
        assert_eq!(m.view(3_000).elapsed_ms, 3_000);
        assert_eq!(m.release(4_000, PushToTalk), []);
    }

    #[test]
    fn pipeline_errors_show_their_message() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.release(1_000, PushToTalk);
        let message = "The voice model could not be loaded".to_owned();
        let outcome = Outcome::Failed(Problem::Failed(message.clone()));
        m.send(1_500, Event::Processed { job, outcome });
        let notice = m.notice().expect("a notice");
        assert_eq!((notice.kind, notice.text), (NoticeKind::Error, message));
    }

    #[test]
    fn a_failed_paste_keeps_the_transcript_for_paste_last() {
        let mut m = Machine::new();
        let job = m.hold(0);
        m.release(1_000, PushToTalk);
        m.send(
            1_500,
            Event::Processed {
                job,
                outcome: transcript("Keep me."),
            },
        );
        m.send(
            1_600,
            Event::InsertFailed {
                job,
                paste_last_shortcut: "⌃⌘V".to_owned(),
            },
        );
        let notice = m.notice().expect("a notice");
        assert_eq!(notice.kind, NoticeKind::PasteFailed);
        assert_eq!(notice.text, "Couldn't paste. Press ⌃⌘V to paste it");
        let action = notice.action.expect("an action");
        assert_eq!(action.kind, NoticeActionKind::CopyLast);
        assert_eq!(action.label, "Copy");

        let effects = m.press(5_000, PasteLast);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Insert { text, .. } if text == "Keep me."))
        );
        assert_eq!(m.view(5_000), DictationState::idle());
    }

    #[test]
    fn paste_last_needs_a_transcript() {
        let mut m = Machine::new();
        let effects = m.send(0, Event::PasteLastRequested);
        assert!(!inserts(&effects));
        assert_eq!(m.notice_kind(), Some(NoticeKind::Error));

        let job = m.hold(1_000);
        m.release(2_000, PushToTalk);
        m.send(
            2_500,
            Event::Processed {
                job,
                outcome: transcript("Again."),
            },
        );
        m.hold(3_000);
        assert_eq!(m.press(3_100, PasteLast), [], "not while dictating");
    }

    #[test]
    fn notices_dismiss_themselves_and_newer_ones_replace_older_ones() {
        let mut m = Machine::new();
        let effects = m.send(0, Event::PasteLastRequested);
        let first = m.notice_timer();
        assert!(effects.contains(&Effect::StartTimer {
            id: first,
            after_ms: notice_duration_ms(NoticeKind::Error)
        }));

        m.hold(1_000);
        m.press(1_200, Cancel);
        let second = m.notice_timer();
        assert_ne!(first, second);
        assert_eq!(m.send(2_000, Event::TimerFired(first)), [], "stale timer");
        assert_eq!(m.notice_kind(), Some(NoticeKind::Cancelled));
        m.send(7_000, Event::TimerFired(second));
        assert_eq!(m.view(7_000), DictationState::idle());
    }

    #[test]
    fn a_notice_never_blocks_a_new_dictation() {
        let mut m = Machine::new();
        m.send(0, Event::PasteLastRequested);
        let timer = m.notice_timer();
        let effects = m.press(100, PushToTalk);
        assert!(effects.contains(&Effect::CancelTimer(timer)));
        assert!(effects.contains(&Effect::StartCapture { job: m.job() }));
        assert_eq!(m.view(100).notice, None);
    }

    #[test]
    fn suspended_hotkeys_are_ignored_but_releases_still_count() {
        let mut m = Machine::new();
        m.send(0, Event::HotkeysSuspended(true));
        assert_eq!(m.press(10, PushToTalk), []);
        assert_eq!(m.press(20, HandsFree), []);
        assert_eq!(m.press(30, PasteLast), []);
        m.release(40, PushToTalk);
        m.send(50, Event::HotkeysSuspended(false));
        assert!(touches_capture(&m.press(60, PushToTalk)));

        let mut m = Machine::new();
        let job = m.hold(0);
        m.send(600, Event::HotkeysSuspended(true));
        assert!(
            m.release(1_000, PushToTalk)
                .contains(&Effect::FinishCapture { job })
        );
    }

    #[test]
    fn late_results_of_an_earlier_dictation_are_ignored() {
        let mut m = Machine::new();
        let old = m.hold(0);
        m.release(1_000, PushToTalk);
        m.press(1_100, Cancel);
        let new = m.hold(1_500);
        let stale = [
            Event::CaptureStarted { job: old },
            Event::CaptureLost { job: old },
            Event::CaptureFailed {
                job: old,
                problem: Problem::NoMicrophone,
            },
            Event::PolishStarted { job: old },
            Event::Processed {
                job: old,
                outcome: transcript("Old."),
            },
            Event::InsertFailed {
                job: old,
                paste_last_shortcut: "⌃⌘V".to_owned(),
            },
            Event::TimerFired(TimerId::SecondTap(old)),
            Event::TimerFired(TimerId::SessionWarning(old)),
            Event::TimerFired(TimerId::SessionLimit(old)),
        ];
        for event in stale {
            assert_eq!(m.send(2_000, event.clone()), [], "{event:?}");
        }
        assert_eq!(m.job(), new);
        assert_eq!(m.view(2_000).phase, DictationPhase::Listening);
    }

    #[test]
    fn view_shows_listening_hands_free_and_processing() {
        let mut m = Machine::new();
        m.send(1_000, Event::StartRequested);
        assert_eq!(
            m.view(4_000),
            DictationState {
                phase: DictationPhase::Listening,
                mode: Some(DictationMode::HandsFree),
                stage: None,
                notice: None,
                elapsed_ms: 3_000,
            }
        );
    }

    /// xorshift64: the crate has no dev-dependencies, and a fixed seed makes
    /// any failure reproducible.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }

        fn pick<T: Clone>(&mut self, items: &[T]) -> T {
            items[self.below(items.len() as u64) as usize].clone()
        }
    }

    fn random_event(rng: &mut Rng, jobs: &[JobId], timers: &[TimerId]) -> Event {
        // Mostly the newest job, sometimes an older one (a late result).
        let job = if rng.below(4) == 0 {
            rng.pick(jobs)
        } else {
            jobs[jobs.len() - 1]
        };
        let action = rng.pick(&HotkeyAction::ALL);
        let problem = rng.pick(&[
            Problem::NoModel,
            Problem::NoMicrophone,
            Problem::MicrophoneDenied,
            Problem::Failed("Broken".to_owned()),
        ]);
        let outcome = rng.pick(&[
            transcript("Text."),
            transcript(""),
            Outcome::NoSpeech,
            Outcome::Failed(problem.clone()),
        ]);
        match rng.below(17) {
            0..=2 => Event::HotkeyPressed(action),
            3..=5 => Event::HotkeyReleased(action),
            6 => Event::ForeignKey,
            7 => Event::HotkeysSuspended(rng.below(3) == 0),
            8 => Event::StartRequested,
            9 => Event::StopRequested,
            10 => Event::CancelRequested,
            11 => Event::UndoRequested,
            12 => Event::PasteLastRequested,
            13 => Event::OpenSettingsRequested,
            14 if !timers.is_empty() => Event::TimerFired(rng.pick(timers)),
            14 | 15 => match rng.below(4) {
                0 => Event::CaptureStarted { job },
                1 => Event::CaptureFailed { job, problem },
                2 => Event::CaptureLost { job },
                _ => Event::PolishStarted { job },
            },
            _ => match rng.below(3) {
                0 => Event::InsertFailed {
                    job,
                    paste_last_shortcut: "⌃⌘V".to_owned(),
                },
                _ => Event::Processed { job, outcome },
            },
        }
    }

    #[test]
    fn random_event_sequences_keep_capture_and_kept_audio_balanced() {
        for seed in 1..=400_u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let mut m = Machine::new();
            let mut now = 0;
            let mut capturing: Option<JobId> = None;
            let mut kept = BTreeSet::new();
            let mut jobs = vec![0];
            let mut timers = Vec::new();
            for _ in 0..300 {
                now += rng.below(1_500);
                let event = random_event(&mut rng, &jobs, &timers);
                let effects = m.send(now, event.clone());
                let context = format!("seed {seed}, {event:?} -> {effects:?}");
                for effect in &effects {
                    match effect {
                        Effect::StartCapture { job } => {
                            assert_eq!(capturing, None, "{context}");
                            capturing = Some(*job);
                            jobs.push(*job);
                        }
                        Effect::FinishCapture { job } => {
                            assert_eq!(capturing.take(), Some(*job), "{context}");
                        }
                        Effect::CancelCapture { job, keep_for_undo } => {
                            assert_eq!(capturing.take(), Some(*job), "{context}");
                            if *keep_for_undo {
                                kept.insert(*job);
                            }
                        }
                        Effect::CancelProcessing { job } => {
                            kept.insert(*job);
                        }
                        Effect::ProcessKeptAudio { kept: audio, job } => {
                            assert!(kept.remove(audio), "{context}");
                            jobs.push(*job);
                        }
                        Effect::DiscardKeptAudio { job } => {
                            assert!(kept.remove(job), "{context}");
                        }
                        Effect::Insert { job, .. } => jobs.push(*job),
                        Effect::StartTimer { id, .. } => timers.push(*id),
                        _ => {}
                    }
                }
                assert!(kept.len() <= 1, "{context}");

                let view = m.view(now);
                assert_eq!(
                    capturing.is_some(),
                    view.phase == DictationPhase::Listening,
                    "{context}"
                );
                match view.phase {
                    DictationPhase::Idle => assert_eq!(view, DictationState::idle(), "{context}"),
                    DictationPhase::Notice => {
                        assert!(view.notice.is_some() && view.mode.is_none(), "{context}");
                    }
                    DictationPhase::Listening => {
                        assert!(view.mode.is_some() && view.stage.is_none(), "{context}");
                    }
                    DictationPhase::Processing => {
                        assert!(view.mode.is_some() && view.stage.is_some(), "{context}");
                    }
                }
                let emitted = effects.iter().rev().find_map(|e| match e {
                    Effect::Emit(state) => Some(state),
                    _ => None,
                });
                if let Some(emitted) = emitted {
                    assert_eq!(emitted, &view, "{context}");
                }
            }
        }
    }
}
