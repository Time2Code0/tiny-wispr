//! The live microphone: device lookup, the cpal stream and the level ticker.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, DeviceId, FromSample, SampleFormat, SizedSample, StreamConfig,
    SupportedStreamConfig,
};

use crate::buffer::{CaptureBuffer, Mode};
use crate::convert::Converter;
use crate::{
    AudioError, EventCallback, LEVEL_HZ, LevelCallback, MAX_RECORDING_MS, MicrophoneInfo,
    PRE_ROLL_MS, Recording, Result, SAMPLE_RATE,
};

/// Input devices with stable ids. Enumerating does not ask for microphone
/// access; opening a stream does.
pub fn list_input_devices() -> Result<Vec<MicrophoneInfo>> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok());
    let devices = host.input_devices().map_err(stream_error)?;
    Ok(devices
        .filter_map(|device| {
            let id = device.id().ok()?;
            Some(MicrophoneInfo {
                name: device_name(&device),
                is_default: default_id.as_ref() == Some(&id),
                id: id.to_string(),
            })
        })
        .collect())
}

fn device_name(device: &Device) -> String {
    device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| device.to_string())
}

/// The device a stream was opened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveDevice {
    pub name: String,
    /// The chosen microphone was not found, so the system default is used.
    pub fell_back_to_default: bool,
}

#[derive(Default)]
struct Callbacks {
    on_level: Option<LevelCallback>,
    on_event: Option<EventCallback>,
}

/// A capture session on one device.
///
/// The OS shows its microphone indicator while a stream is open, so by
/// default the stream only exists between [`Recorder::start`] and
/// [`Recorder::stop`] (or during the microphone test). With
/// [`Recorder::set_warm`] it stays open between dictations and keeps a
/// [`PRE_ROLL_MS`] pre-roll so the first word is never clipped.
///
/// Opening a stream blocks (Bluetooth headsets can take a moment) and is
/// what makes the OS ask for microphone access, so call `start`,
/// `start_monitor` and `set_warm(true)` off the UI thread, and only after
/// the user asked for it.
pub struct Recorder {
    device_id: Option<String>,
    warm: bool,
    callbacks: Arc<Mutex<Callbacks>>,
    live: Option<LiveStream>,
}

impl Recorder {
    /// Prepares capture from `device_id`, or the system default when
    /// `None`. Does not open the microphone yet.
    pub fn open(device_id: Option<&str>) -> Result<Self> {
        Ok(Self {
            device_id: device_id.map(str::to_owned),
            warm: false,
            callbacks: Arc::default(),
            live: None,
        })
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    /// The device of the open stream, if one is open.
    pub fn active_device(&self) -> Option<&ActiveDevice> {
        self.live.as_ref().map(|live| &live.device)
    }

    /// Receives a smoothed 0..1 level about [`LEVEL_HZ`] times a second while
    /// recording or monitoring. Runs on a background thread.
    pub fn set_level_callback(&mut self, on_level: Option<LevelCallback>) {
        lock(&self.callbacks).on_level = on_level;
    }

    /// Receives [`CaptureEvent`]s as they happen. Runs on a background
    /// thread.
    pub fn set_event_callback(&mut self, on_event: Option<EventCallback>) {
        lock(&self.callbacks).on_event = on_event;
    }

    /// Keeps the stream open between dictations (`true`) or only while
    /// dictating or testing (`false`, the default).
    pub fn set_warm(&mut self, warm: bool) -> Result<()> {
        self.warm = warm;
        if warm {
            self.ensure_stream()?;
        } else if self.live.as_ref().is_some_and(|l| l.mode() == Mode::Idle) {
            self.live = None;
        }
        Ok(())
    }

    /// Starts keeping audio (including the pre-roll when warm).
    pub fn start(&mut self) -> Result<()> {
        lock(&self.ensure_stream()?.buffer).start_recording();
        Ok(())
    }

    /// Stops and returns everything captured since [`Recorder::start`].
    /// Returns an empty recording when nothing was being recorded.
    pub fn stop(&mut self) -> Result<Recording> {
        let Some(live) = &self.live else {
            return Ok(Recording::default());
        };
        let recording = lock(&live.buffer).finish();
        self.close_unless_warm();
        Ok(recording)
    }

    /// Stops and drops the audio.
    pub fn cancel(&mut self) {
        if let Some(live) = &self.live {
            lock(&live.buffer).finish();
        }
        self.close_unless_warm();
    }

    /// Level metering without keeping audio (onboarding mic test).
    pub fn start_monitor(&mut self) -> Result<()> {
        let live = self.ensure_stream()?;
        let mut buffer = lock(&live.buffer);
        if buffer.mode() == Mode::Recording {
            return Err(AudioError::Stream("already recording".to_owned()));
        }
        buffer.start_monitoring();
        Ok(())
    }

    pub fn stop_monitor(&mut self) {
        if let Some(live) = &self.live {
            let mut buffer = lock(&live.buffer);
            if buffer.mode() == Mode::Monitoring {
                buffer.finish();
            }
        }
        self.close_unless_warm();
    }

    fn ensure_stream(&mut self) -> Result<&LiveStream> {
        if self.live.as_ref().is_some_and(LiveStream::is_dead) {
            self.live = None;
        }
        if self.live.is_none() {
            self.live = Some(LiveStream::open(
                self.device_id.as_deref(),
                Arc::clone(&self.callbacks),
            )?);
        }
        self.live
            .as_ref()
            .ok_or_else(|| AudioError::Stream("no stream".to_owned()))
    }

    fn close_unless_warm(&mut self) {
        let dead = self.live.as_ref().is_some_and(LiveStream::is_dead);
        if !self.warm || dead {
            self.live = None;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An open cpal stream plus the thread that reports levels and events.
/// Dropping it closes the microphone.
struct LiveStream {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<CaptureBuffer>>,
    device: ActiveDevice,
    ticker_stop: Arc<AtomicBool>,
    ticker: Option<JoinHandle<()>>,
}

impl LiveStream {
    fn open(device_id: Option<&str>, callbacks: Arc<Mutex<Callbacks>>) -> Result<Self> {
        let host = cpal::default_host();
        let (device, fell_back_to_default) = resolve_device(&host, device_id)?;
        let name = device_name(&device);
        let buffer = Arc::new(Mutex::new(CaptureBuffer::new(
            samples_for_ms(PRE_ROLL_MS),
            samples_for_ms(MAX_RECORDING_MS),
        )));

        let mut first_error = None;
        for config in candidate_configs(&device)? {
            match build_stream(&device, &config, &buffer).and_then(|stream| {
                stream.play().map_err(stream_error)?;
                Ok(stream)
            }) {
                Ok(stream) => {
                    tracing::info!(
                        device = %name,
                        rate = config.sample_rate(),
                        channels = config.channels(),
                        format = %config.sample_format(),
                        fell_back_to_default,
                        "microphone open"
                    );
                    let ticker_stop = Arc::new(AtomicBool::new(false));
                    let ticker =
                        spawn_ticker(Arc::clone(&buffer), callbacks, Arc::clone(&ticker_stop))?;
                    return Ok(Self {
                        _stream: stream,
                        buffer,
                        device: ActiveDevice {
                            name,
                            fell_back_to_default,
                        },
                        ticker_stop,
                        ticker: Some(ticker),
                    });
                }
                Err(err) => {
                    tracing::warn!(device = %name, %err, ?config, "the microphone refused a format");
                    first_error.get_or_insert(err);
                }
            }
        }
        Err(first_error.unwrap_or(AudioError::NoDevice))
    }

    fn mode(&self) -> Mode {
        lock(&self.buffer).mode()
    }

    fn is_dead(&self) -> bool {
        lock(&self.buffer).device_lost()
    }
}

impl Drop for LiveStream {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            ticker.thread().unpark();
            let _ = ticker.join();
        }
    }
}

fn samples_for_ms(ms: u32) -> usize {
    (u64::from(SAMPLE_RATE) * u64::from(ms) / 1_000) as usize
}

fn resolve_device(host: &cpal::Host, device_id: Option<&str>) -> Result<(Device, bool)> {
    if let Some(id) = device_id {
        match id.parse::<DeviceId>() {
            Ok(parsed) => {
                if let Some(device) = host.device_by_id(&parsed) {
                    return Ok((device, false));
                }
                tracing::warn!(%id, "the chosen microphone is gone; using the system default");
            }
            Err(err) => {
                tracing::warn!(%id, %err, "unreadable microphone id; using the system default")
            }
        }
    }
    let device = host.default_input_device().ok_or(AudioError::NoDevice)?;
    Ok((device, device_id.is_some()))
}

/// The device's default format first, then the others it lists (some
/// drivers refuse their own default), preferring rates close to 16 kHz.
fn candidate_configs(device: &Device) -> Result<Vec<SupportedStreamConfig>> {
    let mut configs = Vec::new();
    match device.default_input_config() {
        Ok(config) => configs.push(config),
        Err(err) => tracing::warn!(%err, "the microphone has no default format"),
    }
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            let rate = [SAMPLE_RATE, 48_000, 44_100]
                .into_iter()
                .find(|&r| (range.min_sample_rate()..=range.max_sample_rate()).contains(&r))
                .unwrap_or_else(|| range.max_sample_rate());
            let config = range.with_sample_rate(rate);
            if !configs.contains(&config) {
                configs.push(config);
            }
        }
    }
    if configs.is_empty() {
        return Err(AudioError::NoDevice);
    }
    Ok(configs)
}

fn build_stream(
    device: &Device,
    config: &SupportedStreamConfig,
    buffer: &Arc<Mutex<CaptureBuffer>>,
) -> Result<cpal::Stream> {
    let stream_config = StreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        buffer_size: BufferSize::Default,
    };
    match config.sample_format() {
        SampleFormat::I8 => build_typed::<i8>(device, stream_config, buffer),
        SampleFormat::I16 => build_typed::<i16>(device, stream_config, buffer),
        SampleFormat::I24 => build_typed::<cpal::I24>(device, stream_config, buffer),
        SampleFormat::I32 => build_typed::<i32>(device, stream_config, buffer),
        SampleFormat::U8 => build_typed::<u8>(device, stream_config, buffer),
        SampleFormat::U16 => build_typed::<u16>(device, stream_config, buffer),
        SampleFormat::U24 => build_typed::<cpal::U24>(device, stream_config, buffer),
        SampleFormat::U32 => build_typed::<u32>(device, stream_config, buffer),
        SampleFormat::F32 => build_typed::<f32>(device, stream_config, buffer),
        SampleFormat::F64 => build_typed::<f64>(device, stream_config, buffer),
        other => Err(AudioError::Stream(format!(
            "unsupported sample format {other}"
        ))),
    }
}

fn build_typed<T>(
    device: &Device,
    config: StreamConfig,
    buffer: &Arc<Mutex<CaptureBuffer>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut converter = Converter::new(config.sample_rate, config.channels)?;
    let mut converted = Vec::with_capacity(4_096);
    let data_buffer = Arc::clone(buffer);
    let error_buffer = Arc::clone(buffer);
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                converter.push(data, &mut converted);
                lock(&data_buffer).push(&converted);
                converted.clear();
            },
            move |err| on_stream_error(&error_buffer, &err),
            Some(Duration::from_secs(5)),
        )
        .map_err(stream_error)
}

fn on_stream_error(buffer: &Mutex<CaptureBuffer>, err: &cpal::Error) {
    use cpal::ErrorKind;
    match err.kind() {
        // Glitches on a stream that keeps running; logging from the audio
        // thread would only make the next cycle late too.
        ErrorKind::Xrun | ErrorKind::RealtimeDenied => {}
        ErrorKind::DeviceChanged => tracing::info!("the default microphone changed; following it"),
        _ => {
            tracing::warn!(%err, "microphone stream failed");
            lock(buffer).mark_device_lost();
        }
    }
}

fn stream_error(err: cpal::Error) -> AudioError {
    use cpal::ErrorKind;
    match err.kind() {
        ErrorKind::DeviceBusy => AudioError::DeviceBusy,
        ErrorKind::PermissionDenied => AudioError::PermissionDenied,
        ErrorKind::DeviceNotAvailable => AudioError::NoDevice,
        _ => AudioError::Stream(err.to_string()),
    }
}

fn spawn_ticker(
    buffer: Arc<Mutex<CaptureBuffer>>,
    callbacks: Arc<Mutex<Callbacks>>,
    stop: Arc<AtomicBool>,
) -> Result<JoinHandle<()>> {
    let interval = Duration::from_secs(1) / LEVEL_HZ;
    std::thread::Builder::new()
        .name("tw-audio-levels".to_owned())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(interval);
                let (level, events) = {
                    let mut buffer = lock(&buffer);
                    (buffer.take_level(), buffer.take_events())
                };
                let (on_level, on_event) = {
                    let callbacks = lock(&callbacks);
                    (callbacks.on_level.clone(), callbacks.on_event.clone())
                };
                if let (Some(level), Some(on_level)) = (level, on_level) {
                    on_level(level);
                }
                if let Some(on_event) = on_event {
                    for event in events {
                        on_event(event);
                    }
                }
            }
        })
        .map_err(AudioError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaptureEvent;

    #[test]
    fn recorder_can_move_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<Recorder>();
    }

    #[test]
    fn a_recorder_without_a_stream_stops_empty() {
        let mut recorder = Recorder::open(Some("coreaudio:missing")).unwrap();
        assert_eq!(recorder.device_id(), Some("coreaudio:missing"));
        assert!(recorder.active_device().is_none());
        assert_eq!(recorder.stop().unwrap(), Recording::default());
        recorder.cancel();
        recorder.stop_monitor();
        recorder.set_warm(false).unwrap();
        assert!(recorder.active_device().is_none());
    }

    #[test]
    fn cpal_errors_map_to_what_the_user_can_act_on() {
        use cpal::ErrorKind;
        assert!(matches!(
            stream_error(ErrorKind::DeviceBusy.into()),
            AudioError::DeviceBusy
        ));
        assert!(matches!(
            stream_error(ErrorKind::PermissionDenied.into()),
            AudioError::PermissionDenied
        ));
        assert!(matches!(
            stream_error(ErrorKind::DeviceNotAvailable.into()),
            AudioError::NoDevice
        ));
        assert!(matches!(
            stream_error(ErrorKind::BackendError.into()),
            AudioError::Stream(_)
        ));
    }

    #[test]
    fn glitches_do_not_end_a_recording_but_a_lost_device_does() {
        use cpal::ErrorKind;
        let buffer = Mutex::new(CaptureBuffer::new(0, 100));
        for kind in [
            ErrorKind::Xrun,
            ErrorKind::RealtimeDenied,
            ErrorKind::DeviceChanged,
        ] {
            on_stream_error(&buffer, &kind.into());
        }
        assert!(!lock(&buffer).device_lost());
        on_stream_error(&buffer, &ErrorKind::DeviceNotAvailable.into());
        assert!(lock(&buffer).device_lost());
        assert_eq!(lock(&buffer).take_events(), vec![CaptureEvent::DeviceLost]);
    }

    #[test]
    fn listing_devices_does_not_fail() {
        let devices = list_input_devices().unwrap();
        assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
        for device in &devices {
            assert!(!device.id.is_empty());
        }
    }

    /// Records two seconds from the default microphone and prints its level.
    /// Opens a real input stream, which asks for microphone access the first
    /// time; run by hand with
    /// `cargo test -p tw-audio -- --ignored --nocapture records_from_the_default_microphone`.
    #[test]
    #[ignore = "opens the microphone"]
    fn records_from_the_default_microphone() {
        let levels = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&levels);
        let mut recorder = Recorder::open(None).unwrap();
        recorder.set_level_callback(Some(Arc::new(move |level| lock(&sink).push(level))));
        recorder.start().unwrap();
        println!("recording from {:?}; speak now", recorder.active_device());
        std::thread::sleep(Duration::from_secs(2));
        let recording = recorder.stop().unwrap();
        let levels = lock(&levels);
        println!(
            "{} ms, {} level updates, peak level {:.2}, speech: {}",
            recording.duration_ms(),
            levels.len(),
            levels.iter().copied().fold(0.0, f32::max),
            crate::vad::has_speech(&recording.samples),
        );
        assert!(recording.duration_ms() > 1_500);
        assert!(levels.len() > 40);
        assert!(
            recorder.active_device().is_none(),
            "stream closed after stop"
        );
    }
}
