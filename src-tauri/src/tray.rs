//! Menu-bar / tray icon (spec §2.4).

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};
use tw_core::dictation::machine::Event;
use tw_core::{DictationPhase, DictationState, NoticeKind};

use crate::commands::app::hide_pill;
use crate::commands::settings::save;
use crate::state::AppState;
use crate::windows::{self, MainRoute};

pub const TRAY_ID: &str = "main";

const OPEN: &str = "open";
const MIC_DEFAULT: &str = "mic-default";
const PASTE_LAST: &str = "paste-last";
const HIDE_PILL: &str = "hide-pill";
const RESTART_TO_UPDATE: &str = "restart-to-update";
/// Also the id of the app menu's Settings item, so this handler covers both.
pub const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Prefix of microphone item ids: `mic:<device id>`.
const MIC_PREFIX: &str = "mic:";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Tiny Wispr")
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event);
    if cfg!(target_os = "macos") {
        let (rgba, width, height) = template_icon();
        builder = builder
            .icon(Image::new_owned(rgba, width, height))
            .icon_as_template(true);
    } else if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Bar heights of the logo's waveform, in pixels at 2x (`app-icon.svg`).
const BARS: [u32; 6] = [8, 20, 32, 18, 24, 8];
const BAR_WIDTH: u32 = 4;
const BAR_PITCH: u32 = 7;
const ICON_PADDING: u32 = 2;
/// macOS draws menu-bar icons 18 pt tall, so 36 px is the 2x size.
const ICON_HEIGHT: u32 = 36;

/// The logo's bars as a black, alpha-only image. macOS tints template images
/// to suit the menu bar, which a full-colour app icon would not.
fn template_icon() -> (Vec<u8>, u32, u32) {
    let bars = u32::try_from(BARS.len()).unwrap_or(0);
    let width = 2 * ICON_PADDING + (bars - 1) * BAR_PITCH + BAR_WIDTH;
    let radius = f64::from(BAR_WIDTH) / 2.0;
    let mid = f64::from(ICON_HEIGHT) / 2.0;
    let mut rgba = vec![0_u8; (width * ICON_HEIGHT * 4) as usize];
    for y in 0..ICON_HEIGHT {
        for x in 0..width {
            let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            // Distance to the nearest bar, each a vertical capsule.
            let distance = BARS
                .iter()
                .zip(0_u32..)
                .map(|(&height, i)| {
                    let cx = f64::from(ICON_PADDING + i * BAR_PITCH) + radius;
                    let half = (f64::from(height) / 2.0 - radius).max(0.0);
                    let dy = ((py - mid).abs() - half).max(0.0);
                    (px - cx).hypot(dy) - radius
                })
                .fold(f64::INFINITY, f64::min);
            let coverage = (0.5 - distance).clamp(0.0, 1.0);
            let alpha = (coverage * 255.0).round() as u8;
            rgba[((y * width + x) * 4 + 3) as usize] = alpha;
        }
    }
    (rgba, width, ICON_HEIGHT)
}

/// Rebuilds the menu, so the microphone list and its check mark are current.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Err(err) = menu(app).and_then(|menu| tray.set_menu(Some(menu))) {
        tracing::warn!(%err, "could not update the tray menu");
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let selected = app
        .try_state::<AppState>()
        .and_then(|state| state.data.settings.get().microphone);
    let ready = app.try_state::<AppState>().and_then(|state| {
        state
            .updates
            .state()
            .installable()
            .map(|release| release.version.clone())
    });
    let microphones = tw_audio::list_input_devices().unwrap_or_else(|err| {
        tracing::warn!(%err, "could not list microphones");
        Vec::new()
    });
    let mut mic_items = vec![CheckMenuItem::with_id(
        app,
        MIC_DEFAULT,
        "System default",
        true,
        selected.is_none(),
        None::<&str>,
    )?];
    for mic in &microphones {
        mic_items.push(CheckMenuItem::with_id(
            app,
            format!("{MIC_PREFIX}{}", mic.id),
            &mic.name,
            true,
            selected.as_deref() == Some(mic.id.as_str()),
            None::<&str>,
        )?);
    }
    let mic_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = mic_items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();
    let microphone = Submenu::with_items(app, "Microphone", true, &mic_refs)?;
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, OPEN, "Open Tiny Wispr", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &microphone,
            &MenuItem::with_id(app, PASTE_LAST, "Paste last transcript", true, None::<&str>)?,
            &MenuItem::with_id(
                app,
                HIDE_PILL,
                "Hide the pill for 1 hour",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quit Tiny Wispr", true, None::<&str>)?,
        ],
    )?;
    if let Some(version) = ready {
        let label = format!("Restart to update to {version}");
        menu.insert_items(
            &[
                &MenuItem::with_id(app, RESTART_TO_UPDATE, label, true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
            ],
            0,
        )?;
    }
    Ok(menu)
}

/// The tooltip says what dictation is doing.
pub fn show_state(app: &AppHandle, state: &DictationState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let text = match (state.phase, state.notice.as_ref().map(|n| n.kind)) {
        (DictationPhase::Listening, _) => "Tiny Wispr · Listening",
        (DictationPhase::Processing, _) => "Tiny Wispr · Transcribing",
        (_, Some(NoticeKind::NoModel)) => "Tiny Wispr · No voice model yet",
        _ => "Tiny Wispr",
    };
    if let Err(err) = tray.set_tooltip(Some(text)) {
        tracing::debug!(%err, "could not set the tray tooltip");
    }
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id.as_ref();
    let result = match id {
        OPEN => windows::open_app(app),
        SETTINGS => windows::show_main(app, Some(MainRoute::Settings)).map(drop),
        PASTE_LAST => {
            if let Some(state) = app.try_state::<AppState>() {
                state.dictation.event(Event::PasteLastRequested);
            }
            Ok(())
        }
        HIDE_PILL => {
            if let Some(state) = app.try_state::<AppState>()
                && let Err(err) = hide_pill(app, &state, 60)
            {
                tracing::warn!(%err, "could not hide the pill");
            }
            Ok(())
        }
        RESTART_TO_UPDATE => {
            match app
                .try_state::<AppState>()
                .map(|state| state.updates.install(app))
            {
                Some(Err(err)) => {
                    tracing::warn!(%err, "could not restart to update");
                    windows::show_main(app, Some(MainRoute::Settings)).map(drop)
                }
                _ => Ok(()),
            }
        }
        QUIT => {
            app.exit(0);
            Ok(())
        }
        MIC_DEFAULT => select_microphone(app, None),
        other => match other.strip_prefix(MIC_PREFIX) {
            Some(device) => select_microphone(app, Some(device.to_owned())),
            None => Ok(()),
        },
    };
    if let Err(err) = result {
        tracing::warn!(%err, item = id, "tray action failed");
    }
}

fn select_microphone(app: &AppHandle, device: Option<String>) -> tauri::Result<()> {
    if let Some(state) = app.try_state::<AppState>()
        && let Err(err) = save(app, &state, |s| s.microphone = device)
    {
        tracing::warn!(%err, "could not save the microphone");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_icon_draws_every_bar() {
        let (rgba, width, height) = template_icon();
        assert_eq!(height, ICON_HEIGHT);
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        let alpha = |x: u32, y: u32| rgba[((y * width + x) * 4 + 3) as usize];
        let mid = height / 2;
        for i in 0..BARS.len() as u32 {
            let centre = ICON_PADDING + i * BAR_PITCH + BAR_WIDTH / 2;
            assert_eq!(alpha(centre, mid), 255, "bar {i}");
        }
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(ICON_PADDING + BAR_PITCH / 2 + 2, mid), 0, "gap");
    }
}
