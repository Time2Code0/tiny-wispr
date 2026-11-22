use tw_core::{PermissionKind, Permissions};

use crate::error::CommandResult;

/// Never prompts.
#[tauri::command]
pub fn get_permissions() -> Permissions {
    tw_os::permissions::all()
}

/// Shows the OS prompt for `kind`. Emits `permissions:changed` once granted.
#[tauri::command]
pub fn request_permission(kind: PermissionKind) -> CommandResult<()> {
    Ok(tw_os::permissions::request(kind)?)
}

#[tauri::command]
pub fn open_permission_settings(kind: PermissionKind) -> CommandResult<()> {
    Ok(tw_os::permissions::open_settings(kind)?)
}
