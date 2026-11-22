//! Decoding real files. A WAV of synthesized speech is made with
//! `scripts/make-speech-fixture.sh` and re-encoded with `ffmpeg`; tests are
//! skipped where those tools (or one of ffmpeg's encoders) are missing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use tw_audio::decode::{DecodeError, decode_file};
use tw_audio::{level, wav};

static NEVER: AtomicBool = AtomicBool::new(false);

fn dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("decode");
    fs::create_dir_all(&dir).ok();
    dir
}

fn original() -> Option<PathBuf> {
    let out = dir().join("original.wav");
    if !out.exists() {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-speech-fixture.sh");
        let made = Command::new(script)
            .args([
                "Files are decoded on this device. Nothing leaves the computer.",
                out.to_str()?,
                "Samantha",
            ])
            .output()
            .ok()?;
        if !made.status.success() {
            eprintln!(
                "skipping: could not make a speech fixture: {}",
                String::from_utf8_lossy(&made.stderr)
            );
            return None;
        }
    }
    Some(out)
}

/// Runs ffmpeg with `args` between the input and the output file.
fn ffmpeg(inputs: &[&str], args: &[&str], name: &str) -> Option<PathBuf> {
    let out = dir().join(name);
    if out.exists() {
        return Some(out);
    }
    let made = Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y"])
        .args(inputs)
        .args(args)
        .arg(&out)
        .output()
        .ok()?;
    if !made.status.success() {
        eprintln!(
            "skipping {name}: ffmpeg could not make it: {}",
            String::from_utf8_lossy(&made.stderr)
        );
        return None;
    }
    Some(out)
}

fn encoded(codec_args: &[&str], name: &str) -> Option<(Vec<f32>, PathBuf)> {
    let original = original()?;
    let reference = wav::read(&original).ok()?.samples;
    let input = original.to_str()?.to_owned();
    // 44.1 kHz stereo, so channel mixing and resampling are exercised too.
    // The pan filter copies the voice to both channels at full level, where
    // `-ac 2` would lower it by 3 dB.
    let mut args = vec!["-ar", "44100", "-af", "pan=stereo|c0=c0|c1=c0"];
    args.extend_from_slice(codec_args);
    let path = ffmpeg(&["-i", &input], &args, name)?;
    Some((reference, path))
}

fn assert_matches_original(codec_args: &[&str], name: &str) {
    let Some((reference, path)) = encoded(codec_args, name) else {
        return;
    };
    let decoded = decode_file(&path, &NEVER)
        .unwrap_or_else(|err| panic!("{name}: {err}"))
        .samples;
    let diff_ms = (decoded.len() as i64 - reference.len() as i64).abs() * 1_000 / 16_000;
    assert!(
        diff_ms <= 100,
        "{name}: {} samples decoded, {} expected",
        decoded.len(),
        reference.len()
    );
    let ratio = level::rms(&decoded) / level::rms(&reference);
    assert!(
        (0.85..=1.15).contains(&ratio),
        "{name}: energy ratio {ratio}"
    );
}

#[test]
fn wav_decodes() {
    assert_matches_original(&["-c:a", "pcm_s16le"], "speech.wav");
}

#[test]
fn mp3_decodes() {
    assert_matches_original(&["-c:a", "libmp3lame", "-b:a", "96k"], "speech.mp3");
}

#[test]
fn m4a_decodes() {
    assert_matches_original(&["-c:a", "aac", "-b:a", "96k"], "speech.m4a");
}

#[test]
fn flac_decodes() {
    assert_matches_original(&["-c:a", "flac"], "speech.flac");
}

#[test]
fn ogg_vorbis_decodes() {
    assert_matches_original(&["-c:a", "libvorbis", "-q:a", "4"], "speech.ogg");
}

fn video(audio: bool, name: &str) -> Option<PathBuf> {
    let original = original()?;
    let input = original.to_str()?.to_owned();
    let mut inputs = vec!["-f", "lavfi", "-i", "color=c=gray:s=64x64:d=4"];
    let mut args = vec!["-c:v", "mpeg4"];
    if audio {
        inputs.extend(["-i", &input]);
        args.extend(["-c:a", "aac", "-shortest"]);
    }
    ffmpeg(&inputs, &args, name)
}

#[test]
fn audio_of_mp4_and_mov_videos_decodes() {
    for name in ["video.mp4", "video.mov"] {
        let Some(path) = video(true, name) else {
            return;
        };
        let decoded = decode_file(&path, &NEVER).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert!(
            decoded.duration_ms() > 2_000,
            "{name}: {}",
            decoded.duration_ms()
        );
        assert!(level::rms(&decoded.samples) > 0.01, "{name} is silent");
    }
}

#[test]
fn video_without_audio_has_no_audio_track() {
    let Some(path) = video(false, "silent-video.mp4") else {
        return;
    };
    assert!(matches!(
        decode_file(&path, &NEVER),
        Err(DecodeError::NoAudioTrack)
    ));
}

#[test]
fn text_renamed_to_mp3_is_corrupt() {
    let path = dir().join("notes.mp3");
    fs::write(&path, "Shopping list: milk, eggs, bread.\n".repeat(200)).ok();
    assert!(matches!(
        decode_file(&path, &NEVER),
        Err(DecodeError::Corrupt)
    ));
}

#[test]
fn empty_file_is_refused() {
    let path = dir().join("empty.m4a");
    fs::write(&path, b"").ok();
    assert!(matches!(
        decode_file(&path, &NEVER),
        Err(DecodeError::Empty)
    ));
}

#[test]
fn cancel_stops_decoding() {
    let Some((_, path)) = encoded(&["-c:a", "flac"], "speech.flac") else {
        return;
    };
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        decode_file(&path, &cancelled),
        Err(DecodeError::Cancelled)
    ));
}

#[test]
fn files_over_three_hours_are_refused() {
    let Some(path) = ffmpeg(
        &["-f", "lavfi", "-i", "anullsrc=r=8000:cl=mono"],
        &["-t", "10860", "-c:a", "flac"],
        "long.flac",
    ) else {
        return;
    };
    assert!(matches!(
        decode_file(&path, &NEVER),
        Err(DecodeError::TooLong)
    ));
}
