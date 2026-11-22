//! The real seams: the microphone through `tw-audio`, and the OS through
//! `tw-os`.

use std::sync::Arc;

use tw_audio::{AudioError, CaptureEvent, EventCallback, LevelCallback, Recorder, Recording};
use tw_core::dictation::machine::{Problem, Sound};
use tw_core::text::AppInfo;
use tw_core::{PermissionKind, PermissionStatus, Settings};
use tw_os::insert::InsertOptions;

use super::{AudioSource, Desktop};

pub struct SystemDesktop;

impl Desktop for SystemDesktop {
    fn frontmost_app(&self) -> Option<AppInfo> {
        tw_os::apps::frontmost().unwrap_or_else(|err| {
            tracing::debug!(%err, "could not read the frontmost app");
            None
        })
    }

    fn insert(&self, text: &str, settings: &Settings) -> Result<(), String> {
        let options = InsertOptions {
            method: settings.insertion_method,
            restore_clipboard: settings.restore_clipboard,
            ..InsertOptions::default()
        };
        tw_os::insert::insert_text(text, &options).map_err(|err| err.to_string())
    }

    fn play(&self, sound: Sound) {
        crate::sound::play(sound);
    }

    fn microphone_denied(&self) -> bool {
        tw_os::permissions::status(PermissionKind::Microphone) == PermissionStatus::Denied
    }
}

/// The microphone, reopened when the selected device changes.
pub struct MicSource {
    recorder: Option<Recorder>,
    on_level: LevelCallback,
    on_lost: Arc<dyn Fn() + Send + Sync>,
}

impl MicSource {
    pub fn new(on_level: LevelCallback, on_lost: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            recorder: None,
            on_level,
            on_lost,
        }
    }

    fn recorder(&mut self, device: Option<&str>) -> tw_audio::Result<&mut Recorder> {
        if self
            .recorder
            .as_ref()
            .is_some_and(|r| r.device_id() != device)
        {
            self.recorder = None;
        }
        if self.recorder.is_none() {
            let mut recorder = Recorder::open(device)?;
            recorder.set_level_callback(Some(Arc::clone(&self.on_level)));
            let on_lost = Arc::clone(&self.on_lost);
            let on_event: EventCallback = Arc::new(move |event| match event {
                CaptureEvent::DeviceLost => on_lost(),
                CaptureEvent::LimitReached => tracing::info!("recording limit reached"),
            });
            recorder.set_event_callback(Some(on_event));
            self.recorder = Some(recorder);
        }
        self.recorder
            .as_mut()
            .ok_or_else(|| AudioError::Stream("no recorder".into()))
    }
}

impl AudioSource for MicSource {
    fn start(&mut self, device: Option<&str>) -> Result<(), Problem> {
        let recorder = self.recorder(device).map_err(problem)?;
        recorder.start().map_err(problem)?;
        if let Some(active) = recorder.active_device() {
            tracing::info!(
                device = %active.name,
                fell_back = active.fell_back_to_default,
                "recording"
            );
        }
        Ok(())
    }

    fn stop(&mut self) -> Recording {
        let Some(recorder) = &mut self.recorder else {
            return Recording::default();
        };
        recorder.stop().unwrap_or_else(|err| {
            tracing::warn!(%err, "could not stop the recording");
            Recording::default()
        })
    }

    fn cancel(&mut self) {
        if let Some(recorder) = &mut self.recorder {
            recorder.cancel();
        }
    }

    fn start_monitor(&mut self, device: Option<&str>) -> tw_audio::Result<()> {
        // A denied microphone on macOS opens fine and records silence.
        if tw_os::permissions::status(PermissionKind::Microphone) == PermissionStatus::Denied {
            return Err(AudioError::PermissionDenied);
        }
        self.recorder(device)?.start_monitor()
    }

    fn stop_monitor(&mut self) {
        if let Some(recorder) = &mut self.recorder {
            recorder.stop_monitor();
        }
    }
}

fn problem(err: AudioError) -> Problem {
    tracing::warn!(%err, "microphone");
    match err {
        AudioError::NoDevice => Problem::NoMicrophone,
        AudioError::PermissionDenied => Problem::MicrophoneDenied,
        other => Problem::Failed(other.to_string()),
    }
}
