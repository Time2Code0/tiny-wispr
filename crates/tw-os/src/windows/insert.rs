//! Paste and typed insertion on Windows: the Win32 clipboard and SendInput.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Ole::{
    CF_BITMAP, CF_ENHMETAFILE, CF_METAFILEPICT, CF_PALETTE, CF_UNICODETEXT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

use crate::insert::{
    Clipboard, InsertOptions, InsertionMethod, PendingRestore, Stroke, paste_with, typing_strokes,
    wait_for,
};
use crate::{OsError, Result};

/// Set as `dwExtraInfo` on every key this crate sends, so its own hook lets
/// them pass unreported.
pub(crate) const SYNTHETIC_EVENT_MARKER: usize = 0x7477_6973;
const SHORTCUT_EVENT_GAP: Duration = Duration::from_millis(15);
const COPY_TIMEOUT: Duration = Duration::from_millis(300);

static PENDING: Mutex<PendingRestore<Snapshot>> = Mutex::new(PendingRestore::new());

pub(crate) fn insert_text(text: &str, options: &InsertOptions) -> Result<()> {
    wait_for_modifier_release(options.modifier_timeout);
    match options.method {
        InsertionMethod::Paste => {
            paste_with(
                &Arc::new(WinClipboard),
                &PENDING,
                text,
                options.restore_clipboard,
                options.restore_after,
                || send_ctrl(layout_key('v')),
            )?;
            Ok(())
        }
        InsertionMethod::Type => type_text(text),
    }
}

pub(crate) fn copy_to_clipboard(text: &str) -> Result<()> {
    WinClipboard.write_text(text, false)
}

pub(crate) fn read_selection() -> Result<Option<String>> {
    wait_for_modifier_release(Duration::from_millis(
        crate::insert::MODIFIER_RELEASE_TIMEOUT_MS,
    ));
    let clipboard = WinClipboard;
    let before = clipboard.change_count();
    let snapshot = clipboard.snapshot()?;
    send_ctrl(layout_key('c'))?;
    if !wait_for(COPY_TIMEOUT, || clipboard.change_count() != before) {
        return Ok(None);
    }
    let copied = clipboard.change_count();
    let text = clipboard.read_text()?;
    if clipboard.change_count() == copied {
        clipboard.restore(&snapshot)?;
    }
    Ok(text.filter(|text| !text.is_empty()))
}

pub(crate) fn refresh_keyboard_layout() -> Result<()> {
    Ok(())
}

fn wait_for_modifier_release(timeout: Duration) {
    let released = wait_for(timeout, || {
        [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
            .iter()
            // SAFETY: plain key state reads.
            .all(|&vk| unsafe { GetAsyncKeyState(i32::from(vk)) } >= 0)
    });
    if !released {
        tracing::warn!("modifiers still held; inserting anyway");
    }
}

/// The virtual key that types `letter` on the active layout.
fn layout_key(letter: char) -> u16 {
    // SAFETY: plain layout lookup.
    let scan = unsafe { VkKeyScanW(letter as u16) };
    match u16::try_from(scan) {
        Ok(scan) if scan & 0xFF != 0xFF => scan & 0xFF,
        _ => letter.to_ascii_uppercase() as u16,
    }
}

fn key_input(vk: u16, unicode: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: unicode,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_EVENT_MARKER,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    let size = i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(i32::MAX);
    let count = u32::try_from(inputs.len()).unwrap_or(u32::MAX);
    // SAFETY: a valid slice of initialised inputs.
    let sent = unsafe { SendInput(count, inputs.as_ptr(), size) };
    if sent == count {
        Ok(())
    } else {
        Err(OsError::Platform(
            "the system blocked the keystrokes".into(),
        ))
    }
}

/// Presses and releases virtual keys, ignoring failures: used from the hook.
pub(crate) fn send_keys(keys: &[(u16, bool)]) {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, down)| key_input(vk, 0, if down { 0 } else { KEYEVENTF_KEYUP }))
        .collect();
    if let Err(error) = send(&inputs) {
        tracing::warn!(%error, "could not send the menu mask key");
    }
}

fn send_ctrl(key: u16) -> Result<()> {
    let steps = [
        (VK_CONTROL, 0),
        (key, 0),
        (key, KEYEVENTF_KEYUP),
        (VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    for (index, (vk, flags)) in steps.into_iter().enumerate() {
        if index > 0 {
            std::thread::sleep(SHORTCUT_EVENT_GAP);
        }
        send(&[key_input(vk, 0, flags)])?;
    }
    Ok(())
}

fn type_text(text: &str) -> Result<()> {
    let mut inputs = Vec::new();
    for stroke in typing_strokes(text, usize::MAX) {
        match stroke {
            Stroke::Text(units) => {
                for unit in units {
                    inputs.push(key_input(0, unit, KEYEVENTF_UNICODE));
                    inputs.push(key_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
            }
            Stroke::LineBreak => {
                inputs.push(key_input(VK_SHIFT, 0, 0));
                inputs.push(key_input(VK_RETURN, 0, 0));
                inputs.push(key_input(VK_RETURN, 0, KEYEVENTF_KEYUP));
                inputs.push(key_input(VK_SHIFT, 0, KEYEVENTF_KEYUP));
            }
        }
    }
    send(&inputs)
}

/// Every clipboard format held in global memory, as `(format, bytes)`.
type Snapshot = Vec<(u32, Vec<u8>)>;

struct WinClipboard;

/// The clipboard, open for this thread until dropped.
struct Open;

impl Open {
    fn new() -> Result<Self> {
        // Another app may hold the clipboard for a moment.
        for _ in 0..20 {
            // SAFETY: opening without an owner window is allowed.
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(OsError::Platform("the clipboard is busy".into()))
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: we opened it.
        unsafe { CloseClipboard() };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies `bytes` into movable global memory the clipboard then owns.
fn set_bytes(format: u32, bytes: &[u8]) -> Result<()> {
    // SAFETY: allocation, copy into the locked block, and hand-over; freed
    // only when the clipboard refuses it.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if memory.is_null() {
            return Err(OsError::Platform("out of memory".into()));
        }
        let target = GlobalLock(memory);
        if target.is_null() {
            GlobalFree(memory);
            return Err(OsError::Platform("out of memory".into()));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        GlobalUnlock(memory);
        if SetClipboardData(format, memory as HANDLE).is_null() {
            GlobalFree(memory);
            return Err(OsError::Platform("could not write the clipboard".into()));
        }
    }
    Ok(())
}

fn get_bytes(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open; the handle stays valid until it closes.
    unsafe {
        let handle = GetClipboardData(format);
        if handle.is_null() {
            return None;
        }
        let size = GlobalSize(handle);
        let source = GlobalLock(handle);
        if source.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(source.cast::<u8>(), size).to_vec();
        GlobalUnlock(handle);
        Some(bytes)
    }
}

impl WinClipboard {
    fn read_text(&self) -> Result<Option<String>> {
        let _open = Open::new()?;
        Ok(get_bytes(u32::from(CF_UNICODETEXT)).map(|bytes| {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_le_bytes(pair))
                .take_while(|&unit| unit != 0)
                .collect();
            String::from_utf16_lossy(&units)
        }))
    }
}

impl Clipboard for WinClipboard {
    type Snapshot = Snapshot;

    fn change_count(&self) -> i64 {
        // SAFETY: no arguments.
        i64::from(unsafe { GetClipboardSequenceNumber() })
    }

    fn snapshot(&self) -> Result<Snapshot> {
        let _open = Open::new()?;
        let mut snapshot = Vec::new();
        let mut format = 0;
        loop {
            // SAFETY: the clipboard is open.
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            // GDI handles are not global memory; the DIB formats carry the
            // same images.
            let gdi = [CF_BITMAP, CF_ENHMETAFILE, CF_METAFILEPICT, CF_PALETTE].map(u32::from);
            if gdi.contains(&format) {
                continue;
            }
            if let Some(bytes) = get_bytes(format) {
                snapshot.push((format, bytes));
            }
        }
        Ok(snapshot)
    }

    fn write_text(&self, text: &str, transient: bool) -> Result<()> {
        let _open = Open::new()?;
        // SAFETY: the clipboard is open.
        unsafe { EmptyClipboard() };
        let units = wide(text);
        let bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        set_bytes(u32::from(CF_UNICODETEXT), &bytes)?;
        if transient {
            // Keeps the transcript out of clipboard history, cloud sync and
            // clipboard managers that honour the convention.
            for (name, value) in [
                ("ExcludeClipboardContentFromMonitorProcessing", &[0_u8][..]),
                ("CanIncludeInClipboardHistory", &0_u32.to_le_bytes()[..]),
                ("CanUploadToCloudClipboard", &0_u32.to_le_bytes()[..]),
            ] {
                // SAFETY: a valid NUL-terminated wide string.
                let format = unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) };
                if format != 0 {
                    set_bytes(format, value)?;
                }
            }
        }
        Ok(())
    }

    fn restore(&self, snapshot: &Snapshot) -> Result<()> {
        let _open = Open::new()?;
        // SAFETY: the clipboard is open.
        unsafe { EmptyClipboard() };
        for (format, bytes) in snapshot {
            if let Err(error) = set_bytes(*format, bytes) {
                tracing::debug!(format, %error, "skipped a clipboard format on restore");
            }
        }
        Ok(())
    }
}
