# Updating

Tiny Wispr updates itself from inside the app. Nothing is downloaded or installed without you clicking.

## Check for an update

Open Settings → About. Under **Updates**, click **Check for updates**. The app downloads a small file from its GitHub releases that says which version is newest, and tells you either that it's up to date or which version is available, with what's new in it.

To have the app check for you, turn on **Check for updates automatically**. It's off unless you turn it on. When it's on, the app checks a minute after it starts and then once a day while it's open. It only checks: you still choose when to download.

Every check is listed under Settings → Privacy → **Network activity**. With **Stay offline** on, the app doesn't check at all.

## Install it

1. Click **Download**. A progress bar shows how far along it is. The download is checked against Tiny Wispr's signature before it's kept; one that doesn't match is thrown away.
2. Click **Restart to update**, or choose **Restart to update** in the menu bar icon's menu. Tiny Wispr replaces itself and opens again on the new version.

If a dictation or a file transcription is running, finish it first: the restart would cut it off, so the app waits.

If Tiny Wispr is in a folder your user account can't write to, macOS asks for an administrator password to replace it.

## Your permissions stay

Every release is signed with the same certificate, so macOS keeps the Microphone, Accessibility and Input Monitoring permissions across updates. You don't need to grant them again, and you don't need to click **Open Anyway** again either: that is only for the first copy you download from the website.

## When something goes wrong

- **"Couldn't reach the update server"**: check your internet connection and try again.
- **"The download didn't match its signature"**: the file was damaged on the way, or wasn't made by Tiny Wispr. Nothing was installed. Click **Try again**.
- **"No release was found on the update server"**: there's no published release to compare with yet.

You can always install a release by hand: download the DMG from the [releases page](https://github.com/Time2Code0/tiny-wispr/releases) and drag Tiny Wispr to Applications, replacing the old copy. Your settings, history and models are kept, because they live in a separate folder ([Privacy](privacy.md#where-your-data-lives)).
