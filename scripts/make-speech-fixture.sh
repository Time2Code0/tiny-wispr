#!/usr/bin/env bash
# Makes a 16 kHz mono 16-bit WAV of synthesized speech for tests.
# Usage: scripts/make-speech-fixture.sh "<text>" <out.wav> [voice]
# Needs macOS `say` and `ffmpeg`. Write fixtures under .afk/ or target/, never
# into tracked paths.
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 \"<text>\" <out.wav> [voice]" >&2
  exit 64
fi

text=$1
out=$2
voice=${3:-}

for tool in say ffmpeg; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is not installed" >&2
    exit 69
  fi
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

say_args=(-o "$tmp/speech.aiff")
if [[ -n $voice ]]; then
  say_args+=(-v "$voice")
fi
say "${say_args[@]}" -- "$text"

mkdir -p "$(dirname "$out")"
ffmpeg -nostdin -loglevel error -y -i "$tmp/speech.aiff" -ar 16000 -ac 1 -c:a pcm_s16le "$out"
