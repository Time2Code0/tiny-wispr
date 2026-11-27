# Settings

Open Settings from the sidebar, the menu bar icon, or `⌘,`. It has four sections. Changes apply right away and are saved on your computer.

## General

### Shortcuts

| Shortcut | What it does |
| --- | --- |
| Push to talk | Hold to dictate, release to paste. Tap twice for hands-free. |
| Hands-free | Press to start, press again to stop and paste. |
| Cancel | Stop and throw away what you said. |
| Paste last transcript | Paste your last dictation again. |

Choose a shortcut to record a new one: press the keys you want, then let go. While recording, other shortcuts are paused so they don't trigger a dictation. The app tells you when a shortcut can't be used:

- It needs at least one modifier key (`Ctrl`, `Alt`, `fn` …), except Cancel, which can be a lone key. `Esc` alone only works for Cancel.
- At most three keys.
- It must not already be used by another Tiny Wispr action.
- It must not be a system shortcut reserved by the OS.
- `fn` only exists on macOS.

**Reset shortcuts** restores the defaults for your platform.

### Microphone

Pick an input device, or follow the system default. Below it, a level meter lets you test it. If you unplug the microphone mid-dictation, what was recorded so far is transcribed.

### Languages

By default Whisper detects the language of every dictation. Turn auto-detect off to name the languages you speak: with one chosen, Whisper is told to expect it; with several, it picks among them. This can improve accuracy and speed. English-only voice models ignore the setting.

## System

| Setting | Default | What it does |
| --- | --- | --- |
| Launch at login | Off | Start Tiny Wispr when you log in. |
| Show the pill at all times | On | When off, the pill only appears while you dictate. |
| Sounds | On | A soft sound when the microphone starts and stops. |
| Show in Dock | On | macOS only. When off, Tiny Wispr lives in the menu bar. |
| Theme | System | System, Light or Dark. The pill is always dark. |

### Inserting text

How your words get into the app:

- **Paste** (default): puts the text on the clipboard, sends the paste shortcut, then restores your clipboard.
- **Type it out**: types the text as keystrokes, for apps that block paste. Slower for long text.

## Privacy

- **History**: keep dictations forever, 30 days, 24 hours, or never. Shorter settings delete older entries right away. Your stats are kept apart from history, so they stay even when history is deleted.
- **Delete all history**: removes every saved dictation. Your stats are kept.
- **Permissions**: shows each permission's status, with a button to grant it or open System Settings.
- **Where your data lives**: the folder that holds everything the app stores.
- **Network activity**: every network request the app has ever made, with the address, size, time and result. See [Privacy](privacy.md).

## About

- The version.
- **Updates**: **Check for updates** asks GitHub whether a newer version is out. When there is one, the page shows what's new and offers **Download**, then **Restart to update**. See [Updating](updating.md).
- **Check for updates automatically**: off by default. When on, the app checks once a day while it's open (the first check a minute after it starts). It never downloads an update on its own.
- The open-source projects Tiny Wispr is built on.

## Menu bar icon

Open Tiny Wispr, a microphone submenu, Paste last transcript, Settings, and Quit. Once an update is downloaded, **Restart to update to <version>** appears at the top. A template icon follows the menu bar's light and dark look.
