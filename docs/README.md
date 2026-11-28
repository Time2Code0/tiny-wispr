# Tiny Wispr documentation

Tiny Wispr is a local, private dictation app: hold a key, speak, release, and the text is transcribed on your computer and pasted into the app you're using. No account, no cloud, no telemetry.

The docs are split by reader.

## Using Tiny Wispr

For people who want to install the app and dictate.

- [Getting started](user/getting-started.md): install, the first-run setup, permissions
- [Dictating](user/dictating.md): hold to talk, hands-free, cancel and undo, the pill, shortcuts
- [Text, styles, dictionary and snippets](user/text-and-style.md): what happens to your words before they're pasted
- [Models](user/models.md): voice and language models, which to pick, importing your own
- [Transcribing audio files](user/transcribe-files.md)
- [Settings](user/settings.md): every option, page by page
- [Updating](user/updating.md): checking for, downloading and installing a new version
- [Privacy](user/privacy.md): what stays on your computer and the only network requests the app makes
- [Troubleshooting](user/troubleshooting.md)

## Internals

For developers and coding agents working on the code. Read [Architecture](internals/architecture.md) first.

- [Architecture](internals/architecture.md): the crates, the windows, how a dictation flows through them
- [Dictation runtime](internals/dictation-runtime.md): the state machine, the runtime thread, effects and timers
- [Text pipeline](internals/text-pipeline.md): rule-based formatting, the LLM polish step, dictionary, snippets, style
- [Models and the language model worker](internals/models-and-llm.md): the catalog, downloads, Whisper, llama.cpp in its own process
- [OS integration](internals/os-integration.md): global keys, insertion, permissions, frontmost app, per-platform status
- [IPC](internals/ipc.md): commands, events, errors, and how to add one
- [Frontend](internals/frontend.md): windows, routing, hooks, the mock backend, UI conventions
- [Data and persistence](internals/data-and-persistence.md): what is stored where, and how
- [Updates](internals/updates.md): the in-app updater, the update manifest and how an update is verified

## Operations

For building, testing and shipping.

- [Development](operations/development.md): toolchain, running the app, the browser-only mode
- [Testing](operations/testing.md): the test layers and how to test without a microphone
- [Building and releasing](operations/building-and-releasing.md): release builds, the worker sidecar, signing, the release workflow

## Agent skills

Configuration for the engineering skills that file issues, triage and keep domain docs.

- [Issue tracker](agents/issue-tracker.md): GitHub Issues through the `gh` CLI
- [Triage labels](agents/triage-labels.md): the label for each triage role
- [Domain docs](agents/domain.md): how agents read `GLOSSARY.md` and `docs/adr/`

The conventions that apply to every change (code style, UI tokens, commit messages) are in [AGENTS.md](../AGENTS.md).

## Keeping the docs true

These docs describe what the code does today. When you change behaviour, update the page that describes it in the same commit, the same way the IPC types are changed on both sides at once. Prefer pointing at a file or a constant over copying its value, so there is less to go stale. Where a feature exists in the code but isn't wired up yet, the docs say so.
