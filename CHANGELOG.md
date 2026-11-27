# Changelog

## 0.1.0 (2026-11-27)

### Features

- Offer launch at login on the last onboarding step
- Run debug builds as tiny wispr dev next to the installed app
- Check for, download and install updates from settings
- Fetch small files into memory with the logged downloader

### Fixes

- Let the first release use the current version
- Keep launch at login in sync with the system's login items
- Mention update checks in the models page's network promise
- Build whisper.cpp and llama.cpp for a portable cpu baseline
- Let model imports finish on windows
- Read the windows clipboard with as_chunks for current clippy
- Build the dock reopen handler on macos only
- Stop the linux app crashing at start when the pill is hidden
