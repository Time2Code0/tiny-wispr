//! OS permissions and input devices.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionKind {
    Microphone,
    /// macOS: needed to paste and to see the shortcut.
    Accessibility,
    /// macOS: needed by the global key listener on some versions.
    InputMonitoring,
}

impl PermissionKind {
    pub const ALL: [Self; 3] = [Self::Microphone, Self::Accessibility, Self::InputMonitoring];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    Granted,
    Denied,
    #[default]
    NotDetermined,
    /// This OS does not have this permission.
    NotApplicable,
}

impl PermissionStatus {
    pub fn is_satisfied(self) -> bool {
        matches!(self, Self::Granted | Self::NotApplicable)
    }
}

/// Payload of `get_permissions` and `permissions:changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub microphone: PermissionStatus,
    pub accessibility: PermissionStatus,
    pub input_monitoring: PermissionStatus,
}

impl Permissions {
    pub fn get(&self, kind: PermissionKind) -> PermissionStatus {
        match kind {
            PermissionKind::Microphone => self.microphone,
            PermissionKind::Accessibility => self.accessibility,
            PermissionKind::InputMonitoring => self.input_monitoring,
        }
    }

    pub fn all_satisfied(&self) -> bool {
        PermissionKind::ALL
            .into_iter()
            .all(|k| self.get(k).is_satisfied())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneInfo {
    /// Stable id to store in settings.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
