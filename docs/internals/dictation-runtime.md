# Dictation runtime

The dictation flow has two halves:

- **The machine** (`crates/tw-core/src/dictation/machine.rs`): a pure reducer. Given the current `State`, an `Event` and the time, it returns the next `State` and a list of `Effect`s. No I/O, no clock, no threads.
- **The runtime** (`src-tauri/src/dictation/runtime.rs`): one thread, `tw-dictation`, that owns the state, feeds events into the machine and performs the effects it asks for.

Almost all behaviour (what a short tap does, when Undo is offered, what a second press means) lives in the machine and is covered by its tests. The runtime is plumbing.

## The machine

```rust
pub fn step(state: &State, event: Event, now: Instant) -> (State, Vec<Effect>)
```

`Instant` is milliseconds on a monotonic clock the caller owns. The machine never reads a clock: if it needs to be woken later, it returns `Effect::StartTimer { id, after_ms }` and gets `Event::TimerFired(id)` back.

### Phases

```text
 Idle ──start──► Listening ──finish──► Processing ──inserted / notice──► Idle
   ▲                │  │                   │
   └─ cancel ───────┘  └── cancel ─────────┘   (cancel keeps the audio for Undo)
```

`DictationPhase` as the UI sees it is `idle`, `listening`, `processing` or `notice`. Internally a *notice* is not a phase but a layer: it sits on top of any phase, is the whole picture in Idle, and shows above the pill otherwise. A new dictation replaces the current notice.

Inside Listening, the session is one of three `Press` states:

| Press | Meaning |
| --- | --- |
| `Held` | Push to talk is down. A hold or a tap is decided on release. |
| `Tapped` | Released in under 500 ms. Capture keeps running for 500 ms waiting for a second press; a timer ends it silently. |
| `HandsFree` | Locked by a double-tap, the hands-free shortcut or a pill click. |

### Constants

Defined in `crates/tw-core/src/dictation/mod.rs`:

| Constant | Value | Meaning |
| --- | --- | --- |
| `MIN_HOLD_MS` | 500 | A shorter press doesn't paste |
| `DOUBLE_TAP_MS` | 500 | Window for the second tap; a press this soon after hands-free began cancels instead of stopping |
| `FOREIGN_SHORTCUT_WINDOW_MS` | 1000 | Another shortcut that uses the hold's keys cancels silently within this time |
| `SESSION_WARN_MS` | 19 min | A notice is shown |
| `SESSION_LIMIT_MS` | 20 min | The session ends and is transcribed |

### Events

Inputs, in `machine.rs`:

- From the key stream: `HotkeyPressed`, `HotkeyReleased`, `ForeignKey`, `HotkeysSuspended` (while the shortcut recorder is open).
- From commands and the UI: `StartRequested` (always hands-free), `StopRequested`, `CancelRequested`, `UndoRequested`, `PasteLastRequested`, `OpenSettingsRequested`.
- From timers: `TimerFired(SecondTap | SessionWarning | SessionLimit | NoticeDismiss)`.
- Reports from the runtime: `CaptureStarted`, `CaptureFailed`, `CaptureLost`, `PolishStarted`, `Processed { outcome }`, `InsertFailed`.

### Effects

`StartCapture`, `FinishCapture`, `CancelCapture { keep_for_undo }`, `CancelProcessing`, `ProcessKeptAudio { kept, job }`, `DiscardKeptAudio`, `Insert`, `RecordHistory`, `PlaySound`, `StartTimer`, `CancelTimer`, `OpenSettings`, and `Emit(DictationState)` which becomes the `dictation:state` event.

### Job ids

Every capture, processing job and notice carries an id. A late result, a stale timer or an expired Undo offer arrives with an id that doesn't match the current one and is ignored. That is how the machine stays right with threads reporting back in any order: when you add an effect that reports back asynchronously, give it the job id and check it.

### Changing the machine

1. Add a test first. The tests in `machine.rs` are written as sequences (`m.press(0, PushToTalk)`, `m.release(300, PushToTalk)` …) and read like a specification.
2. Keep it pure. If you need the clock, the OS or a setting, make it an input event or an effect.
3. If you add a `DictationState` field or a notice kind, mirror it in `src/lib/ipc/types.ts` and the mock ([IPC](ipc.md)).

## The key stream

```text
tw-os KeyListener ──Msg::Key──► Runtime::on_key
                                   │  if the shortcut recorder is open: send HotkeyCapture to the UI, stop
                                   ▼
                              hotkey::Matcher::handle(key, &shortcuts)  ──► [HotkeySignal]
                                   ▼
                              Event::from(signal) → dispatch
```

`Matcher` (`tw-core/src/hotkey.rs`) turns raw key transitions into bindings pressed and released:

- A binding is pressed on the key-down that makes exactly its keys held, and released as soon as any of its keys is up. Letting go of Space in `fn Space` never presses push to talk.
- *Hand-over*: when a key-down turns one held binding into another (`fn`, then Space), the new binding's `Pressed` comes first and the old one's `Released` second, so the machine sees hands-free begin while push to talk is still down and the release isn't mistaken for a short tap.
- A key-down that completes no binding while one is held is a `ForeignKey`: the user is typing another shortcut.

After every event, `sync_swallowed` recomputes which combos the OS listener must hide from the focused app (`hotkey::swallow_set`): the completing key of `fn Space` or `⌃⌘V` is swallowed so it never reaches the app. Cancel (`Esc`) is only swallowed while dictating. Modifier-only bindings are never swallowed.

Shortcut validation (`validate`, the `HotkeyIssue` messages, the reserved-shortcut tables per OS) is also in `hotkey.rs`. `Shortcuts` holds five bindings. **Command mode** has a default (`fn⌃` on macOS) and is matched, but the machine treats it as a foreign key and nothing acts on it yet. Likewise `Settings.dictation_mode` (hold to talk, or press to start and press again to stop) is in the settings type, the IPC types and the defaults, but no runtime code or settings page reads it: push to talk always behaves as hold to talk with a double-tap for hands-free.

## The runtime

`Dictation::spawn` starts the `tw-dictation` thread and returns a handle (`AppState.dictation`). Everything talks to it with `Msg`:

| `Msg` | Sent by |
| --- | --- |
| `Event(event)` | Commands (`start_dictation` …), the pill, tray |
| `Key(event)` | The key listener |
| `Captured { job, recording }` | The audio thread when a stop or cancel finishes |
| `CaptureLost` | The audio source when the microphone disappears |
| `Processed { job, outcome }` | A `tw-pipeline` thread |
| `HotkeyCapture(bool)` | `start_hotkey_capture`, `stop_hotkey_capture` |
| `MicTest(..)` | `start_mic_test`, `stop_mic_test` |
| `SettingsChanged` | `update_settings`, to refresh what is swallowed |
| `Shutdown` | App exit |

The loop waits on the channel with a timeout set by the earliest timer, fires due timers, and handles messages. `dispatch` runs the machine and performs each effect; effects may queue more events (`CaptureFailed` when no model is installed) which are handled in the same pass.

### What the effects do

| Effect | Runtime action |
| --- | --- |
| `StartCapture` | Fails with `NoModel` if no voice model is installed. Otherwise asks the audio thread to open the microphone, and starts loading the voice model on another thread. The model is held (`model_hold`) until the capture ends, so the idle unload can't drop it mid-sentence. |
| `FinishCapture` | Marks the job `Pending::Process`; when the audio thread returns the `Recording`, spawns a `tw-pipeline` thread. |
| `CancelCapture { keep_for_undo }` | Stops the audio; keeps the samples in `kept` under the job id if Undo may be asked for. |
| `CancelProcessing` | Sets the job's `AtomicBool`, which the pipeline checks between steps, and keeps its audio. |
| `ProcessKeptAudio` | Undo: runs the kept samples as a new job. |
| `Insert` | Spawns a thread that calls `Desktop::insert`, which blocks until the modifier keys are up. On failure sends `InsertFailed`. |
| `RecordHistory` | Builds a `HistoryEntry`, saves it with the fixes count, tells the UI (`history:changed`). Also remembers the text for paste-last and `copy_last_transcript`. |
| `PlaySound` | Skipped when the `sounds` setting is off. |
| `Emit` | Stores the view for `get_dictation_state`, calls `Sink::state`. |

### The pipeline thread

`dictation/pipeline.rs::process` turns samples into an `Outcome` (`Transcript`, `NoSpeech` or `Failed(Problem)`). It returns `None` when cancelled, and the machine wants no answer. The steps are on [Text pipeline](text-pipeline.md#the-whole-path). The same function serves file transcription (`files.rs`).

### The traits

`dictation/mod.rs` defines what the runtime depends on. `Deps` bundles them with `Data` and `Keys`.

| Trait | Real implementation | What it hides |
| --- | --- | --- |
| `AudioSource` | `MicSource` (`desktop.rs`, wraps `tw_audio::Recorder`) | The microphone |
| `Desktop` | `SystemDesktop` | Frontmost app, insertion, sounds, microphone-denied check |
| `Transcribers` | `WhisperModels` (`engine.rs`) | Loading and caching the Whisper model |
| `Polisher` | `LlmPolish` (`polish.rs`) | The worker process |
| `Sink` | `AppSink` (`sink.rs`) | Events to windows |

`runtime/tests.rs` drives the whole runtime with fakes of these. `smoke.rs` (debug builds only) replaces `AudioSource` and `Desktop` with a WAV file and a text file; see [Testing](../operations/testing.md#the-smoke-run).

### Shutdown

`shutdown` in `lib.rs` cancels file jobs, stops the runtime, removes the key listener, ends the LLM worker and **unloads the voice model**. The last one matters: ggml's Metal backend aborts the process at exit if a model is still loaded, so `WhisperModels::unload` waits up to three seconds for a cancelled transcription to let go of it.
