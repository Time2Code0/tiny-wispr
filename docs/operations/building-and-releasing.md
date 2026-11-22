# Building and releasing

## Release build

```sh
pnpm tauri build --bundles app
```

The output is `target/release/bundle/macos/Tiny Wispr.app`. Use `--bundles app` when nobody is at the machine: the default also builds a DMG, and that step drives Finder through AppleScript, which prompts for automation permission.

What the build does, in order (`src-tauri/tauri.conf.json`):

1. `beforeBuildCommand`: `node scripts/build-worker.mjs --release && pnpm build`. Builds `tw-llm-worker` in release mode and copies it to `src-tauri/binaries/tw-llm-worker-<target triple>`, then builds the frontend into `dist/` (three entries: `index.html`, `onboarding.html`, `pill.html`).
2. Cargo builds the app (`lto = "thin"`, `codegen-units = 1`, `strip = "debuginfo"`; slow, compiles whisper.cpp and llama.cpp).
3. Tauri bundles the app and the worker sidecar (`bundle.externalBin`) into `Contents/MacOS/`, with `Info.plist` and the entitlements.
4. The bundle is signed.

`pnpm build:worker` on its own builds the debug worker, for working on the app without `tauri dev`.

## What's in the bundle

| Item | Where it comes from |
| --- | --- |
| The app | `src-tauri`, `productName` "Tiny Wispr", identifier `dev.time2code.tinywispr`, category Productivity |
| `tw-llm-worker` | Sidecar, next to the app binary. The app finds it as `current_exe().parent()/tw-llm-worker`. |
| `NSMicrophoneUsageDescription` | `src-tauri/Info.plist`: the text macOS shows when it asks for the microphone |
| `com.apple.security.device.audio-input` | `src-tauri/Entitlements.plist` |
| Minimum macOS | 13.0 (`bundle.macOS.minimumSystemVersion`) |
| Icons | `src-tauri/icons/` (from `src-tauri/app-icon.svg`) |
| Models | **Not bundled.** The user downloads them. |

The CSP in `tauri.conf.json` allows only the app's own files and Tauri IPC (`connect-src ipc: http://ipc.localhost`): the webviews can't make network requests. Keep it that way; downloads happen in Rust.

`build.rs` links clang's runtime on macOS: whisper.cpp's Metal backend uses `@available`, which needs `__isPlatformVersionAtLeast`, and rustc links with `-nodefaultlibs`. If the link fails on that symbol after a toolchain change, look there first.

## Signing

Where things stand:

- **Dev builds** (`pnpm app:mac`) are signed with a self-signed *Tiny Wispr Development* certificate, so macOS keeps permission grants across rebuilds. See [Development](development.md#why-appmac-makes-a-certificate).
- **Release builds** are signed ad hoc (`signingIdentity: "-"`). An ad hoc signature changes with every build, so a user who updates loses the macOS permission grants, and Gatekeeper warns when opening the app.
- **Notarization isn't set up.** A build distributed outside your own machine needs a Developer ID certificate, hardened-runtime signing with the entitlements above, and notarization (`xcrun notarytool`) before people can open it without a warning. Until then, tell testers to right-click → Open the first time.
- Signing the worker: the sidecar must be signed with the same identity as the app, and a notarized bundle needs it signed with the hardened runtime too. Check `codesign --verify --deep --strict "Tiny Wispr.app"` after any signing change.

Whatever the identity is, **keep it the same from release to release**: a changing signature is what makes users re-grant Accessibility and Input Monitoring.

## Versions

The version is in three places that must agree: `version` in `package.json`, `version` in `src-tauri/tauri.conf.json`, and `[workspace.package] version` in the root `Cargo.toml`. The About page shows the `package.json` one. There is no updater and no automatic update check, by design: the app makes no network requests apart from model downloads.

## Windows and Linux

The code has Windows and Linux (X11) implementations behind `cfg` ([OS integration](../internals/os-integration.md#platform-status)) and `bundle.targets` is `"all"`, but only macOS is built and tested regularly. Before shipping another platform:

- Build on that platform (the sidecar script uses the host's target triple; there is no cross compile).
- Run the smoke run and the full test suite there.
- Check each row of the platform table in [OS integration](../internals/os-integration.md#platform-status) by hand with a real keyboard and a real second app: the key listener and insertion can't be unit-tested.
- Linux needs the Tauri system libraries (`webkit2gtk`, etc.) plus X11 dev libraries; Wayland sessions have no global shortcuts or paste.

## Release checklist

1. `pnpm verify` is green on a clean checkout (and on the release platform).
2. Smoke run with a real voice model, and once with a language model selected.
3. Bump the version in the three places above.
4. `pnpm tauri build --bundles app`.
5. Open the built app **from the bundle** on a machine or user account that has never run it: onboarding appears, each permission prompt names Tiny Wispr, the first dictation works after downloading a model.
6. Check Settings → Privacy → Network activity lists only the model download you did.
7. Tag the commit.
