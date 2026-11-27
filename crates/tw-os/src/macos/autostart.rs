use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::{OsError, Result};

fn status() -> SMAppServiceStatus {
    // SAFETY: plain reads of the app's own login item.
    unsafe { SMAppService::mainAppService().status() }
}

pub(crate) fn is_enabled() -> Result<bool> {
    Ok(status() == SMAppServiceStatus::Enabled)
}

pub(crate) fn set_enabled(enabled: bool) -> Result<()> {
    let current = status();
    if !enabled
        && (current == SMAppServiceStatus::NotRegistered || current == SMAppServiceStatus::NotFound)
    {
        // Unregistering a login item that isn't there is an error.
        return Ok(());
    }
    // SAFETY: registers or removes the app's own login item.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|error| OsError::Platform(error.localizedDescription().to_string()))?;
    if enabled && status() == SMAppServiceStatus::RequiresApproval {
        // SAFETY: opens System Settings; the user just asked for this.
        unsafe { SMAppService::openSystemSettingsLoginItems() };
        return Err(OsError::Platform(
            "Allow Tiny Wispr under Login Items in System Settings, then try again".into(),
        ));
    }
    Ok(())
}
