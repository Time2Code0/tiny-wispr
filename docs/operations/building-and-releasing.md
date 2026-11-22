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

| Build | Identity | Why |
| --- | --- | --- |
| Dev (`pnpm app:mac`) | Self-signed *Tiny Wispr Development*, made in your login keychain on first run | macOS keeps permission grants across rebuilds. See [Development](development.md#why-appmac-makes-a-certificate). |
| Local `pnpm tauri build` | Ad hoc (`signingIdentity: "-"`) | Fine for trying a build. Not for sharing: the signature changes every build. |
| Release (CI) | Self-signed *Tiny Wispr*, from the `MACOS_CERTIFICATE` secret | The same certificate signs every release, so users keep their grants across updates. |

There is no Apple Developer account, so releases aren't notarized. Gatekeeper blocks the first launch of a downloaded copy until the user clicks **Open Anyway** in System Settings → Privacy & Security. The release notes and [Getting started](../user/getting-started.md#install) say so. Updates installed by the app don't trigger it again: the app writes the new bundle itself, so it has no quarantine flag.

The sidecar must be signed with the same identity as the app. The release workflow checks that both `tiny-wispr` and `tw-llm-worker` show `Authority=Tiny Wispr` and that `codesign --verify --deep --strict` passes.

**Never change the release certificate.** macOS ties the Microphone, Accessibility and Input Monitoring grants to it; a release signed with a different one makes every user grant them again. It is valid for 20 years.

## Keys and secrets

Two keys, both made once on a maintainer's Mac and kept outside the repo (by default in `~/.tiny-wispr-release-keys/`, mode 700). **Keep a copy of every file there in a password manager.** Losing the certificate resets everyone's permissions on the next release; losing the updater key means installed apps can never update again and have to be reinstalled by hand.

| File | Made with | GitHub secret |
| --- | --- | --- |
| `release-certificate.p12` | `node scripts/make-release-cert.mjs` | `MACOS_CERTIFICATE` (base64 of the file) |
| `release-certificate.password` | the same | `MACOS_CERTIFICATE_PASSWORD` |
| `updater.key` | `tauri signer generate --ci -p <password> -w updater.key` | `TAURI_SIGNING_PRIVATE_KEY` (the file's contents) |
| `updater.key.password` | the same | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |
| `updater.key.pub` | the same | none: it's `plugins.updater.pubkey` in `tauri.conf.json` |

Set or replace the secrets with:

```sh
keys=~/.tiny-wispr-release-keys
base64 -i "$keys/release-certificate.p12" | gh secret set MACOS_CERTIFICATE
gh secret set MACOS_CERTIFICATE_PASSWORD < "$keys/release-certificate.password"
gh secret set TAURI_SIGNING_PRIVATE_KEY < "$keys/updater.key"
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < "$keys/updater.key.password"
```

Call the updater binary directly (`node_modules/.bin/tauri signer generate …`): `pnpm tauri …` prints the command line, password included.

## Versions

The version is in three places that must agree: `version` in `package.json`, `version` in `src-tauri/tauri.conf.json`, and `[workspace.package] version` in the root `Cargo.toml`. The About page shows the `package.json` one; the updater compares the one Tauri reads from `tauri.conf.json`. `pnpm release` changes all three, and the release workflow refuses a tag that doesn't match them (`scripts/release-ci.mjs check-version`).

Versions are semver. `X.Y.Z` is a release; `X.Y.Z-rc.N` (any suffix) is a prerelease.

## The release workflow

`.github/workflows/release.yml` runs when a `v*.*.*` tag is pushed.

| Job | Runs on | Does |
| --- | --- | --- |
| `preflight` | ubuntu | The tag matches the three versions; a release tag (no suffix) is on `main`; the four secrets are set |
| `verify` | macos-15 | `.github/workflows/ci.yml`: `pnpm verify`, then a smoke run of the debug build with a `say` recording and `ggml-tiny.en.bin` |
| `build` | macos-15 (Apple Silicon) | Imports the certificate into a temporary keychain, `pnpm tauri build --bundles app,dmg` with `createUpdaterArtifacts`, checks the code signature, checks the update signature names the version and verifies with the public key in `tauri.conf.json`, starts the app and checks it's still running after 20 seconds |
| `release` | ubuntu | Writes the notes, `latest.json` and `SHA256SUMS`, creates a draft release with every file, then publishes it |

The release files:

- `Tiny-Wispr_<version>_aarch64.dmg`: what people download.
- `Tiny-Wispr_<version>_aarch64.app.tar.gz` and its `.sig`: what the updater downloads.
- `latest.json`: the manifest the updater reads ([Updates](../internals/updates.md#the-manifest)).
- `SHA256SUMS`.

A release tag is published as the latest release, which installed apps are offered. A prerelease is published as a GitHub prerelease: `releases/latest` skips it, so no installed app is offered it unless pointed at it with `TINY_WISPR_UPDATE_URL`. Only Apple Silicon is built. Intel Macs build from source.

The notes come from the commit subjects since the previous tag (`scripts/release-ci.mjs notes`): `feat`, `fix` and `perf` commits, grouped, with the first letter capitalised. Everything else is left out. That is why commit subjects are written for people.

`.github/workflows/ci.yml` also runs on every push to `main` and every pull request.

## Windows and Linux

The code has Windows and Linux (X11) implementations behind `cfg` ([OS integration](../internals/os-integration.md#platform-status)) and `bundle.targets` is `"all"`, but only macOS is built and tested regularly. Before shipping another platform:

- Build on that platform (the sidecar script uses the host's target triple; there is no cross compile).
- Run the smoke run and the full test suite there.
- Check each row of the platform table in [OS integration](../internals/os-integration.md#platform-status) by hand with a real keyboard and a real second app: the key listener and insertion can't be unit-tested.
- Linux needs the Tauri system libraries (`webkit2gtk`, etc.) plus X11 dev libraries; Wayland sessions have no global shortcuts or paste.

## Release checklist

1. On `main`, with everything merged and CI green: `pnpm release 0.2.0`. It refuses a dirty tree, a version that isn't newer, or a release (not a prerelease) from another branch.
2. Read the notes it prints. They become the release notes and the "What's new" text in the app.
3. `git push origin main v0.2.0`.
4. Watch the Release workflow. When it's done, the release page has the DMG, the archive, its signature, `latest.json` and `SHA256SUMS`.
5. On a Mac running the previous release: Settings → About → **Check for updates**, **Download**, **Restart to update**. The app comes back on the new version, the permissions still work, and Network activity lists `latest.json` and the archive.
6. Open the DMG on a user account that has never run the app: **Open Anyway** is needed once, onboarding appears, the first dictation works after downloading a model.

## Testing an update

Before a change to the updater or the release workflow reaches users, release it as prereleases from the branch:

1. On the branch: `pnpm release 0.2.0-rc.1`, push the branch and the tag. Install that build from its DMG and grant the permissions.
2. `pnpm release 0.2.0-rc.2`, push.
3. Quit Tiny Wispr and start rc.1 pointed at rc.2's manifest:

   ```sh
   TINY_WISPR_UPDATE_URL=https://github.com/Time2Code0/tiny-wispr/releases/download/v0.2.0-rc.2/latest.json \
     "/Applications/Tiny Wispr.app/Contents/MacOS/tiny-wispr"
   ```

4. Check, download, restart. About shows rc.2, dictation works without granting anything again.

A local server works too: build with the updater key in the environment (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and `--config '{"bundle":{"createUpdaterArtifacts":true}}'`, sign the archive with `tauri signer sign --app-version`, write a `latest.json` by hand that points at `http://localhost:…`, and serve the folder.

## Pulling a bad release

Installed apps read `latest.json` from the newest published release. To stop offering a broken one, delete the release (or mark it as a prerelease) on GitHub: `releases/latest` then points at the one before, and nobody else is offered the bad version. Apps that already installed it stay on it, because the updater never offers an older version. Fix forward with a new patch release.
