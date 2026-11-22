# Tiny Wispr

Tiny Wispr is a small dictation app for macOS, Windows and Linux. Hold a key, speak, and release: your words are transcribed on your computer with Whisper and pasted into whatever app you're using.

Nothing you say leaves your computer. There's no account and no telemetry. The only network requests the app makes are the model downloads you ask for and update checks. An optional local language model can tidy up what you said.

## Prerequisites

- [Rust](https://rustup.rs) with the `clippy` and `rustfmt` components
- [pnpm](https://pnpm.io) and Node.js 24
- CMake, to build whisper.cpp and llama.cpp: `brew install cmake` on macOS

## Development

```sh
pnpm install
pnpm tauri dev    # run the app
pnpm dev          # UI only, in a browser at http://localhost:1420, with a mock backend
pnpm verify       # lint, type-check, test and build the frontend and the Rust workspace
pnpm tauri build --bundles app    # release build in target/release/bundle
```

On macOS the app asks for Microphone and Accessibility access during setup. Accessibility lets it notice your shortcut and paste into other apps.

Under `pnpm tauri dev` macOS names the app that started it (your terminal or editor) in those prompts, because the dev binary isn't an app bundle. Granting that app access works for development. To have macOS ask for Tiny Wispr itself, run the bundled app:

```sh
pnpm app:mac    # builds a debug bundle and opens it
```

The debug bundle is signed with a self-signed "Tiny Wispr Development" certificate that the script creates in your login keychain the first time. A signature that stays the same keeps macOS from asking for the permissions again after every rebuild. Release builds are still signed ad hoc: see [Building and releasing](docs/operations/building-and-releasing.md).

In browser-only mode, open `/`, `/onboarding.html` or `/pill.html`. You can drive the mock from the console with `window.__tinyWisprMock`.

## Documentation

The docs are in [`docs/`](docs/README.md):

- **[Using Tiny Wispr](docs/README.md#using-tiny-wispr):** setup, dictating, text styles, models, privacy, troubleshooting
- **[Internals](docs/README.md#internals):** architecture, the dictation runtime, the text pipeline, IPC, OS integration
- **[Operations](docs/README.md#operations):** development, testing, building and releasing

## License

MIT
