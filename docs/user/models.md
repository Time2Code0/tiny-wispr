# Models

Tiny Wispr uses two kinds of models, both run on your computer. Downloading one is the only time the app uses the internet.

| Kind | Job | Required | Format | Stored in |
| --- | --- | --- | --- | --- |
| **Voice** | Turns speech into text. OpenAI's Whisper, run by whisper.cpp. | Yes | `ggml-*.bin` | `models/whisper/` |
| **Language** | Cleans up the transcript: grammar, false starts, self-corrections, lists. Runs through llama.cpp. | No | `.gguf` | `models/llm/` |

Both folders sit in the app's data folder, see [Privacy](privacy.md#where-your-data-lives).

## Choosing a voice model

The Models page shows one row per model size, smallest first. Bigger models are more accurate and slower, and use more memory.

| Size | Download | Memory while running | Good for |
| --- | --- | --- | --- |
| Tiny | about 78 MB | about 280 MB | Quick tests; rough with accents and noise |
| Base | about 150 MB | about 390 MB | Small, quick, any computer |
| Small | about 490 MB | about 860 MB | A good balance on most computers |
| Medium | about 1.5 GB | about 2.1 GB | Accurate, slower |
| Large v3 Turbo | about 1.6 GB | about 2 GB | Close to the most accurate, much faster than Large v3 |
| Large v3 | about 3.1 GB | about 3.9 GB | The most accurate and the slowest |

The numbers are approximate: the app shows the exact size next to each model.

**Languages.** Models whose name ends in `.en` only understand English, and are a little more accurate at it. The other models understand many languages. Use the "You'll dictate in" filter to hide models that can't do what you need. Under Settings → General you can let Whisper detect the language each time, or limit it to the ones you speak.

**Compressed copies.** Many models also come as a smaller, compressed file. A row offers them in its Download column: "light" is close to the original at about half the size; "strong" is about a third of the size and sometimes a little less accurate.

**Recommended.** The app reads your computer's memory, processor and whether it's an Apple silicon Mac, and marks one voice model and one language model as recommended. On a 36 GB Apple silicon Mac that's Large v3 Turbo; on a modest laptop, Base or Small.

## Language models (polish)

Optional. When one is selected and downloaded, each dictation is sent through it between the rule-based steps, and the pill says "Polishing". It adds a second or two.

- Without one, you still get spoken punctuation, filler removal, the dictionary, snippets and styles. Choose **No language model** on the Models page to turn polish off.
- The catalog has small models in four families: Llama, Qwen, Ministral and DeepSeek (the DeepSeek models "think" first and are slower). Smaller ones are quick; larger ones write more carefully.
- Very short dictations, short sentences that already read cleanly, and very long ones (over 500 words, which would take too long) skip the model. They're still cleaned up by the rule-based steps.
- If the model fails or returns something that doesn't look like your text, the rule-based text is pasted instead.

## Downloads

Select **Download** on a row. You see progress, and can pause and resume or cancel. A download that is interrupted continues where it stopped, and the finished file is checked against its published checksum before it's used. The app makes sure there's room on the disk first.

The "On this computer" list at the bottom of the page shows the space your models take, and lets you delete them.

**Stay offline** under Settings → Privacy blocks all downloads, in case you want a hard guarantee.

## Importing your own

**Import** at the bottom of each section takes a file you already have: a Whisper `ggml-*.bin` for voice, or a `.gguf` for language. It's copied into the models folder and shows up as an imported model. This is the way to use a fine-tuned or distilled Whisper model, or a different language model.

## Switching

Choose **Use** on a downloaded model. The change applies to the next dictation. A voice model stays in memory for 10 minutes after you last dictate and a language model for 5, then both are released so the app doesn't hold gigabytes while idle. The next dictation loads it again, which takes up to a second or two.
