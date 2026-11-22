//! The macOS menu bar: Tauri's default menu with Settings… (⌘,) added, so
//! the shortcut works while no window is open.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

use crate::tray;

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    if let Some(app_submenu) = menu.items()?.first().and_then(|item| item.as_submenu()) {
        // After About and its separator, as in other macOS apps.
        app_submenu.insert_items(
            &[
                &MenuItem::with_id(app, tray::SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?,
                &PredefinedMenuItem::separator(app)?,
            ],
            2,
        )?;
    }
    Ok(menu)
}
