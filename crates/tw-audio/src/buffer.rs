//! What happens to converted audio as it arrives, independent of cpal.

use std::collections::VecDeque;

use crate::level::{LevelMeter, RmsAccumulator};
use crate::{CaptureEnd, CaptureEvent, Recording};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The stream is open but nobody is listening: only the pre-roll fills.
    Idle,
    /// Levels only, nothing kept (microphone test).
    Monitoring,
    Recording,
}

pub(crate) struct CaptureBuffer {
    mode: Mode,
    pre_roll: VecDeque<f32>,
    pre_roll_len: usize,
    recorded: Vec<f32>,
    max_len: usize,
    window: RmsAccumulator,
    meter: LevelMeter,
    limit_reached: bool,
    device_lost: bool,
    unreported: Vec<CaptureEvent>,
}

impl CaptureBuffer {
    pub(crate) fn new(pre_roll_len: usize, max_len: usize) -> Self {
        Self {
            mode: Mode::Idle,
            pre_roll: VecDeque::with_capacity(pre_roll_len),
            pre_roll_len,
            recorded: Vec::new(),
            max_len,
            window: RmsAccumulator::default(),
            meter: LevelMeter::default(),
            limit_reached: false,
            device_lost: false,
            unreported: Vec::new(),
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    /// Takes mono samples at the target rate.
    pub(crate) fn push(&mut self, samples: &[f32]) {
        match self.mode {
            Mode::Idle => {
                let overflow =
                    (self.pre_roll.len() + samples.len()).saturating_sub(self.pre_roll_len);
                self.pre_roll.drain(..overflow.min(self.pre_roll.len()));
                let skip = samples.len().saturating_sub(self.pre_roll_len);
                self.pre_roll.extend(&samples[skip..]);
            }
            Mode::Monitoring => self.window.add(samples),
            Mode::Recording => {
                self.window.add(samples);
                let room = self.max_len - self.recorded.len();
                if samples.len() > room && !self.limit_reached {
                    self.limit_reached = true;
                    self.unreported.push(CaptureEvent::LimitReached);
                }
                self.recorded
                    .extend_from_slice(&samples[..samples.len().min(room)]);
            }
        }
    }

    /// Starts keeping audio, beginning with the pre-roll.
    pub(crate) fn start_recording(&mut self) {
        self.recorded.clear();
        self.recorded.extend(self.pre_roll.drain(..));
        self.recorded.truncate(self.max_len);
        self.limit_reached = false;
        self.start(Mode::Recording);
    }

    pub(crate) fn start_monitoring(&mut self) {
        self.start(Mode::Monitoring);
    }

    fn start(&mut self, mode: Mode) {
        self.mode = mode;
        self.window = RmsAccumulator::default();
        self.meter = LevelMeter::default();
    }

    /// Ends recording or monitoring and returns what was kept.
    pub(crate) fn finish(&mut self) -> Recording {
        let was_recording = self.mode == Mode::Recording;
        self.mode = Mode::Idle;
        self.pre_roll.clear();
        if !was_recording {
            return Recording::default();
        }
        let ended = if self.device_lost {
            CaptureEnd::DeviceLost
        } else if self.limit_reached {
            CaptureEnd::LimitReached
        } else {
            CaptureEnd::Stopped
        };
        Recording {
            samples: std::mem::take(&mut self.recorded),
            ended,
        }
    }

    pub(crate) fn mark_device_lost(&mut self) {
        if !self.device_lost {
            self.device_lost = true;
            self.unreported.push(CaptureEvent::DeviceLost);
        }
    }

    pub(crate) fn device_lost(&self) -> bool {
        self.device_lost
    }

    /// The smoothed level since the last call, while recording or
    /// monitoring and audio arrived.
    pub(crate) fn take_level(&mut self) -> Option<f32> {
        if self.mode == Mode::Idle || self.window.is_empty() {
            return None;
        }
        let rms = self.window.rms();
        self.window = RmsAccumulator::default();
        Some(self.meter.update(rms))
    }

    pub(crate) fn take_events(&mut self) -> Vec<CaptureEvent> {
        std::mem::take(&mut self.unreported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(from: usize, len: usize) -> Vec<f32> {
        (from..from + len).map(|i| i as f32).collect()
    }

    #[test]
    fn idle_keeps_only_the_latest_pre_roll() {
        let mut buffer = CaptureBuffer::new(4, 100);
        buffer.push(&ramp(0, 3));
        buffer.push(&ramp(3, 3));
        buffer.start_recording();
        buffer.push(&ramp(6, 2));
        assert_eq!(buffer.finish().samples, ramp(2, 6));
    }

    #[test]
    fn a_chunk_longer_than_the_pre_roll_keeps_its_end() {
        let mut buffer = CaptureBuffer::new(4, 100);
        buffer.push(&ramp(0, 2));
        buffer.push(&ramp(2, 10));
        buffer.start_recording();
        assert_eq!(buffer.finish().samples, ramp(8, 4));
    }

    #[test]
    fn recording_stops_growing_at_the_limit_and_says_so() {
        let mut buffer = CaptureBuffer::new(0, 10);
        buffer.start_recording();
        buffer.push(&ramp(0, 8));
        assert!(buffer.take_events().is_empty());
        buffer.push(&ramp(8, 8));
        buffer.push(&ramp(16, 8));
        assert_eq!(buffer.take_events(), vec![CaptureEvent::LimitReached]);
        let recording = buffer.finish();
        assert_eq!(recording.samples, ramp(0, 10));
        assert_eq!(recording.ended, CaptureEnd::LimitReached);
    }

    #[test]
    fn exactly_filling_the_limit_is_not_reported() {
        let mut buffer = CaptureBuffer::new(0, 10);
        buffer.start_recording();
        buffer.push(&ramp(0, 10));
        assert!(buffer.take_events().is_empty());
        assert_eq!(buffer.finish().ended, CaptureEnd::Stopped);
    }

    #[test]
    fn monitoring_keeps_nothing_but_reports_levels() {
        let mut buffer = CaptureBuffer::new(4, 100);
        buffer.start_monitoring();
        buffer.push(&[0.1, -0.1, 0.1, -0.1]);
        assert!(buffer.take_level().is_some_and(|l| l > 0.0));
        assert_eq!(buffer.take_level(), None);
        assert_eq!(buffer.finish(), Recording::default());
    }

    #[test]
    fn idle_reports_no_levels() {
        let mut buffer = CaptureBuffer::new(4, 100);
        buffer.push(&[0.5, -0.5]);
        assert_eq!(buffer.take_level(), None);
    }

    #[test]
    fn a_lost_device_still_returns_the_audio() {
        let mut buffer = CaptureBuffer::new(0, 100);
        buffer.start_recording();
        buffer.push(&ramp(0, 5));
        buffer.mark_device_lost();
        buffer.mark_device_lost();
        assert_eq!(buffer.take_events(), vec![CaptureEvent::DeviceLost]);
        let recording = buffer.finish();
        assert_eq!(recording.samples, ramp(0, 5));
        assert_eq!(recording.ended, CaptureEnd::DeviceLost);
    }

    #[test]
    fn a_second_recording_starts_clean() {
        let mut buffer = CaptureBuffer::new(0, 4);
        buffer.start_recording();
        buffer.push(&ramp(0, 6));
        buffer.finish();
        buffer.take_events();
        buffer.start_recording();
        buffer.push(&ramp(0, 2));
        let recording = buffer.finish();
        assert_eq!(recording.samples, ramp(0, 2));
        assert_eq!(recording.ended, CaptureEnd::Stopped);
    }
}
