//! Energy-based speech detection on mono [`SAMPLE_RATE`] audio.
//!
//! Each 20 ms frame is compared with the clip's own noise floor, so a quiet
//! room reads as silence and whispered speech above it survives. Short
//! clicks and bumps are ignored.

use std::ops::Range;

use crate::SAMPLE_RATE;
use crate::level::{rms, to_dbfs};

const FRAME: usize = SAMPLE_RATE as usize / 50;
/// A frame must be this far above the noise floor to count as speech.
const MARGIN_DB: f32 = 10.0;
/// Nothing quieter than this counts as speech, however quiet the room.
const MIN_SPEECH_DB: f32 = -55.0;
/// The noise floor is never assumed louder than this, so a clip that is
/// speech from start to end is not mistaken for noise.
const MAX_NOISE_FLOOR_DB: f32 = -50.0;
/// Loud frames must come in runs at least this long (60 ms) to count.
const MIN_RUN_FRAMES: usize = 3;
/// A clip needs at least this much speech (160 ms) to have any.
const MIN_SPEECH_FRAMES: usize = 8;
/// Kept around the speech when trimming, so soft word edges survive.
pub const TRIM_PADDING_MS: u32 = 150;

/// Whether `samples` contain any speech.
pub fn has_speech(samples: &[f32]) -> bool {
    speech_frames(samples).iter().filter(|&&s| s).count() >= MIN_SPEECH_FRAMES
}

/// `samples` without leading and trailing silence, keeping
/// [`TRIM_PADDING_MS`] on either side. Empty when there is no speech.
pub fn trim_silence(samples: &[f32]) -> &[f32] {
    let frames = speech_frames(samples);
    let (Some(first), Some(last)) = (
        frames.iter().position(|&s| s),
        frames.iter().rposition(|&s| s),
    ) else {
        return &samples[..0];
    };
    let padding = (SAMPLE_RATE * TRIM_PADDING_MS / 1_000) as usize;
    let start = (first * FRAME).saturating_sub(padding);
    let end = ((last + 1) * FRAME + padding).min(samples.len());
    &samples[start..end]
}

/// Chunks are cut no later than this, well inside Whisper's 30 s window.
pub const MAX_CHUNK_MS: u32 = 25_000;
/// Chunks are not cut earlier than this when a longer one fits.
const MIN_CHUNK_MS: u32 = 10_000;
/// A cut at a silence at least this long (200 ms) falls between words.
const MIN_PAUSE_FRAMES: usize = 10;

/// A piece of a long recording, for transcribing one at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Sample range in the recording.
    pub range: Range<usize>,
    /// The chunk has enough speech to transcribe.
    pub has_speech: bool,
    /// The chunk ends in a pause (or at the end of the recording), not in
    /// the middle of a word.
    pub ends_at_pause: bool,
}

/// Splits `samples` into chunks of at most [`MAX_CHUNK_MS`], cutting in the
/// middle of the longest pause, so each chunk starts and ends in silence and
/// no word is cut at Whisper's window edge. Without a pause the cut goes at
/// the quietest moment.
pub fn split_at_pauses(samples: &[f32]) -> Vec<Chunk> {
    let (levels, speech) = analyse(samples);
    let frame_ms = 1_000 / (SAMPLE_RATE as usize / FRAME);
    let max_frames = MAX_CHUNK_MS as usize / frame_ms;
    let min_frames = MIN_CHUNK_MS as usize / frame_ms;
    let mut chunks = Vec::new();
    let mut start = 0;
    while levels.len() - start > max_frames {
        let window = start + min_frames..start + max_frames;
        let (cut, ends_at_pause) = match longest_pause(&speech, window.clone()) {
            Some(pause) if pause.len() >= MIN_PAUSE_FRAMES => ((pause.start + pause.end) / 2, true),
            _ => {
                let quietest = window
                    .clone()
                    .min_by(|&a, &b| levels[a].total_cmp(&levels[b]))
                    .unwrap_or(window.end);
                (quietest, false)
            }
        };
        chunks.push(chunk(&speech, start..cut, ends_at_pause, samples.len()));
        start = cut;
    }
    if start < levels.len() {
        chunks.push(chunk(&speech, start..levels.len(), true, samples.len()));
    }
    chunks
}

fn chunk(speech: &[bool], frames: Range<usize>, ends_at_pause: bool, len: usize) -> Chunk {
    let speech_frames = speech[frames.clone()].iter().filter(|&&s| s).count();
    Chunk {
        range: frames.start * FRAME..(frames.end * FRAME).min(len),
        has_speech: speech_frames >= MIN_SPEECH_FRAMES,
        ends_at_pause,
    }
}

/// The longest run of silent frames inside `window`; the later one on a tie.
fn longest_pause(speech: &[bool], window: Range<usize>) -> Option<Range<usize>> {
    let mut best: Option<Range<usize>> = None;
    let mut i = window.start;
    while i < window.end {
        if speech[i] {
            i += 1;
            continue;
        }
        let run = speech[i..window.end].iter().take_while(|&&s| !s).count();
        if best.as_ref().is_none_or(|b| run >= b.len()) {
            best = Some(i..i + run);
        }
        i += run;
    }
    best
}

fn speech_frames(samples: &[f32]) -> Vec<bool> {
    analyse(samples).1
}

/// Each frame's level in dBFS and whether it is speech.
fn analyse(samples: &[f32]) -> (Vec<f32>, Vec<bool>) {
    let levels: Vec<f32> = samples.chunks(FRAME).map(|f| to_dbfs(rms(f))).collect();
    if levels.is_empty() {
        return (levels, Vec::new());
    }
    let threshold = (noise_floor(&levels) + MARGIN_DB).max(MIN_SPEECH_DB);
    let mut loud: Vec<bool> = levels.iter().map(|&db| db > threshold).collect();
    drop_short_runs(&mut loud);
    (levels, loud)
}

/// The 10th percentile of frame levels, capped at [`MAX_NOISE_FLOOR_DB`].
fn noise_floor(levels: &[f32]) -> f32 {
    let mut sorted = levels.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 10].min(MAX_NOISE_FLOOR_DB)
}

fn drop_short_runs(loud: &mut [bool]) {
    let mut i = 0;
    while i < loud.len() {
        if !loud[i] {
            i += 1;
            continue;
        }
        let run = loud[i..].iter().take_while(|&&s| s).count();
        if run < MIN_RUN_FRAMES {
            loud[i..i + run].fill(false);
        }
        i += run;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Deterministic white noise with the given RMS in dBFS.
    pub(crate) fn noise(seconds: f32, dbfs: f32, seed: u32) -> Vec<f32> {
        let amplitude = 10_f32.powf(dbfs / 20.0) * 3_f32.sqrt();
        let mut state = seed.max(1);
        (0..(seconds * SAMPLE_RATE as f32) as usize)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * amplitude
            })
            .collect()
    }

    /// Voiced, syllable-modulated tone with the given peak RMS in dBFS.
    pub(crate) fn speech_like(seconds: f32, dbfs: f32) -> Vec<f32> {
        let amplitude = 10_f32.powf(dbfs / 20.0) * 2_f32.sqrt();
        (0..(seconds * SAMPLE_RATE as f32) as usize)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let syllables = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                let voice = (2.0 * std::f32::consts::PI * 180.0 * t).sin() * 0.7
                    + (2.0 * std::f32::consts::PI * 360.0 * t).sin() * 0.3;
                amplitude * syllables * voice
            })
            .collect()
    }

    fn mix(base: &mut [f32], at_seconds: f32, add: &[f32]) {
        let start = (at_seconds * SAMPLE_RATE as f32) as usize;
        for (b, a) in base[start..].iter_mut().zip(add) {
            *b += a;
        }
    }

    #[test]
    fn digital_silence_has_no_speech() {
        assert!(!has_speech(&vec![0.0; SAMPLE_RATE as usize]));
        assert!(!has_speech(&[]));
        assert!(trim_silence(&[]).is_empty());
    }

    #[test]
    fn a_quiet_room_has_no_speech() {
        for db in [-75.0, -65.0, -55.0] {
            assert!(!has_speech(&noise(2.0, db, 7)), "room noise at {db} dBFS");
        }
    }

    #[test]
    fn a_click_is_not_speech() {
        let mut room = noise(2.0, -65.0, 3);
        mix(&mut room, 1.0, &noise(0.03, -10.0, 9));
        assert!(!has_speech(&room));
    }

    #[test]
    fn normal_and_whispered_speech_are_found() {
        let mut room = noise(2.0, -65.0, 11);
        mix(&mut room, 0.5, &speech_like(1.0, -25.0));
        assert!(has_speech(&room));

        let mut room = noise(2.0, -65.0, 12);
        mix(&mut room, 0.5, &noise(0.8, -45.0, 99));
        assert!(has_speech(&room), "whisper");
    }

    #[test]
    fn speech_filling_the_whole_clip_is_found() {
        assert!(has_speech(&speech_like(1.0, -25.0)));
    }

    #[test]
    fn trimming_keeps_speech_and_padding() {
        let mut clip = noise(3.0, -65.0, 5);
        mix(&mut clip, 1.0, &speech_like(1.0, -25.0));
        let trimmed = trim_silence(&clip);
        let seconds = trimmed.len() as f32 / SAMPLE_RATE as f32;
        assert!((1.25..=1.4).contains(&seconds), "kept {seconds} s");
        let offset = trimmed.as_ptr() as usize - clip.as_ptr() as usize;
        let start = offset as f32 / 4.0 / SAMPLE_RATE as f32;
        assert!((0.82..=0.9).contains(&start), "starts at {start} s");
    }

    /// `pattern` of (seconds, speech?) segments over quiet room noise.
    fn talk(pattern: &[(f32, bool)]) -> Vec<f32> {
        let total: f32 = pattern.iter().map(|(s, _)| s).sum();
        let mut clip = noise(total, -65.0, 21);
        let mut at = 0.0;
        for &(seconds, speaking) in pattern {
            if speaking {
                mix(&mut clip, at, &speech_like(seconds, -25.0));
            }
            at += seconds;
        }
        clip
    }

    fn seconds(samples: usize) -> f32 {
        samples as f32 / SAMPLE_RATE as f32
    }

    #[test]
    fn short_audio_is_one_chunk() {
        let clip = talk(&[(0.5, false), (5.0, true), (0.5, false)]);
        let chunks = split_at_pauses(&clip);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].range, 0..clip.len());
        assert!(chunks[0].has_speech && chunks[0].ends_at_pause);
        assert!(split_at_pauses(&[]).is_empty());
    }

    #[test]
    fn long_audio_is_cut_in_the_middle_of_the_longest_pause() {
        // Sentences with short pauses, one long pause at 18-19 s.
        let clip = talk(&[
            (6.0, true),
            (0.3, false),
            (6.0, true),
            (0.3, false),
            (5.4, true),
            (1.0, false),
            (8.0, true),
            (0.3, false),
            (8.0, true),
        ]);
        let chunks = split_at_pauses(&clip);
        assert!(chunks.len() >= 2, "{chunks:?}");
        let cut = seconds(chunks[0].range.end);
        assert!((18.3..=18.7).contains(&cut), "cut at {cut} s");
        assert!(chunks[0].ends_at_pause);
        for pair in chunks.windows(2) {
            assert_eq!(pair[0].range.end, pair[1].range.start);
        }
        assert_eq!(chunks.last().map(|c| c.range.end), Some(clip.len()));
        for c in &chunks {
            assert!(seconds(c.range.len()) <= 25.0, "{c:?}");
        }
    }

    #[test]
    fn speech_without_pauses_is_cut_at_the_quietest_moment() {
        let clip = speech_like(60.0, -25.0);
        let chunks = split_at_pauses(&clip);
        assert!(chunks.len() >= 3);
        assert!(!chunks[0].ends_at_pause);
        assert!(chunks.iter().all(|c| seconds(c.range.len()) <= 25.0));
    }

    #[test]
    fn silent_stretches_are_marked() {
        let clip = talk(&[(10.0, true), (0.5, false), (40.0, false)]);
        let chunks = split_at_pauses(&clip);
        assert!(chunks[0].has_speech);
        assert!(!chunks.last().is_some_and(|c| c.has_speech), "{chunks:?}");
    }

    #[test]
    fn trimming_silence_leaves_nothing() {
        assert!(trim_silence(&noise(1.0, -65.0, 1)).is_empty());
    }
}
