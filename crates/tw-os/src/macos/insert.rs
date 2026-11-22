//! Paste and typed insertion on macOS: the general pasteboard, posted
//! keyboard events, and the layout lookup that finds the V key.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

use super::ffi::*;
use super::keys::accessibility_trusted;
use crate::insert::{
    Clipboard, InsertOptions, InsertionMethod, PendingRestore, Stroke, paste_with, typing_strokes,
    wait_for,
};
use crate::{OsError, PermissionKind, Result};

/// Set on every event this crate posts, so its own listener lets them pass
/// unreported.
pub(crate) const SYNTHETIC_EVENT_MARKER: i64 = 0x7477_6973_7072;
/// Quartz keeps at most 20 UTF-16 units of a keyboard event's string.
const MAX_UNICODE_UNITS: usize = 20;
/// Remote desktop and VM clients track modifiers from their own events and
/// can read a burst posted in one instant as the bare key.
const SHORTCUT_EVENT_GAP: Duration = Duration::from_millis(15);
const COMMAND_KEY_CODE: u16 = 55;
const SHIFT_KEY_CODE: u16 = 56;
const RETURN_KEY_CODE: u16 = 36;
/// nspasteboard.org markers that ask clipboard managers not to record the
/// transcript, which is only on the clipboard for the paste.
const CLIPBOARD_MANAGER_MARKERS: [&str; 2] = [
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
];
const COPY_TIMEOUT: Duration = Duration::from_millis(300);

static PENDING: Mutex<PendingRestore<Snapshot>> = Mutex::new(PendingRestore::new());

pub(crate) fn insert_text(text: &str, options: &InsertOptions) -> Result<()> {
    if !accessibility_trusted() {
        return Err(OsError::PermissionMissing(PermissionKind::Accessibility));
    }
    wait_for_modifier_release(options.modifier_timeout);
    match options.method {
        InsertionMethod::Paste => {
            let v = layout_keys().v;
            paste_with(
                &Arc::new(MacClipboard),
                &PENDING,
                text,
                options.restore_clipboard,
                options.restore_after,
                || post_command(v),
            )?;
            Ok(())
        }
        InsertionMethod::Type => type_text(text),
    }
}

pub(crate) fn copy_to_clipboard(text: &str) -> Result<()> {
    MacClipboard.write_text(text, false)
}

pub(crate) fn read_selection() -> Result<Option<String>> {
    if !accessibility_trusted() {
        return Err(OsError::PermissionMissing(PermissionKind::Accessibility));
    }
    wait_for_modifier_release(Duration::from_millis(
        crate::insert::MODIFIER_RELEASE_TIMEOUT_MS,
    ));
    let clipboard = MacClipboard;
    let before = clipboard.change_count();
    let snapshot = clipboard.snapshot()?;
    post_command(layout_keys().c)?;
    if !wait_for(COPY_TIMEOUT, || clipboard.change_count() != before) {
        return Ok(None);
    }
    let copied = clipboard.change_count();
    let text = NSPasteboard::generalPasteboard()
        // SAFETY: an immutable framework constant.
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|text| text.to_string());
    if clipboard.change_count() == copied {
        clipboard.restore(&snapshot)?;
    }
    Ok(text.filter(|text| !text.is_empty()))
}

fn wait_for_modifier_release(timeout: Duration) {
    let held = FLAG_SHIFT | FLAG_CONTROL | FLAG_ALTERNATE | FLAG_COMMAND | FLAG_SECONDARY_FN;
    let released = wait_for(timeout, || {
        // SAFETY: plain read of the hardware modifier state.
        unsafe { CGEventSourceFlagsState(SOURCE_STATE_HID_SYSTEM) & held == 0 }
    });
    if !released {
        tracing::warn!("modifiers still held; inserting anyway");
    }
}

/// Every item and representation on the clipboard.
type Snapshot = Vec<Vec<(String, Vec<u8>)>>;

/// The general pasteboard. It is looked up on each call: the object is not
/// `Send`, and the restore runs on another thread.
struct MacClipboard;

impl Clipboard for MacClipboard {
    type Snapshot = Snapshot;

    fn change_count(&self) -> i64 {
        i64::try_from(NSPasteboard::generalPasteboard().changeCount()).unwrap_or_default()
    }

    fn snapshot(&self) -> Result<Snapshot> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let Some(items) = pasteboard.pasteboardItems() else {
            return Ok(Vec::new());
        };
        Ok(items
            .iter()
            .map(|item| {
                item.types()
                    .iter()
                    .filter_map(|kind| {
                        let name = kind.to_string();
                        // Dynamic types are conversions macOS derives from
                        // the real ones; writing them back fails.
                        if name.starts_with("dyn.") {
                            return None;
                        }
                        let data = item.dataForType(&kind)?;
                        Some((name, data.to_vec()))
                    })
                    .collect()
            })
            .collect())
    }

    fn write_text(&self, text: &str, transient: bool) -> Result<()> {
        // One item carrying the text and its markers, written at once, so a
        // polling clipboard manager never sees the text without them.
        let item = NSPasteboardItem::new();
        let empty = NSData::new();
        let marked = !transient
            || CLIPBOARD_MANAGER_MARKERS
                .iter()
                .all(|marker| item.setData_forType(&empty, &NSString::from_str(marker)));
        // SAFETY: an immutable framework constant.
        let written =
            item.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
        if !(marked && written) {
            return Err(OsError::Platform("could not prepare the clipboard".into()));
        }
        write_items(&[item])
    }

    fn restore(&self, snapshot: &Snapshot) -> Result<()> {
        let items: Vec<_> = snapshot
            .iter()
            .map(|representations| {
                let item = NSPasteboardItem::new();
                for (kind, data) in representations {
                    if !item.setData_forType(&NSData::with_bytes(data), &NSString::from_str(kind)) {
                        tracing::debug!(kind, "skipped a clipboard format on restore");
                    }
                }
                item
            })
            .collect();
        write_items(&items)
    }
}

fn write_items(items: &[Retained<NSPasteboardItem>]) -> Result<()> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    if items.is_empty() {
        return Ok(());
    }
    let objects: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = items
        .iter()
        .map(|item| ProtocolObject::from_ref(&**item))
        .collect();
    if pasteboard.writeObjects(&NSArray::from_slice(&objects)) {
        Ok(())
    } else {
        Err(OsError::Platform("could not write the clipboard".into()))
    }
}

/// An owned CoreGraphics keyboard event.
struct KeyboardEvent(EventRef);

impl KeyboardEvent {
    fn new(key: u16, down: bool, flags: u64) -> Result<Self> {
        // SAFETY: a null source selects the default; the event is owned.
        let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), key, down) };
        if event.is_null() {
            return Err(OsError::Platform(
                "could not create a keyboard event".into(),
            ));
        }
        // SAFETY: `event` is valid.
        unsafe {
            // A real keyboard reports modifier keys as flag changes, and
            // remote desktops track ⌘ from those.
            if matches!(key, COMMAND_KEY_CODE | SHIFT_KEY_CODE) {
                CGEventSetType(event, EVENT_FLAGS_CHANGED);
            }
            CGEventSetFlags(event, flags);
            CGEventSetIntegerValueField(
                event,
                FIELD_EVENT_SOURCE_USER_DATA,
                SYNTHETIC_EVENT_MARKER,
            );
        }
        Ok(Self(event))
    }

    fn post(&self) {
        // SAFETY: `self.0` is a valid event.
        unsafe { CGEventPost(HID_EVENT_TAP, self.0) };
    }
}

impl Drop for KeyboardEvent {
    fn drop(&mut self) {
        // SAFETY: we own the event.
        unsafe { CFRelease(self.0 as CfRef) };
    }
}

/// The events of `modifier`+`key`: modifier down, key down and up, modifier
/// up.
fn chord(modifier: (u16, u64), key: u16) -> [(u16, bool, u64); 4] {
    let (modifier_key, flag) = modifier;
    [
        (modifier_key, true, flag),
        (key, true, flag),
        (key, false, flag),
        (modifier_key, false, 0),
    ]
}

fn post_command(key: u16) -> Result<()> {
    let events = chord((COMMAND_KEY_CODE, FLAG_COMMAND), key)
        .into_iter()
        .map(|(key, down, flags)| KeyboardEvent::new(key, down, flags))
        .collect::<Result<Vec<_>>>()?;
    for (index, event) in events.iter().enumerate() {
        if index > 0 {
            std::thread::sleep(SHORTCUT_EVENT_GAP);
        }
        event.post();
    }
    Ok(())
}

fn type_text(text: &str) -> Result<()> {
    let mut events = Vec::new();
    for stroke in typing_strokes(text, MAX_UNICODE_UNITS) {
        match stroke {
            Stroke::Text(units) => {
                for down in [true, false] {
                    let event = KeyboardEvent::new(0, down, 0)?;
                    // SAFETY: Quartz copies the buffer.
                    unsafe {
                        CGEventKeyboardSetUnicodeString(event.0, units.len(), units.as_ptr())
                    };
                    events.push(event);
                }
            }
            Stroke::LineBreak => {
                for (key, down, flags) in chord((SHIFT_KEY_CODE, FLAG_SHIFT), RETURN_KEY_CODE) {
                    events.push(KeyboardEvent::new(key, down, flags)?);
                }
            }
        }
    }
    for event in events {
        event.post();
    }
    Ok(())
}

/// Key codes of the letters the shortcuts need, on the active layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayoutKeys {
    v: u16,
    c: u16,
}

/// ANSI positions, used until the layout is read.
static LAYOUT_KEYS: RwLock<LayoutKeys> = RwLock::new(LayoutKeys { v: 9, c: 8 });

fn layout_keys() -> LayoutKeys {
    *LAYOUT_KEYS
        .read()
        .unwrap_or_else(|error| error.into_inner())
}

/// Carbon's `cmdKey >> 8`, UCKeyTranslate's modifier state for ⌘.
const COMMAND_MODIFIER_STATE: u32 = 1;
const KEY_ACTION_DOWN: u16 = 0;
const KEY_TRANSLATE_NO_DEAD_KEYS: u32 = 1;

/// The key that types `letter` in a ⌘ shortcut: on the ⌘ layer first, since
/// layouts like "Dvorak – QWERTY ⌘" switch arrangement while ⌘ is held.
fn find_key(letter: char, translate: impl Fn(u16, u32) -> Option<char>) -> Option<u16> {
    [COMMAND_MODIFIER_STATE, 0].into_iter().find_map(|state| {
        (0..128)
            .find(|&code| translate(code, state).is_some_and(|c| c.eq_ignore_ascii_case(&letter)))
    })
}

/// Reads the active keyboard layout. Text Input Sources must be used on the
/// main thread (recent macOS versions abort otherwise), so the app calls this
/// at launch and when the input source changes.
pub(crate) fn refresh_keyboard_layout() -> Result<()> {
    let lookup = |letter| {
        // Non-Latin layouts type no Latin letters; AppKit falls back to the
        // ASCII-capable layout for shortcuts, and so does this.
        // SAFETY: the copy functions return +1 references, owned by `Owned`.
        let sources = unsafe {
            [
                TISCopyCurrentKeyboardLayoutInputSource(),
                TISCopyCurrentASCIICapableKeyboardLayoutInputSource(),
            ]
        };
        sources
            .into_iter()
            .filter_map(Owned::new)
            .find_map(|source| find_key(letter, |code, state| translate(&source, code, state)))
    };
    let (Some(v), Some(c)) = (lookup('v'), lookup('c')) else {
        return Err(OsError::Platform(
            "the keyboard layout has no V or C key".into(),
        ));
    };
    *LAYOUT_KEYS
        .write()
        .unwrap_or_else(|error| error.into_inner()) = LayoutKeys { v, c };
    Ok(())
}

fn translate(source: &Owned, code: u16, modifier_state: u32) -> Option<char> {
    // SAFETY: `source` is a live input source; the property is CFData
    // holding a UCKeyboardLayout and lives as long as the source.
    unsafe {
        let data = TISGetInputSourceProperty(source.0, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return None;
        }
        let mut dead_keys = 0;
        let mut output = [0_u16; 4];
        let mut length = 0;
        let status = UCKeyTranslate(
            CFDataGetBytePtr(data),
            code,
            KEY_ACTION_DOWN,
            modifier_state,
            u32::from(LMGetKbdType()),
            KEY_TRANSLATE_NO_DEAD_KEYS,
            &raw mut dead_keys,
            output.len(),
            &raw mut length,
            output.as_mut_ptr(),
        );
        if status != 0 || length == 0 {
            return None;
        }
        char::decode_utf16(output[..length.min(output.len())].iter().copied())
            .next()?
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qwerty(code: u16, _state: u32) -> Option<char> {
        match code {
            8 => Some('c'),
            9 => Some('v'),
            47 => Some('.'),
            _ => None,
        }
    }

    /// Dvorak – QWERTY ⌘: Dvorak when typing, QWERTY while ⌘ is held.
    fn dvorak_qwerty_command(code: u16, state: u32) -> Option<char> {
        if state == COMMAND_MODIFIER_STATE {
            return qwerty(code, state);
        }
        match code {
            47 => Some('v'),
            34 => Some('c'),
            _ => None,
        }
    }

    fn dvorak(code: u16, _state: u32) -> Option<char> {
        dvorak_qwerty_command(code, 0)
    }

    #[test]
    fn finds_the_command_layer_key_first() {
        assert_eq!(find_key('v', qwerty), Some(9));
        assert_eq!(find_key('v', dvorak_qwerty_command), Some(9));
        assert_eq!(find_key('v', dvorak), Some(47));
        assert_eq!(find_key('V', dvorak), Some(47));
        assert_eq!(find_key('c', |_, _| None), None);
    }

    #[test]
    fn a_chord_presses_the_modifier_around_the_key() {
        assert_eq!(
            chord((COMMAND_KEY_CODE, FLAG_COMMAND), 9),
            [
                (55, true, FLAG_COMMAND),
                (9, true, FLAG_COMMAND),
                (9, false, FLAG_COMMAND),
                (55, false, 0),
            ]
        );
    }

    #[test]
    fn insertion_without_accessibility_is_a_typed_error() {
        if accessibility_trusted() {
            return;
        }
        let error = insert_text("hello", &InsertOptions::default()).expect_err("no permission");
        assert!(matches!(
            error,
            OsError::PermissionMissing(PermissionKind::Accessibility)
        ));
    }

    /// Writes to the real clipboard and puts the machine's contents back
    /// exactly. No permission needed.
    #[test]
    fn real_clipboard_round_trip_restores_every_representation() {
        let clipboard = MacClipboard;
        let original = clipboard.snapshot().expect("snapshot");
        clipboard
            .write_text("tiny wispr test", true)
            .expect("write");
        let written = clipboard.snapshot().expect("snapshot");
        let types: Vec<&str> = written[0].iter().map(|(kind, _)| kind.as_str()).collect();
        assert!(
            types.contains(&"org.nspasteboard.ConcealedType"),
            "{types:?}"
        );
        assert!(
            types.contains(&"org.nspasteboard.TransientType"),
            "{types:?}"
        );
        assert!(types.contains(&"public.utf8-plain-text"), "{types:?}");
        clipboard.restore(&original).expect("restore");
        assert_eq!(clipboard.snapshot().expect("snapshot"), original);
    }
}
