//! Physical key codes per OS, mapped to W3C `KeyboardEvent.code` names so a
//! binding means the same key whatever the keyboard layout.

use tw_core::hotkey::Modifier;

use super::engine::Side;

/// What a raw key code is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scan {
    Modifier(Modifier, Side),
    Key(&'static str),
}

/// macOS virtual key codes (`kVK_*`) of non-modifier keys. Modifiers arrive
/// as flag changes and are read from the flags instead.
const MACOS: &[(u16, &str)] = &[
    (0, "KeyA"),
    (1, "KeyS"),
    (2, "KeyD"),
    (3, "KeyF"),
    (4, "KeyH"),
    (5, "KeyG"),
    (6, "KeyZ"),
    (7, "KeyX"),
    (8, "KeyC"),
    (9, "KeyV"),
    (10, "IntlBackslash"),
    (11, "KeyB"),
    (12, "KeyQ"),
    (13, "KeyW"),
    (14, "KeyE"),
    (15, "KeyR"),
    (16, "KeyY"),
    (17, "KeyT"),
    (18, "Digit1"),
    (19, "Digit2"),
    (20, "Digit3"),
    (21, "Digit4"),
    (22, "Digit6"),
    (23, "Digit5"),
    (24, "Equal"),
    (25, "Digit9"),
    (26, "Digit7"),
    (27, "Minus"),
    (28, "Digit8"),
    (29, "Digit0"),
    (30, "BracketRight"),
    (31, "KeyO"),
    (32, "KeyU"),
    (33, "BracketLeft"),
    (34, "KeyI"),
    (35, "KeyP"),
    (36, "Enter"),
    (37, "KeyL"),
    (38, "KeyJ"),
    (39, "Quote"),
    (40, "KeyK"),
    (41, "Semicolon"),
    (42, "Backslash"),
    (43, "Comma"),
    (44, "Slash"),
    (45, "KeyN"),
    (46, "KeyM"),
    (47, "Period"),
    (48, "Tab"),
    (49, "Space"),
    (50, "Backquote"),
    (51, "Backspace"),
    (53, "Escape"),
    (57, "CapsLock"),
    (64, "F17"),
    (65, "NumpadDecimal"),
    (67, "NumpadMultiply"),
    (69, "NumpadAdd"),
    (71, "NumLock"),
    (72, "AudioVolumeUp"),
    (73, "AudioVolumeDown"),
    (74, "AudioVolumeMute"),
    (75, "NumpadDivide"),
    (76, "NumpadEnter"),
    (78, "NumpadSubtract"),
    (79, "F18"),
    (80, "F19"),
    (81, "NumpadEqual"),
    (82, "Numpad0"),
    (83, "Numpad1"),
    (84, "Numpad2"),
    (85, "Numpad3"),
    (86, "Numpad4"),
    (87, "Numpad5"),
    (88, "Numpad6"),
    (89, "Numpad7"),
    (90, "F20"),
    (91, "Numpad8"),
    (92, "Numpad9"),
    (93, "IntlYen"),
    (94, "IntlRo"),
    (95, "NumpadComma"),
    (96, "F5"),
    (97, "F6"),
    (98, "F7"),
    (99, "F3"),
    (100, "F8"),
    (101, "F9"),
    (102, "Lang2"),
    (103, "F11"),
    (104, "Lang1"),
    (105, "F13"),
    (106, "F16"),
    (107, "F14"),
    (109, "F10"),
    (110, "ContextMenu"),
    (111, "F12"),
    (113, "F15"),
    (114, "Insert"),
    (115, "Home"),
    (116, "PageUp"),
    (117, "Delete"),
    (118, "F4"),
    (119, "End"),
    (120, "F2"),
    (121, "PageDown"),
    (122, "F1"),
    (123, "ArrowLeft"),
    (124, "ArrowRight"),
    (125, "ArrowDown"),
    (126, "ArrowUp"),
];

/// Windows set-1 scan codes of non-modifier keys, `(scan, extended, code)`.
/// Scan codes follow the physical key; virtual keys follow the layout.
const WINDOWS: &[(u32, bool, &str)] = &[
    (0x01, false, "Escape"),
    (0x02, false, "Digit1"),
    (0x03, false, "Digit2"),
    (0x04, false, "Digit3"),
    (0x05, false, "Digit4"),
    (0x06, false, "Digit5"),
    (0x07, false, "Digit6"),
    (0x08, false, "Digit7"),
    (0x09, false, "Digit8"),
    (0x0A, false, "Digit9"),
    (0x0B, false, "Digit0"),
    (0x0C, false, "Minus"),
    (0x0D, false, "Equal"),
    (0x0E, false, "Backspace"),
    (0x0F, false, "Tab"),
    (0x10, false, "KeyQ"),
    (0x11, false, "KeyW"),
    (0x12, false, "KeyE"),
    (0x13, false, "KeyR"),
    (0x14, false, "KeyT"),
    (0x15, false, "KeyY"),
    (0x16, false, "KeyU"),
    (0x17, false, "KeyI"),
    (0x18, false, "KeyO"),
    (0x19, false, "KeyP"),
    (0x1A, false, "BracketLeft"),
    (0x1B, false, "BracketRight"),
    (0x1C, false, "Enter"),
    (0x1C, true, "NumpadEnter"),
    (0x1E, false, "KeyA"),
    (0x1F, false, "KeyS"),
    (0x20, false, "KeyD"),
    (0x21, false, "KeyF"),
    (0x22, false, "KeyG"),
    (0x23, false, "KeyH"),
    (0x24, false, "KeyJ"),
    (0x25, false, "KeyK"),
    (0x26, false, "KeyL"),
    (0x27, false, "Semicolon"),
    (0x28, false, "Quote"),
    (0x29, false, "Backquote"),
    (0x2B, false, "Backslash"),
    (0x2C, false, "KeyZ"),
    (0x2D, false, "KeyX"),
    (0x2E, false, "KeyC"),
    (0x2F, false, "KeyV"),
    (0x30, false, "KeyB"),
    (0x31, false, "KeyN"),
    (0x32, false, "KeyM"),
    (0x33, false, "Comma"),
    (0x34, false, "Period"),
    (0x35, false, "Slash"),
    (0x35, true, "NumpadDivide"),
    (0x37, false, "NumpadMultiply"),
    (0x37, true, "PrintScreen"),
    (0x39, false, "Space"),
    (0x3A, false, "CapsLock"),
    (0x3B, false, "F1"),
    (0x3C, false, "F2"),
    (0x3D, false, "F3"),
    (0x3E, false, "F4"),
    (0x3F, false, "F5"),
    (0x40, false, "F6"),
    (0x41, false, "F7"),
    (0x42, false, "F8"),
    (0x43, false, "F9"),
    (0x44, false, "F10"),
    (0x45, false, "Pause"),
    (0x45, true, "NumLock"),
    (0x46, false, "ScrollLock"),
    (0x47, false, "Numpad7"),
    (0x47, true, "Home"),
    (0x48, false, "Numpad8"),
    (0x48, true, "ArrowUp"),
    (0x49, false, "Numpad9"),
    (0x49, true, "PageUp"),
    (0x4A, false, "NumpadSubtract"),
    (0x4B, false, "Numpad4"),
    (0x4B, true, "ArrowLeft"),
    (0x4C, false, "Numpad5"),
    (0x4D, false, "Numpad6"),
    (0x4D, true, "ArrowRight"),
    (0x4E, false, "NumpadAdd"),
    (0x4F, false, "Numpad1"),
    (0x4F, true, "End"),
    (0x50, false, "Numpad2"),
    (0x50, true, "ArrowDown"),
    (0x51, false, "Numpad3"),
    (0x51, true, "PageDown"),
    (0x52, false, "Numpad0"),
    (0x52, true, "Insert"),
    (0x53, false, "NumpadDecimal"),
    (0x53, true, "Delete"),
    (0x56, false, "IntlBackslash"),
    (0x57, false, "F11"),
    (0x58, false, "F12"),
    (0x5D, true, "ContextMenu"),
    (0x64, false, "F13"),
    (0x65, false, "F14"),
    (0x66, false, "F15"),
    (0x67, false, "F16"),
    (0x68, false, "F17"),
    (0x69, false, "F18"),
    (0x6A, false, "F19"),
    (0x6B, false, "F20"),
];

/// Linux evdev key codes (an X11 keycode minus 8).
const EVDEV: &[(u32, Scan)] = &[
    (1, Scan::Key("Escape")),
    (2, Scan::Key("Digit1")),
    (3, Scan::Key("Digit2")),
    (4, Scan::Key("Digit3")),
    (5, Scan::Key("Digit4")),
    (6, Scan::Key("Digit5")),
    (7, Scan::Key("Digit6")),
    (8, Scan::Key("Digit7")),
    (9, Scan::Key("Digit8")),
    (10, Scan::Key("Digit9")),
    (11, Scan::Key("Digit0")),
    (12, Scan::Key("Minus")),
    (13, Scan::Key("Equal")),
    (14, Scan::Key("Backspace")),
    (15, Scan::Key("Tab")),
    (16, Scan::Key("KeyQ")),
    (17, Scan::Key("KeyW")),
    (18, Scan::Key("KeyE")),
    (19, Scan::Key("KeyR")),
    (20, Scan::Key("KeyT")),
    (21, Scan::Key("KeyY")),
    (22, Scan::Key("KeyU")),
    (23, Scan::Key("KeyI")),
    (24, Scan::Key("KeyO")),
    (25, Scan::Key("KeyP")),
    (26, Scan::Key("BracketLeft")),
    (27, Scan::Key("BracketRight")),
    (28, Scan::Key("Enter")),
    (29, Scan::Modifier(Modifier::Ctrl, Side::Left)),
    (30, Scan::Key("KeyA")),
    (31, Scan::Key("KeyS")),
    (32, Scan::Key("KeyD")),
    (33, Scan::Key("KeyF")),
    (34, Scan::Key("KeyG")),
    (35, Scan::Key("KeyH")),
    (36, Scan::Key("KeyJ")),
    (37, Scan::Key("KeyK")),
    (38, Scan::Key("KeyL")),
    (39, Scan::Key("Semicolon")),
    (40, Scan::Key("Quote")),
    (41, Scan::Key("Backquote")),
    (42, Scan::Modifier(Modifier::Shift, Side::Left)),
    (43, Scan::Key("Backslash")),
    (44, Scan::Key("KeyZ")),
    (45, Scan::Key("KeyX")),
    (46, Scan::Key("KeyC")),
    (47, Scan::Key("KeyV")),
    (48, Scan::Key("KeyB")),
    (49, Scan::Key("KeyN")),
    (50, Scan::Key("KeyM")),
    (51, Scan::Key("Comma")),
    (52, Scan::Key("Period")),
    (53, Scan::Key("Slash")),
    (54, Scan::Modifier(Modifier::Shift, Side::Right)),
    (55, Scan::Key("NumpadMultiply")),
    (56, Scan::Modifier(Modifier::Alt, Side::Left)),
    (57, Scan::Key("Space")),
    (58, Scan::Key("CapsLock")),
    (59, Scan::Key("F1")),
    (60, Scan::Key("F2")),
    (61, Scan::Key("F3")),
    (62, Scan::Key("F4")),
    (63, Scan::Key("F5")),
    (64, Scan::Key("F6")),
    (65, Scan::Key("F7")),
    (66, Scan::Key("F8")),
    (67, Scan::Key("F9")),
    (68, Scan::Key("F10")),
    (69, Scan::Key("NumLock")),
    (70, Scan::Key("ScrollLock")),
    (71, Scan::Key("Numpad7")),
    (72, Scan::Key("Numpad8")),
    (73, Scan::Key("Numpad9")),
    (74, Scan::Key("NumpadSubtract")),
    (75, Scan::Key("Numpad4")),
    (76, Scan::Key("Numpad5")),
    (77, Scan::Key("Numpad6")),
    (78, Scan::Key("NumpadAdd")),
    (79, Scan::Key("Numpad1")),
    (80, Scan::Key("Numpad2")),
    (81, Scan::Key("Numpad3")),
    (82, Scan::Key("Numpad0")),
    (83, Scan::Key("NumpadDecimal")),
    (86, Scan::Key("IntlBackslash")),
    (87, Scan::Key("F11")),
    (88, Scan::Key("F12")),
    (96, Scan::Key("NumpadEnter")),
    (97, Scan::Modifier(Modifier::Ctrl, Side::Right)),
    (98, Scan::Key("NumpadDivide")),
    (99, Scan::Key("PrintScreen")),
    (100, Scan::Modifier(Modifier::Alt, Side::Right)),
    (102, Scan::Key("Home")),
    (103, Scan::Key("ArrowUp")),
    (104, Scan::Key("PageUp")),
    (105, Scan::Key("ArrowLeft")),
    (106, Scan::Key("ArrowRight")),
    (107, Scan::Key("End")),
    (108, Scan::Key("ArrowDown")),
    (109, Scan::Key("PageDown")),
    (110, Scan::Key("Insert")),
    (111, Scan::Key("Delete")),
    (119, Scan::Key("Pause")),
    (125, Scan::Modifier(Modifier::Meta, Side::Left)),
    (126, Scan::Modifier(Modifier::Meta, Side::Right)),
    (127, Scan::Key("ContextMenu")),
    (183, Scan::Key("F13")),
    (184, Scan::Key("F14")),
    (185, Scan::Key("F15")),
    (186, Scan::Key("F16")),
    (187, Scan::Key("F17")),
    (188, Scan::Key("F18")),
    (189, Scan::Key("F19")),
    (190, Scan::Key("F20")),
];

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_key(code: u16) -> Option<&'static str> {
    MACOS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_code(name: &str) -> Option<u16> {
    MACOS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(code, _)| *code)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn windows_key(scan: u32, extended: bool) -> Option<&'static str> {
    WINDOWS
        .iter()
        .find(|(s, e, _)| *s == scan && *e == extended)
        .or_else(|| WINDOWS.iter().find(|(s, _, _)| *s == scan))
        .map(|(_, _, name)| *name)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn evdev(code: u32) -> Option<Scan> {
    EVDEV
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, scan)| *scan)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn evdev_code(name: &str) -> Option<u32> {
    EVDEV
        .iter()
        .find(|(_, scan)| matches!(scan, Scan::Key(key) if *key == name))
        .map(|(code, _)| *code)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn tables_have_no_duplicates() {
        let mac: HashSet<_> = MACOS.iter().map(|(code, _)| code).collect();
        assert_eq!(mac.len(), MACOS.len());
        let mac_names: HashSet<_> = MACOS.iter().map(|(_, name)| name).collect();
        assert_eq!(mac_names.len(), MACOS.len());
        let windows: HashSet<_> = WINDOWS.iter().map(|(s, e, _)| (s, e)).collect();
        assert_eq!(windows.len(), WINDOWS.len());
        let evdev: HashSet<_> = EVDEV.iter().map(|(code, _)| code).collect();
        assert_eq!(evdev.len(), EVDEV.len());
    }

    #[test]
    fn same_physical_keys_on_every_os() {
        for name in [
            "KeyA", "KeyV", "KeyZ", "Space", "Escape", "Digit1", "F12", "Slash",
        ] {
            assert!(macos_code(name).is_some(), "{name} on macOS");
            assert!(
                WINDOWS.iter().any(|(_, _, n)| *n == name),
                "{name} on Windows"
            );
            assert!(evdev_code(name).is_some(), "{name} on Linux");
        }
    }

    #[test]
    fn macos_lookups() {
        assert_eq!(macos_key(9), Some("KeyV"));
        assert_eq!(macos_key(49), Some("Space"));
        assert_eq!(macos_key(53), Some("Escape"));
        assert_eq!(macos_code("KeyV"), Some(9));
        assert_eq!(macos_key(55), None, "Command is a modifier");
    }

    #[test]
    fn windows_extended_bit_picks_the_navigation_key() {
        assert_eq!(windows_key(0x48, false), Some("Numpad8"));
        assert_eq!(windows_key(0x48, true), Some("ArrowUp"));
        assert_eq!(windows_key(0x1C, true), Some("NumpadEnter"));
        assert_eq!(windows_key(0x2F, false), Some("KeyV"));
        assert_eq!(windows_key(0x2F, true), Some("KeyV"));
    }

    #[test]
    fn evdev_lookups() {
        assert_eq!(evdev(47), Some(Scan::Key("KeyV")));
        assert_eq!(evdev(125), Some(Scan::Modifier(Modifier::Meta, Side::Left)));
        assert_eq!(evdev(100), Some(Scan::Modifier(Modifier::Alt, Side::Right)));
        assert_eq!(evdev_code("Space"), Some(57));
    }
}
