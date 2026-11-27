use tauri::{AppHandle, State};
use tw_core::hotkey::Shortcuts;
use tw_core::{Settings, SettingsPatch};

use crate::build_flavor;
use crate::dictation::Msg;
use crate::error::CommandResult;
use crate::state::AppState;
use crate::{events, pill, tray};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.data.settings.get()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> CommandResult<Settings> {
    if let Some(enabled) = patch.launch_at_login {
        // The login item can change behind our back in System Settings, so
        // compare with the OS rather than the saved value.
        let current = tw_os::autostart::is_enabled()
            .unwrap_or_else(|_| state.data.settings.get().launch_at_login);
        if enabled != current {
            tw_os::autostart::set_enabled(enabled)?;
        }
    }
    save(&app, &state, |s| s.apply(patch))
}

#[tauri::command]
pub fn reset_shortcuts(app: AppHandle, state: State<'_, AppState>) -> CommandResult<Settings> {
    save(&app, &state, |s| {
        s.shortcuts = build_flavor::default_shortcuts();
    })
}

/// What `reset_shortcuts` restores. Development builds have their own.
#[tauri::command]
pub fn get_default_shortcuts() -> Shortcuts {
    build_flavor::default_shortcuts()
}

/// Applies `change`, persists it, emits `settings:changed` and applies the
/// settings that take effect immediately.
pub fn save(
    app: &AppHandle,
    state: &AppState,
    change: impl FnOnce(&mut Settings),
) -> CommandResult<Settings> {
    let before = state.data.settings.get();
    let settings = state.data.settings.update(change)?;
    events::emit(app, events::SETTINGS_CHANGED, settings.clone());
    pill::sync_visibility(app);
    if before.shortcuts != settings.shortcuts {
        state.dictation.send(Msg::SettingsChanged);
    }
    if before.microphone != settings.microphone {
        tray::refresh(app);
    }
    if before.show_in_dock != settings.show_in_dock {
        show_in_dock(app, settings.show_in_dock);
    }
    if before.history_retention != settings.history_retention {
        match state.data.change_history(|_| ()) {
            Ok(()) => events::emit(app, events::HISTORY_CHANGED, ()),
            Err(err) => tracing::warn!(%err, "could not apply history retention"),
        }
    }
    Ok(settings)
}

/// Makes the saved setting match the login item, which the user can remove
/// or add outside the app.
pub fn sync_launch_at_login(data: &crate::state::Data) {
    match tw_os::autostart::is_enabled() {
        Ok(enabled) if enabled != data.settings.get().launch_at_login => {
            if let Err(err) = data.settings.update(|s| s.launch_at_login = enabled) {
                tracing::warn!(%err, "could not save launch at login");
            }
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "could not read launch at login"),
    }
}

/// macOS: a regular app with a Dock icon, or an accessory that lives in the
/// menu bar only.
pub fn show_in_dock(app: &AppHandle, visible: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if visible {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(err) = app.set_activation_policy(policy) {
            tracing::warn!(%err, "could not change the Dock icon");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, visible);
}
