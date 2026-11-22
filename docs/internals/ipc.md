# IPC

The frontend and backend talk through Tauri commands (request/response) and events (backend → frontend). The contract is defined **once in Rust** and mirrored **by hand in TypeScript**. Change both sides in the same commit.

| Side | Where |
| --- | --- |
| Rust types | `crates/tw-core/src/*` (shared domain types), `src-tauri/src/{error,events,windows}.rs`, and payload types next to the commands that use them |
| Rust commands | `src-tauri/src/commands/*.rs`, registered in `commands/mod.rs` |
| TypeScript types | `src/lib/ipc/types.ts`: domain types, the `Commands` map, the `Events` map |
| TypeScript API | `src/lib/ipc/index.ts`: one typed function per command, `on()` for events |
| Real backend | `src/lib/ipc/tauri.ts` |
| Mock backend | `src/lib/ipc/mock.ts` (and `mock-*.ts` helpers) |
| Defaults | `src/lib/ipc/defaults.ts`, mirrors `Default` impls in Rust |

All serde types use `#[serde(rename_all = "camelCase")]`, enums included. Enums with data are tagged (`ModelStatus` is `{ "state": "downloading", "received": 1, "total": 2 }`). Timestamps are `Millis` (`i64` milliseconds since the epoch).

## Rules

1. **The frontend only talks to the backend through `src/lib/ipc`.** No `invoke` or `listen` from `@tauri-apps/api` anywhere else.
2. **When there's no Tauri, there's the mock.** `index.ts` picks the backend with `isTauri` (`window.__TAURI_INTERNALS__`). When a command gains behaviour in Rust, make the mock behave the same way.
3. **No state library.** Read shared state with the hooks in `src/hooks`: `useSettings`, `useDictationState`, `useIpcQuery`, `useIpcEvent`, `useAudioLevels`.
4. Commands are thin. They validate, call into a service or the runtime, and return. Logic goes in `tw-core` (pure) or a service so it can be tested without Tauri.

## Errors

Every command returns `CommandResult<T>` = `Result<T, CommandError>`. `CommandError` serialises as `{ kind, message }`; the frontend wraps it in `IpcError` (`src/lib/ipc/errors.ts`).

`ErrorKind`: `notImplemented`, `invalidInput`, `notFound`, `permissionMissing`, `noMicrophone`, `microphoneBusy`, `permissionDenied`, `offline` (Stay offline refused a download or an update check), `io`, `internal`. The UI branches on `kind` and shows `message` (which is written for people, in the product's voice). Library errors convert with `From` impls (`ModelsError` → `CommandError` in `models.rs`).

## Commands

| Group | Commands |
| --- | --- |
| Settings | `get_settings`, `update_settings(patch)`, `reset_shortcuts` |
| Shortcuts | `validate_hotkey`, `start_hotkey_capture`, `stop_hotkey_capture`, `get_listener_status` |
| Permissions | `get_permissions`, `request_permission`, `open_permission_settings` |
| Audio | `list_microphones`, `start_mic_test`, `stop_mic_test` |
| Models | `list_models`, `download_model`, `cancel_download`, `delete_model`, `import_model`, `get_hardware_info`, `get_recommendation` |
| Dictation | `start_dictation`, `stop_dictation`, `cancel_dictation`, `undo_cancel`, `paste_last_transcript`, `copy_last_transcript`, `get_dictation_state` |
| Files | `transcribe_file`, `cancel_file_transcription`, `get_file_transcription`, `save_file_transcript` |
| History | `list_history(cursor, query)`, `delete_history_entry`, `clear_history`, `get_stats` |
| Text | `list_dictionary`, `upsert_dictionary_entry`, `delete_dictionary_entry`, `list_snippets`, `upsert_snippet`, `delete_snippet`, `test_polish` |
| App | `complete_onboarding`, `open_main_window(route)`, `hide_pill_for(minutes)`, `set_pill_hit_regions(rects)`, `list_network_log` |
| Updates | `get_update_state`, `check_for_updates`, `download_update`, `install_update` ([Updates](updates.md)) |

`update_settings` takes a `SettingsPatch`: absent fields are left alone, and for nullable settings an explicit `null` clears the value (`Option<Option<T>>` with a `double_option` deserializer). It registers or removes the login item when `launch_at_login` changes, then `save()` persists, emits `settings:changed`, and applies what takes effect immediately: pill visibility, the swallowed shortcuts (`Msg::SettingsChanged`), the tray's microphone list, Dock visibility, pausing downloads when `offline` turns on, and history retention. It does **not** validate shortcuts: the recorder calls `validate_hotkey` first.

## Events

Names are constants in `src-tauri/src/events.rs` (`files:progress` in `files.rs`), payload types in the `Events` map in `types.ts`.

| Event | Payload | Sent when |
| --- | --- | --- |
| `dictation:state` | `DictationState` | Every state change (`Effect::Emit`) |
| `audio:level` | `{ level }` (smoothed RMS, 0..1, about 30 Hz) | While listening or testing the microphone |
| `models:progress` | `ModelProgress` | During a download, throttled to 100 ms |
| `models:changed` | none | A model was added, removed or finished |
| `settings:changed` | `Settings` | After `update_settings` or `reset_shortcuts` |
| `permissions:changed` | `Permissions` | The permission poll saw a change |
| `listener:status` | `ListenerStatus` | The key listener's health changed |
| `history:changed` | none | An entry was added or deleted |
| `hotkey:captured` | `HotkeyCapture { hotkey, done }` | While the shortcut recorder is open |
| `files:progress` | `FileProgress` | During a file transcription |
| `updates:state` | `UpdateState` | Every updater state change; during a download, throttled to 100 ms |
| `window:navigate` | `Navigate { route }` | To the main window only: switch page |

The pill gets its own `dictation:state` stream: the machine's `Emit` goes through `AppSink` to all windows. `audio:level` is emitted at 30 Hz, so subscribe with `useAudioLevels`, which keeps it out of React state.

## Adding a command

1. **Rust.** Write the function in the right `commands/<concern>.rs` module with `#[tauri::command]`. Take `State<'_, AppState>` for services. Return `CommandResult<T>`. Add it to `tauri::generate_handler![…]` in `commands/mod.rs`.
2. **Types.** If it uses a new type, put the type in `tw-core` when it's domain data, with `rename_all = "camelCase"`. Mirror it in `types.ts`.
3. **TypeScript.** Add the command to the `Commands` map in `types.ts` (`name: { args: …; result: … }`; `args: undefined` for none) and a typed function in `index.ts`.
4. **Mock.** Add a handler to `mock.ts` that behaves the same way, including errors. Keep the mock's rules (limits, validation messages) equal to Rust's: copy the constants.
5. **Test.** Pure logic in `tw-core` tests; mock behaviour in `src/lib/ipc/mock-*.test.ts`.
6. Commands need no capability entry (`src-tauri/capabilities/*.json` covers plugin and core permissions only), but a new plugin does.

## Adding an event

Add the name constant in `events.rs`, a payload type, call `events::emit(&app, NAME, payload)`, add it to the `Events` map in `types.ts`, make the mock send it, and consume it with `useIpcEvent` or as a `useIpcQuery` refetch trigger.

## The mock

`window.__tinyWisprMock` (browser only) drives the mock from the console:

```js
__tinyWisprMock.setPhase("listening", { mode: "handsFree", elapsedMs: 60000 })
__tinyWisprMock.setPhase("processing", { stage: "polishing" })
__tinyWisprMock.notice("cancelled")          // a pill notice
__tinyWisprMock.level(0.8)                   // fixed input level, null for simulated speech
__tinyWisprMock.listener("secureInput")      // listener banner states
__tinyWisprMock.micTestFails("noMicrophone")
__tinyWisprMock.fileFails("noSpeech")
__tinyWisprMock.settings({ offline: true })
__tinyWisprMock.update("ready")              // updater states, with a sample release
__tinyWisprMock.updateOutcome("upToDate")    // what the next check finds: available, upToDate, checkFails, downloadFails
__tinyWisprMock.navigate("settings")
```

Each window (tab) has its own mock state; settings persist in `localStorage` under `tinyWispr.mock.settings`. The pill page also has a harness: open `/pill.html?controls` for buttons that set the phases.
