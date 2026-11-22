//! Paste and typed insertion on X11: arboard for the clipboard, XTEST for
//! keystrokes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arboard::SetExtLinux;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, Keysym};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use super::{connect, x11};
use crate::insert::{
    Clipboard, InsertOptions, InsertionMethod, PendingRestore, Stroke, paste_with, typing_strokes,
    wait_for,
};
use crate::{OsError, Result};

const SHORTCUT_EVENT_GAP: Duration = Duration::from_millis(15);
const COPY_TIMEOUT: Duration = Duration::from_millis(300);
const KEYSYM_CONTROL_L: Keysym = 0xffe3;
const KEYSYM_SHIFT_L: Keysym = 0xffe1;
const KEYSYM_RETURN: Keysym = 0xff0d;

static PENDING: Mutex<PendingRestore<Option<String>>> = Mutex::new(PendingRestore::new());
/// On X11 the clipboard is served by its owner, so the handle lives as long
/// as the process.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

pub(crate) fn insert_text(text: &str, options: &InsertOptions) -> Result<()> {
    let (conn, root) = connect()?;
    let keymap = Keymap::read(&conn)?;
    wait_for_modifier_release(&conn, root, options.modifier_timeout);
    match options.method {
        InsertionMethod::Paste => {
            paste_with(
                &Arc::new(X11Clipboard),
                &PENDING,
                text,
                options.restore_clipboard,
                options.restore_after,
                || send_ctrl(&conn, root, &keymap, Keysym::from(b'v')),
            )?;
            Ok(())
        }
        InsertionMethod::Type => type_text(&conn, root, &keymap, text),
    }
}

pub(crate) fn copy_to_clipboard(text: &str) -> Result<()> {
    X11Clipboard.write_text(text, false)
}

pub(crate) fn read_selection() -> Result<Option<String>> {
    let (conn, root) = connect()?;
    let keymap = Keymap::read(&conn)?;
    wait_for_modifier_release(
        &conn,
        root,
        Duration::from_millis(crate::insert::MODIFIER_RELEASE_TIMEOUT_MS),
    );
    let clipboard = X11Clipboard;
    let snapshot = clipboard.snapshot()?;
    let before = clipboard.change_count();
    send_ctrl(&conn, root, &keymap, Keysym::from(b'c'))?;
    // The text is the only way to see a change here, so copying the same
    // text as before reads as no selection.
    if !wait_for(COPY_TIMEOUT, || clipboard.change_count() != before) {
        return Ok(None);
    }
    let text = clipboard.snapshot()?;
    clipboard.restore(&snapshot)?;
    Ok(text.filter(|text| !text.is_empty()))
}

pub(crate) fn refresh_keyboard_layout() -> Result<()> {
    Ok(())
}

fn with_clipboard<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<T>) -> Result<T> {
    let mut guard = CLIPBOARD.lock().unwrap_or_else(|error| error.into_inner());
    if guard.is_none() {
        *guard = Some(x11(arboard::Clipboard::new())?);
    }
    let clipboard = guard
        .as_mut()
        .ok_or_else(|| OsError::Platform("no clipboard".into()))?;
    f(clipboard)
}

/// Text only: arboard reads no other formats on Linux.
struct X11Clipboard;

impl Clipboard for X11Clipboard {
    type Snapshot = Option<String>;

    /// X11 has no change counter; a hash of the text stands in for one.
    fn change_count(&self) -> i64 {
        let text = self.snapshot().ok().flatten();
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        i64::from_ne_bytes(hasher.finish().to_ne_bytes())
    }

    fn snapshot(&self) -> Result<Option<String>> {
        with_clipboard(|clipboard| Ok(clipboard.get_text().ok()))
    }

    fn write_text(&self, text: &str, transient: bool) -> Result<()> {
        with_clipboard(|clipboard| {
            let set = clipboard.set();
            let set = if transient {
                set.exclude_from_history()
            } else {
                set
            };
            x11(set.text(text))
        })
    }

    fn restore(&self, snapshot: &Option<String>) -> Result<()> {
        with_clipboard(|clipboard| match snapshot {
            Some(text) => x11(clipboard.set_text(text)),
            None => x11(clipboard.clear()),
        })
    }
}

/// The keyboard mapping: keysyms per keycode.
struct Keymap {
    min: u8,
    per: u8,
    syms: Vec<Keysym>,
}

impl Keymap {
    fn read(conn: &RustConnection) -> Result<Self> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = x11(x11(conn.get_keyboard_mapping(min, max - min + 1))?.reply())?;
        Ok(Self {
            min,
            per: reply.keysyms_per_keycode,
            syms: reply.keysyms,
        })
    }

    fn codes(&self) -> impl Iterator<Item = (u8, &[Keysym])> {
        self.syms
            .chunks(usize::from(self.per.max(1)))
            .enumerate()
            .filter_map(|(index, syms)| {
                Some((self.min.checked_add(u8::try_from(index).ok()?)?, syms))
            })
    }

    /// The keycode typing `keysym` without Shift.
    fn unshifted(&self, keysym: Keysym) -> Option<u8> {
        self.codes()
            .find(|(_, syms)| syms.first() == Some(&keysym))
            .map(|(code, _)| code)
    }

    /// A keycode with nothing mapped, borrowed to type other characters.
    fn spare(&self) -> Option<u8> {
        self.codes()
            .filter(|(_, syms)| syms.iter().all(|&sym| sym == 0))
            .map(|(code, _)| code)
            .last()
    }
}

fn keysym(c: char) -> Keysym {
    let code = u32::from(c);
    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        code
    } else {
        0x0100_0000 + code
    }
}

fn fake(conn: &RustConnection, root: u32, code: u8, down: bool) -> Result<()> {
    let kind = if down {
        KEY_PRESS_EVENT
    } else {
        KEY_RELEASE_EVENT
    };
    x11(conn.xtest_fake_input(kind, code, x11rb::CURRENT_TIME, root, 0, 0, 0))?;
    Ok(())
}

fn code_for(keymap: &Keymap, keysym: Keysym) -> Result<u8> {
    keymap
        .unshifted(keysym)
        .ok_or_else(|| OsError::Platform(format!("no key for keysym {keysym:#x}")))
}

fn send_ctrl(conn: &RustConnection, root: u32, keymap: &Keymap, letter: Keysym) -> Result<()> {
    let ctrl = code_for(keymap, KEYSYM_CONTROL_L)?;
    let key = code_for(keymap, letter)?;
    for (index, (code, down)) in [(ctrl, true), (key, true), (key, false), (ctrl, false)]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            std::thread::sleep(SHORTCUT_EVENT_GAP);
        }
        fake(conn, root, code, down)?;
        x11(conn.flush())?;
    }
    Ok(())
}

fn type_text(conn: &RustConnection, root: u32, keymap: &Keymap, text: &str) -> Result<()> {
    let spare = keymap.spare();
    let per = keymap.per.max(1);
    let result = (|| {
        for stroke in typing_strokes(text, usize::MAX) {
            let Stroke::Text(units) = stroke else {
                let shift = code_for(keymap, KEYSYM_SHIFT_L)?;
                let enter = code_for(keymap, KEYSYM_RETURN)?;
                for (code, down) in [(shift, true), (enter, true), (enter, false), (shift, false)] {
                    fake(conn, root, code, down)?;
                }
                continue;
            };
            for c in char::decode_utf16(units).filter_map(std::result::Result::ok) {
                let sym = keysym(c);
                let code = match keymap.unshifted(sym) {
                    Some(code) => code,
                    None => {
                        // Map the character onto the spare key for this
                        // press, as xdotool does.
                        let spare =
                            spare.ok_or_else(|| OsError::Platform("no spare keycode".into()))?;
                        let syms = vec![sym; usize::from(per)];
                        x11(x11(conn.change_keyboard_mapping(1, spare, per, &syms))?.check())?;
                        std::thread::sleep(Duration::from_millis(10));
                        spare
                    }
                };
                fake(conn, root, code, true)?;
                fake(conn, root, code, false)?;
                x11(x11(conn.get_input_focus())?.reply())?;
            }
        }
        Ok(())
    })();
    if let Some(spare) = spare {
        let _ = conn.change_keyboard_mapping(1, spare, per, &vec![0; usize::from(per)]);
        let _ = conn.flush();
    }
    result
}

/// Shift, Control, Mod1 (Alt) and Mod4 (Super) in the core modifier mask.
const HELD_MODIFIERS: u16 = 0b0100_1101;

fn wait_for_modifier_release(conn: &RustConnection, root: u32, timeout: Duration) {
    let released = wait_for(timeout, || {
        conn.query_pointer(root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_none_or(|reply| u16::from(reply.mask) & HELD_MODIFIERS == 0)
    });
    if !released {
        tracing::warn!("modifiers still held; inserting anyway");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysyms_follow_the_x11_convention() {
        assert_eq!(keysym('v'), 0x76);
        assert_eq!(keysym('é'), 0xe9);
        assert_eq!(keysym('€'), 0x0100_20ac);
        assert_eq!(keysym('😀'), 0x0101_f600);
    }

    #[test]
    fn keymap_finds_keys_and_a_spare() {
        let keymap = Keymap {
            min: 8,
            per: 2,
            syms: vec![0x76, 0x56, 0, 0, 0xffe3, 0],
        };
        assert_eq!(keymap.unshifted(0x76), Some(8));
        assert_eq!(keymap.unshifted(0x56), None, "shifted");
        assert_eq!(keymap.unshifted(KEYSYM_CONTROL_L), Some(10));
        assert_eq!(keymap.spare(), Some(9));
    }
}
