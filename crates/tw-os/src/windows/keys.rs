//! The Windows key listener: a `WH_KEYBOARD_LL` hook on its own
//! message-loop thread.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::thread::JoinHandle;
use std::time::Duration;

use tw_core::hotkey::{KeyCode, Modifier};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::insert::{SYNTHETIC_EVENT_MARKER, send_keys};
use crate::keys::{Engine, MenuMask, Shared, Side, codes};
use crate::{OsError, Result, now_ms};

/// An unassigned virtual key: pressing it while Win or Alt is held turns the
/// release into a chord, so the Start menu or menu bar stays closed.
const MASK_KEY: u16 = 0xE8;

pub(crate) struct Listener {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

struct HookState {
    shared: Arc<Shared>,
    engine: Engine,
    mask: MenuMask,
}

thread_local! {
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

impl Listener {
    pub(crate) fn start(shared: Arc<Shared>) -> Result<Self> {
        let (ready, started) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("tw-key-listener".into())
            .spawn(move || {
                STATE.with_borrow_mut(|state| {
                    *state = Some(HookState {
                        shared,
                        engine: Engine::default(),
                        mask: MenuMask::default(),
                    });
                });
                // SAFETY: a module handle for this process and a valid hook
                // procedure; the hook lives until this thread's loop ends.
                let hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(hook_proc),
                        GetModuleHandleW(std::ptr::null()),
                        0,
                    )
                };
                if hook.is_null() {
                    let _ = ready.send(Err(OsError::Platform(
                        "could not install the keyboard hook".into(),
                    )));
                    return;
                }
                // SAFETY: no arguments.
                let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
                // SAFETY: a zeroed MSG is a valid out parameter.
                let mut message: MSG = unsafe { std::mem::zeroed() };
                // SAFETY: standard message loop on this thread.
                while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
                    // SAFETY: as above.
                    unsafe { DispatchMessageW(&message) };
                }
                // SAFETY: the hook installed above.
                unsafe { UnhookWindowsHookEx(hook) };
            })?;
        match started.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(OsError::Platform("the key listener did not start".into())),
        }
    }

    pub(crate) fn can_swallow(&self) -> bool {
        true
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // SAFETY: posting to a thread id we own.
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("key listener thread panicked");
        }
    }
}

fn modifier(vk: u16) -> Option<(Modifier, Side)> {
    Some(match vk {
        VK_LCONTROL => (Modifier::Ctrl, Side::Left),
        VK_RCONTROL => (Modifier::Ctrl, Side::Right),
        VK_LMENU => (Modifier::Alt, Side::Left),
        VK_RMENU => (Modifier::Alt, Side::Right),
        VK_LSHIFT => (Modifier::Shift, Side::Left),
        VK_RSHIFT => (Modifier::Shift, Side::Right),
        VK_LWIN => (Modifier::Meta, Side::Left),
        VK_RWIN => (Modifier::Meta, Side::Right),
        _ => return None,
    })
}

fn modifier_down(modifier: Modifier) -> bool {
    let vks: &[u16] = match modifier {
        Modifier::Ctrl => &[VK_CONTROL],
        Modifier::Alt => &[VK_MENU],
        Modifier::Shift => &[VK_SHIFT],
        Modifier::Meta => &[VK_LWIN, VK_RWIN],
        Modifier::Fn => &[],
    };
    // SAFETY: plain key state reads.
    vks.iter()
        .any(|&vk| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0)
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let swallow = std::panic::catch_unwind(|| handle(info)).unwrap_or_else(|_| {
            tracing::error!("key listener panicked on an event");
            false
        });
        if swallow {
            return 1;
        }
    }
    // SAFETY: passing the event on unchanged.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Returns whether to keep the event from the focused app.
fn handle(info: &KBDLLHOOKSTRUCT) -> bool {
    if info.dwExtraInfo == SYNTHETIC_EVENT_MARKER {
        return false;
    }
    let down = info.flags & LLKHF_UP == 0;
    let at = now_ms();
    let Ok(vk) = u16::try_from(info.vkCode) else {
        return false;
    };
    STATE.with_borrow_mut(|state| {
        let Some(state) = state.as_mut() else {
            return false;
        };
        let shared = Arc::clone(&state.shared);
        // A release Windows kept to itself (Win+L locks before Win comes up)
        // shows in the async key state once the next key arrives.
        let held: BTreeSet<Modifier> = state
            .engine
            .modifiers()
            .into_iter()
            .filter(|&held| modifier(vk).is_some_and(|(m, _)| m == held) || modifier_down(held))
            .collect();
        shared.emit_keys(state.engine.sync_modifiers(&held, at));
        if let Some((modifier, side)) = modifier(vk) {
            let event = state.engine.modifier(modifier, side, down, at);
            let held = state.engine.modifiers();
            if down {
                state
                    .mask
                    .modifiers_pressed(&held, &shared.swallow(), shared.capturing());
            } else if state.mask.release(modifier, &held) {
                send_keys(&[(MASK_KEY, true), (MASK_KEY, false)]);
            }
            shared.emit_keys(event);
            return false;
        }
        let extended = info.flags & LLKHF_EXTENDED != 0;
        let Some(name) = codes::windows_key(info.scanCode, extended) else {
            return false;
        };
        let outcome = state.engine.key(
            KeyCode::new(name),
            down,
            &shared.swallow(),
            shared.capturing(),
            at,
        );
        if outcome.swallow && down {
            state.mask.key_swallowed(&state.engine.modifiers());
        }
        shared.emit_keys(outcome.event);
        outcome.swallow
    })
}
