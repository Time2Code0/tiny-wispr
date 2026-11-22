# Architecture

Tiny Wispr is a Tauri v2 app: a Rust backend and a React frontend in three small windows. This page is the map. The other internals pages go deep on one part each.

## Principles

These decide where code goes. Most design questions are answered by one of them.

1. **Everything runs on this device.** The only network code is model downloads, in `tw-models`. Nothing else may open a socket. Every request is logged for the user ([Data and persistence](data-and-persistence.md#network-log)).
2. **Pure logic is separate from the OS.** `tw-core` has no native dependencies, so everything that can be decided without a microphone, a window or a key tap is decided there and unit-tested.
3. **The dictation flow is a reducer.** `step(&State, Event, now) -> (State, Vec<Effect>)` in `tw-core`, driven by a runtime that performs the effects. See [Dictation runtime](dictation-runtime.md).
4. **The OS sits behind traits.** The runtime depends on `AudioSource`, `Desktop`, `Transcribers`, `Polisher` and `Sink`, so the whole thing can be driven by fakes in tests and a scripted file in the smoke run.
5. **Tests never use the microphone, an event tap or a permission prompt.** See [Testing](../operations/testing.md).
6. **Never request an OS permission unless the user clicked something that asks for it.**

## The workspace

```text
tiny-wispr/
├─ crates/
│  ├─ tw-core/        pure Rust: types, state machine, text pipeline. The IPC contract.
│  ├─ tw-audio/       capture, levels, speech detection, file decoding
│  ├─ tw-stt/         whisper.cpp (whisper-rs)
│  ├─ tw-models/      downloads, inventory, hardware. The only network code.
│  ├─ tw-llm/         client for the worker, polish call
│  ├─ tw-llm-worker/  llama.cpp in its own process (a sidecar binary)
│  └─ tw-os/          global keys, insertion, permissions, frontmost app, autostart
├─ src-tauri/         the app: windows, tray, commands, persistence, the dictation runtime
└─ src/               the React frontend (three window entries)
```

Dependencies point one way:

```text
                    ┌──────────────┐
                    │  src-tauri   │
                    └───────┬──────┘
      ┌───────┬───────┬─────┴──┬────────┬────────┐
      ▼       ▼       ▼        ▼        ▼        ▼
  tw-audio  tw-stt  tw-models  tw-llm  tw-os   (tw-llm-worker, as a sidecar process)
      └───────┴───────┴────┬────┴────────┘
                           ▼
                       tw-core
```

| Crate | Owns | Notes |
| --- | --- | --- |
| `tw-core` | `Settings`, `Hotkey` and the matcher, `DictationState` and the state machine, history and stats, the model catalog and recommendation, the text pipeline, LLM prompt building | Serde types here are the IPC contract |
| `tw-audio` | `Recorder` (cpal), level metering, `vad`, `decode_file`, WAV helpers | Mono `f32` at 16 kHz; 300 ms pre-roll; 20 min cap |
| `tw-stt` | `WhisperEngine`, `Transcriber`, `clean_transcript` | Pads short audio, adds trailing silence, serialises calls |
| `tw-models` | `Downloader`, `Inventory`, `hardware()` | Resumable, checksummed HTTPS downloads |
| `tw-llm` | `LlmClient` (process supervisor), `LlmPolisher`, the wire `protocol` | JSON lines over the worker's stdin and stdout |
| `tw-llm-worker` | llama.cpp via `llama-cpp-2` | Separate process: both whisper.cpp and llama.cpp embed ggml and can't be linked into one binary |
| `tw-os` | `keys`, `insert`, `permissions`, `apps`, `autostart` | macOS primary; Windows and X11 behind `cfg` |

## The app (`src-tauri/src`)

| Module | Job |
| --- | --- |
| `lib.rs` | `run()` and `setup()`: loads data, builds every service, wires them into `AppState`, spawns threads, creates windows |
| `state.rs` | `AppPaths`, `Data` (everything persisted) and `AppState` (everything commands share) |
| `store.rs`, `settings_store.rs` | Atomic JSON persistence |
| `commands/` | One module per concern: `settings`, `hotkeys`, `permissions`, `audio`, `models`, `dictation`, `files`, `history`, `text`, `app`. Registered in `commands/mod.rs` |
| `events.rs` | Event names and the `emit` helper |
| `error.rs` | `CommandError { kind, message }` |
| `dictation/` | The runtime: `runtime.rs` (thread and effects), `pipeline.rs` (audio to text), `engine.rs` (Whisper cache), `polish.rs` (LLM), `audio.rs` (audio thread), `desktop.rs` (real OS), `smoke.rs` (debug scripted run) |
| `listener.rs` | Starts the key listener once the permission it needs is granted, restarts it, reports health |
| `windows.rs`, `pill.rs` | Window creation and routing; the pill's placement, visibility and click-through |
| `tray.rs`, `app_menu.rs` | Menu bar icon and menu; the macOS app menu's "Settings…" |
| `files.rs` | Audio file transcription jobs |
| `models.rs` | Download orchestration over `tw-models` |
| `network_log.rs` | Persists the download requests |
| `sink.rs`, `sound.rs` | `Sink` implementation (events to windows); start and stop sounds |

## The windows

| Window | Entry | Purpose |
| --- | --- | --- |
| `pill` | `pill.html` → `src/windows/pill` | Transparent, always on top, no focus, no Dock entry, on all Spaces. 360 × 280 logical pixels, much bigger than the capsule: hints, notices and the menu are drawn above it. Clicks outside the shapes the page reports with `set_pill_hit_regions` pass through. |
| `onboarding` | `onboarding.html` → `src/windows/onboarding` | First-run steps. Its progress is saved in `settings.onboardingStep`. |
| `main` | `index.html` → `src/windows/main` | Home, Dictionary, Snippets, Style, Models, Settings. Hash routes such as `#/settings/privacy`. |

`tauri.conf.json` declares no windows; `windows.rs` creates them at startup. Closing the last window doesn't exit: the tray keeps the app alive. `RunEvent::Reopen` (Dock click) reopens a window. A second launch focuses the first (single-instance plugin).

## One dictation, end to end

```text
 key event (tw-os listener thread)
   │  Msg::Key
   ▼
 tw-dictation thread ── Matcher ──► HotkeySignal ──► Event ──► step(State, Event)
   ▲                                                              │ (State, [Effect])
   │ Msg::Event / Captured / Processed                            ▼
   │                                              perform(Effect):
   │                                                StartCapture  → audio thread opens the mic
   │                                                              → voice model loads while you speak
   │                                                FinishCapture → audio thread returns the Recording
   │                                                              → spawn "tw-pipeline" thread
   │                                                Insert        → spawn insert thread (tw-os)
   │                                                RecordHistory → Data.history
   │                                                Emit(state)   → Sink → `dictation:state` event → pill
   │
 tw-pipeline thread:  vad::has_speech → Whisper → clean_transcript → frontmost app
                      → rule-based formatting → (LLM polish) → dictionary → snippets → style
```

The commands (`start_dictation`, `cancel_dictation`, `undo_cancel` …) don't do any of this themselves: they send an `Event` to the same runtime, so keys, clicks and commands all go through one path. See [Dictation runtime](dictation-runtime.md) for the state machine and [Text pipeline](text-pipeline.md) for the last box.

## Threads

| Thread | Owns |
| --- | --- |
| main | Tauri event loop, windows |
| `tw-dictation` | The state machine state, the timers, the effects |
| audio thread (`dictation/audio.rs`) | The microphone stream; blocking device open and stop |
| `tw-pipeline` (per job) | One audio buffer through to text; cancellable with an `AtomicBool` |
| insert thread (per job) | Blocking insertion, which waits for modifier keys to be released |
| `tw-permissions` | Polls permissions (2 s while something is missing, 10 s once settled), starts and restarts the key listener |
| listener thread (`tw-os`) | The OS event tap or hook |
| Whisper idle check | Every minute, drops the voice model after 10 idle minutes |
| download tasks | Model downloads |
| pill tracker (`pill.rs`) | Follows the cursor so the pill sits on the right display and passes clicks through |
| file job thread | One audio file at a time |

Whenever a `Mutex` is held across data that other threads read, use `state::lock`, which recovers from poisoning instead of panicking.

## Where to change what

| I want to… | Start in |
| --- | --- |
| Change how a key press behaves | `tw-core/src/dictation/machine.rs`, then its tests |
| Add a shortcut or change validation | `tw-core/src/hotkey.rs` |
| Change how text is cleaned | `tw-core/src/text/` ([Text pipeline](text-pipeline.md)) |
| Add a setting | `tw-core/src/settings.rs`, `src/lib/ipc/types.ts` and `defaults.ts`, the settings page |
| Add or change a command or event | [IPC](ipc.md) |
| Add a model to the catalog | `tw-core/src/models.rs` ([Models](models-and-llm.md#the-catalog)) |
| Support another OS feature | `tw-os` ([OS integration](os-integration.md)) |
| Add a page to the main window | [Frontend](frontend.md#adding-a-page) |
