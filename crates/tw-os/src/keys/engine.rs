//! The OS-independent half of the listener: which keys are held, what each
//! raw transition means, and which events are kept from the focused app.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use tw_core::hotkey::{Hotkey, Key, KeyCode, KeySet, KeyTransition, Modifier};

use super::KeyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
}

impl Side {
    fn bit(self) -> u8 {
        match self {
            Self::Left => 1,
            Self::Right => 2,
        }
    }
}

/// The combos whose events the listener keeps from the focused app.
#[derive(Debug, Clone, Default)]
pub(crate) struct SwallowSet {
    combos: Vec<Hotkey>,
}

impl SwallowSet {
    pub(crate) fn new(combos: impl IntoIterator<Item = Hotkey>) -> Self {
        Self {
            combos: combos.into_iter().filter(|c| !c.is_empty()).collect(),
        }
    }

    /// The combos that have a key, for OSes that swallow by grabbing them.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn keyed(&self) -> impl Iterator<Item = &Hotkey> {
        self.combos.iter().filter(|c| c.key.is_some())
    }

    /// Whether `key` pressed with exactly `modifiers` held is one of the
    /// combos. Exact, so `Ctrl+Shift+V` is not taken for `Ctrl+V`.
    pub(crate) fn takes_key(&self, key: &KeyCode, modifiers: &BTreeSet<Modifier>) -> bool {
        self.combos
            .iter()
            .any(|c| c.key.as_ref() == Some(key) && &c.modifiers == modifiers)
    }

    /// Whether exactly `modifiers` is a modifier-only combo.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn takes_modifiers(&self, modifiers: &BTreeSet<Modifier>) -> bool {
        !modifiers.is_empty()
            && self
                .combos
                .iter()
                .any(|c| c.key.is_none() && &c.modifiers == modifiers)
    }

    /// Whether a modifier-only combo holds `modifier`, so a bare press of it
    /// is ours and must not trigger its system action (Fn/Globe on macOS).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn owns_bare(&self, modifier: Modifier) -> bool {
        self.combos
            .iter()
            .any(|c| c.key.is_none() && c.modifiers.contains(&modifier))
    }
}

/// What the listener does with one raw key transition.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// `None` for a release of a key that was never seen going down.
    pub(crate) event: Option<KeyEvent>,
    /// Keep the OS event from the focused app.
    pub(crate) swallow: bool,
}

/// Tracks held keys from raw transitions and turns them into [`KeyEvent`]s.
#[derive(Debug, Default)]
pub(crate) struct Engine {
    /// Bit 1 left, bit 2 right; a modifier is held while any bit is set.
    modifiers: BTreeMap<Modifier, u8>,
    keys: BTreeSet<KeyCode>,
    /// Decided when a key goes down and kept for its repeats and release, so
    /// a release is never swallowed without its press (or the reverse).
    swallowed: HashMap<KeyCode, bool>,
}

impl Engine {
    pub(crate) fn modifiers(&self) -> BTreeSet<Modifier> {
        self.modifiers.keys().copied().collect()
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> &BTreeSet<KeyCode> {
        &self.keys
    }

    fn event(&self, transition: KeyTransition, key: Key, repeat: bool, at: u64) -> KeyEvent {
        KeyEvent {
            transition,
            key,
            repeat,
            pressed: KeySet {
                modifiers: self.modifiers(),
                keys: self.keys.clone(),
            },
            at,
        }
    }

    /// A non-modifier key went down or up.
    pub(crate) fn key(
        &mut self,
        code: KeyCode,
        down: bool,
        swallow: &SwallowSet,
        capture: bool,
        at: u64,
    ) -> Outcome {
        if down {
            let repeat = !self.keys.insert(code.clone());
            let take = match self.swallowed.get(&code) {
                Some(&take) if repeat => take,
                _ => {
                    let take = capture || swallow.takes_key(&code, &self.modifiers());
                    self.swallowed.insert(code.clone(), take);
                    take
                }
            };
            let event = self.event(KeyTransition::Down, Key::Code(code), repeat, at);
            return Outcome {
                event: Some(event),
                swallow: take,
            };
        }
        let take = self.swallowed.remove(&code).unwrap_or(capture);
        if !self.keys.remove(&code) {
            return Outcome {
                event: None,
                swallow: take,
            };
        }
        Outcome {
            event: Some(self.event(KeyTransition::Up, Key::Code(code), false, at)),
            swallow: take,
        }
    }

    /// One side of a modifier went down or up (Windows, X11). Merges sides.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn modifier(
        &mut self,
        modifier: Modifier,
        side: Side,
        down: bool,
        at: u64,
    ) -> Option<KeyEvent> {
        let before = self.modifiers.get(&modifier).copied().unwrap_or(0);
        let after = if down {
            before | side.bit()
        } else {
            before & !side.bit()
        };
        if after == 0 {
            self.modifiers.remove(&modifier);
        } else {
            self.modifiers.insert(modifier, after);
        }
        let kind = match (before != 0, after != 0) {
            (false, true) => KeyTransition::Down,
            (true, false) => KeyTransition::Up,
            _ => return None,
        };
        Some(self.event(kind, Key::Modifier(modifier), false, at))
    }

    /// Makes the held modifiers exactly `held` (macOS reports the whole set
    /// with every event; also used to resync after missed events). Releases
    /// come before presses.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    pub(crate) fn sync_modifiers(&mut self, held: &BTreeSet<Modifier>, at: u64) -> Vec<KeyEvent> {
        let current = self.modifiers();
        let mut events = Vec::new();
        for &modifier in current.difference(held) {
            self.modifiers.remove(&modifier);
            events.push(self.event(KeyTransition::Up, Key::Modifier(modifier), false, at));
        }
        for &modifier in held.difference(&current) {
            self.modifiers.insert(modifier, Side::Left.bit());
            events.push(self.event(KeyTransition::Down, Key::Modifier(modifier), false, at));
        }
        events
    }

    /// Releases the held keys the OS no longer reads as down: a release the
    /// listener never saw (swallowed by a system shortcut, a lost event)
    /// must not leave a binding held forever.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn release_keys_not_down(
        &mut self,
        is_down: impl Fn(&KeyCode) -> bool,
        at: u64,
    ) -> Vec<KeyEvent> {
        let stale: Vec<KeyCode> = self.keys.iter().filter(|k| !is_down(k)).cloned().collect();
        stale
            .into_iter()
            .map(|code| {
                self.keys.remove(&code);
                self.swallowed.remove(&code);
                self.event(KeyTransition::Up, Key::Code(code), false, at)
            })
            .collect()
    }
}

/// Windows opens the Start menu when Win is released alone and focuses the
/// menu bar when Alt is. A press of an unassigned key between press and
/// release makes it a chord, as AutoHotkey and PowerToys do.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Default)]
pub(crate) struct MenuMask {
    pending: bool,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl MenuMask {
    fn holds_menu(modifiers: &BTreeSet<Modifier>) -> bool {
        modifiers.contains(&Modifier::Alt) || modifiers.contains(&Modifier::Meta)
    }

    /// After every modifier press: a modifier-only combo holding Alt or Win
    /// is ours, so its release must not open anything.
    pub(crate) fn modifiers_pressed(
        &mut self,
        modifiers: &BTreeSet<Modifier>,
        swallow: &SwallowSet,
        capture: bool,
    ) {
        if Self::holds_menu(modifiers) && (capture || swallow.takes_modifiers(modifiers)) {
            self.pending = true;
        }
    }

    /// After a key press the listener swallowed: the OS saw Alt or Win
    /// alone, without the key.
    pub(crate) fn key_swallowed(&mut self, modifiers: &BTreeSet<Modifier>) {
        if Self::holds_menu(modifiers) {
            self.pending = true;
        }
    }

    /// On an Alt or Win release: whether to replay it behind the mask key.
    /// `still_held` is the modifier set after the release.
    pub(crate) fn release(&mut self, released: Modifier, still_held: &BTreeSet<Modifier>) -> bool {
        if !matches!(released, Modifier::Alt | Modifier::Meta) || !self.pending {
            return false;
        }
        if !Self::holds_menu(still_held) {
            self.pending = false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Modifier::{Alt, Ctrl, Fn, Meta, Shift};

    fn set(modifiers: &[Modifier]) -> BTreeSet<Modifier> {
        modifiers.iter().copied().collect()
    }

    fn code(name: &str) -> KeyCode {
        KeyCode::new(name)
    }

    fn mac_defaults() -> SwallowSet {
        SwallowSet::new([
            Hotkey::modifiers([Fn]),
            Hotkey::with_key([Fn], "Space"),
            Hotkey::with_key([Ctrl, Meta], "KeyV"),
        ])
    }

    #[test]
    fn fn_down_and_up_are_modifier_only_events() {
        let mut engine = Engine::default();
        let down = engine.sync_modifiers(&set(&[Fn]), 10);
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].transition, KeyTransition::Down);
        assert_eq!(down[0].key, Key::Modifier(Fn));
        assert_eq!(down[0].pressed.modifiers, set(&[Fn]));
        assert_eq!(down[0].at, 10);
        let up = engine.sync_modifiers(&set(&[]), 20);
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].transition, KeyTransition::Up);
        assert!(up[0].pressed.modifiers.is_empty());
        assert!(engine.sync_modifiers(&set(&[]), 30).is_empty());
    }

    #[test]
    fn fn_space_is_swallowed_with_its_release_but_fn_alone_is_reported() {
        let swallow = mac_defaults();
        let mut engine = Engine::default();
        engine.sync_modifiers(&set(&[Fn]), 0);
        let down = engine.key(code("Space"), true, &swallow, false, 1);
        assert!(down.swallow);
        let event = down.event.expect("space down");
        assert_eq!(event.pressed.modifiers, set(&[Fn]));
        assert_eq!(event.pressed.keys, [code("Space")].into_iter().collect());
        // Fn released before Space: the release still belongs to the combo.
        engine.sync_modifiers(&set(&[]), 2);
        let up = engine.key(code("Space"), false, &swallow, false, 3);
        assert!(up.swallow);
        assert!(up.event.expect("space up").pressed.keys.is_empty());
    }

    #[test]
    fn swallowing_needs_exactly_the_combo_modifiers() {
        let swallow = mac_defaults();
        let mut engine = Engine::default();
        assert!(!engine.key(code("Space"), true, &swallow, false, 0).swallow);
        engine.key(code("Space"), false, &swallow, false, 0);
        engine.sync_modifiers(&set(&[Fn, Shift]), 0);
        assert!(!engine.key(code("Space"), true, &swallow, false, 0).swallow);
        engine.key(code("Space"), false, &swallow, false, 0);
        engine.sync_modifiers(&set(&[Ctrl, Meta]), 0);
        assert!(engine.key(code("KeyV"), true, &swallow, false, 0).swallow);
        assert!(!engine.key(code("KeyC"), true, &swallow, false, 0).swallow);
    }

    #[test]
    fn capture_swallows_everything() {
        let mut engine = Engine::default();
        let none = SwallowSet::default();
        let down = engine.key(code("KeyA"), true, &none, true, 0);
        assert!(down.swallow);
        assert!(engine.key(code("KeyA"), false, &none, true, 0).swallow);
    }

    #[test]
    fn key_repeat_is_flagged_and_keeps_the_first_decision() {
        let swallow = mac_defaults();
        let mut engine = Engine::default();
        engine.sync_modifiers(&set(&[Fn]), 0);
        let first = engine.key(code("Space"), true, &swallow, false, 0);
        assert!(!first.event.expect("down").repeat);
        // Fn let go while Space repeats: still the combo's key.
        engine.sync_modifiers(&set(&[]), 1);
        let repeat = engine.key(code("Space"), true, &swallow, false, 2);
        assert!(repeat.swallow);
        assert!(repeat.event.expect("repeat").repeat);
    }

    #[test]
    fn ctrl_opt_hold_is_reported_side_merged() {
        let mut engine = Engine::default();
        let ctrl = engine
            .modifier(Ctrl, Side::Left, true, 0)
            .expect("ctrl down");
        assert_eq!(ctrl.key, Key::Modifier(Ctrl));
        let alt = engine.modifier(Alt, Side::Left, true, 1).expect("alt down");
        assert_eq!(alt.pressed.modifiers, set(&[Ctrl, Alt]));
        assert_eq!(engine.modifier(Ctrl, Side::Right, true, 2), None);
        assert_eq!(engine.modifier(Ctrl, Side::Left, false, 3), None);
        let up = engine
            .modifier(Ctrl, Side::Right, false, 4)
            .expect("last ctrl up");
        assert_eq!(up.transition, KeyTransition::Up);
        assert_eq!(up.pressed.modifiers, set(&[Alt]));
        assert_eq!(engine.modifier(Shift, Side::Left, false, 5), None);
    }

    #[test]
    fn missed_releases_resync() {
        let swallow = SwallowSet::default();
        let mut engine = Engine::default();
        engine.sync_modifiers(&set(&[Meta]), 0);
        engine.key(code("Tab"), true, &swallow, false, 0);
        engine.key(code("KeyA"), true, &swallow, false, 0);
        // Cmd+Tab: the system kept Tab's release.
        let released = engine.release_keys_not_down(|k| k.as_str() == "KeyA", 5);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].key, Key::Code(code("Tab")));
        assert_eq!(released[0].transition, KeyTransition::Up);
        assert_eq!(engine.keys(), &[code("KeyA")].into_iter().collect());
        // A missed Cmd release shows up in the next full modifier set.
        let up = engine.sync_modifiers(&set(&[]), 6);
        assert_eq!(up[0].key, Key::Modifier(Meta));
        // A stray release of a key never seen down reports nothing.
        assert_eq!(
            engine.key(code("KeyZ"), false, &swallow, false, 7).event,
            None
        );
    }

    #[test]
    fn sync_reports_releases_before_presses() {
        let mut engine = Engine::default();
        engine.sync_modifiers(&set(&[Ctrl]), 0);
        let events = engine.sync_modifiers(&set(&[Alt]), 1);
        assert_eq!(events[0].transition, KeyTransition::Up);
        assert_eq!(events[1].transition, KeyTransition::Down);
        assert_eq!(events[1].pressed.modifiers, set(&[Alt]));
    }

    #[test]
    fn swallow_set_queries() {
        let swallow = SwallowSet::new([
            Hotkey::modifiers([Ctrl, Meta]),
            Hotkey::default(),
            Hotkey::with_key([Alt, Shift], "KeyZ"),
        ]);
        assert!(swallow.takes_modifiers(&set(&[Ctrl, Meta])));
        assert!(!swallow.takes_modifiers(&set(&[Ctrl])));
        assert!(
            !swallow.takes_modifiers(&set(&[])),
            "the empty combo is dropped"
        );
        assert!(swallow.owns_bare(Meta));
        assert!(!swallow.owns_bare(Shift), "Shift is only bound with a key");
        assert!(!swallow.owns_bare(Fn));
        assert!(mac_defaults().owns_bare(Fn));
    }

    #[test]
    fn menu_mask_covers_owned_alt_and_win_releases_only() {
        let swallow = SwallowSet::new([Hotkey::modifiers([Ctrl, Meta])]);
        let mut mask = MenuMask::default();
        mask.modifiers_pressed(&set(&[Ctrl]), &swallow, false);
        assert!(!mask.release(Ctrl, &set(&[])));
        mask.modifiers_pressed(&set(&[Ctrl, Meta]), &swallow, false);
        assert!(!mask.release(Ctrl, &set(&[Meta])), "Ctrl opens nothing");
        assert!(mask.release(Meta, &set(&[])));
        assert!(!mask.release(Meta, &set(&[])), "only once per hold");

        let mut mask = MenuMask::default();
        mask.modifiers_pressed(&set(&[Alt]), &swallow, false);
        assert!(!mask.release(Alt, &set(&[])), "a plain Alt tap stays a tap");
        mask.key_swallowed(&set(&[Ctrl, Meta]));
        assert!(mask.release(Meta, &set(&[Ctrl])));
    }
}
