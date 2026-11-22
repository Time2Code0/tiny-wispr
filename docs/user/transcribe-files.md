# Transcribing audio files

Besides dictating, Tiny Wispr can transcribe a recording: a voice memo, a meeting, the audio of a video. It uses the same voice model, formatting and (if selected) language model as dictation, and nothing leaves your computer.

## How

On Home, choose **Transcribe a file…** and pick a file, or drag one onto the window.

Supported: WAV, MP3, M4A, AAC, FLAC, Ogg Vorbis, and the audio of MP4 and MOV videos. Files up to three hours long.

A card shows the stages: decoding, transcribing, polishing. You can cancel at any time. When it's done:

- **Copy** puts the transcript on the clipboard.
- **Save as text…** writes a `.txt` file named after the recording.

The transcript is also added to your history, so it shows up on Home like a dictation.

## Good to know

- One file at a time.
- The audio is cut at pauses and transcribed piece by piece, then joined into paragraphs. Long pauses start a new paragraph.
- A live dictation always goes first. If you start dictating while a file is being transcribed, the file waits between pieces and carries on when you've finished.
- The file's own sound is never changed or stored.
- Styles don't change the text: a file has no app to take a style from, so the Other style applies.

If a file can't be used, the card says why: no voice model installed, no audio track, no speech found, an unsupported type, or a damaged file.
