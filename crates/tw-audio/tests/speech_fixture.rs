//! Speech detection on real synthesized speech. Fixtures are made with
//! `scripts/make-speech-fixture.sh` (macOS `say` and `ffmpeg`); the tests are
//! skipped where those tools are missing.

use std::path::{Path, PathBuf};
use std::process::Command;

use tw_audio::{SAMPLE_RATE, level, vad, wav};

fn fixture(name: &str, text: &str, voice: &str) -> Option<Vec<f32>> {
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.wav"));
    if !out.exists() {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/make-speech-fixture.sh");
        let made = Command::new(script)
            .args([text, out.to_str()?, voice])
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
    Some(wav::read(&out).ok()?.samples)
}

/// Deterministic white noise at roughly `dbfs` RMS.
fn room(seconds: f32, dbfs: f32) -> Vec<f32> {
    let amplitude = 10_f32.powf(dbfs / 20.0) * 3_f32.sqrt();
    let mut state = 0x9e37_79b9_u32;
    (0..(seconds * SAMPLE_RATE as f32) as usize)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

/// `speech` scaled to `dbfs` RMS, with a second of room noise either side.
fn in_a_quiet_room(speech: &[f32], dbfs: f32) -> (Vec<f32>, usize) {
    let gain = 10_f32.powf(dbfs / 20.0) / level::rms(speech);
    let lead = SAMPLE_RATE as usize;
    let mut clip = room(2.0 + speech.len() as f32 / SAMPLE_RATE as f32, -65.0);
    for (c, s) in clip[lead..].iter_mut().zip(speech) {
        *c += s * gain;
    }
    (clip, lead)
}

#[test]
fn spoken_sentence_is_found_and_trimmed() {
    let Some(speech) = fixture(
        "sentence",
        "The quick brown fox jumps over the lazy dog.",
        "Samantha",
    ) else {
        return;
    };
    let (clip, lead) = in_a_quiet_room(&speech, -24.0);
    assert!(vad::has_speech(&clip));

    let trimmed = vad::trim_silence(&clip);
    let start = (trimmed.as_ptr() as usize - clip.as_ptr() as usize) / size_of::<f32>();
    let end = start + trimmed.len();
    let padding = (SAMPLE_RATE * vad::TRIM_PADDING_MS / 1_000) as usize;
    // `say` leaves a little silence of its own around the words.
    assert!(start + padding >= lead, "trim starts before the speech");
    assert!(
        start < lead + SAMPLE_RATE as usize / 4,
        "trim cut into the speech"
    );
    assert!(end <= lead + speech.len() + padding);
    assert!(
        end > lead + speech.len() - SAMPLE_RATE as usize / 4,
        "trim cut the last word"
    );
}

#[test]
fn whispered_sentence_is_found() {
    let Some(speech) = fixture("whisper", "This is a quiet sentence.", "Whisper") else {
        return;
    };
    let (clip, _) = in_a_quiet_room(&speech, -45.0);
    assert!(vad::has_speech(&clip));
}

#[test]
fn the_room_alone_is_not_speech() {
    assert!(!vad::has_speech(&room(3.0, -65.0)));
}
