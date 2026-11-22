pub(crate) mod insert;
pub(crate) mod keys;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads a string value under `HKEY_CURRENT_USER`.
fn read_user_string(key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    let mut buffer = vec![0_u16; 1024];
    let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
    // SAFETY: valid wide strings and an output buffer of `size` bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            wide(value).as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    buffer.truncate(usize::try_from(size).unwrap_or_default() / 2);
    Some(
        String::from_utf16_lossy(&buffer)
            .trim_end_matches('\0')
            .to_owned(),
    )
}

pub(crate) mod permissions {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::{read_user_string, wide};
    use crate::permissions::windows_settings_url;
    use crate::{OsError, PermissionKind, PermissionStatus, Result};

    const MICROPHONE_CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    pub(crate) fn status(kind: PermissionKind) -> PermissionStatus {
        if kind != PermissionKind::Microphone {
            return PermissionStatus::NotApplicable;
        }
        // Desktop apps are covered by the global switch and its
        // "NonPackaged" desktop-apps switch.
        let denied = [
            MICROPHONE_CONSENT.to_owned(),
            format!(r"{MICROPHONE_CONSENT}\NonPackaged"),
        ]
        .iter()
        .any(|key| read_user_string(key, "Value").is_some_and(|value| value == "Deny"));
        if denied {
            PermissionStatus::Denied
        } else {
            PermissionStatus::Granted
        }
    }

    /// Windows has no prompt for desktop apps.
    pub(crate) fn request(_kind: PermissionKind) -> Result<()> {
        Ok(())
    }

    pub(crate) fn open_settings(kind: PermissionKind) -> Result<()> {
        let url = windows_settings_url(kind).ok_or(OsError::Unsupported)?;
        // SAFETY: valid wide strings; no owner window.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                wide("open").as_ptr(),
                wide(url).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute reports success as a value above 32.
        if result as isize > 32 {
            Ok(())
        } else {
            Err(OsError::Platform("could not open Settings".into()))
        }
    }
}

pub(crate) mod apps {
    use std::path::Path;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    use crate::apps::app_info;
    use crate::{AppInfo, Result};

    pub(crate) fn frontmost() -> Result<Option<AppInfo>> {
        // SAFETY: plain queries; the process handle is closed below.
        let path = unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return Ok(None);
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Ok(None);
            }
            let mut buffer = [0_u16; 1024];
            let mut size = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut size,
            );
            CloseHandle(process);
            if ok == 0 {
                return Ok(None);
            }
            String::from_utf16_lossy(&buffer[..usize::try_from(size).unwrap_or_default()])
        };
        let path = Path::new(&path);
        let exe = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some(app_info(name, exe)))
    }
}

pub(crate) mod autostart {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };

    use super::{read_user_string, wide};
    use crate::{OsError, Result};

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Tiny Wispr";

    pub(crate) fn is_enabled() -> Result<bool> {
        Ok(read_user_string(RUN_KEY, VALUE).is_some())
    }

    pub(crate) fn set_enabled(enabled: bool) -> Result<()> {
        // SAFETY: valid wide strings and a data buffer of the given size.
        let status = unsafe {
            if enabled {
                let command = wide(&format!("\"{}\"", std::env::current_exe()?.display()));
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    wide(RUN_KEY).as_ptr(),
                    wide(VALUE).as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    u32::try_from(command.len() * 2).unwrap_or(u32::MAX),
                )
            } else {
                match RegDeleteKeyValueW(
                    HKEY_CURRENT_USER,
                    wide(RUN_KEY).as_ptr(),
                    wide(VALUE).as_ptr(),
                ) {
                    // Already off.
                    2 => 0,
                    status => status,
                }
            }
        };
        if status == 0 {
            Ok(())
        } else {
            Err(OsError::Io(std::io::Error::from_raw_os_error(
                status as i32,
            )))
        }
    }
}
