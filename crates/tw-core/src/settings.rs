//! User settings (`<app data>/settings.json`) and partial updates.

use serde::{Deserialize, Deserializer, Serialize};

use crate::Millis;
use crate::dictation::DictationMode;
use crate::history::HistoryRetention;
use crate::hotkey::Shortcuts;
use crate::text::{CleanupLevel, Styles};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertionMethod {
    /// Clipboard + paste, restoring the clipboard afterwards.
    #[default]
    Paste,
    /// Simulated keystrokes, for apps that block paste.
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnboardingStep {
    #[default]
    Welcome,
    Permissions,
    Microphone,
    Shortcut,
    VoiceModel,
    Polish,
    TryIt,
}

impl OnboardingStep {
    pub const ALL: [Self; 7] = [
        Self::Welcome,
        Self::Permissions,
        Self::Microphone,
        Self::Shortcut,
        Self::VoiceModel,
        Self::Polish,
        Self::TryIt,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Languages {
    /// Let Whisper detect the language of each dictation.
    pub auto_detect: bool,
    /// ISO 639-1 codes; used when `auto_detect` is off, kept when it is on.
    pub selected: Vec<String>,
}

impl Default for Languages {
    fn default() -> Self {
        Self {
            auto_detect: true,
            selected: vec!["en".to_owned()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub shortcuts: Shortcuts,
    /// How the push-to-talk shortcut behaves: hold to talk (double-tap for
    /// hands-free), or press to start and press again to stop.
    pub dictation_mode: DictationMode,
    /// Input device id; `None` follows the system default.
    pub microphone: Option<String>,
    pub languages: Languages,
    pub stt_model: Option<String>,
    pub llm_model: Option<String>,
    pub cleanup_level: CleanupLevel,
    pub styles: Styles,
    pub insertion_method: InsertionMethod,
    pub restore_clipboard: bool,
    pub show_pill_always: bool,
    pub sounds: bool,
    pub launch_at_login: bool,
    /// macOS only.
    pub show_in_dock: bool,
    pub theme: Theme,
    pub history_retention: HistoryRetention,
    /// "Stay offline": no network use at all, so model downloads are refused.
    pub offline: bool,
    pub onboarding_completed: bool,
    /// Where onboarding resumes if the app quits mid-way.
    pub onboarding_step: OnboardingStep,
    /// "Hide for 1 hour": the pill stays hidden until this time.
    pub pill_hidden_until: Option<Millis>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcuts: Shortcuts::default(),
            dictation_mode: DictationMode::Hold,
            microphone: None,
            languages: Languages::default(),
            stt_model: None,
            llm_model: None,
            cleanup_level: CleanupLevel::Light,
            styles: Styles::default(),
            insertion_method: InsertionMethod::Paste,
            restore_clipboard: true,
            show_pill_always: true,
            sounds: true,
            launch_at_login: false,
            show_in_dock: true,
            theme: Theme::System,
            history_retention: HistoryRetention::Forever,
            offline: false,
            onboarding_completed: false,
            onboarding_step: OnboardingStep::Welcome,
            pill_hidden_until: None,
        }
    }
}

/// `missing → None`, `null → Some(None)`, `value → Some(Some(value))`.
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// A partial update. Absent fields are left alone; for nullable settings an
/// explicit `null` clears the value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcuts: Option<Shortcuts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dictation_mode: Option<DictationMode>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub microphone: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Languages>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub stt_model: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub llm_model: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_level: Option<CleanupLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub styles: Option<Styles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insertion_method: Option<InsertionMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_clipboard: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_pill_always: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sounds: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_at_login: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_in_dock: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_retention: Option<HistoryRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub onboarding_step: Option<OnboardingStep>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub pill_hidden_until: Option<Option<Millis>>,
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        fn set<T>(slot: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *slot = value;
            }
        }
        let SettingsPatch {
            shortcuts,
            dictation_mode,
            microphone,
            languages,
            stt_model,
            llm_model,
            cleanup_level,
            styles,
            insertion_method,
            restore_clipboard,
            show_pill_always,
            sounds,
            launch_at_login,
            show_in_dock,
            theme,
            history_retention,
            offline,
            onboarding_completed,
            onboarding_step,
            pill_hidden_until,
        } = patch;
        set(&mut self.shortcuts, shortcuts);
        set(&mut self.dictation_mode, dictation_mode);
        set(&mut self.microphone, microphone);
        set(&mut self.languages, languages);
        set(&mut self.stt_model, stt_model);
        set(&mut self.llm_model, llm_model);
        set(&mut self.cleanup_level, cleanup_level);
        set(&mut self.styles, styles);
        set(&mut self.insertion_method, insertion_method);
        set(&mut self.restore_clipboard, restore_clipboard);
        set(&mut self.show_pill_always, show_pill_always);
        set(&mut self.sounds, sounds);
        set(&mut self.launch_at_login, launch_at_login);
        set(&mut self.show_in_dock, show_in_dock);
        set(&mut self.theme, theme);
        set(&mut self.history_retention, history_retention);
        set(&mut self.offline, offline);
        set(&mut self.onboarding_completed, onboarding_completed);
        set(&mut self.onboarding_step, onboarding_step);
        set(&mut self.pill_hidden_until, pill_hidden_until);
    }

    pub fn applied(mut self, patch: SettingsPatch) -> Self {
        self.apply(patch);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{Hotkey, Modifier, Platform};
    use crate::text::WritingStyle;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.cleanup_level, CleanupLevel::Light);
        assert!(s.show_pill_always && s.sounds && s.restore_clipboard);
        assert!(!s.onboarding_completed && !s.launch_at_login && !s.offline);
        assert_eq!(s.onboarding_step, OnboardingStep::Welcome);
        assert_eq!(s.styles.email, WritingStyle::Formal);
        assert_eq!(s.shortcuts, Shortcuts::defaults(Platform::current()));
        assert!(s.languages.auto_detect);
    }

    #[test]
    fn round_trip() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"showPillAlways\":true"));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{ "sounds": false }"#).unwrap();
        assert!(!s.sounds && !s.offline);
        assert_eq!(s.theme, Theme::System);
    }

    #[test]
    fn patch_sets_and_clears() {
        let mut s = Settings {
            microphone: Some("usb".into()),
            ..Settings::default()
        };
        let patch: SettingsPatch =
            serde_json::from_str(r#"{ "theme": "dark", "microphone": null }"#).unwrap();
        assert_eq!(patch.microphone, Some(None));
        assert_eq!(patch.stt_model, None);
        s.apply(patch);
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.microphone, None);

        let patch: SettingsPatch = serde_json::from_str(r#"{ "sttModel": "base.en" }"#).unwrap();
        s.apply(patch);
        assert_eq!(s.stt_model.as_deref(), Some("base.en"));
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn patch_shortcut() {
        let mut shortcuts = Shortcuts::defaults(Platform::Macos);
        shortcuts.push_to_talk = Hotkey::modifiers([Modifier::Ctrl, Modifier::Alt]);
        let s = Settings::default().applied(SettingsPatch {
            shortcuts: Some(shortcuts.clone()),
            ..SettingsPatch::default()
        });
        assert_eq!(s.shortcuts, shortcuts);
    }
}
