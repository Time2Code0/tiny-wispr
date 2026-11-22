//! The C APIs of CoreGraphics, CoreFoundation, HIServices and HIToolbox this
//! crate calls, declared by hand: the objc2 family covers the Objective-C
//! side, and these few plain functions don't need a binding crate.

use std::ffi::c_void;

pub(crate) type CfRef = *const c_void;
pub(crate) type EventRef = *mut c_void;
pub(crate) type MachPortRef = *mut c_void;
pub(crate) type RunLoopRef = *mut c_void;

pub(crate) type EventTapCallback = unsafe extern "C" fn(
    proxy: *mut c_void,
    kind: u32,
    event: EventRef,
    info: *mut c_void,
) -> EventRef;

#[repr(C)]
pub(crate) struct RunLoopTimerContext {
    pub(crate) version: isize,
    pub(crate) info: *mut c_void,
    pub(crate) retain: *const c_void,
    pub(crate) release: *const c_void,
    pub(crate) copy_description: *const c_void,
}

pub(crate) const ANNOTATED_SESSION_EVENT_TAP: u32 = 2;
pub(crate) const HID_EVENT_TAP: u32 = 0;
pub(crate) const HEAD_INSERT_EVENT_TAP: u32 = 0;
pub(crate) const TAP_OPTION_DEFAULT: u32 = 0;
pub(crate) const TAP_OPTION_LISTEN_ONLY: u32 = 1;

pub(crate) const EVENT_KEY_DOWN: u32 = 10;
pub(crate) const EVENT_KEY_UP: u32 = 11;
pub(crate) const EVENT_FLAGS_CHANGED: u32 = 12;
pub(crate) const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub(crate) const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

pub(crate) const FIELD_KEYBOARD_KEYCODE: u32 = 9;
pub(crate) const FIELD_EVENT_SOURCE_USER_DATA: u32 = 42;

pub(crate) const FLAG_SHIFT: u64 = 1 << 17;
pub(crate) const FLAG_CONTROL: u64 = 1 << 18;
pub(crate) const FLAG_ALTERNATE: u64 = 1 << 19;
pub(crate) const FLAG_COMMAND: u64 = 1 << 20;
pub(crate) const FLAG_SECONDARY_FN: u64 = 1 << 23;

pub(crate) const SOURCE_STATE_COMBINED_SESSION: u32 = 0;
pub(crate) const SOURCE_STATE_HID_SYSTEM: u32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: EventTapCallback,
        info: *mut c_void,
    ) -> MachPortRef;
    pub(crate) fn CGEventTapEnable(tap: MachPortRef, enable: bool);
    pub(crate) fn CGEventTapIsEnabled(tap: MachPortRef) -> bool;
    pub(crate) fn CGEventGetFlags(event: EventRef) -> u64;
    pub(crate) fn CGEventGetIntegerValueField(event: EventRef, field: u32) -> i64;
    pub(crate) fn CGEventSetIntegerValueField(event: EventRef, field: u32, value: i64);
    pub(crate) fn CGEventSourceFlagsState(state: u32) -> u64;
    pub(crate) fn CGEventSourceKeyState(state: u32, key: u16) -> bool;
    pub(crate) fn CGEventCreateKeyboardEvent(source: *mut c_void, key: u16, down: bool)
    -> EventRef;
    pub(crate) fn CGEventSetFlags(event: EventRef, flags: u64);
    pub(crate) fn CGEventSetType(event: EventRef, kind: u32);
    pub(crate) fn CGEventKeyboardSetUnicodeString(event: EventRef, length: usize, text: *const u16);
    pub(crate) fn CGEventPost(tap: u32, event: EventRef);
    pub(crate) fn CGPreflightListenEventAccess() -> bool;
    pub(crate) fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn AXIsProcessTrusted() -> bool;
    pub(crate) fn AXIsProcessTrustedWithOptions(options: CfRef) -> bool;
    pub(crate) static kAXTrustedCheckOptionPrompt: CfRef;
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn IsSecureEventInputEnabled() -> bool;
    pub(crate) fn TISCopyCurrentKeyboardLayoutInputSource() -> CfRef;
    pub(crate) fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> CfRef;
    pub(crate) fn TISGetInputSourceProperty(source: CfRef, key: CfRef) -> CfRef;
    pub(crate) static kTISPropertyUnicodeKeyLayoutData: CfRef;
    pub(crate) fn UCKeyTranslate(
        layout: *const u8,
        key_code: u16,
        key_action: u16,
        modifier_state: u32,
        keyboard_type: u32,
        options: u32,
        dead_key_state: *mut u32,
        max_length: usize,
        actual_length: *mut usize,
        output: *mut u16,
    ) -> i32;
    pub(crate) fn LMGetKbdType() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CFRetain(value: CfRef) -> CfRef;
    pub(crate) fn CFRelease(value: CfRef);
    pub(crate) fn CFDataGetBytePtr(data: CfRef) -> *const u8;
    pub(crate) fn CFDictionaryCreate(
        allocator: CfRef,
        keys: *const CfRef,
        values: *const CfRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CfRef;
    pub(crate) static kCFBooleanTrue: CfRef;
    pub(crate) static kCFTypeDictionaryKeyCallBacks: c_void;
    pub(crate) static kCFTypeDictionaryValueCallBacks: c_void;
    pub(crate) fn CFMachPortCreateRunLoopSource(
        allocator: CfRef,
        port: MachPortRef,
        order: isize,
    ) -> *mut c_void;
    pub(crate) fn CFMachPortInvalidate(port: MachPortRef);
    pub(crate) fn CFRunLoopGetCurrent() -> RunLoopRef;
    pub(crate) fn CFRunLoopRun();
    pub(crate) fn CFRunLoopStop(run_loop: RunLoopRef);
    pub(crate) fn CFRunLoopAddSource(run_loop: RunLoopRef, source: *mut c_void, mode: CfRef);
    pub(crate) fn CFRunLoopRemoveSource(run_loop: RunLoopRef, source: *mut c_void, mode: CfRef);
    pub(crate) fn CFRunLoopAddTimer(run_loop: RunLoopRef, timer: *mut c_void, mode: CfRef);
    pub(crate) fn CFRunLoopTimerCreate(
        allocator: CfRef,
        fire_date: f64,
        interval: f64,
        flags: usize,
        order: isize,
        callout: unsafe extern "C" fn(timer: *mut c_void, info: *mut c_void),
        context: *mut RunLoopTimerContext,
    ) -> *mut c_void;
    pub(crate) fn CFRunLoopTimerInvalidate(timer: *mut c_void);
    pub(crate) fn CFAbsoluteTimeGetCurrent() -> f64;
    pub(crate) static kCFRunLoopCommonModes: CfRef;
}

/// A retained CoreFoundation object, released on drop.
pub(crate) struct Owned(pub(crate) CfRef);

impl Owned {
    /// Takes ownership of a +1 reference; `None` for null.
    pub(crate) fn new(value: CfRef) -> Option<Self> {
        (!value.is_null()).then_some(Self(value))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: `Owned` only wraps non-null references it owns.
        unsafe { CFRelease(self.0) };
    }
}
