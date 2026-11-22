# Text pipeline

Whisper's output is raw: right words, but with fillers, no spoken punctuation turned into symbols, and no knowledge of your names, snippets or the app you're writing in. The text pipeline fixes that. The rule-based steps live in `crates/tw-core/src/text/` (pure, unit-tested, no I/O). The orchestration, which has to talk to Whisper, the language model and the OS, is in `src-tauri/src/dictation/pipeline.rs`.

## The whole path

`pipeline::process(deps, samples, cancel, polish_started) -> Option<Outcome>`:

```text
samples (16 kHz mono f32)
  │
  ├─ all zeros and the OS says the mic is denied?    → Failed(MicrophoneDenied)
  ├─ vad::has_speech?  no                            → NoSpeech
  ├─ transcribers.load(settings)                     → Failed(NoModel) …
  ├─ vad::trim_silence(samples)
  ├─ Whisper transcribe (options: language(s), vocabulary prompt)
  ├─ tw_stt::clean_transcript   strips [BLANK_AUDIO], (music), ♪, invented phrases. Empty → NoSpeech
  ├─ desktop.frontmost_app()    → category → StyleCategory → WritingStyle
  │
  ├─ before_polish   (rule-based, skipped at cleanup None)
  │      format_spoken   "comma" → ","
  │      remove_fillers  "um", stutters
  ├─ polish          (only if cleanup ≠ None and a language model is active)
  ├─ after_polish
  │      apply_dictionary   corrections, then casing
  │      expand_snippets
  │      apply_style        capitals and final punctuation per category
  │      whitespace cleanup
  └─ Transcript { raw, text, app, fixes }
```

`None` is returned when `cancel` was set: the machine has moved on and wants no answer. A cancel flag is checked after transcription and after polish.

`format()` in the same file is the part after Whisper. File transcription calls it per paragraph, and the Style page's live example (`test_polish`) calls `before_polish` and `after_polish` directly.

## Cleanup levels

`CleanupLevel::{None, Light, Medium}`. At `None` only the dictionary and snippets run: the raw text, plus what the user explicitly configured. Light and Medium run the rule-based steps. The level also picks the prompt for the language model.

`PipelineContext { level, style, category, dictionary, snippets }` carries everything a rule needs. Every step returns the number of fixes it made (`Fixes`), and the "fixes made" stat counts only `fillers_removed + dictionary_fixes`: spoken commands and snippets are things the user asked for, not corrections.

## The rule-based steps

Each is a module in `tw-core/src/text/`. They are string functions with long tests: read the tests before changing one, they record the cases that went wrong.

| Module | Does | Care taken |
| --- | --- | --- |
| `spoken.rs` | Spoken punctuation and breaks (`COMMANDS` table: period, comma, question mark, new line, new paragraph …) | Words that are *mentioned* rather than *dictated* stay words: "remove the comma" (`MENTION_BEFORE`), "trial period" (`PERIOD_COMPOUNDS`), "colon cancer". Also absorbs the punctuation Whisper already put around a command. |
| `fillers.rs` | Hesitation sounds ("um", "uh", "hmm") and stutters ("the the") | Only function words count as stutters ("that that" can be correct). "er" is a filler only where Whisper marked a pause after it (it is German for "he"). "uh huh" is an answer. |
| `dictionary.rs` | Corrections and casing from the user's dictionary; the Whisper vocabulary prompt | Whole-word, case-insensitive, longest phrase first, newest entry wins. Possessives follow. |
| `snippets.rs` | Trigger → expansion | Expansions are swapped for private-use placeholder characters while later steps run, so style and whitespace rules never touch them. A dictation that is only a trigger becomes exactly the expansion. |
| `style.rs` | Capitals and final punctuation per `WritingStyle` and category | URLs, emails, paths and code keep their case. Casual in a messaging category drops the final period only in short messages and not after an abbreviation or an ellipsis. Dictionary words keep their capitals in very casual. |
| `words.rs` | Tokenising, `count_words` | Shared by all of the above |

Input rules for dictionary entries and snippets (`validate_dictionary_entry`, `validate_snippet`) are in the same modules and return an `InputError` with the message the UI shows. Limits: `MAX_PHRASE_CHARS` (60), `MAX_EXPANSION_CHARS` (4,000).

### Vocabulary hint for Whisper

`vocabulary_prompt(entries, WHISPER_PROMPT_BYTES)` builds `Vocabulary: A, B, C.` (newest first, within 600 bytes) and is passed as Whisper's initial prompt. It's phrased as a labelled sentence on purpose: a bare list makes small models treat speech that repeats it as already transcribed.

## The language model step

`tw-core/src/text/polish.rs` builds the prompt and judges the output; `tw-llm/src/polisher.rs` runs it. The model sees only what rules can't do.

**What the model is asked** (`build_prompt`): a system message with `LIGHT_RULES` or `MEDIUM_RULES`, a style rule for the chosen style and category, the dictionary words ("Spell these words exactly like this"), `COMMON_RULES`, then five worked examples as earlier chat turns (English, a question that must *not* be answered, a prompt-injection attempt that must be cleaned, not obeyed, Spanish and German). The transcript goes in `<transcript>` tags, and `</transcript>` is the stop sequence. The prompt says to reply in the transcript's language.

**When it runs** (`LlmPolisher::polish_timed`):

1. `should_polish`: not at `None`; never for one or two words; always above 12 words; in between, only if the text is lowercase or unpunctuated or contains an edit cue or a nearby repeated word. Otherwise `PolishError::Skipped`.
2. More than `MAX_INPUT_WORDS` (500) words → `TooLong`, skipped.
3. The model is loaded in the worker if needed.
4. `split_for_polish` cuts the text at sentence ends into pieces of about `CHUNK_WORDS` (50), because small models copy a long input unchanged instead of editing it. Each piece is one completion; accepted pieces are joined as paragraphs.
5. Each answer is passed through `sanitize_output` (strips `<think>` blocks, code fences, transcript tags, "Here is the cleaned text:" preambles, trailing notes, wrapping quotes) and judged by `is_acceptable`.

**What is rejected** (`RejectReason`): empty, far longer than the input, far shorter, containing words that weren't said (`NewWords`), or sharing too few words with the input (`Rewritten`, which catches translations and answers). A rejected piece is replaced by the input piece. Thresholds are looser at Medium, which is allowed to reword.

Every failure path leaves the rule-based text in place. `LlmPolish::polish` in the app logs the error and returns `None`, and the pipeline keeps going. Polish can only improve a dictation, never lose it.

**Tuning.** `cargo run -p tw-llm --example polish_eval` scores a model on 26 dictated transcripts. Use it when changing the prompt, the examples, or the acceptance thresholds, and when adding a model to the catalog. It needs the model on disk (see [Testing](../operations/testing.md)).

## Adding a rule

1. Add the failing case to the module's tests first.
2. Keep it conservative: leaving a filler in is better than deleting a word the user said. When in doubt, don't touch the text. That is why backtracking ("scratch that") and numbered lists are left to the language model: rules must not guess.
3. Return a fix count if the user would call it a correction.
4. If it needs a new field in `Settings` or the context, update the IPC types too ([IPC](ipc.md)).
