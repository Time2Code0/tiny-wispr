use objc2_app_kit::NSWorkspace;

use crate::apps::app_info;
use crate::{AppInfo, Result};

pub(crate) fn frontmost() -> Result<Option<AppInfo>> {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return Ok(None);
    };
    let id = app.bundleIdentifier().map(|id| id.to_string());
    let name = app
        .localizedName()
        .map(|name| name.to_string())
        .or_else(|| id.clone())
        .unwrap_or_default();
    Ok(Some(app_info(name, id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_frontmost_app_without_permission() {
        if let Some(app) = frontmost().expect("frontmost") {
            assert!(!app.name.is_empty());
        }
    }
}
