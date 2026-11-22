//! Start and stop sounds, synthesised and played through the default output
//! device. The webview can't play them: WKWebView blocks audio that no click
//! started.

use std::f32::consts::TAU;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use tw_core::dictation::machine::Sound;

/// One tone: a pitch glide with a fast attack and an exponential decay.
#[derive(Debug, Clone, Copy)]
struct Tone {
    from_hz: f32,
    to_hz: f32,
    duration: f32,
    gain: f32,
}

impl Tone {
    fn of(sound: Sound) -> Self {
        match sound {
            Sound::Start => Self {
                from_hz: 880.0,
                to_hz: 1_320.0,
                duration: 0.14,
                gain: 0.22,
            },
            Sound::Stop => Self {
                from_hz: 990.0,
                to_hz: 660.0,
                duration: 0.11,
                gain: 0.12,
            },
        }
    }

    /// Mono samples at `rate`.
    fn render(self, rate: u32) -> Vec<f32> {
        #[allow(clippy::cast_precision_loss)]
        let rate = rate as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let len = (self.duration * rate) as usize;
        let mut phase = 0.0_f32;
        (0..len)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / rate;
                let progress = t / self.duration;
                let hz = self.from_hz + (self.to_hz - self.from_hz) * progress;
                phase = (phase + TAU * hz / rate) % TAU;
                let attack = (t / 0.005).min(1.0);
                let decay = (-5.0 * progress).exp() * (1.0 - progress);
                phase.sin() * self.gain * attack * decay
            })
            .collect()
    }
}

/// Plays `sound` on a short-lived thread. Failures are only logged: a
/// missing output device must never get in the way of dictation.
pub fn play(sound: Sound) {
    let spawned = std::thread::Builder::new()
        .name("tw-sound".into())
        .spawn(move || {
            if let Err(err) = play_blocking(Tone::of(sound)) {
                tracing::debug!(%err, "could not play sound");
            }
        });
    if let Err(err) = spawned {
        tracing::debug!(%err, "could not start the sound thread");
    }
}

fn play_blocking(tone: Tone) -> Result<(), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config: StreamConfig = supported.config();
    let samples = tone.render(config.sample_rate);
    let channels = usize::from(config.channels);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, samples, channels),
        SampleFormat::I16 => build::<i16>(&device, &config, samples, channels),
        SampleFormat::U16 => build::<u16>(&device, &config, samples, channels),
        SampleFormat::I32 => build::<i32>(&device, &config, samples, channels),
        other => return Err(format!("unsupported output format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_secs_f32(tone.duration) + Duration::from_millis(80));
    Ok(())
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Vec<f32>,
    channels: usize,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let mut next = 0;
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |out: &mut [T], _| {
                for frame in out.chunks_mut(channels) {
                    let value = samples.get(next).copied().unwrap_or(0.0);
                    next += 1;
                    for slot in frame {
                        *slot = T::from_sample(value);
                    }
                }
            },
            |err| tracing::debug!(%err, "sound stream error"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tones_are_short_quiet_and_fade_out() {
        for sound in [Sound::Start, Sound::Stop] {
            let samples = Tone::of(sound).render(48_000);
            assert!(samples.len() < 48_000 / 4);
            assert!(samples.iter().all(|s| s.abs() <= 0.25));
            assert!(samples.iter().any(|s| s.abs() > 0.05));
            assert!(samples.last().unwrap().abs() < 0.001);
        }
        let start = Tone::of(Sound::Start).render(48_000);
        let stop = Tone::of(Sound::Stop).render(48_000);
        let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!(peak(&stop) < peak(&start), "stop is softer than start");
    }
}
