//! Any device format to mono `f32` at [`SAMPLE_RATE`].

use cpal::{FromSample, Sample};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use crate::{AudioError, Result, SAMPLE_RATE};

/// Input frames per resampler call: ~20 ms at common device rates, small
/// enough that levels and the pre-roll stay current.
const CHUNK_FRAMES: usize = 1_024;

/// Streams interleaved audio in any sample format, channel count and rate
/// into mono samples at [`SAMPLE_RATE`]. Channels are averaged.
pub struct Converter {
    channels: usize,
    resampler: Option<Resampling>,
}

struct Resampling {
    fft: Fft<f32>,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    delay_left: usize,
    input_total: u64,
    output_total: u64,
    ratio: f64,
}

impl Converter {
    pub fn new(input_rate: u32, channels: u16) -> Result<Self> {
        if input_rate == 0 || channels == 0 {
            return Err(AudioError::Stream(format!(
                "unsupported input format: {channels} channels at {input_rate} Hz"
            )));
        }
        let resampler = if input_rate == SAMPLE_RATE {
            None
        } else {
            let fft = Fft::<f32>::new(
                input_rate as usize,
                SAMPLE_RATE as usize,
                CHUNK_FRAMES,
                1,
                FixedSync::Input,
            )
            .map_err(|err| AudioError::Stream(format!("could not set up resampling: {err}")))?;
            Some(Resampling {
                delay_left: fft.output_delay(),
                pending: Vec::with_capacity(CHUNK_FRAMES * 2),
                scratch: vec![0.0; fft.output_frames_max()],
                fft,
                input_total: 0,
                output_total: 0,
                ratio: f64::from(SAMPLE_RATE) / f64::from(input_rate),
            })
        };
        Ok(Self {
            channels: usize::from(channels),
            resampler,
        })
    }

    /// Converts `interleaved` and appends the result to `out`. A trailing
    /// partial frame is ignored.
    pub fn push<S>(&mut self, interleaved: &[S], out: &mut Vec<f32>)
    where
        S: Sample,
        f32: FromSample<S>,
    {
        let mono = interleaved.chunks_exact(self.channels).map(|frame| {
            let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
            sum / self.channels as f32
        });
        match &mut self.resampler {
            None => out.extend(mono),
            Some(resampling) => {
                resampling.pending.extend(mono);
                resampling.drain_full_chunks(out);
            }
        }
    }

    /// Flushes what the resampler still holds. Call once at the end of a
    /// clip; the converter must not be pushed to afterwards.
    pub fn finish(mut self, out: &mut Vec<f32>) {
        if let Some(resampling) = &mut self.resampler {
            resampling.finish(out);
        }
    }
}

impl Resampling {
    fn drain_full_chunks(&mut self, out: &mut Vec<f32>) {
        let mut consumed = 0;
        while self.pending.len() - consumed >= self.fft.input_frames_next() {
            consumed += self.process(consumed, None, out);
        }
        self.pending.drain(..consumed);
    }

    fn finish(&mut self, out: &mut Vec<f32>) {
        self.drain_full_chunks(out);
        let remainder = self.pending.len();
        if remainder > 0 {
            self.process(0, Some(remainder), out);
            self.pending.clear();
        }
        let expected = (self.input_total as f64 * self.ratio).ceil() as u64;
        // A call can legitimately produce nothing while the resampler gathers
        // a whole FFT block, so only give up after several in a row.
        let mut idle_calls = 0;
        while self.output_total < expected && idle_calls < 16 {
            let before = (self.output_total, self.delay_left);
            self.process(0, Some(0), out);
            if (self.output_total, self.delay_left) == before {
                idle_calls += 1;
            } else {
                idle_calls = 0;
            }
        }
        if self.output_total > expected {
            let extra = (self.output_total - expected) as usize;
            out.truncate(out.len().saturating_sub(extra));
            self.output_total = expected;
        }
    }

    /// Runs one resampler call reading from `pending[offset..]` and returns
    /// how many input frames it consumed.
    fn process(&mut self, offset: usize, partial: Option<usize>, out: &mut Vec<f32>) -> usize {
        let frames = partial.unwrap_or_else(|| self.fft.input_frames_next());
        let indexing = partial.map(|len| Indexing::new().partial_len(len));
        let input = &self.pending[offset..offset + frames];
        let frames_out = self.scratch.len();
        let result = InterleavedSlice::new(input, 1, frames)
            .and_then(|input| {
                InterleavedSlice::new_mut(&mut self.scratch, 1, frames_out)
                    .map(|output| (input, output))
            })
            .map_err(|err| err.to_string())
            .and_then(|(input, mut output)| {
                self.fft
                    .process_into_buffer(&input, &mut output, indexing.as_ref())
                    .map_err(|err| err.to_string())
            });
        let produced = match result {
            Ok((_, produced)) => produced,
            Err(err) => {
                tracing::warn!(%err, "resampling failed; dropping a chunk");
                0
            }
        };
        let skip = self.delay_left.min(produced);
        self.delay_left -= skip;
        out.extend_from_slice(&self.scratch[skip..produced]);
        self.output_total += (produced - skip) as u64;
        self.input_total += frames as u64;
        frames
    }
}

/// Converts a whole clip at once.
pub fn to_mono_16k<S>(interleaved: &[S], channels: u16, input_rate: u32) -> Result<Vec<f32>>
where
    S: Sample,
    f32: FromSample<S>,
{
    let mut converter = Converter::new(input_rate, channels)?;
    let frames = interleaved.len() / usize::from(channels.max(1));
    let mut out =
        Vec::with_capacity(frames * SAMPLE_RATE as usize / input_rate.max(1) as usize + 1);
    converter.push(interleaved, &mut out);
    converter.finish(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Estimated frequency from zero crossings.
    fn frequency(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        crossings as f32 * rate as f32 / samples.len() as f32
    }

    #[test]
    fn already_16k_mono_passes_through() {
        let input = sine(440.0, 16_000, 0.1, 0.5);
        let out = to_mono_16k(&input, 1, 16_000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn stereo_is_averaged() {
        let out = to_mono_16k(&[0.2_f32, 0.4, -1.0, 0.0], 2, 16_000).unwrap();
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.3).abs() < 1e-6);
        assert!((out[1] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn integer_formats_are_scaled_to_unit_range() {
        let out = to_mono_16k(&[i16::MIN, 0, i16::MAX / 2], 1, 16_000).unwrap();
        assert!((out[0] + 1.0).abs() < 1e-4);
        assert!(out[1].abs() < 1e-4);
        assert!((out[2] - 0.5).abs() < 1e-3);

        let out = to_mono_16k(&[0_u16, 32_768, u16::MAX], 1, 16_000).unwrap();
        assert!((out[0] + 1.0).abs() < 1e-4);
        assert!(out[1].abs() < 1e-4);
        assert!((out[2] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn resampling_keeps_length_pitch_and_level() {
        for rate in [8_000, 22_050, 44_100, 48_000, 96_000] {
            let input = sine(440.0, rate, 1.0, 0.5);
            let out = to_mono_16k(&input, 1, rate).unwrap();
            assert_eq!(out.len(), 16_000, "length at {rate} Hz");
            let middle = &out[2_000..14_000];
            assert!(
                (frequency(middle, 16_000) - 440.0).abs() < 3.0,
                "pitch at {rate} Hz"
            );
            assert!(
                (rms(middle) - 0.5 / 2_f32.sqrt()).abs() < 0.01,
                "level at {rate} Hz"
            );
        }
    }

    #[test]
    fn streaming_in_odd_chunks_matches_one_shot() {
        let input = sine(300.0, 48_000, 0.5, 0.4);
        let whole = to_mono_16k(&input, 1, 48_000).unwrap();
        let mut converter = Converter::new(48_000, 1).unwrap();
        let mut streamed = Vec::new();
        for chunk in input.chunks(333) {
            converter.push(chunk, &mut streamed);
        }
        converter.finish(&mut streamed);
        assert_eq!(streamed.len(), whole.len());
        for (a, b) in streamed.iter().zip(&whole) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn content_above_8k_is_filtered_out() {
        let input = sine(12_000.0, 48_000, 0.5, 0.5);
        let out = to_mono_16k(&input, 1, 48_000).unwrap();
        assert!(rms(&out[1_000..7_000]) < 0.01);
    }

    #[test]
    fn short_clips_survive() {
        let input = sine(440.0, 44_100, 0.01, 0.5);
        let out = to_mono_16k(&input, 2, 44_100).unwrap();
        assert_eq!(out.len(), 80);
    }

    #[test]
    fn rejects_nonsense_formats() {
        assert!(Converter::new(0, 1).is_err());
        assert!(Converter::new(48_000, 0).is_err());
    }
}
