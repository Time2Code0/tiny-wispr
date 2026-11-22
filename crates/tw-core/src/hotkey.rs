//! Hotkey model: what a shortcut is, how it is shown, its platform defaults,
//! the validation rules from spec §2.6, and the [`Matcher`] that turns the
//! global key stream into binding presses and releases.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Maximum number of keys (modifiers + key) in one shortcut.
pub const MAX_KEYS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

impl Platform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// A modifier key. `Alt` is Option on macOS; `Meta` is Command on macOS, the
/// Windows key on Windows and Super on Linux. `Fn` only exists on macOS.
///
/// The declaration order is the display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Fn,
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    pub fn label(self, platform: Platform) -> &'static str {
        match (platform, self) {
            (Platform::Macos, Self::Fn) => "fn",
            (Platform::Macos, Self::Ctrl) => "⌃",
            (Platform::Macos, Self::Alt) => "⌥",
            (Platform::Macos, Self::Shift) => "⇧",
            (Platform::Macos, Self::Meta) => "⌘",
            (_, Self::Fn) => "Fn",
            (_, Self::Ctrl) => "Ctrl",
            (_, Self::Alt) => "Alt",
            (_, Self::Shift) => "Shift",
            (Platform::Windows, Self::Meta) => "Win",
            (Platform::Linux, Self::Meta) => "Super",
        }
    }
}

/// A non-modifier key, named by its W3C `KeyboardEvent.code` (`"Space"`,
/// `"KeyV"`, `"Escape"`, `"CapsLock"`, `"F5"`). Physical, layout-independent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyCode(pub String);

impl KeyCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn label(&self, platform: Platform) -> String {
        let code = self.0.as_str();
        if let Some(letter) = code.strip_prefix("Key") {
            return letter.to_owned();
        }
        if let Some(digit) = code.strip_prefix("Digit") {
            return digit.to_owned();
        }
        let mac = platform == Platform::Macos;
        let label = match code {
            "Escape" if mac => "esc",
            "Escape" => "Esc",
            "Enter" if mac => "↩",
            "Backspace" if mac => "⌫",
            "Tab" if mac => "⇥",
            "CapsLock" if mac => "⇪",
            "CapsLock" => "Caps Lock",
            "ArrowUp" => "↑",
            "ArrowDown" => "↓",
            "ArrowLeft" => "←",
            "ArrowRight" => "→",
            "Backquote" => "`",
            "Minus" => "-",
            "Equal" => "=",
            "BracketLeft" => "[",
            "BracketRight" => "]",
            "Backslash" => "\\",
            "Semicolon" => ";",
            "Quote" => "'",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            other => other,
        };
        label.to_owned()
    }
}

impl From<&str> for KeyCode {
    fn from(code: &str) -> Self {
        Self::new(code)
    }
}

/// A shortcut: a set of modifiers plus at most one other key.
/// Modifier-only shortcuts (`fn`, `Ctrl+Win`) have `key: None`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    pub modifiers: BTreeSet<Modifier>,
    pub key: Option<KeyCode>,
}

impl Hotkey {
    pub fn new(modifiers: impl IntoIterator<Item = Modifier>, key: Option<KeyCode>) -> Self {
        Self {
            modifiers: modifiers.into_iter().collect(),
            key,
        }
    }

    pub fn modifiers(modifiers: impl IntoIterator<Item = Modifier>) -> Self {
        Self::new(modifiers, None)
    }

    pub fn with_key(modifiers: impl IntoIterator<Item = Modifier>, key: &str) -> Self {
        Self::new(modifiers, Some(KeyCode::from(key)))
    }

    pub fn key_count(&self) -> usize {
        self.modifiers.len() + usize::from(self.key.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.key_count() == 0
    }

    /// The labels of each key, in display order.
    pub fn parts(&self, platform: Platform) -> Vec<String> {
        self.modifiers
            .iter()
            .map(|m| m.label(platform).to_owned())
            .chain(self.key.iter().map(|k| k.label(platform)))
            .collect()
    }

    /// `"fn"`, `"⌃⌘V"`, `"fn Space"` on macOS; `"Ctrl+Win"`, `"Alt+Shift+Z"` elsewhere.
    pub fn display(&self, platform: Platform) -> String {
        let parts = self.parts(platform);
        if platform != Platform::Macos {
            return parts.join("+");
        }
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                let prev_wide = parts[i - 1].chars().count() > 1;
                let this_wide = part.chars().count() > 1;
                if prev_wide || this_wide {
                    out.push(' ');
                }
            }
            out.push_str(part);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyAction {
    PushToTalk,
    HandsFree,
    Cancel,
    PasteLast,
    CommandMode,
}

impl HotkeyAction {
    pub const ALL: [Self; 5] = [
        Self::PushToTalk,
        Self::HandsFree,
        Self::Cancel,
        Self::PasteLast,
        Self::CommandMode,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::PushToTalk => "Push to talk",
            Self::HandsFree => "Hands-free",
            Self::Cancel => "Cancel",
            Self::PasteLast => "Paste last transcript",
            Self::CommandMode => "Command mode",
        }
    }
}

/// One shortcut per action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcuts {
    pub push_to_talk: Hotkey,
    pub hands_free: Hotkey,
    pub cancel: Hotkey,
    pub paste_last: Hotkey,
    pub command_mode: Hotkey,
}

impl Shortcuts {
    /// Platform defaults from spec §2.1.
    pub fn defaults(platform: Platform) -> Self {
        use Modifier::{Alt, Ctrl, Fn, Meta, Shift};
        match platform {
            Platform::Macos => Self {
                push_to_talk: Hotkey::modifiers([Fn]),
                hands_free: Hotkey::with_key([Fn], "Space"),
                cancel: Hotkey::with_key([], "Escape"),
                paste_last: Hotkey::with_key([Ctrl, Meta], "KeyV"),
                command_mode: Hotkey::modifiers([Fn, Ctrl]),
            },
            Platform::Windows | Platform::Linux => Self {
                push_to_talk: Hotkey::modifiers([Ctrl, Meta]),
                hands_free: Hotkey::with_key([Ctrl, Meta], "Space"),
                cancel: Hotkey::with_key([], "Escape"),
                paste_last: Hotkey::with_key([Alt, Shift], "KeyZ"),
                command_mode: Hotkey::modifiers([Ctrl, Meta, Alt]),
            },
        }
    }

    /// Push to talk on Macs whose keyboard has no Fn key.
    pub fn macos_push_to_talk_without_fn() -> Hotkey {
        Hotkey::modifiers([Modifier::Ctrl, Modifier::Alt])
    }

    pub fn get(&self, action: HotkeyAction) -> &Hotkey {
        match action {
            HotkeyAction::PushToTalk => &self.push_to_talk,
            HotkeyAction::HandsFree => &self.hands_free,
            HotkeyAction::Cancel => &self.cancel,
            HotkeyAction::PasteLast => &self.paste_last,
            HotkeyAction::CommandMode => &self.command_mode,
        }
    }

    pub fn set(&mut self, action: HotkeyAction, hotkey: Hotkey) {
        let slot = match action {
            HotkeyAction::PushToTalk => &mut self.push_to_talk,
            HotkeyAction::HandsFree => &mut self.hands_free,
            HotkeyAction::Cancel => &mut self.cancel,
            HotkeyAction::PasteLast => &mut self.paste_last,
            HotkeyAction::CommandMode => &mut self.command_mode,
        };
        *slot = hotkey;
    }

    pub fn iter(&self) -> impl Iterator<Item = (HotkeyAction, &Hotkey)> {
        HotkeyAction::ALL.into_iter().map(|a| (a, self.get(a)))
    }
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self::defaults(Platform::current())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyIssueKind {
    Empty,
    TooManyKeys,
    NeedsModifier,
    EscapeOnlyForCancel,
    Reserved,
    Duplicate,
}

/// Why a shortcut was rejected, with the inline message to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct HotkeyIssue {
    pub kind: HotkeyIssueKind,
    pub message: String,
    pub conflicts_with: Option<HotkeyAction>,
}

impl HotkeyIssue {
    pub fn new(kind: HotkeyIssueKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            conflicts_with: None,
        }
    }
}

/// Checks `hotkey` for `action` against the rules in spec §2.6 and the other
/// bindings in `current`.
///
/// Bindings that are prefixes of each other are fine (`fn`, `fn Space` and
/// `fn ⌃` are the macOS defaults): [`Matcher`] only fires a binding when
/// exactly its keys are held. Only identical bindings conflict.
pub fn validate(
    action: HotkeyAction,
    hotkey: &Hotkey,
    platform: Platform,
    current: &Shortcuts,
) -> Result<(), HotkeyIssue> {
    if hotkey.is_empty() {
        return Err(HotkeyIssue::new(HotkeyIssueKind::Empty, "Press a shortcut"));
    }
    if hotkey.key_count() > MAX_KEYS {
        return Err(HotkeyIssue::new(
            HotkeyIssueKind::TooManyKeys,
            "Use up to 3 keys",
        ));
    }
    if platform != Platform::Macos && hotkey.modifiers.contains(&Modifier::Fn) {
        return Err(HotkeyIssue::new(
            HotkeyIssueKind::Reserved,
            format!("Fn can't be used on {}", platform_name(platform)),
        ));
    }
    if is_reserved(hotkey, platform) {
        return Err(HotkeyIssue::new(
            HotkeyIssueKind::Reserved,
            format!("That shortcut is reserved by {}", platform_name(platform)),
        ));
    }
    if hotkey.modifiers.is_empty() {
        check_bare_key(action, hotkey.key.as_ref(), platform)?;
    }
    if let Some((other, _)) = current
        .iter()
        .find(|(other, bound)| *other != action && *bound == hotkey)
    {
        return Err(HotkeyIssue {
            kind: HotkeyIssueKind::Duplicate,
            message: format!("Already used for {}", other.label()),
            conflicts_with: Some(other),
        });
    }
    Ok(())
}

/// A shortcut without modifiers is only allowed for Caps Lock (push to talk,
/// hands-free), Escape (cancel) and function keys.
fn check_bare_key(
    action: HotkeyAction,
    key: Option<&KeyCode>,
    platform: Platform,
) -> Result<(), HotkeyIssue> {
    let code = key.map_or("", KeyCode::as_str);
    match code {
        "Escape" if action == HotkeyAction::Cancel => Ok(()),
        "Escape" => Err(HotkeyIssue::new(
            HotkeyIssueKind::EscapeOnlyForCancel,
            "Escape alone only works for Cancel",
        )),
        "CapsLock" if matches!(action, HotkeyAction::PushToTalk | HotkeyAction::HandsFree) => {
            Ok(())
        }
        "CapsLock" => Err(HotkeyIssue::new(
            HotkeyIssueKind::NeedsModifier,
            "Caps Lock alone only works for push to talk and hands-free",
        )),
        _ if is_function_key(code) => Ok(()),
        _ => Err(HotkeyIssue::new(
            HotkeyIssueKind::NeedsModifier,
            match platform {
                Platform::Macos => "Add a modifier key such as Ctrl or Fn",
                Platform::Windows | Platform::Linux => "Add a modifier key such as Ctrl or Alt",
            },
        )),
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Macos => "macOS",
        Platform::Windows => "Windows",
        Platform::Linux => "Linux",
    }
}

fn is_function_key(code: &str) -> bool {
    code.strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n))
}

/// Combinations the OS or every common app already owns. Each group is a
/// modifier set and the keys that are reserved with exactly those modifiers.
struct ReservedTable {
    combos: &'static [(&'static [Modifier], &'static [&'static str])],
    modifier_only: &'static [&'static [Modifier]],
}

fn reserved_table(platform: Platform) -> ReservedTable {
    use Modifier::{Alt, Ctrl, Fn, Meta, Shift};
    const ARROWS: &[&str] = &["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];
    match platform {
        Platform::Macos => ReservedTable {
            combos: &[
                (
                    &[Meta],
                    &[
                        "KeyA",
                        "KeyC",
                        "KeyH",
                        "KeyM",
                        "KeyQ",
                        "KeyS",
                        "KeyV",
                        "KeyW",
                        "KeyX",
                        "KeyZ",
                        "Tab",
                        "Space",
                        "Comma",
                        "Backquote",
                    ],
                ),
                (
                    &[Meta, Shift],
                    &["KeyZ", "KeyQ", "Digit3", "Digit4", "Digit5"],
                ),
                (&[Meta, Alt], &["Escape", "KeyD", "KeyH"]),
                (&[Ctrl, Meta], &["KeyQ", "KeyF", "Space"]),
                (&[Ctrl], ARROWS),
                (&[Ctrl], &["Space"]),
                (&[Fn], ARROWS),
                (
                    &[Fn],
                    &["KeyA", "KeyC", "KeyE", "KeyF", "KeyN", "KeyQ", "Backspace"],
                ),
                (&[], &["F11"]),
            ],
            modifier_only: &[],
        },
        Platform::Windows => ReservedTable {
            combos: &[
                (
                    &[Ctrl],
                    &[
                        "KeyA", "KeyC", "KeyS", "KeyV", "KeyX", "KeyY", "KeyZ", "Escape",
                    ],
                ),
                (&[Alt], &["Tab", "F4", "Escape", "Space", "PrintScreen"]),
                (&[Ctrl, Alt], &["Delete"]),
                (&[Ctrl, Shift], &["Escape"]),
                (
                    &[Meta],
                    &[
                        "KeyA",
                        "KeyD",
                        "KeyE",
                        "KeyG",
                        "KeyH",
                        "KeyI",
                        "KeyK",
                        "KeyL",
                        "KeyM",
                        "KeyP",
                        "KeyR",
                        "KeyS",
                        "KeyV",
                        "KeyX",
                        "Tab",
                        "Space",
                        "Period",
                        "PrintScreen",
                    ],
                ),
                (&[Meta], ARROWS),
                (&[Meta, Shift], &["KeyS"]),
                (&[Ctrl, Meta], &["ArrowLeft", "ArrowRight", "KeyD", "F4"]),
                (&[], &["PrintScreen", "F5", "F11"]),
            ],
            modifier_only: &[&[Meta]],
        },
        Platform::Linux => ReservedTable {
            combos: &[
                (&[Ctrl], &["KeyA", "KeyC", "KeyS", "KeyV", "KeyX", "KeyZ"]),
                (&[Alt], &["Tab", "F2", "F4", "PrintScreen"]),
                (&[Shift], &["PrintScreen"]),
                (&[Ctrl, Alt], &["Delete", "KeyT", "KeyL", "Backspace"]),
                (&[Ctrl, Alt], ARROWS),
                (
                    &[Ctrl, Alt],
                    &[
                        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
                    ],
                ),
                (&[Meta], &["KeyA", "KeyD", "KeyL", "KeyV", "Tab", "Space"]),
                (&[Meta], ARROWS),
                (&[], &["PrintScreen", "F11"]),
            ],
            modifier_only: &[&[Meta]],
        },
    }
}

fn same_modifiers(set: &BTreeSet<Modifier>, list: &[Modifier]) -> bool {
    set.len() == list.len() && list.iter().all(|m| set.contains(m))
}

fn is_reserved(hotkey: &Hotkey, platform: Platform) -> bool {
    let table = reserved_table(platform);
    match &hotkey.key {
        Some(key) => table.combos.iter().any(|(modifiers, keys)| {
            same_modifiers(&hotkey.modifiers, modifiers) && keys.contains(&key.as_str())
        }),
        None => table
            .modifier_only
            .iter()
            .any(|modifiers| same_modifiers(&hotkey.modifiers, modifiers)),
    }
}

/// Bindings whose completing key-down (and the matching key-up) the OS
/// listener should swallow, so the Space of `fn Space` or the V of `⌃⌘V` never
/// reaches the focused app. Cancel is only swallowed while dictating, so
/// Escape keeps working everywhere else.
///
/// Modifier-only bindings are never swallowed: dropping a modifier change
/// would leave other apps with the wrong idea of which modifiers are held.
pub fn swallow_set(bindings: &Shortcuts, dictating: bool) -> Vec<Hotkey> {
    bindings
        .iter()
        .filter(|(action, hotkey)| {
            hotkey.key.is_some() && (dictating || *action != HotkeyAction::Cancel)
        })
        .map(|(_, hotkey)| hotkey.clone())
        .collect()
}

/// One physical key: a modifier or anything else.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Modifier(Modifier),
    Code(KeyCode),
}

/// The keys held at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeySet {
    pub modifiers: BTreeSet<Modifier>,
    pub keys: BTreeSet<KeyCode>,
}

impl KeySet {
    pub fn new(
        modifiers: impl IntoIterator<Item = Modifier>,
        keys: impl IntoIterator<Item = KeyCode>,
    ) -> Self {
        Self {
            modifiers: modifiers.into_iter().collect(),
            keys: keys.into_iter().collect(),
        }
    }

    pub fn contains(&self, key: &Key) -> bool {
        match key {
            Key::Modifier(m) => self.modifiers.contains(m),
            Key::Code(code) => self.keys.contains(code),
        }
    }

    fn insert(&mut self, key: Key) {
        match key {
            Key::Modifier(m) => {
                self.modifiers.insert(m);
            }
            Key::Code(code) => {
                self.keys.insert(code);
            }
        }
    }

    fn remove(&mut self, key: &Key) {
        match key {
            Key::Modifier(m) => {
                self.modifiers.remove(m);
            }
            Key::Code(code) => {
                self.keys.remove(code);
            }
        }
    }

    /// True when exactly the keys of `hotkey` are held, nothing more.
    pub fn is_exactly(&self, hotkey: &Hotkey) -> bool {
        !hotkey.is_empty()
            && self.modifiers == hotkey.modifiers
            && match &hotkey.key {
                Some(key) => self.keys.len() == 1 && self.keys.contains(key),
                None => self.keys.is_empty(),
            }
    }

    /// True when every key of `hotkey` is held, possibly among others.
    fn holds(&self, hotkey: &Hotkey) -> bool {
        hotkey.modifiers.is_subset(&self.modifiers)
            && hotkey
                .key
                .as_ref()
                .is_none_or(|key| self.keys.contains(key))
    }

    fn without(&self, hotkey: &Hotkey) -> Self {
        let mut rest = self.clone();
        for m in &hotkey.modifiers {
            rest.modifiers.remove(m);
        }
        if let Some(key) = &hotkey.key {
            rest.keys.remove(key);
        }
        rest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyTransition {
    Down,
    Up,
}

/// One raw key event from the OS listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    pub transition: KeyTransition,
    pub key: Key,
    /// Every key the OS reports as held after this transition. It is trusted
    /// over the matcher's own bookkeeping, which is how a missed key-up heals.
    pub pressed: KeySet,
}

/// What the matcher makes of the key stream. Feed these to the dictation
/// machine (`Event::from`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeySignal {
    Pressed(HotkeyAction),
    Released(HotkeyAction),
    /// A key went down while a binding was held and it completes no other
    /// binding: the user is typing some other shortcut.
    ForeignKey,
}

/// Turns raw key events into binding presses and releases.
///
/// A binding is pressed on the key-down that makes exactly its keys held, and
/// released as soon as any of its keys is up. Releasing keys never presses a
/// binding, so letting go of Space in `fn Space` does not start push to talk.
///
/// Hand-over: when a key-down turns one held binding into another (`fn`, then
/// Space makes `fn Space`), the new binding's `Pressed` comes first and the old
/// binding's `Released` second. The machine sees hands-free start while push
/// to talk is still down, so the release cannot be taken for a short tap.
///
/// A key-down that is no binding on its own but is one once the held binding's
/// keys are set aside (Escape while holding `fn`) presses that binding too,
/// which is how Cancel works in the middle of a hold.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    pressed: KeySet,
    held: Vec<(HotkeyAction, Hotkey)>,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, input: &KeyInput, bindings: &Shortcuts) -> Vec<HotkeySignal> {
        let repeat = input.transition == KeyTransition::Down && self.pressed.contains(&input.key);
        let mut pressed = input.pressed.clone();
        match input.transition {
            KeyTransition::Down => pressed.insert(input.key.clone()),
            KeyTransition::Up => pressed.remove(&input.key),
        }
        let mut signals = self.sync(pressed);
        if input.transition == KeyTransition::Down && !repeat {
            self.activate(bindings, &mut signals);
        }
        signals
    }

    /// Adopts the held keys as the OS reports them (for example from a
    /// periodic modifier-flags poll) and releases any binding that is no
    /// longer held. Never presses a binding.
    pub fn sync(&mut self, pressed: KeySet) -> Vec<HotkeySignal> {
        self.pressed = pressed;
        let mut signals = Vec::new();
        let pressed = &self.pressed;
        self.held.retain(|(action, hotkey)| {
            let still_held = pressed.holds(hotkey);
            if !still_held {
                signals.push(HotkeySignal::Released(*action));
            }
            still_held
        });
        signals
    }

    fn activate(&mut self, bindings: &Shortcuts, signals: &mut Vec<HotkeySignal>) {
        if let Some(action) = self.unheld_match(bindings, &self.pressed) {
            signals.push(HotkeySignal::Pressed(action));
            signals.extend(
                self.held
                    .drain(..)
                    .map(|(old, _)| HotkeySignal::Released(old)),
            );
            self.held.push((action, bindings.get(action).clone()));
            return;
        }
        if self.held.is_empty() {
            return;
        }
        let rest = self
            .held
            .iter()
            .fold(self.pressed.clone(), |rest, (_, hotkey)| {
                rest.without(hotkey)
            });
        match self.unheld_match(bindings, &rest) {
            Some(action) => {
                signals.push(HotkeySignal::Pressed(action));
                self.held.push((action, bindings.get(action).clone()));
            }
            None => signals.push(HotkeySignal::ForeignKey),
        }
    }

    fn unheld_match(&self, bindings: &Shortcuts, keys: &KeySet) -> Option<HotkeyAction> {
        bindings
            .iter()
            .find(|(action, hotkey)| {
                keys.is_exactly(hotkey) && !self.held.iter().any(|(held, _)| held == action)
            })
            .map(|(action, _)| action)
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(Platform::current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Modifier::{Alt, Ctrl, Fn, Meta, Shift};

    #[test]
    fn mac_display() {
        let p = Platform::Macos;
        assert_eq!(Hotkey::modifiers([Fn]).display(p), "fn");
        assert_eq!(Hotkey::with_key([Fn], "Space").display(p), "fn Space");
        assert_eq!(Hotkey::with_key([Meta, Ctrl], "KeyV").display(p), "⌃⌘V");
        assert_eq!(Hotkey::modifiers([Alt, Ctrl]).display(p), "⌃⌥");
        assert_eq!(Hotkey::with_key([], "Escape").display(p), "esc");
    }

    #[test]
    fn windows_and_linux_display() {
        assert_eq!(
            Hotkey::modifiers([Meta, Ctrl]).display(Platform::Windows),
            "Ctrl+Win"
        );
        assert_eq!(
            Hotkey::with_key([Shift, Alt], "KeyZ").display(Platform::Linux),
            "Alt+Shift+Z"
        );
        assert_eq!(
            Hotkey::with_key([Ctrl, Meta], "Space").display(Platform::Linux),
            "Ctrl+Super+Space"
        );
    }

    #[test]
    fn defaults_match_spec() {
        let mac = Shortcuts::defaults(Platform::Macos);
        assert_eq!(mac.push_to_talk, Hotkey::modifiers([Fn]));
        assert_eq!(mac.paste_last.display(Platform::Macos), "⌃⌘V");
        let win = Shortcuts::defaults(Platform::Windows);
        assert_eq!(win.push_to_talk.display(Platform::Windows), "Ctrl+Win");
        assert_eq!(win.hands_free.display(Platform::Windows), "Ctrl+Win+Space");
        assert_eq!(win.paste_last.display(Platform::Windows), "Alt+Shift+Z");
        for (_, hotkey) in mac.iter().chain(win.iter()) {
            assert!(hotkey.key_count() <= MAX_KEYS);
        }
    }

    #[test]
    fn serde_shape() {
        let json = serde_json::to_value(Hotkey::with_key([Ctrl, Meta], "KeyV")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "modifiers": ["ctrl", "meta"], "key": "KeyV" })
        );
        let back: Hotkey = serde_json::from_value(json).unwrap();
        assert_eq!(back, Hotkey::with_key([Meta, Ctrl], "KeyV"));
    }

    #[test]
    fn rejects_four_keys() {
        let hk = Hotkey::with_key([Ctrl, Alt, Shift], "KeyA");
        let err = validate(
            HotkeyAction::PushToTalk,
            &hk,
            Platform::Macos,
            &Shortcuts::defaults(Platform::Macos),
        )
        .unwrap_err();
        assert_eq!(err.kind, HotkeyIssueKind::TooManyKeys);
    }

    const PLATFORMS: [Platform; 3] = [Platform::Macos, Platform::Windows, Platform::Linux];

    fn check(action: HotkeyAction, hotkey: &Hotkey, platform: Platform) -> Result<(), HotkeyIssue> {
        validate(action, hotkey, platform, &Shortcuts::defaults(platform))
    }

    #[test]
    fn defaults_are_valid_on_every_platform() {
        for platform in PLATFORMS {
            let defaults = Shortcuts::defaults(platform);
            for (action, hotkey) in defaults.iter() {
                assert_eq!(
                    validate(action, hotkey, platform, &defaults),
                    Ok(()),
                    "{action:?} on {platform:?}"
                );
            }
        }
        let mac = Shortcuts::defaults(Platform::Macos);
        let without_fn = Shortcuts::macos_push_to_talk_without_fn();
        let mut fallback = mac.clone();
        fallback.push_to_talk = without_fn.clone();
        fallback.hands_free = Hotkey::with_key([Ctrl, Alt], "Space");
        fallback.command_mode = Hotkey::modifiers([Ctrl, Alt, Shift]);
        for (action, hotkey) in fallback.iter() {
            assert_eq!(validate(action, hotkey, Platform::Macos, &fallback), Ok(()));
        }
    }

    #[test]
    fn rejects_empty() {
        let err = check(
            HotkeyAction::PushToTalk,
            &Hotkey::default(),
            Platform::Macos,
        );
        assert_eq!(err.unwrap_err().kind, HotkeyIssueKind::Empty);
    }

    #[test]
    fn bare_keys_need_a_modifier() {
        for code in ["KeyA", "Digit1", "Comma", "Space", "Enter"] {
            let err = check(
                HotkeyAction::PushToTalk,
                &Hotkey::with_key([], code),
                Platform::Macos,
            )
            .unwrap_err();
            assert_eq!(err.kind, HotkeyIssueKind::NeedsModifier, "{code}");
            assert_eq!(err.message, "Add a modifier key such as Ctrl or Fn");
        }
        let err = check(
            HotkeyAction::HandsFree,
            &Hotkey::with_key([], "KeyA"),
            Platform::Windows,
        )
        .unwrap_err();
        assert_eq!(err.message, "Add a modifier key such as Ctrl or Alt");
        assert_eq!(
            check(
                HotkeyAction::PasteLast,
                &Hotkey::with_key([], "F13"),
                Platform::Macos
            ),
            Ok(())
        );
    }

    #[test]
    fn modifier_only_is_allowed() {
        let hk = Hotkey::modifiers([Ctrl, Shift]);
        assert_eq!(
            check(HotkeyAction::PushToTalk, &hk, Platform::Macos),
            Ok(())
        );
        assert_eq!(
            check(HotkeyAction::PushToTalk, &hk, Platform::Linux),
            Ok(())
        );
    }

    #[test]
    fn caps_lock_alone_only_for_dictation() {
        let caps = Hotkey::with_key([], "CapsLock");
        for action in [HotkeyAction::PushToTalk, HotkeyAction::HandsFree] {
            assert_eq!(check(action, &caps, Platform::Macos), Ok(()));
        }
        for action in [
            HotkeyAction::Cancel,
            HotkeyAction::PasteLast,
            HotkeyAction::CommandMode,
        ] {
            let err = check(action, &caps, Platform::Macos).unwrap_err();
            assert_eq!(err.kind, HotkeyIssueKind::NeedsModifier);
            assert_eq!(
                err.message,
                "Caps Lock alone only works for push to talk and hands-free"
            );
        }
    }

    #[test]
    fn escape_alone_only_for_cancel() {
        let esc = Hotkey::with_key([], "Escape");
        let mut shortcuts = Shortcuts::defaults(Platform::Macos);
        shortcuts.cancel = Hotkey::with_key([Ctrl], "KeyQ");
        let err =
            validate(HotkeyAction::PushToTalk, &esc, Platform::Macos, &shortcuts).unwrap_err();
        assert_eq!(err.kind, HotkeyIssueKind::EscapeOnlyForCancel);
        assert_eq!(err.message, "Escape alone only works for Cancel");
        assert_eq!(
            validate(HotkeyAction::Cancel, &esc, Platform::Macos, &shortcuts),
            Ok(())
        );
    }

    #[test]
    fn rejects_reserved_combinations() {
        let cases = [
            (Platform::Macos, Hotkey::with_key([Meta], "KeyC")),
            (Platform::Macos, Hotkey::with_key([Meta], "Space")),
            (Platform::Macos, Hotkey::with_key([Meta, Shift], "Digit4")),
            (Platform::Macos, Hotkey::with_key([Ctrl], "ArrowLeft")),
            (Platform::Windows, Hotkey::with_key([Alt], "Tab")),
            (Platform::Windows, Hotkey::with_key([Alt], "F4")),
            (Platform::Windows, Hotkey::with_key([Meta], "KeyL")),
            (Platform::Windows, Hotkey::with_key([Ctrl, Alt], "Delete")),
            (Platform::Windows, Hotkey::with_key([], "PrintScreen")),
            (Platform::Windows, Hotkey::with_key([], "F5")),
            (Platform::Windows, Hotkey::modifiers([Meta])),
            (Platform::Linux, Hotkey::with_key([Ctrl, Alt], "KeyT")),
            (Platform::Linux, Hotkey::with_key([Meta], "KeyL")),
        ];
        for (platform, hotkey) in cases {
            let err = check(HotkeyAction::PasteLast, &hotkey, platform).unwrap_err();
            assert_eq!(
                err.kind,
                HotkeyIssueKind::Reserved,
                "{hotkey:?} on {platform:?}"
            );
        }
        let err = check(
            HotkeyAction::PasteLast,
            &Hotkey::with_key([Meta], "KeyV"),
            Platform::Macos,
        )
        .unwrap_err();
        assert_eq!(err.message, "That shortcut is reserved by macOS");
        assert_eq!(
            check(
                HotkeyAction::PasteLast,
                &Hotkey::with_key([Meta], "KeyC"),
                Platform::Windows
            ),
            Ok(()),
            "Win+C is not reserved"
        );
    }

    #[test]
    fn fn_only_on_macos() {
        let err = check(
            HotkeyAction::PushToTalk,
            &Hotkey::modifiers([Fn]),
            Platform::Windows,
        )
        .unwrap_err();
        assert_eq!(err.message, "Fn can't be used on Windows");
    }

    #[test]
    fn rejects_duplicates_but_not_prefixes() {
        let mac = Shortcuts::defaults(Platform::Macos);
        let err = validate(
            HotkeyAction::PasteLast,
            &Hotkey::with_key([Fn], "Space"),
            Platform::Macos,
            &mac,
        )
        .unwrap_err();
        assert_eq!(err.kind, HotkeyIssueKind::Duplicate);
        assert_eq!(err.message, "Already used for Hands-free");
        assert_eq!(err.conflicts_with, Some(HotkeyAction::HandsFree));

        assert_eq!(
            validate(
                HotkeyAction::PushToTalk,
                &Hotkey::modifiers([Fn]),
                Platform::Macos,
                &mac
            ),
            Ok(()),
            "re-saving an action's own binding is not a duplicate"
        );
        assert_eq!(
            validate(
                HotkeyAction::PasteLast,
                &Hotkey::with_key([Fn], "KeyV"),
                Platform::Macos,
                &mac
            ),
            Ok(()),
            "fn V shares a prefix with fn"
        );
    }

    #[test]
    fn swallows_keyed_bindings_and_escape_only_while_dictating() {
        let mac = Shortcuts::defaults(Platform::Macos);
        let idle = swallow_set(&mac, false);
        assert_eq!(
            idle,
            vec![
                Hotkey::with_key([Fn], "Space"),
                Hotkey::with_key([Ctrl, Meta], "KeyV"),
            ]
        );
        let dictating = swallow_set(&mac, true);
        assert!(dictating.contains(&Hotkey::with_key([], "Escape")));
        assert!(!dictating.contains(&Hotkey::modifiers([Fn])));
    }

    mod matcher {
        use super::*;
        use HotkeySignal::{ForeignKey, Pressed, Released};

        struct Keys {
            matcher: Matcher,
            pressed: KeySet,
            bindings: Shortcuts,
        }

        impl Keys {
            fn new(platform: Platform) -> Self {
                Self {
                    matcher: Matcher::new(),
                    pressed: KeySet::default(),
                    bindings: Shortcuts::defaults(platform),
                }
            }

            fn key(name: &str) -> Key {
                match name {
                    "fn" => Key::Modifier(Fn),
                    "ctrl" => Key::Modifier(Ctrl),
                    "alt" => Key::Modifier(Alt),
                    "shift" => Key::Modifier(Shift),
                    "meta" => Key::Modifier(Meta),
                    code => Key::Code(KeyCode::from(code)),
                }
            }

            fn send(&mut self, transition: KeyTransition, name: &str) -> Vec<HotkeySignal> {
                let key = Self::key(name);
                match transition {
                    KeyTransition::Down => self.pressed.insert(key.clone()),
                    KeyTransition::Up => self.pressed.remove(&key),
                }
                let input = KeyInput {
                    transition,
                    key,
                    pressed: self.pressed.clone(),
                };
                self.matcher.handle(&input, &self.bindings)
            }

            fn down(&mut self, name: &str) -> Vec<HotkeySignal> {
                self.send(KeyTransition::Down, name)
            }

            fn up(&mut self, name: &str) -> Vec<HotkeySignal> {
                self.send(KeyTransition::Up, name)
            }
        }

        #[test]
        fn press_and_release() {
            let mut k = Keys::new(Platform::Macos);
            assert_eq!(k.down("fn"), [Pressed(HotkeyAction::PushToTalk)]);
            assert_eq!(k.up("fn"), [Released(HotkeyAction::PushToTalk)]);
        }

        #[test]
        fn space_hands_over_from_push_to_talk_to_hands_free() {
            let mut k = Keys::new(Platform::Macos);
            assert_eq!(k.down("fn"), [Pressed(HotkeyAction::PushToTalk)]);
            assert_eq!(
                k.down("Space"),
                [
                    Pressed(HotkeyAction::HandsFree),
                    Released(HotkeyAction::PushToTalk)
                ]
            );
            assert_eq!(k.up("Space"), [Released(HotkeyAction::HandsFree)]);
            assert_eq!(k.up("fn"), [], "fn alone after Space is not a new press");
            assert_eq!(k.down("fn"), [Pressed(HotkeyAction::PushToTalk)]);
        }

        #[test]
        fn modifier_order_does_not_matter() {
            let mut k = Keys::new(Platform::Windows);
            assert_eq!(k.down("meta"), []);
            assert_eq!(k.down("ctrl"), [Pressed(HotkeyAction::PushToTalk)]);
            assert_eq!(
                k.down("alt"),
                [
                    Pressed(HotkeyAction::CommandMode),
                    Released(HotkeyAction::PushToTalk)
                ]
            );
            assert_eq!(k.up("ctrl"), [Released(HotkeyAction::CommandMode)]);
        }

        #[test]
        fn needs_exactly_the_bound_keys() {
            let mut k = Keys::new(Platform::Macos);
            assert_eq!(k.down("shift"), []);
            assert_eq!(k.down("fn"), [], "shift + fn is not push to talk");
            assert_eq!(k.up("shift"), [], "releasing never presses");
        }

        #[test]
        fn key_repeat_is_ignored() {
            let mut k = Keys::new(Platform::Macos);
            assert_eq!(k.down("fn"), [Pressed(HotkeyAction::PushToTalk)]);
            assert_eq!(k.down("fn"), []);
            k.down("Space");
            assert_eq!(k.down("Space"), []);
        }

        #[test]
        fn other_key_during_hold_is_foreign() {
            let mut k = Keys::new(Platform::Windows);
            k.down("ctrl");
            assert_eq!(k.down("meta"), [Pressed(HotkeyAction::PushToTalk)]);
            assert_eq!(k.down("KeyD"), [ForeignKey]);
            assert_eq!(k.up("KeyD"), []);
            assert_eq!(k.up("meta"), [Released(HotkeyAction::PushToTalk)]);
        }

        #[test]
        fn cancel_works_on_top_of_a_held_binding() {
            let mut k = Keys::new(Platform::Macos);
            k.down("fn");
            assert_eq!(k.down("Escape"), [Pressed(HotkeyAction::Cancel)]);
            assert_eq!(k.up("Escape"), [Released(HotkeyAction::Cancel)]);
            assert_eq!(k.up("fn"), [Released(HotkeyAction::PushToTalk)]);
        }

        #[test]
        fn keyed_binding_fires_on_its_key() {
            let mut k = Keys::new(Platform::Macos);
            assert_eq!(k.down("ctrl"), []);
            assert_eq!(k.down("meta"), []);
            assert_eq!(k.down("KeyV"), [Pressed(HotkeyAction::PasteLast)]);
        }

        #[test]
        fn missed_key_up_heals_from_the_reported_set() {
            let mut k = Keys::new(Platform::Macos);
            k.down("fn");
            k.pressed = KeySet::default();
            assert_eq!(
                k.down("KeyA"),
                [Released(HotkeyAction::PushToTalk)],
                "fn is gone from the reported set"
            );

            let mut k = Keys::new(Platform::Macos);
            k.down("fn");
            assert_eq!(
                k.matcher.sync(KeySet::default()),
                [Released(HotkeyAction::PushToTalk)]
            );
            assert_eq!(k.matcher.sync(KeySet::default()), []);
        }
    }
}
