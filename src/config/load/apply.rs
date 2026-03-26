use super::*;

pub(super) fn publish_config(cfg: Config) {
    {
        let mut current = lock_config();
        *current = cfg;
        sync_audio_mix_levels_from_config(&current);
        logging::set_file_logging_enabled(current.log_to_file);
    }
    info!("Configuration loaded from '{CONFIG_PATH}'.");
}

pub(super) fn publish_keymap(conf: &SimpleIni) {
    let km = load_keymap_from_ini_local(conf);
    crate::core::input::set_keymap(km);
}

pub(super) fn load_defaults_after_error() {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = String::new();
}

pub(super) fn apply_input_runtime_state() {
    let mut dedicated = get().only_dedicated_menu_buttons;
    if dedicated && !crate::core::input::any_player_has_dedicated_menu_buttons() {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has dedicated menu buttons mapped — disabling."
        );
        dedicated = false;
        lock_config().only_dedicated_menu_buttons = false;
    }
    crate::core::input::set_only_dedicated_menu_buttons(dedicated);
    crate::core::input::set_input_debounce_seconds(get().input_debounce_seconds);
}
