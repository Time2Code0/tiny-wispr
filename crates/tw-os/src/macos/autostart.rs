use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::{OsError, Result};

pub(crate) fn is_enabled() -> Result<bool> {
    // SAFETY: plain reads of the app's own login item.
    let status = unsafe { SMAppService::mainAppService().status() };
    Ok(status == SMAppServiceStatus::Enabled)
}

pub(crate) fn set_enabled(enabled: bool) -> Result<()> {
    // SAFETY: registers or removes the app's own login item.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|error| OsError::Platform(error.localizedDescription().to_string()))
}
