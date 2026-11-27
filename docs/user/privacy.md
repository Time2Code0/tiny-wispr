# Privacy

Tiny Wispr is built so that nothing you say, and nothing you write, ever leaves your computer.

- **No account.** There is nothing to sign up for.
- **No cloud.** Transcription runs on your computer with Whisper. The optional cleanup runs a language model on your computer.
- **No telemetry.** No analytics, no crash reporting, no usage statistics.
- **No audio is saved.** Audio lives in memory while you speak and while it's transcribed, then it's dropped. The only exception is the few seconds after you cancel, so Undo can work.

## The only network requests

The app goes online for two things, and at no other time:

- **Model downloads you start.** They go to Hugging Face over HTTPS, to an address listed in the model's catalog entry.
- **Update checks.** When you click **Check for updates** under Settings → About, or once a day if you turned on **Check for updates automatically** (it's off unless you do). The check downloads a small file, `latest.json`, from the app's GitHub releases. It sends nothing about you: no ID, no settings, no usage. GitHub sees what any web request shows, your IP address and the app's version. An update is only downloaded when you click **Download**.

The app doesn't phone home in any other way.

Don't take our word for it:

- Settings → Privacy → **Network activity** lists every request the app has made: the full address, how much was transferred, when, and whether it finished, was cancelled or failed.
- The source is open, and every request goes through one place: the downloader in the `tw-models` crate.

## Where your data lives

Everything is kept in one folder:

| OS | Folder |
| --- | --- |
| macOS | `~/Library/Application Support/dev.time2code.tinywispr` |
| Windows | `%APPDATA%\dev.time2code.tinywispr` |
| Linux | `~/.local/share/dev.time2code.tinywispr` |

| File or folder | Contains |
| --- | --- |
| `settings.json` | Your settings |
| `history.json` | Dictation history and the numbers behind your stats |
| `dictionary.json` | Your dictionary |
| `snippets.json` | Your snippets |
| `network-log.json` | The network activity list |
| `models/whisper/`, `models/llm/` | Downloaded and imported models |

These are plain files. Back them up, move them, or delete them whenever you want. Deleting the folder resets the app, including the first-run setup. The files are not encrypted, so anyone who can read your user folder can read your history. If that matters, set history to **Never**.

## Your history

By default the app keeps the text of your dictations, the app each one went to, and when. It keeps both what Whisper heard and the cleaned-up result. Under Settings → Privacy you can keep it for 30 days, 24 hours, or not at all, and delete everything at once.

## The clipboard

To paste, Tiny Wispr briefly puts your text on the clipboard and restores what was there half a second later. On macOS the text is marked as concealed, which clipboard managers that follow the convention skip. With **Type it out** the clipboard isn't touched.

## Permissions, and why

| Permission | Used for | Not used for |
| --- | --- | --- |
| Microphone | Recording while you hold your shortcut (and while the microphone test is open). | Listening at any other time. |
| Accessibility (macOS) | Noticing your shortcut, and pasting. | Reading what's on your screen or in other apps. |
| Input Monitoring (macOS) | Seeing shortcut keys. | Recording what you type. The app looks only for your shortcuts. |

The app also reads the name of the frontmost app, which needs no permission, to choose a writing style. That name is saved in your history entry.
