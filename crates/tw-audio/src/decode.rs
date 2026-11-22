//! Audio files to mono `f32` at [`SAMPLE_RATE`]: WAV, MP3, AAC in an MP4
//! container (`.m4a`, and the audio of `.mp4` and `.mov` videos), FLAC and
//! Ogg Vorbis.

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use crate::convert::Converter;
use crate::{Recording, SAMPLE_RATE};

/// File extensions [`decode_file`] accepts, lowercase.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "mp4", "mov", "flac", "ogg", "oga",
];

/// Longer files are refused: three hours.
pub const MAX_FILE_MS: u64 = 3 * 60 * 60 * 1_000;

const MAX_SAMPLES: u64 = MAX_FILE_MS * SAMPLE_RATE as u64 / 1_000;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("This file type isn't supported")]
    Unsupported,
    #[error("This file has no audio track")]
    NoAudioTrack,
    #[error("This file is damaged or isn't an audio file")]
    Corrupt,
    #[error("This file is empty")]
    Empty,
    #[error("This file is longer than 3 hours")]
    TooLong,
    #[error("Couldn't open this file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
}

/// Whether `path` has one of the [`SUPPORTED_EXTENSIONS`].
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Decodes the first audio track of `path`. Checks `cancel` between packets.
pub fn decode_file(path: &Path, cancel: &AtomicBool) -> Result<Recording, DecodeError> {
    if !is_supported(path) {
        return Err(DecodeError::Unsupported);
    }
    let file = File::open(path)?;
    if file.metadata()?.len() == 0 {
        return Err(DecodeError::Empty);
    }
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|err| match err {
            SymphoniaError::IoError(err) if err.kind() != std::io::ErrorKind::UnexpectedEof => {
                DecodeError::Io(err)
            }
            _ => DecodeError::Corrupt,
        })?;
    decode_track(format, cancel)
}

fn decode_track(
    mut format: Box<dyn FormatReader + '_>,
    cancel: &AtomicBool,
) -> Result<Recording, DecodeError> {
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(DecodeError::NoAudioTrack)?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or(DecodeError::NoAudioTrack)?
        .clone();
    if let (Some(frames), Some(rate)) = (track.num_frames, params.sample_rate)
        && rate > 0
        && frames.saturating_mul(1_000) / u64::from(rate) > MAX_FILE_MS
    {
        return Err(DecodeError::TooLong);
    }
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|err| match err {
            SymphoniaError::Unsupported(_) => DecodeError::Unsupported,
            _ => DecodeError::Corrupt,
        })?;

    let mut samples = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    // Rebuilt if the stream changes rate or channel count part-way.
    let mut converter: Option<(Converter, u32, usize)> = None;
    let mut bad_packets = 0_usize;
    let mut good_packets = 0_usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DecodeError::Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) | Err(SymphoniaError::ResetRequired) => break,
            // A truncated file: keep what was there.
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::IoError(err)) => return Err(DecodeError::Io(err)),
            Err(_) => return Err(DecodeError::Corrupt),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // One bad packet is a click in the audio, not a reason to give up.
            Err(SymphoniaError::DecodeError(_)) => {
                bad_packets += 1;
                continue;
            }
            Err(SymphoniaError::Unsupported(_)) => return Err(DecodeError::Unsupported),
            Err(_) => return Err(DecodeError::Corrupt),
        };
        good_packets += 1;
        let rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count();
        decoded.copy_to_vec_interleaved(&mut interleaved);
        if converter
            .as_ref()
            .is_none_or(|(_, r, c)| (*r, *c) != (rate, channels))
        {
            if let Some((previous, _, _)) = converter.take() {
                previous.finish(&mut samples);
            }
            let channel_count = u16::try_from(channels).map_err(|_| DecodeError::Unsupported)?;
            let fresh = Converter::new(rate, channel_count).map_err(|_| DecodeError::Corrupt)?;
            converter = Some((fresh, rate, channels));
        }
        if let Some((converter, _, _)) = converter.as_mut() {
            converter.push(&interleaved, &mut samples);
        }
        if samples.len() as u64 > MAX_SAMPLES {
            return Err(DecodeError::TooLong);
        }
    }
    if let Some((converter, _, _)) = converter {
        converter.finish(&mut samples);
    }
    if good_packets == 0 && bad_packets > 0 {
        return Err(DecodeError::Corrupt);
    }
    if bad_packets > 0 {
        tracing::warn!(bad_packets, good_packets, "skipped undecodable audio");
    }
    Ok(Recording {
        samples,
        ..Recording::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_matched_without_case() {
        assert!(is_supported(Path::new("memo.MP3")));
        assert!(is_supported(Path::new("/a/b/call.m4a")));
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(!is_supported(Path::new("noextension")));
    }

    #[test]
    fn unsupported_extension_is_refused_before_opening() {
        let never = AtomicBool::new(false);
        assert!(matches!(
            decode_file(Path::new("/does/not/exist.txt"), &never),
            Err(DecodeError::Unsupported)
        ));
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let never = AtomicBool::new(false);
        assert!(matches!(
            decode_file(Path::new("/does/not/exist.wav"), &never),
            Err(DecodeError::Io(_))
        ));
    }
}
