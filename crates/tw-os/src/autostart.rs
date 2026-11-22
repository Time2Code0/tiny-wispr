//! Launch at login.

use super::{Result, platform};

/// Whether the app starts at login. macOS reports false while the login
/// item still waits for the user's approval in System Settings.
pub fn is_enabled() -> Result<bool> {
    platform::autostart::is_enabled()
}

/// macOS registers the app bundle itself (`SMAppService.mainApp`), so this
/// fails when not running from a bundled `.app`. Windows writes the
/// current executable to the `Run` key; Linux writes an XDG autostart entry.
pub fn set_enabled(enabled: bool) -> Result<()> {
    platform::autostart::set_enabled(enabled)
}

/// The XDG autostart entry for `exe`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn desktop_entry(exe: &std::path::Path) -> String {
    let exe = exe.to_string_lossy();
    let mut quoted = String::new();
    for c in exe.chars() {
        // Exec quoting escapes these with a backslash, and the desktop-entry
        // string syntax then doubles every backslash.
        match c {
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            _ => quoted.push(c),
        }
    }
    format!(
        "[Desktop Entry]\nType=Application\nName=Tiny Wispr\nExec=\"{quoted}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_quotes_the_path() {
        let entry = desktop_entry(std::path::Path::new("/opt/Tiny Wispr/tiny-wispr"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(
            entry.contains("\nExec=\"/opt/Tiny Wispr/tiny-wispr\"\n"),
            "{entry}"
        );
        let entry = desktop_entry(std::path::Path::new("/a\"b"));
        assert!(entry.contains("Exec=\"/a\\\\\"b\""), "{entry}");
    }
}
