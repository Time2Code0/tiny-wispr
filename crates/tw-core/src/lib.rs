//! Shared, platform-independent core of Tiny Wispr.
//!
//! Every type here that crosses the IPC boundary serialises with
//! `rename_all = "camelCase"` and is mirrored by hand in `src/lib/ipc/types.ts`.

pub mod dictation;
pub mod history;
pub mod hotkey;
pub mod models;
pub mod permissions;
pub mod settings;
pub mod text;
pub mod updates;

pub use dictation::{DictationMode, DictationPhase, DictationState, Notice, NoticeKind};
pub use history::{HistoryEntry, Stats};
pub use hotkey::{Hotkey, HotkeyAction, KeyCode, Modifier, Platform};
pub use models::{HardwareInfo, ModelInfo, ModelKind, ModelStatus};
pub use permissions::{MicrophoneInfo, PermissionKind, PermissionStatus, Permissions};
pub use settings::{Settings, SettingsPatch};
pub use text::{
    AppCategory, AppInfo, CleanupLevel, DictionaryEntry, Snippet, StyleCategory, WritingStyle,
};
pub use updates::{Release, UpdateState};

/// Milliseconds since the Unix epoch. Used for every timestamp that crosses IPC.
pub type Millis = i64;
