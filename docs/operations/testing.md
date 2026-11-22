# Testing

**Tests never open the microphone, create an event tap or prompt for a permission.** Speech comes from files. A test that needs a person at the machine must be `#[ignore]`d and say so.

## Layers

| Layer | Where | Needs |
| --- | --- | --- |
| Pure logic | `#[cfg(test)]` in `tw-core`: the state machine, hotkey matcher and validation, history and stats, the model catalog and recommendation, every text step, polish prompts and acceptance | Nothing |
| Runtime with fakes | `src-tauri/src/dictation/runtime/tests.rs`, `src-tauri/src/files/tests.rs` | Nothing: fake `AudioSource`, `Desktop`, `Transcribers`, `Polisher` and `Sink` |
| Audio | `crates/tw-audio/tests/` (decoding, speech detection on a spoken fixture) | Nothing |
| Downloads | `crates/tw-models/tests/download.rs` against a local HTTP server: resume, checksum mismatch, offline, disk space | Nothing |
| Whisper | `crates/tw-stt/tests/whisper.rs` | `TINY_WISPR_TEST_MODELS` |
| The worker | `crates/tw-llm-worker/tests/worker.rs`: raw protocol, load errors, streaming | `TINY_WISPR_TEST_MODELS` for the model-backed ones |
| Network | `crates/tw-models/tests/network.rs`: every catalog URL resolves, a real resumed download | `TINY_WISPR_NETWORK_TESTS=1` |
| Frontend logic | `src/**/*.test.ts` (vitest, node): hotkey formatting, the mock backend's rules, the pill model, model lists, routing, history grouping | Nothing |
| End to end | The smoke run, below | A debug build and models |

Tests that need models **skip with a message** when their variable isn't set, so `cargo test` is always green on a clean machine.

## Environment variables

| Variable | Effect |
| --- | --- |
| `TINY_WISPR_TEST_MODELS=<dir>` | Turns on model-backed tests. Expects `<dir>/whisper/ggml-tiny.en.bin`, `ggml-base.bin`, and the catalog's GGUF files in `<dir>/llm/` |
| `TINY_WISPR_NETWORK_TESTS=1` | Turns on the catalog URL check and a real resumed download |
| `TINY_WISPR_SMOKE=<wav>` | Debug builds: run one dictation through the real runtime with this audio |
| `TINY_WISPR_SMOKE_OUT=<txt>` | Where the smoke run writes the inserted text |
| `TINY_WISPR_DATA_DIR=<dir>`, `TINY_WISPR_MODELS_DIR=<dir>` | Debug builds: use these instead of the real app data and models folders |

## Speech without a microphone

```sh
scripts/make-speech-fixture.sh "Hello, this is a test." target/hello.wav
```

Makes a 16 kHz mono 16-bit WAV with macOS `say` and `ffmpeg`. Write fixtures under `target/` or another untracked path, never into tracked ones.

## The smoke run

`src-tauri/src/dictation/smoke.rs`, debug builds only. It replaces the microphone with a WAV and the OS with a text file, and runs one push-to-talk dictation through the **real** runtime: state machine, Whisper, text pipeline, optional polish, insertion. This is how the app is checked end to end without a person, and without any permission.

```sh
TINY_WISPR_SMOKE=target/hello.wav \
TINY_WISPR_SMOKE_OUT=target/hello.txt \
TINY_WISPR_MODELS_DIR=/path/to/models \
TINY_WISPR_DATA_DIR=target/smoke-data \
  target/debug/tiny-wispr
cat target/hello.txt
```

It waits two seconds, presses the key, plays the file, releases, writes the inserted text and exits 0 (non-zero on failure, with a 120 s timeout). A voice model must be in `<models dir>/whisper/`. To include polish, put `{"llmModel":"<catalog id>"}` in `<data dir>/settings.json` and the GGUF in `<models dir>/llm/`.

## Evaluating the language model

```sh
cargo build -p tw-llm-worker
TINY_WISPR_TEST_MODELS=<models>/llm cargo run -p tw-llm --example polish_eval -- [model.gguf …] [--level light|medium]
```

Runs 26 dictated transcripts through `LlmPolisher`, the same path the app uses (skip check, prompt, model, sanitising, acceptance), and checks each result with assertions that don't depend on exact wording. Without model arguments it runs every catalog file found in the directory, which for this example is the `llm` folder itself. The worker is `TW_LLM_WORKER` or the one built next to the example. Run it when you change the prompt, the examples or the thresholds in `tw-core/src/text/polish.rs`, and for every new catalog model.

## Writing tests

- **State machine:** the helpers in `machine.rs`'s tests (`press(at, action)`, `release(at, action)`, `hold`) make a test read as a script of what the user did, with the time of each step. Add one for every bug, and test the timing boundaries (`a_hold_of_exactly_half_a_second_pastes`).
- **Text rules:** a table of input → output next to the module. Add the case that went wrong first. Include the "don't touch" cases: "remove the comma", "uh huh".
- **Runtime:** use the fakes in `runtime/tests.rs`. They record effects so you can assert the order: capture started, sound played, text inserted.
- **Anything with the clock:** pass the time in. Don't `sleep` in a test.
- **Anything that touches `tw-os`:** keep the testable part (the key engine, option handling) free of OS calls. Real hooks and taps aren't tested. `cargo test -p tw-audio -- --ignored --nocapture records_from_the_default_microphone` is the one ignored test and is run by a person.
- **Frontend:** put logic in plain `.ts` modules and test those. Mirror any rule you add in Rust in the mock and add a test to `mock-*.test.ts`.

## Before you push

```sh
pnpm verify
```

and, for changes that touch dictation, a smoke run with a real model. CI doesn't exist yet, so this is the gate.
