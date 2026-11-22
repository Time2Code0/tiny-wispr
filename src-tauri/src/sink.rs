//! The dictation runtime's reports, delivered to the windows and the tray.

use tauri::AppHandle;
use tw_core::DictationState;

use crate::dictation::Sink;
use crate::events::{self, AudioLevel, HotkeyCapture};
use crate::windows::{self, MainRoute};
use crate::{pill, tray};

pub struct AppSink {
    pub app: AppHandle,
}

impl Sink for AppSink {
    fn state(&self, state: &DictationState) {
        events::emit(&self.app, events::DICTATION_STATE, state.clone());
        pill::sync_visibility(&self.app);
        tray::show_state(&self.app, state);
    }

    fn level(&self, level: f32) {
        events::emit(&self.app, events::AUDIO_LEVEL, AudioLevel { level });
    }

    fn history_changed(&self) {
        events::emit(&self.app, events::HISTORY_CHANGED, ());
    }

    fn hotkey_captured(&self, capture: HotkeyCapture) {
        events::emit(&self.app, events::HOTKEY_CAPTURED, capture);
    }

    fn open_settings(&self) {
        if let Err(err) = windows::show_main(&self.app, Some(MainRoute::Models)) {
            tracing::warn!(%err, "could not open settings");
        }
    }
}
