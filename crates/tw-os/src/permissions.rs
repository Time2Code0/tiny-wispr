//! Permission status, requests and deep links to system settings.
//!
//! macOS: Accessibility covers both the key listener (an active event tap)
//! and insertion (posting keystrokes). Input Monitoring alone allows an
//! observe-only listener that cannot swallow keys and cannot insert.
//! Windows and Linux have no such permissions; the microphone can be turned
//! off for desktop apps in Windows privacy settings.

use super::{PermissionKind, PermissionStatus, Permissions, Result, platform};

/// Reads the status. Never prompts; safe to poll.
pub fn status(kind: PermissionKind) -> PermissionStatus {
    platform::permissions::status(kind)
}

pub fn all() -> Permissions {
    Permissions {
        microphone: status(PermissionKind::Microphone),
        accessibility: status(PermissionKind::Accessibility),
        input_monitoring: status(PermissionKind::InputMonitoring),
    }
}

/// Shows the OS prompt. Only call on an explicit user action. Returns at
/// once; poll `status` for the answer. macOS only prompts the first time:
/// afterwards open the settings instead.
pub fn request(kind: PermissionKind) -> Result<()> {
    platform::permissions::request(kind)
}

/// Opens the matching pane of System Settings or Windows Settings.
pub fn open_settings(kind: PermissionKind) -> Result<()> {
    platform::permissions::open_settings(kind)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_settings_url(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        PermissionKind::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        PermissionKind::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn windows_settings_url(kind: PermissionKind) -> Option<&'static str> {
    (kind == PermissionKind::Microphone).then_some("ms-settings:privacy-microphone")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_urls_name_the_right_pane() {
        assert!(macos_settings_url(PermissionKind::Microphone).ends_with("?Privacy_Microphone"));
        assert!(
            macos_settings_url(PermissionKind::Accessibility).ends_with("?Privacy_Accessibility")
        );
        assert!(
            macos_settings_url(PermissionKind::InputMonitoring).ends_with("?Privacy_ListenEvent")
        );
        assert_eq!(
            windows_settings_url(PermissionKind::Microphone),
            Some("ms-settings:privacy-microphone")
        );
        assert_eq!(windows_settings_url(PermissionKind::Accessibility), None);
    }

    #[test]
    fn status_reads_without_prompting() {
        // Reads only; a prompt here would hang an unattended run.
        let all = all();
        for kind in PermissionKind::ALL {
            assert_eq!(all.get(kind), status(kind));
        }
    }
}
