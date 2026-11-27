//! Every IPC command from spec §4, one module per concern so owners can work
//! in parallel. Payload types are mirrored in `src/lib/ipc/types.ts`.

pub mod app;
pub mod audio;
pub mod dictation;
pub mod files;
pub mod history;
pub mod hotkeys;
pub mod models;
pub mod permissions;
pub mod settings;
pub mod text;
pub mod updates;

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        settings::get_settings,
        settings::update_settings,
        settings::reset_shortcuts,
        settings::get_default_shortcuts,
        hotkeys::validate_hotkey,
        hotkeys::start_hotkey_capture,
        hotkeys::stop_hotkey_capture,
        hotkeys::get_listener_status,
        permissions::get_permissions,
        permissions::request_permission,
        permissions::open_permission_settings,
        audio::list_microphones,
        audio::start_mic_test,
        audio::stop_mic_test,
        models::list_models,
        models::download_model,
        models::cancel_download,
        models::delete_model,
        models::import_model,
        models::get_hardware_info,
        models::get_recommendation,
        dictation::start_dictation,
        dictation::stop_dictation,
        dictation::cancel_dictation,
        dictation::undo_cancel,
        dictation::paste_last_transcript,
        dictation::copy_last_transcript,
        dictation::get_dictation_state,
        files::transcribe_file,
        files::cancel_file_transcription,
        files::get_file_transcription,
        files::save_file_transcript,
        history::list_history,
        history::delete_history_entry,
        history::clear_history,
        history::get_stats,
        text::list_dictionary,
        text::upsert_dictionary_entry,
        text::delete_dictionary_entry,
        text::list_snippets,
        text::upsert_snippet,
        text::delete_snippet,
        text::test_polish,
        app::complete_onboarding,
        app::open_main_window,
        app::hide_pill_for,
        app::set_pill_hit_regions,
        app::list_network_log,
        updates::get_update_state,
        updates::check_for_updates,
        updates::download_update,
        updates::install_update,
    ]
}
