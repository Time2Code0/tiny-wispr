//! The 0..1 input level shown by the pill's bars and the microphone test.

/// RMS at or below this reads as 0: a quiet room.
const FLOOR_DB: f32 = -60.0;
/// RMS at or above this reads as 1: loud speech close to the microphone.
const CEILING_DB: f32 = -10.0;
/// How much of the gap to a louder reading is closed per tick.
const ATTACK: f32 = 0.6;
/// How much of the gap to a quieter reading is closed per tick, so the bars
/// fall more slowly than they rise.
const RELEASE: f32 = 0.25;

/// Root mean square around the mean, so a DC offset (some built-in
/// microphones have one) never reads as sound.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut acc = RmsAccumulator::default();
    acc.add(samples);
    acc.rms()
}

/// Decibels relative to full scale; very small values clamp to -120.
pub fn to_dbfs(rms: f32) -> f32 {
    20.0 * rms.max(1e-6).log10()
}

/// Maps an RMS value to 0..1 on a decibel scale: silence near 0, normal
/// speech around 0.4–0.8.
pub fn level_from_rms(rms: f32) -> f32 {
    ((to_dbfs(rms) - FLOOR_DB) / (CEILING_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

/// Sums samples between two level ticks.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RmsAccumulator {
    sum: f64,
    sum_sq: f64,
    count: usize,
}

impl RmsAccumulator {
    pub(crate) fn add(&mut self, samples: &[f32]) {
        for &s in samples {
            self.sum += f64::from(s);
            self.sum_sq += f64::from(s) * f64::from(s);
        }
        self.count += samples.len();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub(crate) fn rms(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let n = self.count as f64;
        let mean = self.sum / n;
        (self.sum_sq / n - mean * mean).max(0.0).sqrt() as f32
    }
}

/// Smooths successive levels: quick to rise, slower to fall.
#[derive(Debug, Default, Clone, Copy)]
pub struct LevelMeter {
    smoothed: f32,
}

impl LevelMeter {
    /// Feeds the RMS of the latest window and returns the smoothed level.
    pub fn update(&mut self, rms: f32) -> f32 {
        let target = level_from_rms(rms);
        let rate = if target > self.smoothed {
            ATTACK
        } else {
            RELEASE
        };
        self.smoothed += (target - self.smoothed) * rate;
        if self.smoothed < 1e-3 {
            self.smoothed = 0.0;
        }
        self.smoothed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amplitude: f32) -> Vec<f32> {
        (0..1_600)
            .map(|i| amplitude * (i as f32 * 0.2).sin())
            .collect()
    }

    #[test]
    fn rms_ignores_dc_offset() {
        assert!(rms(&[0.3; 512]) < 1e-6);
        let shifted: Vec<f32> = tone(0.1).iter().map(|s| s + 0.5).collect();
        assert!((rms(&shifted) - rms(&tone(0.1))).abs() < 1e-3);
    }

    #[test]
    fn silence_reads_as_zero() {
        assert_eq!(level_from_rms(0.0), 0.0);
        assert_eq!(level_from_rms(rms(&tone(0.0005))), 0.0);
    }

    #[test]
    fn normal_speech_sits_in_the_middle() {
        // Speech at a laptop microphone is roughly -35 to -20 dBFS RMS.
        for db in [-35.0_f32, -28.0, -21.0] {
            let level = level_from_rms(10_f32.powf(db / 20.0));
            assert!((0.4..=0.8).contains(&level), "{db} dBFS -> {level}");
        }
    }

    #[test]
    fn loud_input_clamps_to_one() {
        assert_eq!(level_from_rms(1.0), 1.0);
    }

    #[test]
    fn meter_rises_fast_and_falls_slowly() {
        let mut meter = LevelMeter::default();
        let loud = 10_f32.powf(-20.0 / 20.0);
        let first = meter.update(loud);
        assert!(first > 0.4);
        let peak = (0..10).map(|_| meter.update(loud)).last().unwrap_or(0.0);
        assert!((peak - 0.8).abs() < 0.01);
        let after_one_quiet_tick = meter.update(0.0);
        assert!(after_one_quiet_tick > 0.5);
        let settled = (0..40).map(|_| meter.update(0.0)).last().unwrap_or(1.0);
        assert_eq!(settled, 0.0);
    }
}
