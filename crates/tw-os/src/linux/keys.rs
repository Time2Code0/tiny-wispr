//! The X11 key listener: XInput2 raw key events on the root window observe
//! every key, and passive key grabs keep swallowed combos from the focused
//! app.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tw_core::hotkey::{Hotkey, KeyCode, Modifier};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

use super::{connect, x11};
use crate::keys::{Engine, Scan, Shared, codes};
use crate::{Result, now_ms};

const POLL: Duration = Duration::from_millis(5);
/// X keycodes are evdev codes plus 8.
const EVDEV_OFFSET: u32 = 8;

pub(crate) struct Listener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    pub(crate) fn start(shared: Arc<Shared>) -> Result<Self> {
        let (conn, root) = connect()?;
        x11(x11(conn.xinput_xi_query_version(2, 0))?.reply())?;
        let mask = xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE],
        };
        x11(x11(conn.xinput_xi_select_events(root, &[mask]))?.check())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("tw-key-listener".into())
            .spawn(move || {
                let mut engine = Engine::default();
                let mut grabs: Vec<(u8, ModMask)> = Vec::new();
                let mut capturing = false;
                while !stopping.load(Ordering::Acquire) {
                    while let Ok(Some(event)) = conn.poll_for_event() {
                        let (code, down) = match event {
                            Event::XinputRawKeyPress(event) => (event.detail, true),
                            Event::XinputRawKeyRelease(event) => (event.detail, false),
                            _ => continue,
                        };
                        let Some(scan) = code.checked_sub(EVDEV_OFFSET).and_then(codes::evdev)
                        else {
                            continue;
                        };
                        let at = now_ms();
                        match scan {
                            Scan::Modifier(modifier, side) => {
                                shared.emit_keys(engine.modifier(modifier, side, down, at));
                            }
                            Scan::Key(name) => {
                                let swallow = shared.swallow();
                                let outcome =
                                    engine.key(KeyCode::new(name), down, &swallow, false, at);
                                drop(swallow);
                                shared.emit_keys(outcome.event);
                            }
                        }
                    }
                    let wanted = grab_list(shared.swallow().keyed());
                    if wanted != grabs {
                        for &(key, modifiers) in &grabs {
                            let _ = conn.ungrab_key(key, root, modifiers);
                        }
                        for &(key, modifiers) in &wanted {
                            let _ = conn.grab_key(
                                false,
                                root,
                                modifiers,
                                key,
                                GrabMode::ASYNC,
                                GrabMode::ASYNC,
                            );
                        }
                        grabs = wanted;
                    }
                    let capture = shared.capturing();
                    if capture != capturing {
                        capturing = capture;
                        if capture {
                            let _ = conn.grab_keyboard(
                                false,
                                root,
                                x11rb::CURRENT_TIME,
                                GrabMode::ASYNC,
                                GrabMode::ASYNC,
                            );
                        } else {
                            let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
                        }
                    }
                    let _ = conn.flush();
                    std::thread::sleep(POLL);
                }
                for (key, modifiers) in grabs {
                    let _ = conn.ungrab_key(key, root, modifiers);
                }
                let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
                let _ = conn.flush();
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    pub(crate) fn can_swallow(&self) -> bool {
        true
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("key listener thread panicked");
        }
    }
}

/// The passive grabs for the combos: each once per Caps Lock and Num Lock
/// state, since X matches lock modifiers exactly.
fn grab_list<'a>(combos: impl Iterator<Item = &'a Hotkey>) -> Vec<(u8, ModMask)> {
    let mut grabs = Vec::new();
    for combo in combos {
        let Some(key) = combo
            .key
            .as_ref()
            .and_then(|key| codes::evdev_code(key.as_str()))
            .and_then(|code| u8::try_from(code + EVDEV_OFFSET).ok())
        else {
            continue;
        };
        let base = combo
            .modifiers
            .iter()
            .fold(ModMask::from(0_u16), |mask, modifier| {
                mask | match modifier {
                    Modifier::Ctrl => ModMask::CONTROL,
                    Modifier::Alt => ModMask::M1,
                    Modifier::Shift => ModMask::SHIFT,
                    Modifier::Meta => ModMask::M4,
                    Modifier::Fn => ModMask::from(0_u16),
                }
            });
        for locks in [
            ModMask::from(0_u16),
            ModMask::LOCK,
            ModMask::M2,
            ModMask::LOCK | ModMask::M2,
        ] {
            grabs.push((key, base | locks));
        }
    }
    grabs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grabs_cover_every_lock_state() {
        let combo = Hotkey::with_key([Modifier::Alt, Modifier::Shift], "KeyZ");
        let grabs = grab_list([&combo].into_iter());
        assert_eq!(grabs.len(), 4);
        assert!(grabs.iter().all(|&(key, _)| key == 44 + 8));
        assert!(grabs.contains(&(52, ModMask::M1 | ModMask::SHIFT | ModMask::M2)));
    }
}
