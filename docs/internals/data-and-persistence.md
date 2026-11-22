# Data and persistence

Everything the app stores is a plain file in one folder, the OS app data directory for the identifier `dev.time2code.tinywispr`. There is no database. Audio is never stored.

## Files

`AppPaths` (`src-tauri/src/state.rs`) names them. `Data` loads them all at startup and is shared through `AppState.data`.

| File | Type | Owner | Contents |
| --- | --- | --- | --- |
| `settings.json` | `Settings` | `SettingsStore` | All preferences, including onboarding progress |
| `history.json` | `HistoryStore` | `HistoryFile` | Entries and the stats ledger, with a `version` |
| `dictionary.json` | `Vec<DictionaryEntry>` | `JsonStore` | |
| `snippets.json` | `Vec<Snippet>` | `JsonStore` | |
| `network-log.json` | `Vec<NetworkRequest>` | `JsonStore` | Every request ever made: model downloads and update checks |
| `models/whisper/`, `models/llm/` | | `Inventory` | Model files, `*.partial` for unfinished downloads, `*.importing` while copying |

Debug builds honour `TINY_WISPR_DATA_DIR` and `TINY_WISPR_MODELS_DIR` to point elsewhere, so tests and the smoke run leave the real data alone. Release builds ignore them.

## How files are written (`store.rs`)

`JsonStore<T>` keeps the value in memory behind a mutex.

- **Load:** a missing file gives `T::default()`. A file that doesn't parse is **renamed to `<name>.json.bad`** and the default is used, so a bad edit or a version mismatch never silently destroys data. Other read errors start empty and log a warning.
- **Update:** `update(|value| …)` and `try_update` clone the value, apply the change, serialise, write, and only then replace the in-memory value. If the change returns an error or the write fails, **nothing changes in memory**.
- **Write:** `write_atomic` writes `<name>.json.tmp`, fsyncs it, and renames it over the original. A crash never leaves a half-written file.

Take the minimum from the lock: `read(|v| …)` avoids cloning a big value, and never hold a store's lock while calling something that can block.

`SettingsStore` wraps the same idea for `Settings` and applies a `SettingsPatch`. Missing fields in `settings.json` take their `Default` (every struct is `#[serde(default)]`), which is how old files survive new settings.

## Changing a stored type

1. Add fields with `#[serde(default)]` (or a struct-level `default`) so old files still load. Don't rename or remove a field without a migration.
2. For a breaking change to `history.json`, bump `FILE_VERSION` and write the migration in `tw-core/src/history.rs` where the file is loaded.
3. Mirror the type in `src/lib/ipc/types.ts` and `defaults.ts`.

## History and stats

`HistoryStore` (`tw-core/src/history.rs`) holds the entries plus a `StatsLedger`, **kept apart** so deleting entries, or never keeping them, leaves the stats intact.

- A `HistoryEntry` has `id`, `created_at`, `raw_text` (what Whisper gave), `final_text` (what was inserted), `word_count`, `app`, `duration_ms` and `language`. `was_edited()` (raw ≠ final) is what enables "Undo AI edit" in the UI.
- `push_with_fixes(entry, fixes)` counts the dictation in the ledger first, then keeps the entry only if retention allows.
- The ledger buckets activity (dictations, words, speaking time, fixes) per **UTC quarter hour**. Every UTC offset in use is a whole number of quarter hours, so local days come out exact for whatever offset the stats are read with. Per-app totals are kept separately.
- `Stats` is computed on read: current and longest streak (a streak that ended yesterday still counts until today is over), total words, words per minute (words over time spent speaking), dictations, fixes made, `daily` words for the last 26 weeks for the heatmap, and the top five apps.
- `list_history(cursor, query)` returns pages of `PAGE_SIZE` (50), newest first. The cursor is `"<created_at>:<id>"`, so paging stays stable while entries are added. `query` matches either text version and the app name.
- `apply_retention(retention, now)` deletes entries older than the limit (`Forever`, `ThirtyDays`, `OneDay`, `Never`). It runs after every history change, at startup, and when the setting changes. Neither `clear_history` nor `delete_history_entry` touches the ledger: the stats survive both. (The confirmation dialog on the Privacy page still says "Your stats start over", which isn't what the code does.)

## Network log

`network_log.rs` receives a `RequestRecord` from `tw-models` for every request (the model downloader and the updater's downloader both log through it) and appends a `NetworkRequest { url, bytes, started_at, finished_at, result, message }` (`completed`, `cancelled`, `restarted`, `failed`). The Privacy page shows it, oldest first. This is the app's proof that its only network traffic is model downloads and update checks, so **any new network use must go through `tw-models` and be logged here.**

## Not stored

- Audio. Captured samples live in memory (`Arc<[f32]>`) while a job runs and, after a cancel, until the Undo offer ends.
- Models in memory beyond the idle limits ([Models](models-and-llm.md)).
- The mock's state in the browser, apart from the mock settings in `localStorage`.
