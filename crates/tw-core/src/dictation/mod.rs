//! Dictation state as the UI sees it (`dictation:state` event), plus the
//! state machine that drives it.

pub mod machine;

use serde::{Deserialize, Serialize};

/// A press shorter than this does not paste (spec §2.1).
pub const MIN_HOLD_MS: u64 = 500;
/// Window for a second tap (double-tap → hands-free). Pressing again this soon
/// after hands-free started cancels instead of stopping.
pub const DOUBLE_TAP_MS: u64 = 500;
/// Pressing another app's shortcut within this window cancels silently.
pub const FOREIGN_SHORTCUT_WINDOW_MS: u64 = 1_000;
pub const SESSION_WARN_MS: u64 = 19 * 60 * 1_000;
pub const SESSION_LIMIT_MS: u64 = 20 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationPhase {
    #[default]
    Idle,
    Listening,
    Processing,
    Notice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DictationMode {
    /// Hold the shortcut, release to finish.
    #[default]
    Hold,
    /// Started by double-tap, the hands-free shortcut or a pill click.
    HandsFree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessingStage {
    Transcribing,
    Polishing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    Cancelled,
    PasteFailed,
    NoSpeech,
    NoMicrophone,
    StillProcessing,
    NoModel,
    TimeLimitWarning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeActionKind {
    /// Transcribe the cancelled audio and paste it.
    Undo,
    PasteLast,
    /// Put the last transcript on the clipboard.
    CopyLast,
    OpenSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeAction {
    pub kind: NoticeActionKind,
    pub label: String,
}

/// A small auto-dismissing card above the pill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    pub action: Option<NoticeAction>,
}

/// Payload of `dictation:state` and result of `get_dictation_state`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationState {
    pub phase: DictationPhase,
    /// Set while listening or processing.
    pub mode: Option<DictationMode>,
    /// Set while processing.
    pub stage: Option<ProcessingStage>,
    /// Always set in the notice phase. Also set while listening (the
    /// 19-minute warning) and processing ("Still processing your last
    /// dictation"), shown above the pill without leaving that phase.
    pub notice: Option<Notice>,
    /// Time spent listening in this session: counting up while listening,
    /// frozen at the recorded length while processing, 0 otherwise.
    pub elapsed_ms: u64,
}

impl DictationState {
    pub fn idle() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_serialises_with_nulls() {
        let json = serde_json::to_value(DictationState::idle()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "phase": "idle",
                "mode": null,
                "stage": null,
                "notice": null,
                "elapsedMs": 0
            })
        );
    }
}
