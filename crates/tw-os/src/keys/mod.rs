//! Global key listener: press and release of every key system-wide with the
//! full modifier set, including modifier-only transitions and Fn on macOS.
//!
//! The listener only observes, except for the combos the app asks it to
//! swallow ([`KeyListener::set_swallowed`]) and while the shortcut recorder
//! captures ([`KeyListener::set_capture`]).

pub(crate) mod codes;
mod engine;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub use tw_core::hotkey::{Hotkey, Key, KeyCode, KeyInput, KeySet, KeyTransition, Modifier};

#[cfg(target_os = "linux")]
pub(crate) use codes::Scan;
pub(crate) use engine::{Engine, SwallowSet};
#[cfg(target_os = "windows")]
pub(crate) use engine::{MenuMask, Side};

use crate::{Result, platform};

/// One key transition. Left and right modifiers are merged: a modifier goes
/// down when its first side does and up when its last side does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub transition: KeyTransition,
    /// A modifier or a non-modifier key by its layout-independent code;
    /// `KeyCode::label` gives its display name.
    pub key: Key,
    /// Every key held after this transition.
    pub pressed: KeySet,
    /// The OS repeated a held key. Only ever set on `Down`.
    pub repeat: bool,
    /// Monotonic milliseconds.
    pub at: u64,
}

impl From<KeyEvent> for KeyInput {
    fn from(event: KeyEvent) -> Self {
        Self {
            transition: event.transition,
            key: event.key,
            pressed: event.pressed,
        }
    }
}

/// Whether the OS currently lets the listener see what it should.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerHealth {
    Ok,
    /// macOS Secure Event Input is on (a password field, a terminal's Secure
    /// Keyboard Entry): key presses are hidden, modifier changes still arrive.
    SecureInput,
    /// The permission the listener started with was revoked; nothing arrives
    /// until it is granted again and the listener is restarted.
    PermissionRevoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerEvent {
    Key(KeyEvent),
    /// Reported when it changes; the listener starts in `Ok`.
    Health(ListenerHealth),
    /// The OS stopped delivering events for a while (macOS disables a slow
    /// event tap). The listener re-enabled itself and reported releases for
    /// keys that were let go meanwhile; presses in the gap are lost.
    Interrupted,
}

/// Runs on the listener's own thread, inside the OS hook: return quickly
/// (send to a channel), or the OS disables the hook.
pub type KeyCallback = Arc<dyn Fn(ListenerEvent) + Send + Sync>;

/// State the app changes while the platform listener reads it.
pub(crate) struct Shared {
    swallow: RwLock<SwallowSet>,
    capture: AtomicBool,
    on_event: KeyCallback,
}

impl Shared {
    pub(crate) fn new(on_event: impl Fn(ListenerEvent) + Send + Sync + 'static) -> Self {
        Self {
            swallow: RwLock::new(SwallowSet::default()),
            capture: AtomicBool::new(false),
            on_event: Arc::new(on_event),
        }
    }

    pub(crate) fn set_swallowed(&self, combos: impl IntoIterator<Item = Hotkey>) {
        *self
            .swallow
            .write()
            .unwrap_or_else(|error| error.into_inner()) = SwallowSet::new(combos);
    }

    pub(crate) fn emit(&self, event: ListenerEvent) {
        if catch_unwind(AssertUnwindSafe(|| (self.on_event)(event))).is_err() {
            tracing::error!("key listener callback panicked");
        }
    }

    pub(crate) fn emit_keys(&self, events: impl IntoIterator<Item = KeyEvent>) {
        for event in events {
            self.emit(ListenerEvent::Key(event));
        }
    }

    pub(crate) fn swallow(&self) -> RwLockReadGuard<'_, SwallowSet> {
        self.swallow
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn capturing(&self) -> bool {
        self.capture.load(Ordering::Acquire)
    }

    pub(crate) fn set_capture(&self, capture: bool) {
        self.capture.store(capture, Ordering::Release);
    }
}

/// Stops listening when dropped.
pub struct KeyListener {
    shared: Arc<Shared>,
    inner: platform::keys::Listener,
}

impl KeyListener {
    /// Starts the OS event tap or hook on a dedicated thread. Never prompts:
    /// without the permission it needs this returns `PermissionMissing`
    /// (macOS: Accessibility, or Input Monitoring for an observe-only
    /// listener). On Wayland it returns `Unsupported`. Blocks up to two
    /// seconds while the hook starts.
    pub fn start(on_event: impl Fn(ListenerEvent) + Send + Sync + 'static) -> Result<Self> {
        let shared = Arc::new(Shared::new(on_event));
        let inner = platform::keys::Listener::start(Arc::clone(&shared))?;
        Ok(Self { shared, inner })
    }

    /// Whether this listener can keep events from the focused app. False on
    /// macOS when it runs on Input Monitoring alone, without Accessibility.
    pub fn can_swallow(&self) -> bool {
        self.inner.can_swallow()
    }

    /// Replaces the combos whose key events are kept from the focused app,
    /// typically `tw_core::hotkey::swallow_set` plus the modifier-only
    /// bindings. A key pressed with exactly a combo's modifiers is swallowed,
    /// along with its repeats and release. A modifier-only combo (`fn`,
    /// `Ctrl+Win`) is never swallowed, but its system action is suppressed:
    /// a bare Fn/Globe press on macOS (applied within half a second), the
    /// Start menu and menu bar on Windows. Cheap; call from any thread.
    pub fn set_swallowed(&self, combos: impl IntoIterator<Item = Hotkey>) {
        self.shared.set_swallowed(combos);
    }

    /// While on, every key event is reported and kept from the focused app,
    /// for the shortcut recorder.
    pub fn set_capture(&self, capture: bool) {
        self.shared.set_capture(capture);
    }
}
