# OS integration

`crates/tw-os` is everything that talks to the operating system on behalf of dictation: the global key listener, text insertion, permissions, the frontmost app and launch at login. One API per concern, with a `macos/`, `windows/` and `linux/` implementation behind `cfg`.

Two rules from the crate's docs:

- **Nothing here may trigger an OS permission prompt unless the user asked for it.** `permissions::status` and `permissions::all` only read. `permissions::request` is the one function that prompts, and only a click handler may call it.
- **macOS is the primary target.** Windows and Linux (X11) are implemented, but must at least never break the macOS build, and are less exercised. Check before relying on them for a feature.

## Platform status

| Capability | macOS | Windows | Linux X11 | Linux Wayland |
| --- | --- | --- | --- | --- |
| Global keys | CGEventTap ×2 | `WH_KEYBOARD_LL` hook | XInput2 raw events + passive grabs | Not possible (`Unsupported`) |
| Insert (paste, type) | Pasteboard + posted key events | Win32 clipboard + `SendInput` | `arboard` + XTEST | Not possible (`Unsupported`) |
| Frontmost app | NSWorkspace | Foreground window exe | `_NET_ACTIVE_WINDOW`, `WM_CLASS` | `None` |
| Permissions | Microphone, Accessibility, Input Monitoring | None (microphone toggle in settings) | None | None |
| Launch at login | `SMAppService.mainApp` (needs a bundled `.app`) | `Run` registry key | XDG autostart entry | same |

On Wayland the app still works from the pill: click it to start hands-free, click stop. `ListenerState::Unsupported` tells the UI, which explains it in a banner.

## Keys (`keys/`)

```text
KeyListener::start(callback)  →  callback(ListenerEvent::{Key, Health, Interrupted})
KeyListener::set_swallowed(combos)   hide these combos from the focused app
KeyListener::set_capture(bool)       report and hide every key (the shortcut recorder)
```

A `KeyEvent` is one transition with the full modifier and key set held at that moment, including modifier-only transitions and `fn` on macOS. Left and right modifiers are merged: a modifier is down when its first side goes down and up when its last side comes up. The callback runs on the listener's thread inside the OS hook: **return quickly** (send to a channel), or the OS disables the hook. The app sends to the runtime's `Msg` channel.

Shared pieces:

- `keys/codes.rs`: scan codes ↔ the W3C `KeyboardEvent.code` names that `tw-core::KeyCode` uses. Shortcuts are physical and layout-independent.
- `keys/engine.rs`: the platform-independent part: tracks what's held, decides what to swallow, and keeps menu-mask bookkeeping.
- Every event the crate posts itself is marked (`SYNTHETIC_EVENT_MARKER` on macOS, `dwExtraInfo` on Windows) so the listener lets its own paste keystrokes pass unreported.

**macOS details.**

- Two taps on one run-loop thread: key presses go through an *active* tap so swallowed combos can be dropped, while modifier changes go through a tap that only listens, unless `fn` is bound on its own. An active tap on flags-changed events, even one that returns them unchanged, breaks Finder's Option-key menu alternates.
- Modifier-only bindings are never swallowed, but their system action is suppressed: a bare `fn`/Globe press on macOS (applied within half a second), the Start menu and menu bar on Windows.
- `ListenerHealth::SecureInput`: Secure Event Input is on (a password field, or a terminal's Secure Keyboard Entry). Key presses are hidden from every app; modifier changes still arrive. `PermissionRevoked`: the permission the listener started with was taken away.
- `ListenerEvent::Interrupted`: macOS disables a slow tap. The listener re-enables itself and reports releases for keys let go meanwhile; presses in the gap are lost.
- Accessibility covers the active tap *and* posting keystrokes. Input Monitoring alone allows an observe-only listener that cannot swallow or insert.

**Lifecycle in the app** (`src-tauri/src/listener.rs`): the `tw-permissions` thread polls `permissions::all()` (2 s while something is missing, 10 s once all is granted so a revocation is noticed), emits `permissions:changed`, and starts the listener when its permission is there and restarts it if it stopped. Health is mapped to `ListenerState` (`starting`, `waitingForPermission`, `running`, `secureInput`, `unsupported`, `failed`) and sent as `listener:status`.

## Insertion (`insert.rs`)

`insert::insert(text, &InsertOptions)` puts text in the focused field. It **blocks**: call it off the runtime thread (the runtime does). It fails with `PermissionMissing(Accessibility)` on macOS without the permission, and `Unsupported` on Wayland, and never prompts.

Paste method:

1. Wait up to `MODIFIER_RELEASE_TIMEOUT_MS` (1 s) for the dictation hotkey's modifiers to be let go. Otherwise `⌃⌘V` would arrive with `fn` still down.
2. Save the clipboard. Write the text. On macOS add the `org.nspasteboard.ConcealedType` and `TransientType` markers so clipboard managers skip it.
3. Post the paste keystroke. On macOS the "V" key code is looked up from the *current keyboard layout* (`refresh_keyboard_layout()` at startup), so Dvorak and AZERTY work.
4. Restore the previous clipboard after `CLIPBOARD_RESTORE_MS` (500 ms) on a background thread, if `restore_clipboard` is on.

Type method: send the text as keystrokes. macOS Quartz keeps at most 20 UTF-16 units of a keyboard event's string, so text is chunked (`MAX_UNICODE_UNITS`), and shortcut events are spaced 15 ms apart because remote desktop and VM clients read a burst posted in one instant as the bare key.

If insertion fails, the machine raises the "Couldn't paste. Press ⌃⌘V" notice and keeps the text for paste-last.

## Permissions (`permissions.rs`)

`status(kind)`, `all()`, `request(kind)` and `open_settings(kind)`. The types (`PermissionKind`, `PermissionStatus`, `Permissions { microphone, accessibility, input_monitoring }`) are in `tw-core`.

`request` returns at once; poll `status` for the answer. macOS only prompts the first time: afterwards the UI offers `open_settings`, which deep-links to the System Settings pane (`macos_settings_url`). Where a permission doesn't exist (Windows, Linux) it reads as granted.

The microphone is checked separately in the pipeline: if the samples are all zeros and `Desktop::microphone_denied()` is true the dictation fails with `MicrophoneDenied` ("Microphone access is off") rather than "Didn't catch that".

## Frontmost app (`apps.rs`)

`apps::frontmost()` returns `AppInfo { name, id, category }`: the id is the macOS bundle id, the Windows exe name or the X11 `WM_CLASS`. It needs no permission. The category comes from the `APPS` table, which lists known ids per `AppCategory` (personal messaging, work messaging, email, code, documents, browser), lowercased and covering every OS's spelling of the same app. Unknown apps are `Other`. `AppCategory::style_category()` collapses the six into the four `StyleCategory` values the user can style.

To teach it a new app, add its ids (all platforms) to `APPS`, and a test.

## Launch at login (`autostart.rs`)

`set_enabled(bool)` / `is_enabled()`. On macOS `SMAppService.mainApp` registers the *bundle*, so enabling it from a plain `cargo run` binary fails; test it with `pnpm app:mac`. `is_enabled` reports false while the login item awaits approval in System Settings; when `set_enabled(true)` leaves it awaiting approval, it opens Login Items in System Settings and returns an error asking the user to allow it. Turning it off when nothing is registered is a no-op.

The app treats the OS as the source of truth: at startup `sync_launch_at_login` writes the login item's state into `settings.launchAtLogin`, and `update_settings` compares a requested change with `is_enabled()` rather than the saved value, since the user can add or remove the item in System Settings.

## FFI and `unsafe`

The workspace denies `unsafe_op_in_unsafe_fn`. macOS FFI is gathered in `macos/ffi.rs`. Keep `unsafe` blocks small, with the invariant stated next to them, and keep raw OS types out of the public API: the crate's boundary is the types in `tw-core`.

## Adding a platform feature

1. Define the API in the shared file (`insert.rs`, `apps.rs`…) and a `platform::` function for each OS.
2. Implement macOS. For Windows and Linux either implement it or return `OsError::Unsupported`; a missing implementation must not stop the crate compiling.
3. Test the platform-independent part (`keys/engine.rs`, `insert.rs` option handling) with unit tests that need no OS.
4. Never prompt. If a feature needs a permission, add it to `PermissionKind` and the onboarding step instead.
