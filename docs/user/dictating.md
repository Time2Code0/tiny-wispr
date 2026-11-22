# Dictating

## Two ways to talk

**Hold to talk.** Hold the push-to-talk shortcut, speak, and let go. The text is pasted into the focused field.

**Hands-free.** For longer dictations, start it without holding anything:

- double-tap the push-to-talk shortcut (the second press within half a second), or
- press the hands-free shortcut, or
- click the pill.

Stop with the hands-free shortcut, the push-to-talk shortcut, or the stop button on the pill. Your text is then transcribed and pasted.

## Default shortcuts

| Action | macOS | Windows and Linux |
| --- | --- | --- |
| Push to talk | `fn` (`⌃⌥` without an Fn key) | `Ctrl+Win` / `Ctrl+Super` |
| Hands-free | `fn Space` | `Ctrl+Win+Space` |
| Cancel | `Esc` | `Esc` |
| Paste last transcript | `⌃⌘V` | `Alt+Shift+Z` |

Change any of them under Settings → General. See [Settings](settings.md#shortcuts) for the rules a shortcut must follow.

## What the pill shows

The pill is a small capsule at the bottom centre of the screen the mouse is on. It's always dark, never takes focus, and doesn't appear in the Dock or the app switcher.

| State | What you see |
| --- | --- |
| Idle | A tiny, quiet capsule. Hover over it for a hint with your shortcut. Clicks outside its shape go to the app underneath. |
| Listening (holding) | Bars that rise and fall with your voice. |
| Listening (hands-free) | A cancel button on the left, the bars, a stop button on the right. |
| Processing | "Transcribing", then "Polishing" if a language model is cleaning up the text. |
| Notice | A small card above the pill that goes away by itself. |

Right-click the idle pill for **Hide for 1 hour**, **Change microphone** and **Settings**. Under Settings → System you can have the pill appear only while you dictate.

## Cancelling and undoing

Press `Esc`, or the pill's cancel button, to throw away what you said. The pill shows **Dictation cancelled** with an **Undo** button for a few seconds. Undo transcribes the discarded audio and pastes it, as if you had finished normally.

Some things cancel silently:

- A press shorter than half a second. A single quick tap does nothing, which also leaves room for a double-tap.
- Pressing another shortcut that includes your push-to-talk modifiers, within the first second. For example `fn` held for `fn+F` is not a dictation.

## Notices

| Notice | Meaning |
| --- | --- |
| Dictation cancelled · Undo | You cancelled. Undo pastes it anyway. |
| Didn't catch that | No speech was detected, so nothing was pasted. |
| No microphone found | Plug one in, or pick one in Settings. |
| Still processing your last dictation | Wait a moment: presses are ignored until the last dictation is done. |
| Couldn't paste. Press ⌃⌘V to paste it | The text couldn't be inserted. Your paste-last shortcut puts it in. |
| Download a voice model to start · Open settings | No voice model is installed. |
| The 19-minute warning | A session stops at 20 minutes. What you said is transcribed, not lost. |

## Paste last transcript

The paste-last shortcut (also in the menu bar icon's menu) pastes your most recent dictation again, into whatever app has focus. Useful when the paste failed or landed in the wrong window.

## Where the words go

Tiny Wispr pastes by putting the text on the clipboard, sending the paste keystroke, and restoring your previous clipboard half a second later. On macOS the text is marked as concealed so clipboard managers skip it. If an app blocks pasting, switch to **Type it out** under Settings → System.

The app that had focus when you finished decides the writing style: see [Text, styles, dictionary and snippets](text-and-style.md).

## History and stats

Every dictation is saved on your computer (you can change that under Settings → Privacy). Home lists them by day with the time and the app they went to. Hover a row for:

- **Copy**
- **Undo AI edit / Redo AI edit**, when cleanup changed the text, to see what Whisper actually heard
- **Delete**

Search finds words in either version of the text, or the app name. The numbers above the list are your streak, total words, words per minute, dictations and fixes made (fillers removed and dictionary corrections), with a heatmap of the last 26 weeks. Deleting history, one entry or all of it, doesn't change these numbers: they're kept apart from the entries.
