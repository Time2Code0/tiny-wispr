//! The pill window's behaviour: on the display the cursor is on, shown only
//! when it should be, and click-through outside the shapes the frontend
//! reports.

use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, WebviewWindow};
use tw_core::{DictationPhase, DictationState, Settings};

use crate::state::{AppState, lock};
use crate::windows::{self, PILL};

/// Cursor polling while it is near the pill, and otherwise.
const FAST_POLL: Duration = Duration::from_millis(16);
const SLOW_POLL: Duration = Duration::from_millis(200);
/// "Near": this many logical pixels around the pill window.
const NEAR_MARGIN: f64 = 80.0;

/// A rectangle in CSS pixels, relative to the pill window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PillRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PillRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

#[derive(Default)]
pub struct Pill {
    regions: Mutex<Vec<PillRect>>,
}

impl Pill {
    pub fn set_regions(&self, rects: Vec<PillRect>) {
        *lock(&self.regions) = rects;
    }

    fn hit(&self, x: f64, y: f64) -> bool {
        lock(&self.regions).iter().any(|r| r.contains(x, y))
    }
}

/// Whether the pill is on screen: always while dictating or showing a
/// notice, otherwise per "Show the pill at all times" and "Hide for 1 hour".
pub fn should_show(settings: &Settings, dictation: &DictationState, now_ms: i64) -> bool {
    if dictation.phase != DictationPhase::Idle {
        return true;
    }
    let hidden_for_now = settings.pill_hidden_until.is_some_and(|t| t > now_ms);
    settings.show_pill_always && !hidden_for_now
}

pub fn sync_visibility(app: &AppHandle) {
    let (Some(pill), Some(state)) = (app.get_webview_window(PILL), app.try_state::<AppState>())
    else {
        return;
    };
    let show = should_show(
        &state.data.settings.get(),
        &state.dictation.view(),
        windows::now_ms(),
    );
    let visible = pill.is_visible().unwrap_or(!show);
    let result = match (show, visible) {
        (true, false) => pill.show(),
        (false, true) => pill.hide(),
        _ => Ok(()),
    };
    if let Err(err) = result {
        tracing::warn!(%err, "failed to update pill visibility");
    }
}

/// Floats above full-screen apps on every Space, never activates.
#[cfg(target_os = "macos")]
pub fn configure(window: &WebviewWindow) {
    use objc2_app_kit::{NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let target = window.clone();
    let result = window.run_on_main_thread(move || {
        let Ok(ptr) = target.ns_window() else {
            return;
        };
        // SAFETY: `ns_window` is this window's live NSWindow, and we are on
        // the main thread, where AppKit objects may be used.
        let ns_window: &NSWindow = unsafe { &*ptr.cast::<NSWindow>() };
        ns_window.setLevel(NSStatusWindowLevel);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns_window.setHidesOnDeactivate(false);
    });
    if let Err(err) = result {
        tracing::warn!(%err, "could not configure the pill window");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn configure(_window: &WebviewWindow) {}

/// Polls the cursor: lets clicks through outside the hit regions, and moves
/// the pill to the display the cursor is on.
pub fn spawn_tracker(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("tw-pill".into())
        .spawn(move || {
            let mut ignoring: Option<bool> = None;
            let mut monitor_key: Option<(i32, i32, u32, u32)> = None;
            loop {
                let near = match app.get_webview_window(PILL) {
                    Some(window) => track(&app, &window, &mut ignoring, &mut monitor_key),
                    None => false,
                };
                std::thread::sleep(if near { FAST_POLL } else { SLOW_POLL });
            }
        });
    if let Err(err) = spawned {
        tracing::error!(%err, "could not start the pill tracker");
    }
}

/// Returns whether the cursor is near the pill.
fn track(
    app: &AppHandle,
    window: &WebviewWindow,
    ignoring: &mut Option<bool>,
    monitor_key: &mut Option<(i32, i32, u32, u32)>,
) -> bool {
    let Ok(cursor) = app.cursor_position() else {
        return false;
    };
    if let Ok(Some(monitor)) = app.monitor_from_point(cursor.x, cursor.y) {
        let key = monitor_identity(&monitor);
        if *monitor_key != Some(key) {
            *monitor_key = Some(key);
            if let Err(err) = windows::position_pill(window, &monitor) {
                tracing::warn!(%err, "could not move the pill");
            }
        }
    }
    let (Ok(position), Ok(size), Ok(scale)) = (
        window.outer_position(),
        window.outer_size(),
        window.scale_factor(),
    ) else {
        return false;
    };
    let x = (cursor.x - f64::from(position.x)) / scale;
    let y = (cursor.y - f64::from(position.y)) / scale;
    let width = f64::from(size.width) / scale;
    let height = f64::from(size.height) / scale;
    let visible = window.is_visible().unwrap_or(false);
    let ignore = !(visible && app.state::<AppState>().pill.hit(x, y));
    if *ignoring != Some(ignore) && can_set_click_through(visible) {
        match window.set_ignore_cursor_events(ignore) {
            Ok(()) => *ignoring = Some(ignore),
            Err(err) => tracing::warn!(%err, "could not set pill click-through"),
        }
    }
    visible
        && x > -NEAR_MARGIN
        && y > -NEAR_MARGIN
        && x < width + NEAR_MARGIN
        && y < height + NEAR_MARGIN
}

/// On Linux a window GTK hasn't shown yet has no GDK window, and tao panics
/// when asked to make it click-through. A hidden pill gets no clicks anyway.
pub fn can_set_click_through(visible: bool) -> bool {
    visible || !cfg!(target_os = "linux")
}

fn monitor_identity(monitor: &Monitor) -> (i32, i32, u32, u32) {
    let PhysicalPosition { x, y } = *monitor.position();
    (x, y, monitor.size().width, monitor.size().height)
}

#[cfg(test)]
mod tests {
    use tw_core::dictation::DictationMode;

    use super::*;

    #[test]
    fn hit_regions_are_half_open_rectangles() {
        let pill = Pill::default();
        assert!(!pill.hit(10.0, 10.0));
        pill.set_regions(vec![PillRect {
            x: 160.0,
            y: 110.0,
            width: 40.0,
            height: 10.0,
        }]);
        assert!(pill.hit(160.0, 110.0));
        assert!(pill.hit(199.9, 119.9));
        assert!(!pill.hit(200.0, 115.0));
        assert!(!pill.hit(150.0, 115.0));
    }

    #[test]
    fn visibility_follows_settings_unless_dictating() {
        let idle = DictationState::idle();
        let listening = DictationState {
            phase: DictationPhase::Listening,
            mode: Some(DictationMode::Hold),
            ..DictationState::idle()
        };
        let mut settings = Settings::default();
        assert!(should_show(&settings, &idle, 0));
        settings.pill_hidden_until = Some(1_000);
        assert!(!should_show(&settings, &idle, 500));
        assert!(should_show(&settings, &idle, 1_000));
        assert!(should_show(&settings, &listening, 500));
        settings.show_pill_always = false;
        assert!(!should_show(&settings, &idle, 2_000));
        assert!(should_show(&settings, &listening, 2_000));
    }
}
