# Tiny Wispr

A local, private dictation app: hold a key, speak, release, and the text is transcribed on this device and pasted into the focused app. No account, no cloud, no telemetry. The only network requests are model downloads the user asked for.

Tauri v2 app. The Rust workspace and the Vite + React frontend both live at the repo root.

- `crates/tw-core/` is pure Rust with no native dependencies: settings, hotkey model and validation, the dictation state machine (`(State, Event) -> (State, Vec<Effect>)`), the text pipeline, history and stats, the model catalog, LLM prompt building. Its types are the IPC contract.
- `crates/tw-audio/` (capture, levels, speech detection), `tw-stt/` (whisper.cpp), `tw-models/` (downloads, inventory), `tw-llm/` (protocol and client for the worker), `tw-llm-worker/` (llama.cpp in its own process, because whisper.cpp and llama.cpp both embed ggml), `tw-os/` (global keys, insertion, permissions, frontmost app, launch at login).
- `src-tauri/` is the app: windows (`pill`, `onboarding`, `main`), tray, persistence, one command module per concern in `src-tauri/src/commands/`, and the dictation runtime in `src-tauri/src/dictation/`, which feeds key, command and timer events into the state machine and carries out its effects (capture, Whisper, text pipeline, polish, insertion).
- `src/` is the React app. Three HTML entries (`index.html`, `pill.html`, `onboarding.html`) load `src/windows/{main,pill,onboarding}/`. Shared code is in `src/components` (registry copies in `src/components/ui`), `src/hooks`, `src/lib` and `src/styles`.

## Docs

`docs/` has user guides (`docs/user/`), architecture notes for developers and agents (`docs/internals/`) and runbooks (`docs/operations/`); start at `docs/README.md`. Read `docs/internals/architecture.md` before a change that crosses crates. When a change alters behaviour, a default, a shortcut, a limit, a command or an event, update the page that describes it in the same commit. Where the code has a feature that isn't wired up yet, the docs say so.

## Commands

- `pnpm dev` runs the frontend alone in a browser on port 1420, against the mock backend.
- `pnpm tauri dev` runs the app.
- `pnpm check` (Biome via ultracite, then `tsc`), `pnpm fix` to format, `pnpm test` (vitest).
- `pnpm check:rust` builds the language model worker, then runs `cargo fmt --check`, `clippy -D warnings` and `cargo test` for the workspace. It needs the `clippy` and `rustfmt` toolchain components.
- `pnpm build:worker` builds `tw-llm-worker` and copies it to `src-tauri/binaries/`, where Tauri picks it up as a sidecar. `pnpm tauri dev` and `pnpm tauri build` do this themselves.
- `pnpm tauri build --bundles app` builds the release app.
- `pnpm app:mac` builds the debug app bundle and opens it. It signs with a self-signed "Tiny Wispr Development" certificate, which it creates in the login keychain on first run, so macOS keeps the Microphone, Accessibility and Input Monitoring grants across rebuilds. An ad hoc signature changes with every build and loses them.
- `pnpm verify` runs all of the above plus `pnpm build`.

whisper.cpp and llama.cpp build with CMake, so `cmake` must be on `PATH` before any cargo command.

## Testing without a microphone

Tests never open the microphone, create an event tap or prompt for a permission. Speech comes from files instead:

- `scripts/make-speech-fixture.sh "<text>" <out.wav>` makes a 16 kHz WAV with `say` and `ffmpeg`.
- `TINY_WISPR_TEST_MODELS=<dir>` turns on the tests that need real models (`<dir>/whisper/ggml-tiny.en.bin`, `ggml-base.bin`, and the catalog's GGUF files in `<dir>/llm/`). Without it they skip.
- `TINY_WISPR_NETWORK_TESTS=1` turns on the catalog URL check and a real resumed download.
- Debug builds have a scripted run of one dictation through the real runtime: `TINY_WISPR_SMOKE=<wav> TINY_WISPR_SMOKE_OUT=<txt> TINY_WISPR_MODELS_DIR=<dir> TINY_WISPR_DATA_DIR=<dir> target/debug/tiny-wispr`. It writes the inserted text to the out file and exits with 0. Put `{"llmModel":"<catalog id>"}` in `<data dir>/settings.json` to include polish.
- `cargo run -p tw-llm --example polish_eval` scores a language model on 26 dictated transcripts.

## IPC

The contract is defined once in Rust (`tw-core` types, `src-tauri/src/{error,events,windows}.rs`) with `serde(rename_all = "camelCase")` and mirrored by hand in `src/lib/ipc/types.ts`. Change both sides in the same commit.

The frontend only talks to the backend through `src/lib/ipc`: typed command functions and `on*()` event helpers. When `window.__TAURI_INTERNALS__` is missing (a plain browser), it uses the in-memory mock in `src/lib/ipc/mock.ts`, which simulates dictation, audio levels, downloads, permissions and history. Drive it from the browser console with `window.__tinyWisprMock.setPhase("listening")`, `.notice("cancelled")` or `.level(0.8)`. When a command gains behaviour in Rust, make the mock behave the same way.

Read shared state with the hooks in `src/hooks` (`useSettings`, `useDictationState`, `useIpcQuery`, `useIpcEvent`, `useAudioLevels`). Don't add a state library.

## Conventions

- pnpm only. TypeScript strict, no `any`, `@/*` maps to `src/*`.
- UI comes from time2code/ui (Base UI + Tailwind v4). Read `~/dev/ui/skills/time2code-ui/SKILL.md` and the component's guide before using a component. `render` instead of `asChild`, `*Popup` instead of `*Content`, icons from `@hugeicons/core-free-icons` through `Icon`, `cn()` from the `cn` package.
- Use the tokens (`bg-background`, `bg-background-muted`, `bg-accent`, `text-foreground`, `text-foreground-muted`, the status tokens with `-foreground`, `-background` and `-subtel`). No `dark:` color classes, no raw palette classes, no new colors, radii or spacing values. Plain `border`, logical spacing (`ms-*`, `ps-*`), `gap-*` instead of `space-y-*`, `size-*` for squares.
- Motion uses `motion/react` and respects `prefers-reduced-motion`.
- Rust: edition 2024, clippy clean with `-D warnings`, `cargo fmt`, `thiserror` in libraries and `anyhow` at the app edge, no `unwrap()` outside tests (enforced by `clippy::unwrap_used`).
- Comment only where the code would look wrong without one.
- Interface copy is short, plain and in sentence case, with no exclamation marks.
- Never request an OS permission unless the user clicked something that asks for it.
- Conventional commits in lowercase, one change per commit: `feat: add push-to-talk hotkey listener`.

## Adding a registry component

The hosted registry isn't reachable from here, so copy from the local checkout of `~/dev/ui`:

1. Copy `~/dev/ui/site/src/registry/ui/<name>.tsx` to `src/components/ui/`, and any hooks it uses from `site/src/registry/hooks/` to `src/hooks/`.
2. Rewrite `@/registry/ui/x` to `@/components/ui/x` and `@/registry/hooks/x` to `@/hooks/x`. Replace any `next/*` import with a plain equivalent.
3. Look up the item in `site/src/registry/ui/registry.json`: copy its `registryDependencies` the same way and add its npm `dependencies`.
4. Run `pnpm check`. If a copied file breaks a lint rule, make the smallest fix (a `biome-ignore` with a reason) instead of restyling it.
