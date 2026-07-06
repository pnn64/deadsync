use super::store::{create_default_config_file, current_save_content};
use super::*;
use deadlib_platform::dirs;
use deadsync_config::options::load_bool_option;
use deadsync_config::runtime_state::{RuntimeStateOptions, load_runtime_state_options};
use deadsync_config::update::{dedicated_menu_navigation_label, resolve_dedicated_menu_navigation};

#[path = "load/backfill.rs"]
mod backfill;
#[path = "load/options.rs"]
mod options;

pub fn bootstrap_log_to_file() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().log_to_file;
    if conf.load(dirs::app_dirs().config_path()).is_err() {
        return default;
    }
    load_bool_option(&conf, "Options", "LogToFile", default)
}

pub fn bootstrap_show_console() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().show_console;
    if conf.load(dirs::app_dirs().config_path()).is_err() {
        return default;
    }
    load_bool_option(&conf, "Options", "ShowConsole", default)
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
    publish_runtime_state(RuntimeStateOptions::default());
    deadsync_audio_stream::set_replaygain_enabled(Config::default().enable_replaygain);
    deadsync_audio_stream::set_preserve_pitch_enabled(Config::default().rate_mod_preserves_pitch);
    pad_order::reset();
}

fn load_runtime_state(conf: &SimpleIni) {
    publish_runtime_state(load_runtime_state_options(conf));
    pad_order::load_order_from_ini(conf);
}

fn publish_runtime_state(state: RuntimeStateOptions) {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = state.machine_default_noteskin;
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = state.additional_song_folders;
    *NEVER_CACHE_LIST.lock().unwrap() = state.never_cache_list;
    let ids = state.ids;
    *SMX_P1_SERIAL.lock().unwrap() = ids.smx_p1_serial;
    *SMX_P2_SERIAL.lock().unwrap() = ids.smx_p2_serial;
    *DEFAULT_PROFILE_P1.lock().unwrap() = ids.default_profile_p1;
    *DEFAULT_PROFILE_P2.lock().unwrap() = ids.default_profile_p2;
}

fn apply_input_runtime_state() {
    let cfg = get();
    let supported =
        deadsync_input::any_player_has_dedicated_menu_buttons_for_mode(cfg.three_key_navigation);
    let dedicated = resolve_dedicated_menu_navigation(cfg.only_dedicated_menu_buttons, supported);
    if dedicated.disabled_by_missing_bindings {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has the required dedicated menu buttons mapped for {} mode — disabling.",
            dedicated_menu_navigation_label(cfg.three_key_navigation)
        );
        lock_config().only_dedicated_menu_buttons = false;
    }
    deadsync_input::set_only_dedicated_menu_buttons(dedicated.enabled);
    deadsync_input::set_input_debounce_seconds(cfg.input_debounce_seconds);
}
