//! The macOS key listener: two CGEventTaps on a dedicated run-loop thread.
//!
//! Key presses go through an active tap so swallowed combos can be dropped.
//! Modifier changes go through a separate tap that only listens unless Fn is
//! bound on its own: an active tap on flags-changed events, even one that
//! returns them unchanged, breaks Finder's Option-key alternate menu items.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

use tw_core::hotkey::{KeyCode, Modifier};

use super::ffi::*;
use super::insert::SYNTHETIC_EVENT_MARKER;
use crate::keys::{Engine, ListenerEvent, ListenerHealth, Shared, codes};
use crate::{OsError, PermissionKind, Result, now_ms};

/// The Globe key's own key code, sent beside the Fn flag change by some
/// keyboards and remappers.
const GLOBE_KEY_CODE: u16 = 179;
const FN_KEY_CODE: u16 = 63;
const CAPS_LOCK_KEY_CODE: u16 = 57;
const HEALTH_INTERVAL_SECONDS: f64 = 0.5;
/// Permission checks are an IPC round trip; every 2 s is plenty.
const PERMISSION_CHECK_EVERY_TICKS: u32 = 4;

pub(crate) struct Listener {
    run_loop: Owned,
    thread: Option<JoinHandle<()>>,
    active: bool,
}

// SAFETY: the run loop reference is only used for `CFRunLoopStop`, which is
// thread-safe, and it is retained until drop.
unsafe impl Send for Listener {}
// SAFETY: as above; `&Listener` exposes no run-loop access at all.
unsafe impl Sync for Listener {}

impl Listener {
    pub(crate) fn start(shared: Arc<Shared>) -> Result<Self> {
        // Never create a tap without the permission: creating one is what
        // shows the system prompt.
        let active = if accessibility_trusted() {
            true
        } else if listen_access() {
            false
        } else {
            return Err(OsError::PermissionMissing(PermissionKind::Accessibility));
        };
        let (ready, started) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("tw-key-listener".into())
            .spawn(move || run(shared, active, &ready))?;
        match started.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(run_loop)) => Ok(Self {
                run_loop: Owned(run_loop as CfRef),
                thread: Some(thread),
                active,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(OsError::Platform("the key listener did not start".into())),
        }
    }

    pub(crate) fn can_swallow(&self) -> bool {
        self.active
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // SAFETY: the run loop is retained by `self.run_loop`.
        unsafe { CFRunLoopStop(self.run_loop.0.cast_mut()) };
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("key listener thread panicked");
        }
    }
}

pub(crate) fn accessibility_trusted() -> bool {
    // SAFETY: no arguments; never prompts.
    unsafe { AXIsProcessTrusted() }
}

pub(crate) fn listen_access() -> bool {
    // SAFETY: no arguments; never prompts.
    unsafe { CGPreflightListenEventAccess() }
}

/// One attached event tap, detached and released on drop.
struct Tap {
    port: MachPortRef,
    source: *mut c_void,
    run_loop: RunLoopRef,
}

impl Tap {
    /// # Safety
    /// `info` must outlive the tap, and `run_loop` must be this thread's.
    unsafe fn attach(
        mask: u64,
        listen_only: bool,
        info: *mut c_void,
        run_loop: RunLoopRef,
    ) -> Option<Self> {
        let options = if listen_only {
            TAP_OPTION_LISTEN_ONLY
        } else {
            TAP_OPTION_DEFAULT
        };
        // SAFETY: the callback matches the signature; `info` outlives the tap.
        let port = unsafe {
            CGEventTapCreate(
                ANNOTATED_SESSION_EVENT_TAP,
                HEAD_INSERT_EVENT_TAP,
                options,
                mask,
                on_event,
                info,
            )
        };
        if port.is_null() {
            return None;
        }
        // SAFETY: `port` is a valid mach port.
        let source = unsafe { CFMachPortCreateRunLoopSource(ptr::null(), port, 0) };
        if source.is_null() {
            // SAFETY: we own `port`.
            unsafe {
                CFMachPortInvalidate(port);
                CFRelease(port as CfRef);
            }
            return None;
        }
        // SAFETY: valid source and run loop.
        unsafe {
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(port, true);
        }
        Some(Self {
            port,
            source,
            run_loop,
        })
    }

    /// Re-enables a tap macOS disabled; false when it stays off.
    fn ensure_enabled(&self) -> bool {
        // SAFETY: `port` is valid for the tap's life.
        unsafe {
            if CGEventTapIsEnabled(self.port) {
                return true;
            }
            CGEventTapEnable(self.port, true);
            CGEventTapIsEnabled(self.port)
        }
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: we own both references and attached them to `run_loop`.
        unsafe {
            CGEventTapEnable(self.port, false);
            CFRunLoopRemoveSource(self.run_loop, self.source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(self.port);
            CFRelease(self.source as CfRef);
            CFRelease(self.port as CfRef);
        }
    }
}

const KEY_MASK: u64 = (1 << EVENT_KEY_DOWN) | (1 << EVENT_KEY_UP);
const FLAGS_MASK: u64 = 1 << EVENT_FLAGS_CHANGED;

/// Lives on the listener thread; every callback runs there.
struct Context {
    shared: Arc<Shared>,
    active: bool,
    run_loop: RunLoopRef,
    engine: RefCell<Engine>,
    /// Fn is read only from its own flag changes: arrow and function keys
    /// carry the Fn flag in their events whether or not Fn is held.
    fn_down: Cell<bool>,
    key_tap: RefCell<Option<Tap>>,
    flags_tap: RefCell<Option<Tap>>,
    flags_tap_active: Cell<bool>,
    health: Cell<ListenerHealth>,
    permitted: Cell<bool>,
    ticks: Cell<u32>,
}

fn run(shared: Arc<Shared>, active: bool, ready: &SyncSender<Result<usize>>) {
    // SAFETY: this thread's own run loop.
    let run_loop = unsafe { CFRunLoopGetCurrent() };
    let context = Box::new(Context::new(shared, active, run_loop));
    let info = ptr::from_ref(&*context).cast_mut().cast::<c_void>();
    // SAFETY: `context` outlives both taps, which are dropped below.
    let key_tap = unsafe { Tap::attach(KEY_MASK, !active, info, run_loop) };
    if key_tap.is_none() {
        let missing = if active {
            PermissionKind::Accessibility
        } else {
            PermissionKind::InputMonitoring
        };
        let _ = ready.send(Err(OsError::PermissionMissing(missing)));
        return;
    }
    *context.key_tap.borrow_mut() = key_tap;
    if !context.attach_flags_tap() {
        let _ = ready.send(Err(OsError::Platform(
            "could not observe modifier keys".into(),
        )));
        return;
    }
    let mut timer_context = RunLoopTimerContext {
        version: 0,
        info,
        retain: ptr::null(),
        release: ptr::null(),
        copy_description: ptr::null(),
    };
    // SAFETY: `timer_context` is copied by CoreFoundation; `info` outlives
    // the timer, which is invalidated before `context` drops.
    let timer = unsafe {
        let timer = CFRunLoopTimerCreate(
            ptr::null(),
            CFAbsoluteTimeGetCurrent() + HEALTH_INTERVAL_SECONDS,
            HEALTH_INTERVAL_SECONDS,
            0,
            0,
            on_tick,
            &raw mut timer_context,
        );
        if !timer.is_null() {
            CFRunLoopAddTimer(run_loop, timer, kCFRunLoopCommonModes);
        }
        timer
    };
    context.tick();
    // SAFETY: retained for the `Listener`, which releases it.
    unsafe { CFRetain(run_loop as CfRef) };
    if ready.send(Ok(run_loop as usize)).is_err() {
        // SAFETY: balances the retain above; nobody took it.
        unsafe { CFRelease(run_loop as CfRef) };
    } else {
        // SAFETY: this thread exists to run the taps.
        unsafe { CFRunLoopRun() };
    }
    if !timer.is_null() {
        // SAFETY: we created the timer.
        unsafe {
            CFRunLoopTimerInvalidate(timer);
            CFRelease(timer as CfRef);
        }
    }
    context.flags_tap.borrow_mut().take();
    context.key_tap.borrow_mut().take();
}

unsafe extern "C" fn on_event(
    _proxy: *mut c_void,
    kind: u32,
    event: EventRef,
    info: *mut c_void,
) -> EventRef {
    // SAFETY: `info` is the thread's `Context`, alive while its taps are.
    let context = unsafe { &*info.cast::<Context>() };
    let swallow = catch_unwind(AssertUnwindSafe(|| context.handle(kind, event)));
    match swallow {
        Ok(true) => ptr::null_mut(),
        Ok(false) => event,
        Err(_) => {
            tracing::error!("key listener panicked on an event");
            event
        }
    }
}

unsafe extern "C" fn on_tick(_timer: *mut c_void, info: *mut c_void) {
    // SAFETY: as in `on_event`.
    let context = unsafe { &*info.cast::<Context>() };
    if catch_unwind(AssertUnwindSafe(|| context.tick())).is_err() {
        tracing::error!("key listener panicked in its health check");
    }
}

/// The non-Fn modifiers an event's flags hold. Left and right share a flag.
fn flag_modifiers(flags: u64) -> BTreeSet<Modifier> {
    [
        (FLAG_CONTROL, Modifier::Ctrl),
        (FLAG_ALTERNATE, Modifier::Alt),
        (FLAG_SHIFT, Modifier::Shift),
        (FLAG_COMMAND, Modifier::Meta),
    ]
    .into_iter()
    .filter(|(flag, _)| flags & flag != 0)
    .map(|(_, modifier)| modifier)
    .collect()
}

fn mac_code(code: &KeyCode) -> Option<u16> {
    codes::macos_code(code.as_str())
}

impl Context {
    fn new(shared: Arc<Shared>, active: bool, run_loop: RunLoopRef) -> Self {
        Self {
            shared,
            active,
            run_loop,
            engine: RefCell::new(Engine::default()),
            fn_down: Cell::new(false),
            key_tap: RefCell::new(None),
            flags_tap: RefCell::new(None),
            flags_tap_active: Cell::new(false),
            health: Cell::new(ListenerHealth::Ok),
            permitted: Cell::new(true),
            ticks: Cell::new(0),
        }
    }

    /// Returns whether to drop the event.
    fn handle(&self, kind: u32, event: EventRef) -> bool {
        match kind {
            EVENT_TAP_DISABLED_BY_TIMEOUT | EVENT_TAP_DISABLED_BY_USER_INPUT => {
                self.recover();
                return false;
            }
            EVENT_KEY_DOWN | EVENT_KEY_UP | EVENT_FLAGS_CHANGED => {}
            _ => return false,
        }
        // SAFETY: `event` is valid for the callback.
        let (marker, code, flags) = unsafe {
            (
                CGEventGetIntegerValueField(event, FIELD_EVENT_SOURCE_USER_DATA),
                CGEventGetIntegerValueField(event, FIELD_KEYBOARD_KEYCODE),
                CGEventGetFlags(event),
            )
        };
        if marker == SYNTHETIC_EVENT_MARKER {
            return false;
        }
        let Ok(code) = u16::try_from(code) else {
            return false;
        };
        if kind == EVENT_FLAGS_CHANGED {
            self.flags_changed(code, flags)
        } else {
            self.key(code, kind == EVENT_KEY_DOWN, flags)
        }
    }

    fn owns_bare_fn(&self) -> bool {
        self.shared.swallow().owns_bare(Modifier::Fn)
    }

    fn held(&self, flags: u64) -> BTreeSet<Modifier> {
        let mut held = flag_modifiers(flags);
        if self.fn_down.get() {
            held.insert(Modifier::Fn);
        }
        held
    }

    fn key(&self, code: u16, down: bool, flags: u64) -> bool {
        if code == GLOBE_KEY_CODE {
            return self.active && self.owns_bare_fn();
        }
        let Some(name) = codes::macos_key(code) else {
            return false;
        };
        let at = now_ms();
        let (mut events, outcome) = {
            let mut engine = self.engine.borrow_mut();
            // Each key event carries the full modifier flags, which heals a
            // modifier release the listener missed.
            let events = engine.sync_modifiers(&self.held(flags), at);
            let swallow = self.shared.swallow();
            let outcome = engine.key(
                KeyCode::new(name),
                down,
                &swallow,
                self.shared.capturing(),
                at,
            );
            (events, outcome)
        };
        events.extend(outcome.event);
        self.shared.emit_keys(events);
        self.active && outcome.swallow
    }

    fn flags_changed(&self, code: u16, flags: u64) -> bool {
        let at = now_ms();
        let is_fn = matches!(code, FN_KEY_CODE | GLOBE_KEY_CODE);
        if is_fn {
            self.fn_down.set(flags & FLAG_SECONDARY_FN != 0);
        }
        let mut events = self
            .engine
            .borrow_mut()
            .sync_modifiers(&self.held(flags), at);
        // Caps Lock reports a flag change on press only, not on release, so
        // it can only be reported as a tap.
        if code == CAPS_LOCK_KEY_CODE {
            let mut engine = self.engine.borrow_mut();
            let swallow = self.shared.swallow();
            let caps = KeyCode::new("CapsLock");
            events.extend(engine.key(caps.clone(), true, &swallow, false, at).event);
            events.extend(engine.key(caps, false, &swallow, false, at).event);
        }
        self.shared.emit_keys(events);
        is_fn && self.flags_tap_active.get() && self.owns_bare_fn()
    }

    /// Attaches the modifier tap, active only while a bare Fn must be kept
    /// from the system. Returns false when no tap could be created.
    fn attach_flags_tap(&self) -> bool {
        let want_active = self.active && self.owns_bare_fn();
        self.flags_tap.borrow_mut().take();
        let info = ptr::from_ref(self).cast_mut().cast::<c_void>();
        // SAFETY: `self` outlives the tap; this is the context's run loop.
        let tap = unsafe { Tap::attach(FLAGS_MASK, !want_active, info, self.run_loop) };
        let attached = tap.is_some();
        *self.flags_tap.borrow_mut() = tap;
        self.flags_tap_active.set(attached && want_active);
        attached
    }

    /// macOS disabled a tap because a callback was slow or for user input:
    /// turn it back on and release what was let go in the gap.
    /// Returns whether both taps are on again.
    fn recover(&self) -> bool {
        let enabled = [&self.key_tap, &self.flags_tap]
            .iter()
            .all(|tap| tap.borrow().as_ref().is_none_or(Tap::ensure_enabled));
        self.release_stale(false);
        self.shared.emit(ListenerEvent::Interrupted);
        if !enabled {
            self.set_health(ListenerHealth::PermissionRevoked);
        }
        enabled
    }

    fn tick(&self) {
        let ticks = self.ticks.get().wrapping_add(1);
        self.ticks.set(ticks);
        let enabled = [&self.key_tap, &self.flags_tap].iter().all(|tap| {
            tap.borrow().as_ref().is_none_or(|tap| {
                // SAFETY: the port is valid while the tap is attached.
                unsafe { CGEventTapIsEnabled(tap.port) }
            })
        });
        let enabled = enabled || self.recover();
        if self.active
            && self.owns_bare_fn() != self.flags_tap_active.get()
            && !self.attach_flags_tap()
        {
            tracing::warn!("could not reattach the modifier tap");
        }
        // SAFETY: no arguments.
        let secure = unsafe { IsSecureEventInputEnabled() };
        if secure {
            // Key releases are hidden too while secure input is on.
            self.release_stale(true);
        }
        if ticks % PERMISSION_CHECK_EVERY_TICKS == 1 {
            self.permitted.set(if self.active {
                accessibility_trusted()
            } else {
                listen_access()
            });
        }
        let health = if !self.permitted.get() || !enabled {
            ListenerHealth::PermissionRevoked
        } else if secure {
            ListenerHealth::SecureInput
        } else {
            ListenerHealth::Ok
        };
        self.set_health(health);
    }

    fn set_health(&self, health: ListenerHealth) {
        if self.health.replace(health) != health {
            self.shared.emit(ListenerEvent::Health(health));
        }
    }

    /// Releases keys and modifiers the OS no longer reads as held. Never
    /// presses: the polled state can lag, and a false press is worse than a
    /// late release. `keys_only` leaves modifiers alone.
    fn release_stale(&self, keys_only: bool) {
        let at = now_ms();
        let is_down = |code: &KeyCode| {
            mac_code(code).is_none_or(|code| {
                // SAFETY: plain reads of the system key state.
                unsafe {
                    CGEventSourceKeyState(SOURCE_STATE_HID_SYSTEM, code)
                        || CGEventSourceKeyState(SOURCE_STATE_COMBINED_SESSION, code)
                }
            })
        };
        let mut engine = self.engine.borrow_mut();
        let mut events = engine.release_keys_not_down(is_down, at);
        if !keys_only {
            // SAFETY: plain read of the hardware modifier state.
            let flags = unsafe { CGEventSourceFlagsState(SOURCE_STATE_HID_SYSTEM) };
            let mut os_held = flag_modifiers(flags);
            if flags & FLAG_SECONDARY_FN != 0 {
                os_held.insert(Modifier::Fn);
            }
            let held: BTreeSet<Modifier> =
                engine.modifiers().intersection(&os_held).copied().collect();
            if !held.contains(&Modifier::Fn) {
                self.fn_down.set(false);
            }
            events.extend(engine.sync_modifiers(&held, at));
        }
        drop(engine);
        self.shared.emit_keys(events);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tw_core::hotkey::{Hotkey, Key, KeyTransition};

    use super::*;
    use crate::keys::{KeyEvent, KeyListener};

    const SPACE: u16 = 49;
    const ARROW_LEFT: u16 = 123;
    const LEFT_CONTROL: u16 = 59;
    const LEFT_OPTION: u16 = 58;

    type Seen = Arc<Mutex<Vec<KeyEvent>>>;

    /// A listener context without taps, fed synthetic key codes and flags.
    fn context(combos: Vec<Hotkey>) -> (Context, Seen) {
        let seen = Seen::default();
        let sink = Arc::clone(&seen);
        let shared = Arc::new(Shared::new(move |event| {
            if let ListenerEvent::Key(event) = event {
                sink.lock().expect("lock").push(event);
            }
        }));
        shared.set_swallowed(combos);
        let context = Context::new(shared, true, ptr::null_mut());
        context.flags_tap_active.set(context.owns_bare_fn());
        (context, seen)
    }

    fn mac_defaults() -> Vec<Hotkey> {
        vec![
            Hotkey::modifiers([Modifier::Fn]),
            Hotkey::with_key([Modifier::Fn], "Space"),
            Hotkey::with_key([Modifier::Ctrl, Modifier::Meta], "KeyV"),
        ]
    }

    fn drain(seen: &Seen) -> Vec<(KeyTransition, Key)> {
        seen.lock()
            .expect("lock")
            .drain(..)
            .map(|event| (event.transition, event.key))
            .collect()
    }

    #[test]
    fn flags_merge_sides_and_leave_fn_out() {
        let flags = FLAG_CONTROL | FLAG_COMMAND | FLAG_SECONDARY_FN | 0x01;
        assert_eq!(
            flag_modifiers(flags),
            [Modifier::Ctrl, Modifier::Meta].into_iter().collect()
        );
        assert!(flag_modifiers(0x100).is_empty());
    }

    #[test]
    fn bare_fn_is_reported_and_kept_from_the_system_while_bound() {
        let (context, seen) = context(mac_defaults());
        assert!(context.flags_changed(FN_KEY_CODE, FLAG_SECONDARY_FN));
        assert!(context.flags_changed(FN_KEY_CODE, 0));
        assert_eq!(
            drain(&seen),
            [
                (KeyTransition::Down, Key::Modifier(Modifier::Fn)),
                (KeyTransition::Up, Key::Modifier(Modifier::Fn)),
            ]
        );
        // Unbound, Fn's flag change passes through.
        context.shared.set_swallowed([]);
        assert!(!context.flags_changed(FN_KEY_CODE, FLAG_SECONDARY_FN));
    }

    #[test]
    fn fn_space_swallows_space_and_reports_both_keys() {
        let (context, seen) = context(mac_defaults());
        context.flags_changed(FN_KEY_CODE, FLAG_SECONDARY_FN);
        assert!(context.key(SPACE, true, FLAG_SECONDARY_FN));
        assert!(context.key(SPACE, false, FLAG_SECONDARY_FN));
        context.flags_changed(FN_KEY_CODE, 0);
        let events = seen.lock().expect("lock").clone();
        assert_eq!(events[1].key, Key::Code(KeyCode::new("Space")));
        assert_eq!(
            events[1].pressed.modifiers,
            [Modifier::Fn].into_iter().collect()
        );
        // Space alone reaches the app.
        assert!(!context.key(SPACE, true, 0));
    }

    #[test]
    fn arrow_keys_carrying_the_fn_flag_do_not_press_fn() {
        let (context, seen) = context(mac_defaults());
        context.key(ARROW_LEFT, true, FLAG_SECONDARY_FN);
        context.key(ARROW_LEFT, false, FLAG_SECONDARY_FN);
        // A later modifier change with a stale Fn flag is not Fn either.
        context.flags_changed(LEFT_CONTROL, FLAG_CONTROL | FLAG_SECONDARY_FN);
        assert!(
            seen.lock()
                .expect("lock")
                .iter()
                .all(|event| !event.pressed.modifiers.contains(&Modifier::Fn))
        );
    }

    #[test]
    fn ctrl_option_hold_and_a_missed_release_heal_from_the_next_key() {
        let (context, seen) = context(vec![]);
        context.flags_changed(LEFT_CONTROL, FLAG_CONTROL);
        context.flags_changed(LEFT_OPTION, FLAG_CONTROL | FLAG_ALTERNATE);
        assert_eq!(
            drain(&seen),
            [
                (KeyTransition::Down, Key::Modifier(Modifier::Ctrl)),
                (KeyTransition::Down, Key::Modifier(Modifier::Alt)),
            ]
        );
        // Both releases were missed; the next key's flags say none is held.
        assert!(!context.key(SPACE, true, 0));
        let events = drain(&seen);
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[2],
            (KeyTransition::Down, Key::Code(KeyCode::new("Space")))
        );
    }

    #[test]
    fn capture_mode_swallows_every_key() {
        let (context, _) = context(vec![]);
        context.shared.set_capture(true);
        assert!(context.key(SPACE, true, 0));
        assert!(context.key(SPACE, false, 0));
        context.shared.set_capture(false);
        assert!(!context.key(SPACE, true, 0));
    }

    #[test]
    fn an_observe_only_listener_never_swallows() {
        let (mut context, _) = context(mac_defaults());
        context.active = false;
        context.flags_tap_active.set(false);
        assert!(!context.flags_changed(FN_KEY_CODE, FLAG_SECONDARY_FN));
        assert!(!context.key(SPACE, true, FLAG_SECONDARY_FN));
    }

    #[test]
    fn caps_lock_is_reported_as_a_tap() {
        let (context, seen) = context(vec![]);
        context.flags_changed(CAPS_LOCK_KEY_CODE, 1 << 16);
        let caps = Key::Code(KeyCode::new("CapsLock"));
        assert_eq!(
            drain(&seen),
            [
                (KeyTransition::Down, caps.clone()),
                (KeyTransition::Up, caps)
            ]
        );
    }

    #[test]
    fn start_without_permission_is_a_typed_error_not_a_prompt() {
        if accessibility_trusted() || listen_access() {
            // Granted here: the live path runs and stops cleanly.
            let listener = KeyListener::start(|_| {}).expect("start");
            listener.set_swallowed(mac_defaults());
            drop(listener);
            return;
        }
        let error = KeyListener::start(|_| {}).err().expect("no permission");
        assert!(matches!(
            error,
            OsError::PermissionMissing(PermissionKind::Accessibility)
        ));
    }
}
