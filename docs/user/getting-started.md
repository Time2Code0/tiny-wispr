# Getting started

Tiny Wispr turns speech into text on your own computer. You hold a key, say something, let go, and the words appear in whatever app you're typing in.

## What you need

- macOS 13 or later. macOS is the platform the app is built and tested on. Windows and Linux (X11) are supported in the code, but get less testing. Linux on Wayland can't use global shortcuts or paste into other apps.
- A microphone.
- Room for a voice model: from about 75 MB (Tiny) to about 3 GB (Large v3). The setup recommends one for your computer.

## Install

Download `Tiny-Wispr_<version>_aarch64.dmg` from the [latest release](https://github.com/Time2Code0/tiny-wispr/releases/latest), open it and drag Tiny Wispr to Applications. Releases are built for Macs with Apple Silicon. On an Intel Mac, build the app from source: see [Building and releasing](../operations/building-and-releasing.md).

Tiny Wispr isn't notarized by Apple, so the first time you open it macOS says it can't check the app for malicious software. Open **System Settings → Privacy & Security**, scroll down to the message about Tiny Wispr, and click **Open Anyway**. You only do this once: updates install from inside the app (see [Updating](updating.md)).

## First run

The first time you open the app it walks you through seven steps. You can quit at any point: it picks up where you stopped. Steps you've already done, such as permissions you granted earlier, are skipped.

1. **Welcome.** One screen about what the app is and the privacy promise.
2. **Permissions.** See [Permissions](#permissions) below.
3. **Microphone.** Say a few words. The bars move when Tiny Wispr hears you. If you have more than one microphone, pick the one you want here.
4. **Shortcut.** Shows your push-to-talk shortcut. Hold it to talk; tap it twice to keep listening hands-free. Choose Change to record a different one.
5. **Voice model.** Pick the Whisper model that turns speech into text, and download it. One is recommended for your computer. You can change it later on the Models page.
6. **Polish (optional).** A small local language model can tidy up your text further. You can skip this and add it later.
7. **Try it.** Click in the box, hold your shortcut, and say something. Release to see the text. Choose Finish to open the main window.

You can't dictate until a voice model is downloaded. The pill tells you so ("Download a voice model to start") and offers to open the Models page.

### Permissions

Tiny Wispr only asks for a permission when you click a button that needs it.

| Permission | Why | Platform |
| --- | --- | --- |
| Microphone | To hear you. | macOS; Windows can turn it off for desktop apps in Privacy settings |
| Accessibility | To notice your shortcut while another app has focus, and to paste into that app. | macOS |
| Input Monitoring | Where macOS asks for it, so the app can see your shortcut keys. | macOS |

Each row shows a check mark once the permission is granted. macOS only shows its prompt the first time; after that the button opens System Settings instead, where you switch Tiny Wispr on.

If you change a permission later, or the app is rebuilt from source, you may have to grant it again. See [Troubleshooting](troubleshooting.md).

## Your first dictation

1. Click into any text field: a note, a chat, an email.
2. Hold your push-to-talk shortcut. On a Mac that's the `fn` key by default (`⌃⌥` on keyboards without one). A start sound plays and the bars in the pill move.
3. Speak.
4. Let go. The pill shows "Transcribing", and a moment later your text is pasted.

Next: [Dictating](dictating.md) covers hands-free mode, cancelling and every shortcut.

## The main window

The app lives in the menu bar (and the Dock, unless you turn that off). Open the main window from the menu bar icon, with `⌘,`, or by clicking the Dock icon.

- **Home:** your stats and dictation history.
- **Dictionary:** words to spell correctly.
- **Snippets:** short phrases that expand to longer text.
- **Style:** how formal your text should be, per kind of app.
- **Models:** download, switch and import models.
- **Settings:** everything else.

Closing the window doesn't quit the app. Use Quit in the menu bar icon's menu.
