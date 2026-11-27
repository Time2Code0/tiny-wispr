# Models and the language model worker

Two kinds of model run on the device. Voice models (Whisper) are in `tw-stt`; language models (GGUF through llama.cpp) run in a separate process. Files and downloads are the job of `tw-models`, which is **the only crate that touches the network**. Its `Downloader` also fetches the updater's manifest and archive ([Updates](updates.md)).

## The catalog

`crates/tw-core/src/models.rs` holds `CATALOG`, a static table of `Entry` values. `catalog()` turns it into `ModelInfo` values, all `NotDownloaded`. Each entry has:

| Field | Meaning |
| --- | --- |
| `id`, `label`, `description`, `file` | Identity and display. `file` is the name on disk (`ggml-small.bin`). |
| `kind` | `Voice` (`models/whisper/`) or `Language` (`models/llm/`) |
| `repo`, `revision`, `file` | Build the download URL: `https://huggingface.co/<repo>/resolve/<revision>/<file>`. Pin `revision` to a commit so a model can't change under its checksum. |
| `size_bytes`, `sha256` | Checked before the file is moved into place |
| `variant_of`, `compression` | A compressed copy points at the original (`tiny-q5_1` → `tiny`); `Compression::{None, Light (q8), Strong (q5)}`. The UI shows one row per original and offers copies in its Download column. |
| `english_only`, `speed`, `accuracy` | Display. Relative within a kind (1–5). |
| `ram_mb` | Memory while running, rounded up. Drives the recommendation. |
| `context_length`, `reasoning` | Language models. `reasoning` marks models that emit `<think>` blocks (the DeepSeek distills). |

Sizes and checksums must be exact: a test checks that `ram_mb` exceeds the file size and that compressed copies are smaller than their original. `TINY_WISPR_NETWORK_TESTS=1` also checks every URL resolves.

### Adding a model

1. Add an `Entry` to `CATALOG` with the exact `size_bytes` and `sha256` (from the repo's file page or `shasum -a 256`).
2. Add it to `seedModels()` in `src/lib/ipc/mock.ts` if the mock lists it separately, so the browser mode matches.
3. If it's a language model, score it: `cargo run -p tw-llm --example polish_eval` with the file in `<TINY_WISPR_TEST_MODELS>/llm/`. Models that rewrite or answer their input don't belong in the catalog.
4. Decide whether `recommend()` should prefer it for some memory class.

### Recommendation

`recommend(&HardwareInfo, &[ModelInfo]) -> Recommendation` picks one voice and one language model from RAM (rounded up to whole GiB), CPU core count and whether the machine is Apple silicon (Metal counts as a GPU). Each branch has an ordered list of preferred ids: the first that exists in the catalog wins, otherwise the model needing least memory. The `reason` string is shown in the UI. `hardware()` in `tw-models` reads OS counters only, with no prompts and no subprocesses.

## Inventory

`Inventory` (`tw-models/src/inventory.rs`) is a view of `<root>/whisper/*.bin` and `<root>/llm/*.gguf`:

- `models(&catalog)` joins the catalog with the disk: a file named like an entry is `Downloaded`, a `<file>.partial` is `Paused`, any other model file becomes a `custom` entry.
- `import(source, kind)` copies a user's file into the folder after checking its extension and its magic bytes (`GGUF` for language models, Whisper's header for voice models), refusing an existing name, checking free space, and staging as `<file>.importing` before renaming. Imported files are never matched to the catalog.
- `delete` removes the file and its partial.
- File names are validated: no separators, no traversal.

In debug builds `TINY_WISPR_MODELS_DIR` points the root elsewhere; that's how the smoke run and tests use pre-downloaded models.

## Downloads

`Downloader::download(inventory, request, on_progress, cancel)`:

- Writes to `<file>.partial`. If one exists the request carries `Range: bytes=<n>-` and the server's `Content-Range` is checked; a server that ignores the range restarts from zero (`RequestOutcome::Restarted`).
- Before resuming, the bytes already on disk are re-hashed so the final SHA-256 covers the whole file.
- Checks free space first, keeping `SPACE_MARGIN` (100 MB) spare.
- Progress is throttled to once per 100 ms, then exactly one of `Done`, `Paused` (cancelled, partial kept) or `Failed`.
- A hash mismatch deletes the partial file.
- `get(url, limit, on_progress)` fetches a small file into memory with one logged request, failing with `TooLarge` past `limit`. The updater uses it; models always go to disk through `download`.
- Every request, finished or not, goes to the request log closure given with `with_request_log`.

`src-tauri/src/models.rs::Models` runs downloads on worker threads and tracks them in a map of `Transfer`s. `list_models` returns the catalog + disk + running transfers, with `recommended` set. It emits `models:progress` while a download runs and `models:changed` when the set of files changes.

### The voice model in use

`voice_model_path(inventory, selected)` returns the selected voice model if it's downloaded, **otherwise any downloaded voice model**. The user who deletes the model they selected still gets dictation. Language models have no such fallback: no selection means no polish.

## Whisper (`tw-stt`)

`WhisperEngine::load(path)` and `Transcriber::transcribe(samples, &TranscribeOptions, &cancel)` block for as long as whisper.cpp does: call them off the UI thread. A loaded engine is `Send + Sync` and serialises calls internally, since one engine keeps one set of buffers.

Two workarounds in `lib.rs` matter:

- whisper.cpp returns nothing for input shorter than a second, so audio is padded with silence up to 1.25 s (`MIN_INPUT_MS`).
- It stops decoding once it predicts a timestamp within 100 ms of the end of the input, which drops the last word of a quick release. One second of trailing silence (`TRAILING_SILENCE_MS`) keeps that timestamp away.

`TranscribeOptions` carries `language` (forced), `detect_among` (detect among a list) or neither (auto-detect), and `initial_prompt` (the vocabulary hint). `clean_transcript` removes sound annotations and phrases Whisper invents for silence.

### Lifetime

`WhisperModels` (`src-tauri/src/dictation/engine.rs`) is an `IdleCache<WhisperEngine>`:

- Loaded lazily, and eagerly when a capture starts, so it loads while the user speaks.
- Dropped after `IDLE_LIMIT` (10 minutes) without use, checked every minute; reloading takes up to a second.
- Dropped when the selected model changes and at exit. **It must be dropped before the process exits**: ggml's Metal backend asserts on exit that every buffer is freed and aborts otherwise. Never put an engine in a `static`.

## The language model worker

### Why a separate process

whisper.cpp and llama.cpp both embed their own copy of ggml. Linked into one binary they clash. So llama.cpp lives in `crates/tw-llm-worker`, a small binary the app spawns. Ending the process also frees the model's memory, which is how idle unload works.

### Protocol

`crates/tw-llm/src/protocol.rs`. JSON lines: the client writes one `Request` per line to the worker's stdin, and the worker answers each with one `Response` line on stdout. llama.cpp's own logging is silenced so stdout carries only the protocol.

| Request | Meaning |
| --- | --- |
| `Load { model_path, context_length, gpu_layers }` | Load a model. `gpu_layers: None` offloads every layer, falling back to CPU if that fails. |
| `Complete(Completion)` | Messages, `max_tokens`, temperature, stop strings, optional streaming (`token` lines first) |
| `Cancel { id }` | Gets no answer of its own: the targeted request ends with a `cancelled` error |
| `Unload`, `Info` | |

The worker reads stdin on its own thread so `cancel` and end-of-input are seen while the main thread is generating. When stdin closes (the app exited or crashed) the worker exits. It applies the model's chat template, switches off thinking by pre-filling `<think>\n\n</think>\n\n` where it can, and uses a stop-string scanner that works on a UTF-8-safe stream (`stream.rs`).

### Client and polisher

- `LlmClient` (`client.rs`) supervises the process: spawn, `load`, `complete`, `cancel`, restart after a crash.
- `LlmPolisher` (`polisher.rs`) is what the app uses: `set_model(Option<ModelSpec>)`, `polish(&PolishRequest)`. One call per dictation, safe to share across threads. Context is `DEFAULT_CONTEXT` (4096), temperature 0, a light repeat penalty. A background thread ends the worker after the idle limit (`IDLE_UNLOAD`, 5 minutes in `dictation/polish.rs`) and never while a polish is running. Cancelling the dictation sets a flag checked between chunks.
- `LlmPolish` (`src-tauri/src/dictation/polish.rs`) is the `Polisher` the runtime uses. `active(settings)` is called before each polish: it checks that the worker binary exists, that `settings.llm_model` is a downloaded language model, and updates the polisher's model. If the worker file is missing, polish is silently off and a warning is logged at startup.

How the prompt is built and the answer judged is in [Text pipeline](text-pipeline.md#the-language-model-step).

### Where the worker binary comes from

It is a Tauri sidecar (`bundle.externalBin: ["binaries/tw-llm-worker"]`). `scripts/build-worker.mjs` builds it with cargo and copies it to `src-tauri/binaries/tw-llm-worker-<target triple>`. `pnpm tauri dev` and `pnpm tauri build` run that script first (`beforeDevCommand`, `beforeBuildCommand`). At runtime `worker_path()` looks next to the app executable, which is where Tauri puts a bundled sidecar and also where a workspace build puts it. If you run the app binary some other way and polish never happens, run `pnpm build:worker`.
