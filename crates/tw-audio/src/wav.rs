//! WAV read/write for fixtures and debugging.

use std::io;
use std::path::Path;

use crate::convert::to_mono_16k;
use crate::{AudioError, Recording, Result, SAMPLE_RATE};

/// Reads any PCM or float WAV and converts it to mono at [`SAMPLE_RATE`].
pub fn read(path: &Path) -> Result<Recording> {
    let mut reader = hound::WavReader::open(path).map_err(wav_error)?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(wav_error)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1_i64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(wav_error)?
        }
    };
    let samples = to_mono_16k(&interleaved, spec.channels, spec.sample_rate)?;
    Ok(Recording {
        samples,
        ..Recording::default()
    })
}

/// Writes `recording` as a mono 32-bit float WAV at [`SAMPLE_RATE`].
pub fn write(path: &Path, recording: &Recording) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(wav_error)?;
    for &sample in &recording.samples {
        writer.write_sample(sample).map_err(wav_error)?;
    }
    writer.finalize().map_err(wav_error)
}

fn wav_error(err: hound::Error) -> AudioError {
    match err {
        hound::Error::IoError(err) => AudioError::Io(err),
        other => AudioError::Io(io::Error::new(io::ErrorKind::InvalidData, other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("tw-audio-{}-{name}", std::process::id()))
    }

    #[test]
    fn round_trip() {
        let path = temp_path("round-trip.wav");
        let recording = Recording {
            samples: (0..1_600).map(|i| (i as f32 * 0.01).sin() * 0.5).collect(),
            ..Recording::default()
        };
        write(&path, &recording).unwrap();
        let back = read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(back, recording);
    }

    #[test]
    fn stereo_16_bit_44k_is_converted() {
        let path = temp_path("stereo.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..44_100 {
            let s = ((i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 16_000.0) as i16;
            writer.write_sample(s).unwrap();
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        let recording = read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(recording.samples.len(), SAMPLE_RATE as usize);
        let peak = recording.samples[1_000..15_000]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 16_000.0 / 32_768.0).abs() < 0.02, "peak {peak}");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        let err = read(Path::new("/nonexistent/clip.wav")).unwrap_err();
        assert!(matches!(err, AudioError::Io(_)));
    }

    #[test]
    fn a_non_wav_file_is_rejected() {
        let path = temp_path("not-a-wav.wav");
        std::fs::write(&path, b"definitely not RIFF").unwrap();
        let err = read(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(err, AudioError::Io(e) if e.kind() == io::ErrorKind::InvalidData));
    }
}
