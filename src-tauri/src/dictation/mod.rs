//! The dictation runtime: one thread owns the `tw_core` state machine and
//! performs its effects. Everything that touches the OS sits behind a trait
//! so the whole runtime can be driven by fakes in tests.

mod audio;
pub mod desktop;
pub mod engine;
pub mod pipeline;
pub mod polish;
mod runtime;
#[cfg(debug_assertions)]
pub mod smoke;

use std::sync::Arc;

use tw_core::dictation::machine::{Problem, Sound};
use tw_core::text::AppInfo;
use tw_core::text::polish::PolishRequest;
use tw_core::{DictationState, Settings};

pub use runtime::{Deps, Dictation, Msg};

use crate::events::HotkeyCapture;

/// The microphone. Runs on the runtime's audio thread, so `start` may block
/// while the device opens.
pub trait AudioSource: Send {
    /// Opens `device` (the system default when `None`) and starts keeping
    /// audio.
    fn start(&mut self, device: Option<&str>) -> Result<(), Problem>;
    /// Stops and returns what was captured since `start`.
    fn stop(&mut self) -> tw_audio::Recording;
    /// Stops and drops the audio.
    fn cancel(&mut self);
    /// Level metering only, for the microphone test.
    fn start_monitor(&mut self, device: Option<&str>) -> tw_audio::Result<()>;
    fn stop_monitor(&mut self);
}

/// The rest of the OS: the focused app, insertion and sounds.
pub trait Desktop: Send + Sync {
    fn frontmost_app(&self) -> Option<AppInfo>;
    /// Inserts into whatever has focus. Blocks; called off the runtime thread.
    fn insert(&self, text: &str, settings: &Settings) -> Result<(), String>;
    fn play(&self, sound: Sound);
    /// The OS says this app may not use the microphone.
    fn microphone_denied(&self) -> bool;
}

/// Speech-to-text models, loaded lazily and cached.
pub trait Transcribers: Send + Sync {
    /// A voice model for `settings` is on disk.
    fn has_model(&self, settings: &Settings) -> bool;
    /// Loads (or reuses) the model for `settings`. Blocks while loading.
    fn load(&self, settings: &Settings) -> Result<Arc<dyn tw_stt::Transcriber>, Problem>;
}

/// The LLM polish step between the rule-based passes.
pub trait Polisher: Send + Sync {
    /// A language model is selected and usable, so the `polishing` stage is
    /// shown.
    fn active(&self, settings: &Settings) -> bool;
    /// `None` keeps the text as it was.
    fn polish(&self, request: &PolishRequest) -> Option<String>;
}

/// Polish is off: used where no language model can run.
#[cfg(test)]
pub struct NoPolish;

#[cfg(test)]
impl Polisher for NoPolish {
    fn active(&self, _settings: &Settings) -> bool {
        false
    }

    fn polish(&self, _request: &PolishRequest) -> Option<String> {
        None
    }
}

/// Where the runtime reports to: the windows, in the app.
pub trait Sink: Send + Sync {
    fn state(&self, state: &DictationState);
    fn level(&self, level: f32);
    fn history_changed(&self);
    fn hotkey_captured(&self, capture: HotkeyCapture);
    fn open_settings(&self);
}
