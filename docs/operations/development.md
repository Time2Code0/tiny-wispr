# Development

## Prerequisites

| Tool | Why |
| --- | --- |
| [Rust](https://rustup.rs) 1.95+ with the `clippy` and `rustfmt` components | The workspace is edition 2024, and `pnpm check:rust` runs both |
| [pnpm](https://pnpm.io) and Node.js 24 | The frontend. **pnpm only**: don't use npm or yarn |
| CMake | whisper.cpp and llama.cpp build with it. It must be on `PATH` before **any** cargo command. `brew install cmake` on macOS |
| Xcode command line tools (macOS) | The compiler, the SDK, `codesign` |
| `ffmpeg` (optional) | Only for `scripts/make-speech-fixture.sh`, with macOS `say` |

The first cargo build compiles whisper.cpp and llama.cpp and takes several minutes. Dev profile dependencies are built with `opt-level = 2` (see the root `Cargo.toml`): both libraries are unusably slow unoptimised, even in dev.

```sh
pnpm install
```

## Running

| Command | Does |
| --- | --- |
| `pnpm dev` | The frontend alone in a browser at <http://localhost:1420>, against the mock backend. Fast. Open `/`, `/onboarding.html` or `/pill.html`. |
| `pnpm tauri dev` | The app. Builds the worker sidecar first, then runs Vite and the Rust app with hot reload for the frontend. |
| `pnpm app:mac` | Builds a **debug app bundle**, *Tiny Wispr Dev*, and opens it, signed with a stable self-signed certificate. |
| `pnpm build:worker` | Builds `tw-llm-worker` and copies it to `src-tauri/binaries/` |

### Which one to use

Use `pnpm dev` for UI work. The mock backend simulates dictation, audio levels, downloads, permissions and history, and you can drive it from the console ([IPC](../internals/ipc.md#the-mock)). No permissions, no native build, no models.

Use `pnpm tauri dev` when you change Rust or need the real thing. Under `tauri dev` the binary isn't an app bundle, so macOS names **the app that started it** (your terminal or editor) in the Microphone, Accessibility and Input Monitoring prompts and ties the grants to that. Granting your terminal access is enough for development.

Use `pnpm app:mac` when you need macOS to see *Tiny Wispr itself*: permissions prompts, launch at login (which needs a bundle), the menu bar icon, Dock behaviour.

### Next to the installed app

Every debug build (`pnpm tauri dev`, `pnpm app:mac`, `target/debug/tiny-wispr`) is **Tiny Wispr Dev**, so you can keep using a released Tiny Wispr every day while you work on the next one. `src-tauri/src/build_flavor.rs` sets this up at startup, and `pnpm app:mac` bundles with `src-tauri/tauri.dev.conf.json`, which sets the same name and identifier plus an orange icon.

| | Installed app | Development build |
| --- | --- | --- |
| Identifier | `dev.time2code.tinywispr` | `dev.time2code.tinywispr.dev` |
| Data folder (settings, history, dictionary, snippets) | Its own | Its own, so onboarding runs once |
| Models | Its own `models/` | **The installed app's** `models/`, so nothing is downloaded twice. Deleting a model in one deletes it in both. `TINY_WISPR_MODELS_DIR` overrides it. |
| Permissions, single instance, launch at login | Its own | Its own, because they follow the identifier |
| Updates | From GitHub releases | Never, unless `TINY_WISPR_UPDATE_URL` is set |
| Default shortcuts | fn, fn Space, fn ⌃, ⌃⌘V on macOS | ⌥⌘, ⌥⌘., ⇧⌥⌘, ⌃⇧V on macOS; Ctrl+Shift, Ctrl+Shift+Space, Ctrl+Shift+Alt, Alt+Shift+X elsewhere |
| Menu bar icon | The bars | The bars and a dot |

The development shortcuts (`Shortcuts::development`) don't contain or sit inside any release default, including the ones for Macs without an Fn key, so one press never starts both apps. Escape cancels in both, which is harmless because only the app that is dictating reacts to it. If you change a shortcut in one app, keep it apart from the other's.

### Why `app:mac` makes a certificate

macOS ties Microphone, Accessibility and Input Monitoring grants to the app's code signature. An ad hoc signature changes with every build, so every rebuild loses the grants (and System Settings can show Tiny Wispr as allowed while the grant no longer applies). `scripts/app-mac.mjs` creates a self-signed **"Tiny Wispr Development"** certificate in your login keychain the first time and signs with it, which keeps the signature, and the grants, stable across rebuilds.

If grants misbehave anyway: in System Settings → Privacy & Security remove Tiny Wispr Dev with the minus button, then grant it again. `tccutil reset Accessibility dev.time2code.tinywispr.dev` also clears one.

Grants given to `pnpm app:mac` builds from before the development identity are filed under the release identifier with the development certificate, and they don't match a release. Clear them once with `tccutil reset All dev.time2code.tinywispr` before granting the installed app.

## Checks

| Command | Does |
| --- | --- |
| `pnpm check` | Biome (through ultracite), then `tsc --noEmit` |
| `pnpm fix` | Formats and fixes what Biome can |
| `pnpm test` | Vitest |
| `pnpm check:rust` | Builds the worker, then `cargo fmt --check`, `clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` |
| `pnpm verify` | All of the above plus `pnpm build`. Run before you commit anything large. |

Clippy is strict: warnings are errors, and `clippy::unwrap_used` is denied workspace-wide (`unwrap()` is fine in tests). `unsafe_op_in_unsafe_fn` is denied. See [Testing](testing.md) for what the tests need.

## Conventions

The full list is in [AGENTS.md](../../AGENTS.md). The ones that bite:

- TypeScript strict, no `any`. Rust edition 2024, `thiserror` in libraries, `anyhow` at the app edge.
- Comment only where the code would look wrong without one.
- Interface copy is short, plain and in sentence case, with no exclamation marks.
- **Never request an OS permission unless the user clicked something that asks for it.**
- **IPC changes go in one commit, on both sides.**
- Conventional commits in lowercase, one change per commit: `feat: add push-to-talk hotkey listener`, `fix: …`, `docs: …`.

## Logs

`tracing` with `RUST_LOG`. The default filter is `info,whisper_rs=warn`. Useful ones:

```sh
RUST_LOG=debug pnpm tauri dev                  # includes every dictation event
RUST_LOG=tiny_wispr_lib=debug,tw_os=debug pnpm tauri dev
```

The runtime logs each state change at `info` (`dictation state … phase=…`), each transcription with the audio and processing time, and polish failures with their reason ("kept the rule-based text"). Those lines answer most "why did it do that" questions.

## Layout of the local environment

- `target/`, `dist/`, `node_modules/`, `src-tauri/gen/`, `src-tauri/binaries/` and downloaded models are git-ignored.
- A local `.afk/` folder may exist on a maintainer's machine for scratch: test models, project-local `cmake`, logs. It's excluded from git and nothing depends on it.
