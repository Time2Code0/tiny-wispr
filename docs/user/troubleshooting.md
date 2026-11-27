# Troubleshooting

## Nothing happens when I hold the shortcut

Home shows a banner when the app can't see your shortcut. It says which of these applies.

**"Tiny Wispr can't see your shortcut" (macOS).** The Accessibility permission is missing. Open Settings → Privacy → Permissions, grant it, and follow System Settings.

If Accessibility is already switched on in System Settings but the banner stays: macOS ties the permission to the app's signature, and a rebuilt or updated copy has a different one. In System Settings → Privacy & Security → Accessibility, select Tiny Wispr, remove it with the minus button, and add it again (or grant it from Tiny Wispr). Do the same for Input Monitoring if it's listed.

**"Shortcuts are paused".** A password field, or another app, turned on macOS secure input, which hides keystrokes from every app. Click out of the password field and shortcuts work again.

**"Shortcuts don't work here" (Linux).** On Wayland an app can't listen for global keys or paste into other apps. Click the pill to dictate, or log in to an X11 session.

**"Tiny Wispr can't see your shortcut" with "Quit and reopen".** The key listener stopped. Quit from the menu bar icon and open the app again.

Also check that the shortcut isn't taken: a shortcut another app swallows, or a system shortcut, never reaches Tiny Wispr. Try a different one under Settings → General.

## The pill says "Download a voice model to start"

You need a voice model before you can dictate. Open Models and download one. A recommended model is marked.

## The pill says "No microphone found" or the bars don't move

- Check the microphone under Settings → General and use the level meter to test it.
- Another app may be holding the microphone. Close it and try again.
- macOS: Settings → Privacy → Permissions shows whether Microphone is allowed. If the meter is flat but the pill says it's recording, the permission is probably off.
- Windows: turn on "Let desktop apps access your microphone" in Privacy settings.

## "Didn't catch that"

No speech was found in the recording. Speak closer to the microphone, check the level meter, and hold the key a moment before talking. Very quiet rooms are fine, but a loud background can hide speech. A larger voice model copes better with noise and accents.

## It transcribes but doesn't paste

- The pill says "Couldn't paste. Press ⌃⌘V to paste it": use your paste-last shortcut (or the menu bar icon's Paste last transcript) with the cursor in the right place.
- Accessibility must be granted on macOS for pasting.
- Some apps block pasted text. Try **Type it out** under Settings → System.

## The wrong words, names or spelling

- Add the name to your [dictionary](text-and-style.md#dictionary). With a misspelling, it's corrected every time.
- Try a larger voice model, or a non-English-only (or English-only) one that matches what you speak.
- Under Settings → General turn off auto-detect and choose your languages.

## Dictation is slow

- The first dictation after a pause loads the voice model again. After 10 idle minutes it's released to free memory.
- Pick a smaller voice model, or a compressed copy.
- Polish adds a second or two. Choose **No language model** on Models, or a smaller one, if that's too long.
- Very long dictations take longer to transcribe. The pill shows the stage it's in.

## It pasted my text differently from what I said

That's cleanup. Under Style, set cleanup to **None** for the raw transcript, or **Light** for just fillers and punctuation. In History, **Undo AI edit** shows what Whisper heard. If the language model is rewriting too much, use a lower level or turn it off.

## Downloads fail or won't start

- A failed download keeps what it got: select Download again to resume.
- Check Network activity under Settings → Privacy for the address and error.
- Make sure you have free disk space: the app leaves 100 MB spare and refuses downloads that don't fit.

## I can't see the menu bar icon

On Macs with a notch, icons that don't fit are hidden behind it. Quit a few other menu bar apps or make room. On recent macOS versions check System Settings → Menu Bar to be sure Tiny Wispr is allowed. You can also open the app with `⌘,` while one of its windows is focused, or by clicking its Dock icon.

## Start fresh

Quit the app and delete its data folder (see [Privacy](privacy.md#where-your-data-lives)). Delete only `settings.json` to keep your history and models but go through setup again. A file the app can't read is renamed to `<name>.json.bad` rather than overwritten, so you can recover its content.
