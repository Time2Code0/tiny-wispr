//! The global key listener's lifecycle: started only once the permission it
//! needs is granted (never prompting), restarted when it is revoked and
//! granted again, and its health reported to the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::AppHandle;
use tw_core::Permissions;
use tw_core::hotkey::Hotkey;
use tw_os::OsError;
use tw_os::keys::{KeyListener, ListenerEvent, ListenerHealth};

use crate::dictation::Msg;
use crate::events;
use crate::state::lock;

/// Permissions are polled this often while something is missing.
const POLL_MISSING: Duration = Duration::from_secs(2);
/// And this often once everything is granted, to notice a revocation.
const POLL_SETTLED: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListenerState {
    Starting,
    /// macOS: Accessibility (or Input Monitoring) is not granted yet.
    WaitingForPermission,
    Running,
    /// macOS Secure Event Input is on: shortcuts don't arrive meanwhile.
    SecureInput,
    /// No global shortcuts here (Wayland).
    Unsupported,
    Failed,
}

/// Payload of `listener:status` and result of `get_listener_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub state: ListenerState,
    /// False while the listener runs on Input Monitoring alone: shortcuts
    /// with a key still reach the focused app.
    pub can_swallow: bool,
}

/// The running listener and what the runtime wants from it. Settings made
/// before the listener starts are applied when it does.
pub struct Keys {
    listener: Mutex<Option<KeyListener>>,
    swallowed: Mutex<Vec<Hotkey>>,
    capture: AtomicBool,
    status: Mutex<ListenerStatus>,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            listener: Mutex::new(None),
            swallowed: Mutex::new(Vec::new()),
            capture: AtomicBool::new(false),
            status: Mutex::new(ListenerStatus {
                state: ListenerState::Starting,
                can_swallow: false,
            }),
        }
    }
}

impl Keys {
    pub fn set_swallowed(&self, combos: Vec<Hotkey>) {
        if let Some(listener) = lock(&self.listener).as_ref() {
            listener.set_swallowed(combos.iter().cloned());
        }
        *lock(&self.swallowed) = combos;
    }

    pub fn set_capture(&self, capture: bool) {
        self.capture.store(capture, Ordering::Relaxed);
        if let Some(listener) = lock(&self.listener).as_ref() {
            listener.set_capture(capture);
        }
    }

    pub fn status(&self) -> ListenerStatus {
        *lock(&self.status)
    }

    pub fn is_running(&self) -> bool {
        lock(&self.listener).is_some()
    }

    fn install(&self, listener: KeyListener) {
        listener.set_swallowed(lock(&self.swallowed).iter().cloned());
        listener.set_capture(self.capture.load(Ordering::Relaxed));
        *lock(&self.listener) = Some(listener);
    }

    pub fn remove(&self) {
        lock(&self.listener).take();
    }

    /// Returns true when the status changed.
    fn set_status(&self, state: ListenerState) -> bool {
        let can_swallow = lock(&self.listener)
            .as_ref()
            .is_some_and(KeyListener::can_swallow);
        let next = ListenerStatus { state, can_swallow };
        let mut status = lock(&self.status);
        let changed = *status != next;
        *status = next;
        changed
    }
}

/// Polls permissions, emits `permissions:changed`, and starts or restarts
/// the listener when it can run. Never prompts.
pub fn spawn_supervisor(app: AppHandle, keys: Arc<Keys>, runtime: Sender<Msg>) {
    let spawned = std::thread::Builder::new()
        .name("tw-permissions".into())
        .spawn(move || {
            let mut last: Option<Permissions> = None;
            loop {
                let permissions = tw_os::permissions::all();
                if last != Some(permissions) {
                    tracing::info!(?permissions, "permissions");
                    events::emit(&app, events::PERMISSIONS_CHANGED, permissions);
                    last = Some(permissions);
                }
                if !keys.is_running() && !try_start(&app, &keys, &runtime) {
                    break;
                }
                let settled = permissions.all_satisfied() && keys.is_running();
                std::thread::sleep(if settled { POLL_SETTLED } else { POLL_MISSING });
            }
        });
    if let Err(err) = spawned {
        tracing::error!(%err, "could not start the permission watcher");
    }
}

/// Returns false when the listener can never run here.
fn try_start(app: &AppHandle, keys: &Arc<Keys>, runtime: &Sender<Msg>) -> bool {
    let runtime = runtime.clone();
    let callback_keys = Arc::clone(keys);
    let callback_app = app.clone();
    let started = KeyListener::start(move |event| match event {
        ListenerEvent::Key(key) => {
            let _ = runtime.send(Msg::Key(key));
        }
        ListenerEvent::Health(health) => {
            on_health(&callback_app, &callback_keys, health);
        }
        ListenerEvent::Interrupted => tracing::warn!("key listener was interrupted"),
    });
    let state = match started {
        Ok(listener) => {
            keys.install(listener);
            tracing::info!(
                can_swallow = keys.status().can_swallow,
                "key listener running"
            );
            ListenerState::Running
        }
        Err(OsError::PermissionMissing(kind)) => {
            if keys.status().state != ListenerState::WaitingForPermission {
                tracing::info!(?kind, "key listener waiting for permission");
            }
            ListenerState::WaitingForPermission
        }
        Err(OsError::Unsupported) => {
            tracing::warn!("global shortcuts are not supported on this system");
            ListenerState::Unsupported
        }
        Err(err) => {
            tracing::warn!(%err, "key listener could not start");
            ListenerState::Failed
        }
    };
    if keys.set_status(state) {
        events::emit(app, events::LISTENER_STATUS, keys.status());
    }
    state != ListenerState::Unsupported
}

fn on_health(app: &AppHandle, keys: &Arc<Keys>, health: ListenerHealth) {
    tracing::info!(?health, "key listener health");
    let state = match health {
        ListenerHealth::Ok => ListenerState::Running,
        ListenerHealth::SecureInput => ListenerState::SecureInput,
        ListenerHealth::PermissionRevoked => {
            // Dropping the listener from its own callback would join its
            // thread from inside it; the supervisor restarts it once granted.
            let keys = Arc::clone(keys);
            std::thread::spawn(move || keys.remove());
            ListenerState::WaitingForPermission
        }
    };
    if keys.set_status(state) {
        events::emit(app, events::LISTENER_STATUS, keys.status());
    }
}
