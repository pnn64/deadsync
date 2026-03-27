use super::store::create_default_config_file;
use super::*;

#[path = "load/backfill.rs"]
mod backfill;
#[path = "load/options.rs"]
mod options;
#[path = "load/shared.rs"]
mod shared;
#[path = "load/theme.rs"]
mod theme_load;

pub fn bootstrap_log_to_file() -> bool {
    shared::bootstrap_log_to_file()
}

pub fn load() {
    ensure_config_file();

    let mut conf = SimpleIni::new();
    match conf.load(CONFIG_PATH) {
        Ok(()) => load_from_ini(&conf),
        Err(e) => {
            warn!("Failed to load '{CONFIG_PATH}': {e}. Using default values.");
            load_defaults_after_error();
        }
    }

    apply_input_runtime_state();
}

fn ensure_config_file() {
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }
}

fn load_from_ini(conf: &SimpleIni) {
    shared::load_runtime_state(conf);

    let default = Config::default();
    let mut cfg = default;
    options::load(conf, default, &mut cfg);
    theme_load::load(conf, default, &mut cfg);

    publish_config(cfg);
    publish_keymap(conf);
    backfill::write_missing_fields(conf);
}

fn publish_config(cfg: Config) {
    {
        let mut current = lock_config();
        *current = cfg;
        sync_audio_mix_levels_from_config(&current);
        logging::set_file_logging_enabled(current.log_to_file);
    }
    info!("Configuration loaded from '{CONFIG_PATH}'.");
}

fn publish_keymap(conf: &SimpleIni) {
    let km = load_keymap_from_ini_local(conf);
    crate::engine::input::set_keymap(km);
}

fn load_defaults_after_error() {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = String::new();
}

fn apply_input_runtime_state() {
    let mut dedicated = get().only_dedicated_menu_buttons;
    if dedicated && !crate::engine::input::any_player_has_dedicated_menu_buttons() {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has dedicated menu buttons mapped — disabling."
        );
        dedicated = false;
        lock_config().only_dedicated_menu_buttons = false;
    }
    crate::engine::input::set_only_dedicated_menu_buttons(dedicated);
    crate::engine::input::set_input_debounce_seconds(get().input_debounce_seconds);
}
