//! Per-OS integration behind one API. macOS is the primary target; Windows and
//! Linux live behind `cfg` and must at least not break the macOS build.
//!
//! Nothing here may trigger an OS permission prompt unless the user asked for
//! it (`permissions::request`).

pub use tw_core::{AppInfo, PermissionKind, PermissionStatus, Permissions};

pub mod apps;
pub mod autostart;
pub mod insert;
pub mod keys;
pub mod permissions;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

#[derive(Debug, thiserror::Error)]
pub enum OsError {
    #[error("not implemented")]
    NotImplemented,
    /// The OS does not allow this, such as a global key listener on Wayland.
    #[error("not supported on this platform")]
    Unsupported,
    #[error("{} permission is missing", permission_name(*.0))]
    PermissionMissing(PermissionKind),
    #[error("{0}")]
    Platform(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn permission_name(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Microphone => "Microphone",
        PermissionKind::Accessibility => "Accessibility",
        PermissionKind::InputMonitoring => "Input Monitoring",
    }
}

pub type Result<T, E = OsError> = std::result::Result<T, E>;

/// Milliseconds since the first call, on a monotonic clock shared by every
/// event this crate reports.
pub(crate) fn now_ms() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_error_names_the_permission() {
        assert_eq!(
            OsError::PermissionMissing(PermissionKind::InputMonitoring).to_string(),
            "Input Monitoring permission is missing"
        );
    }
}
