//! Microphone capture for Tiny Wispr.
//!
//! Captures from an input device, converts to mono `f32` at [`SAMPLE_RATE`],
//! reports a smoothed 0..1 level about 30 times a second, and keeps a short
//! pre-roll so the first words are not lost. Also: energy-based speech
//! detection, audio file decoding, and WAV helpers for tests and debugging.
//!
//! Tests never open the microphone. One ignored test records two seconds
//! from the default input for a person at the machine to run:
//! `cargo test -p tw-audio -- --ignored --nocapture records_from_the_default_microphone`.

use std::sync::Arc;

mod buffer;
mod capture;
pub mod convert;
pub mod decode;
pub mod level;
pub mod vad;
pub mod wav;

pub use capture::{ActiveDevice, Recorder, list_input_devices};
pub use decode::{DecodeError, decode_file};
pub use tw_core::MicrophoneInfo;

/// Whisper's input rate.
pub const SAMPLE_RATE: u32 = 16_000;
/// How often the level callback fires.
pub const LEVEL_HZ: u32 = 30;
/// Audio kept from before [`Recorder::start`] while the stream is warm.
pub const PRE_ROLL_MS: u32 = 300;
/// Recording stops growing here: the 20-minute session limit.
pub const MAX_RECORDING_MS: u32 = 20 * 60 * 1_000;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("not implemented")]
    NotImplemented,
    #[error("no microphone found")]
    NoDevice,
    #[error("the microphone is in use by another app")]
    DeviceBusy,
    #[error("microphone access was denied")]
    PermissionDenied,
    #[error("audio stream failed: {0}")]
    Stream(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = AudioError> = std::result::Result<T, E>;

/// Why a recording ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptureEnd {
    #[default]
    Stopped,
    /// The microphone disconnected or its stream failed; the audio up to
    /// that point is kept.
    DeviceLost,
    /// [`MAX_RECORDING_MS`] was reached; later audio was dropped.
    LimitReached,
}

/// Something the app should react to while capturing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEvent {
    /// "Microphone disconnected": stop and transcribe what was recorded.
    DeviceLost,
    /// The session limit was reached: stop and transcribe.
    LimitReached,
}

/// Mono samples at [`SAMPLE_RATE`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub samples: Vec<f32>,
    pub ended: CaptureEnd,
}

impl Recording {
    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1_000 / u64::from(SAMPLE_RATE)
    }

    /// Every sample is exactly zero. macOS delivers this instead of an error
    /// when microphone access is denied, and some devices do after another
    /// app released them.
    pub fn is_digital_silence(&self) -> bool {
        self.samples.iter().all(|&s| s == 0.0)
    }
}

/// Receives a smoothed 0..1 RMS level about [`LEVEL_HZ`] times a second.
pub type LevelCallback = Arc<dyn Fn(f32) + Send + Sync>;
/// Receives [`CaptureEvent`]s.
pub type EventCallback = Arc<dyn Fn(CaptureEvent) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration() {
        let rec = Recording {
            samples: vec![0.0; SAMPLE_RATE as usize / 2],
            ..Recording::default()
        };
        assert_eq!(rec.duration_ms(), 500);
    }

    #[test]
    fn digital_silence() {
        let mut rec = Recording {
            samples: vec![0.0; 100],
            ..Recording::default()
        };
        assert!(rec.is_digital_silence());
        rec.samples[50] = 1e-7;
        assert!(!rec.is_digital_silence());
    }
}
