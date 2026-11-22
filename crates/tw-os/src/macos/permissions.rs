use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_app_kit::NSWorkspace;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use objc2_foundation::{NSString, NSURL};

use super::ffi::*;
use super::keys::{accessibility_trusted, listen_access};
use crate::permissions::macos_settings_url;
use crate::{OsError, PermissionKind, PermissionStatus, Result};

pub(crate) fn status(kind: PermissionKind) -> PermissionStatus {
    match kind {
        PermissionKind::Microphone => microphone_status(),
        // Neither API tells "never asked" from "turned off"; both read as
        // not determined until granted, and the UI offers the settings.
        PermissionKind::Accessibility => granted_or_unknown(accessibility_trusted()),
        PermissionKind::InputMonitoring => granted_or_unknown(listen_access()),
    }
}

fn granted_or_unknown(granted: bool) -> PermissionStatus {
    if granted {
        PermissionStatus::Granted
    } else {
        PermissionStatus::NotDetermined
    }
}

fn microphone_status() -> PermissionStatus {
    // SAFETY: an immutable framework constant.
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return PermissionStatus::NotDetermined;
    };
    // SAFETY: the audio media type is a valid argument.
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::Authorized => PermissionStatus::Granted,
        AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
            PermissionStatus::Denied
        }
        _ => PermissionStatus::NotDetermined,
    }
}

pub(crate) fn request(kind: PermissionKind) -> Result<()> {
    match kind {
        PermissionKind::Microphone => {
            // SAFETY: an immutable framework constant.
            let audio = unsafe { AVMediaTypeAudio }
                .ok_or_else(|| OsError::Platform("no audio media type".into()))?;
            let handler = RcBlock::new(|granted: Bool| {
                tracing::info!(granted = granted.as_bool(), "microphone access answered");
            });
            // SAFETY: valid media type and a block AVFoundation copies.
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &handler)
            };
        }
        PermissionKind::Accessibility => {
            // SAFETY: a one-entry dictionary of framework constants, released
            // by `Owned`.
            unsafe {
                let keys = [kAXTrustedCheckOptionPrompt];
                let values = [kCFBooleanTrue];
                let options = Owned::new(CFDictionaryCreate(
                    std::ptr::null(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    1,
                    (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
                    (&raw const kCFTypeDictionaryValueCallBacks).cast(),
                ))
                .ok_or_else(|| OsError::Platform("could not build the request".into()))?;
                AXIsProcessTrustedWithOptions(options.0);
            }
        }
        PermissionKind::InputMonitoring => {
            // SAFETY: no arguments.
            unsafe { CGRequestListenEventAccess() };
        }
    }
    Ok(())
}

pub(crate) fn open_settings(kind: PermissionKind) -> Result<()> {
    open_url(macos_settings_url(kind))
}

pub(crate) fn open_url(url: &str) -> Result<()> {
    let url = NSURL::URLWithString(&NSString::from_str(url))
        .ok_or_else(|| OsError::Platform(format!("invalid URL {url}")))?;
    if NSWorkspace::sharedWorkspace().openURL(&url) {
        Ok(())
    } else {
        Err(OsError::Platform("could not open System Settings".into()))
    }
}
