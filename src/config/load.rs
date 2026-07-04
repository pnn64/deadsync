use super::store::{create_default_config_file, current_save_content};
use super::*;
use deadlib_platform::dirs;
use deadsync_config::bools::{parse_bool_str, parse_loose_bool_str, parse_u8_bool_or_default};
use deadsync_config::cache::load_never_cache_list;
use deadsync_config::folders::load_additional_song_folders;
use deadsync_config::runtime_state::load_runtime_state_ids;

#[path = "load/backfill.rs"]
mod backfill;
#[path = "load/options.rs"]
mod options;
#[path = "load/theme.rs"]
mod theme_load;

pub fn bootstrap_log_to_file() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().log_to_file;
    if conf.load(dirs::app_dirs().config_path()).is_err() {
        return default;
    }
    conf.get("Options", "LogToFile")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

pub fn bootstrap_show_console() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().show_console;
    if conf.load(dirs::app_dirs().config_path()).is_err() {
        return default;
    }
    conf.get("Options", "ShowConsole")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

pub fn load() {
    ensure_config_file();

    let mut conf = SimpleIni::new();
    let path = dirs::app_dirs().config_path();
    match conf.load(&path) {
        Ok(()) => load_from_ini(&conf),
        Err(e) => {
            warn!(
                "Failed to load '{}': {e}. Using default values.",
                path.display()
            );
            load_defaults_after_error();
        }
    }

    apply_input_runtime_state();
}

fn ensure_config_file() {
    if !dirs::app_dirs().config_path().exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }
}

fn load_from_ini(conf: &SimpleIni) {
    load_runtime_state(conf);

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
        deadsync_audio_stream::set_replaygain_enabled(current.enable_replaygain);
        deadsync_audio_stream::set_preserve_pitch_enabled(current.rate_mod_preserves_pitch);
        deadlib_present::space::set_overscan(
            current.center_image_translate_x,
            current.center_image_translate_y,
            current.center_image_add_width,
            current.center_image_add_height,
        );
        logging::set_file_logging_enabled(current.log_to_file);
    }
    info!(
        "Configuration loaded from '{}'.",
        dirs::app_dirs().config_path().display()
    );
}

fn publish_keymap(conf: &SimpleIni) {
    let km = load_keymap_from_ini_local(conf);
    deadsync_input::set_keymap(km);
}

fn load_defaults_after_error() {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
    ADDITIONAL_SONG_FOLDERS.lock().unwrap().clear();
    NEVER_CACHE_LIST.lock().unwrap().clear();
    *SMX_P1_SERIAL.lock().unwrap() = None;
    *SMX_P2_SERIAL.lock().unwrap() = None;
    *DEFAULT_PROFILE_P1.lock().unwrap() = None;
    *DEFAULT_PROFILE_P2.lock().unwrap() = None;
    deadsync_audio_stream::set_replaygain_enabled(Config::default().enable_replaygain);
    deadsync_audio_stream::set_preserve_pitch_enabled(Config::default().rate_mod_preserves_pitch);
    pad_order::reset();
}

fn load_runtime_state(conf: &SimpleIni) {
    let noteskin = conf
        .get("Options", "DefaultNoteSkin")
        .map(|v| normalize_machine_default_noteskin(&v))
        .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(conf);
    *NEVER_CACHE_LIST.lock().unwrap() = load_never_cache_list(conf);
    let ids = load_runtime_state_ids(conf);
    *SMX_P1_SERIAL.lock().unwrap() = ids.smx_p1_serial;
    *SMX_P2_SERIAL.lock().unwrap() = ids.smx_p2_serial;
    *DEFAULT_PROFILE_P1.lock().unwrap() = ids.default_profile_p1;
    *DEFAULT_PROFILE_P2.lock().unwrap() = ids.default_profile_p2;
    pad_order::load_order_from_ini(conf);
}

fn apply_input_runtime_state() {
    let mut dedicated = get().only_dedicated_menu_buttons;
    let three_key_navigation = get().three_key_navigation;
    if dedicated
        && !deadsync_input::any_player_has_dedicated_menu_buttons_for_mode(three_key_navigation)
    {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has the required dedicated menu buttons mapped for {} mode — disabling.",
            if three_key_navigation {
                "Three Key Menu"
            } else {
                "Five Key Menu"
            }
        );
        dedicated = false;
        lock_config().only_dedicated_menu_buttons = false;
    }
    deadsync_input::set_only_dedicated_menu_buttons(dedicated);
    deadsync_input::set_input_debounce_seconds(get().input_debounce_seconds);
}
