# Updates

The app updates itself from GitHub releases. The user starts every step: a check (or the daily check they turned on), the download, and the restart.

## Pieces

| Piece | Where | Does |
| --- | --- | --- |
| `UpdateState`, `Release`, `auto_check_due` | `crates/tw-core/src/updates.rs` | The IPC contract and the rules: which state may check, download or install, and when an automatic check is due |
| `Updates` | `src-tauri/src/updates.rs` | Runs the checks, downloads and installs on their own threads and emits `updates:state` |
| Commands | `src-tauri/src/commands/updates.rs` | `get_update_state`, `check_for_updates`, `download_update`, `install_update` |
| `Downloader::get` | `crates/tw-models/src/download.rs` | Fetches the manifest and the archive into memory, one logged request each |
| `tauri-plugin-updater` | `src-tauri/Cargo.toml` | Reads the manifest, compares versions, verifies the signature, swaps the app bundle. **It makes no requests.** |
| UI | `src/windows/main/settings/updates.tsx`, `src/hooks/use-update-state.ts` | The Updates section on the About page |
| Tray | `src-tauri/src/tray.rs` | "Restart to update to <version>" while an update is ready |
| Mock | `src/lib/ipc/mock-updates.ts` | The same flow in the browser preview |

## Why the plugin doesn't fetch

The plugin can check and download by itself with its own HTTP client. The app doesn't let it, so that every request goes through `tw_models::Downloader`: it lands in `network-log.json` like a model download, and "Stay offline" refuses it (the commands return `ErrorKind::Offline`). The flow is:

1. `Downloader::get(endpoint)` fetches `latest.json` (1 MB limit).
2. `Updater::restore_update(json)` turns it into an `Update` when its version is newer than the running one. `None` means up to date. `restore_update` turns on `require_signed_version`.
3. `Downloader::get(update.download_url)` fetches the archive into memory (300 MB limit), reporting progress as `Downloading { received, total }`.
4. `Update::verify(bytes)` checks the minisign signature against `plugins.updater.pubkey` in `tauri.conf.json`, and that the signature names the version the manifest announced. A mismatch discards the download.
5. On **Restart to update**, `Update::install(bytes)` verifies again, replaces `Tiny Wispr.app` (asking for an administrator password through AppleScript when the folder isn't writable), and `AppHandle::restart` relaunches. The single-instance lock is released before the new process starts.

The language model worker is stopped before the swap, because the new bundle has its own worker binary.

## States

```text
idle ─check─▶ checking ─▶ upToDate | available | failed(release: null)
available ─download─▶ downloading ─▶ ready | failed(release)
ready ─install─▶ installing ─▶ (restart) | failed(release)
```

- A check is allowed from `idle`, `upToDate`, `available` and `failed`. Not while one runs, and not once an update is `ready`.
- `failed` with a `release` retries the download; without one it retries the check.
- An automatic check that fails restores the previous state and only logs a warning: nobody asked for it.
- `install_update` refuses while a dictation is listening or processing, or a file job runs.

## When it checks

Only when the user clicks **Check for updates**, or when `settings.check_for_updates` is on (default off) and `auto_check_due` says so: `FIRST_CHECK_DELAY_MS` (one minute) after launch, then every `CHECK_INTERVAL_MS` (24 hours) after the last finished check. A thread looks at the clock every minute; no request is made unless a check is due. The last check time is kept in memory only, so each launch checks once.

## The manifest

`plugins.updater.endpoints[0]` in `tauri.conf.json`:
`https://github.com/Time2Code0/tiny-wispr/releases/latest/download/latest.json`. GitHub serves the asset of the newest published release that isn't a prerelease, so drafts and `-rc` builds are never offered.

`TINY_WISPR_UPDATE_URL` replaces the address in any build. That is how a prerelease is tested ([Building and releasing](../operations/building-and-releasing.md#testing-an-update)). It's safe to leave in release builds: an archive is only installed with our signature for the version it claims, and only when that version is newer.

`latest.json` is written by `scripts/release-ci.mjs manifest`:

```json
{
  "version": "0.2.0",
  "notes": "### Features\n\n- …",
  "pub_date": "2026-11-22T10:00:00.000Z",
  "platforms": {
    "darwin-aarch64": {
      "url": "https://github.com/Time2Code0/tiny-wispr/releases/download/v0.2.0/Tiny-Wispr_0.2.0_aarch64.app.tar.gz",
      "signature": "<contents of the .sig file>"
    }
  }
}
```

The notes are Markdown. The About page shows headings, bullets and paragraphs as plain text (`release-notes.ts`) and never renders HTML from the server.

## Signatures

The updater key pair is separate from code signing. The public key is in `tauri.conf.json`; the private key and its password are the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets. The Tauri bundler signs the archive during `tauri build` and writes `version:<version>` into the signature's trusted comment. The release workflow fails if that's missing, because every installed app would refuse the update (`requireSignedVersion`), and verifies the signature with `minisign` against the committed public key, so a secret that doesn't match the key can't ship.

Losing the private key means installed apps can't update any more: they would need a new download by hand. See the key backup in [Building and releasing](../operations/building-and-releasing.md#keys-and-secrets).
