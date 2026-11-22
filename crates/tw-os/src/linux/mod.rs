//! Linux on X11, through x11rb. Wayland gives apps no global key events and
//! no synthetic input, so the listener and insertion return `Unsupported`
//! there; only the clipboard fallback works.

pub(crate) mod insert;
pub(crate) mod keys;

use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

use crate::{OsError, Result};

fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// An X11 connection and its root window, refusing Wayland sessions where
/// X11 only reaches XWayland windows.
fn connect() -> Result<(RustConnection, u32)> {
    if is_wayland() {
        return Err(OsError::Unsupported);
    }
    let (conn, screen) =
        x11rb::connect(None).map_err(|error| OsError::Platform(error.to_string()))?;
    let root = conn
        .setup()
        .roots
        .get(screen)
        .map(|screen| screen.root)
        .ok_or_else(|| OsError::Platform("no X11 screen".into()))?;
    Ok((conn, root))
}

fn x11<T, E: std::fmt::Display>(result: std::result::Result<T, E>) -> Result<T> {
    result.map_err(|error| OsError::Platform(error.to_string()))
}

pub(crate) mod permissions {
    use crate::{OsError, PermissionKind, PermissionStatus, Result};

    /// Linux desktops have no per-app permission for these.
    pub(crate) fn status(_kind: PermissionKind) -> PermissionStatus {
        PermissionStatus::NotApplicable
    }

    pub(crate) fn request(_kind: PermissionKind) -> Result<()> {
        Ok(())
    }

    pub(crate) fn open_settings(_kind: PermissionKind) -> Result<()> {
        Err(OsError::Unsupported)
    }
}

pub(crate) mod apps {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

    use super::{connect, x11};
    use crate::apps::app_info;
    use crate::{AppInfo, OsError, Result};

    pub(crate) fn frontmost() -> Result<Option<AppInfo>> {
        let (conn, root) = match connect() {
            Err(OsError::Unsupported) => return Ok(None),
            other => other?,
        };
        let atom = |name: &str| -> Result<u32> {
            Ok(x11(x11(conn.intern_atom(false, name.as_bytes()))?.reply())?.atom)
        };
        let active = x11(x11(conn.get_property(
            false,
            root,
            atom("_NET_ACTIVE_WINDOW")?,
            AtomEnum::WINDOW,
            0,
            1,
        ))?
        .reply())?;
        let Some(window) = active
            .value32()
            .and_then(|mut values| values.next())
            .filter(|&w| w != 0)
        else {
            return Ok(None);
        };
        let class = x11(x11(conn.get_property(
            false,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            256,
        ))?
        .reply())?;
        // WM_CLASS is "instance\0class\0"; the class names the app.
        let class = class
            .value
            .split(|&byte| byte == 0)
            .rfind(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned());
        let utf8 = atom("UTF8_STRING")?;
        let title =
            x11(
                x11(conn.get_property(false, window, atom("_NET_WM_NAME")?, utf8, 0, 1024))?
                    .reply(),
            )?;
        let name = class
            .clone()
            .unwrap_or_else(|| String::from_utf8_lossy(&title.value).into_owned());
        Ok(Some(app_info(name, class)))
    }
}

pub(crate) mod autostart {
    use std::path::PathBuf;

    use crate::autostart::desktop_entry;
    use crate::{OsError, Result};

    fn entry_path() -> Result<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| OsError::Platform("no home directory".into()))?;
        Ok(config.join("autostart").join("tiny-wispr.desktop"))
    }

    pub(crate) fn is_enabled() -> Result<bool> {
        Ok(entry_path()?.is_file())
    }

    pub(crate) fn set_enabled(enabled: bool) -> Result<()> {
        let path = entry_path()?;
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, desktop_entry(&std::env::current_exe()?))?;
        } else if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.into());
        }
        Ok(())
    }
}
