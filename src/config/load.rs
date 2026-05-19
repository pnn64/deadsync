use super::store::{create_default_config_file, current_save_content};
use super::*;

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
        logging::set_file_logging_enabled(current.log_to_file);
    }
    info!(
        "Configuration loaded from '{}'.",
        dirs::app_dirs().config_path().display()
    );
}

fn publish_keymap(conf: &SimpleIni) {
    let km = load_keymap_from_ini_local(conf);
    crate::engine::input::set_keymap(km);
}

fn load_defaults_after_error() {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = String::new();
}

fn load_runtime_state(conf: &SimpleIni) {
    let noteskin = conf
        .get("Options", "DefaultNoteSkin")
        .map(|v| normalize_machine_default_noteskin(&v))
        .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(conf);
}

fn apply_input_runtime_state() {
    let mut dedicated = get().only_dedicated_menu_buttons;
    let three_key_navigation = get().three_key_navigation;
    if dedicated
        && !crate::engine::input::any_player_has_dedicated_menu_buttons_for_mode(
            three_key_navigation,
        )
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
    crate::engine::input::set_only_dedicated_menu_buttons(dedicated);
    crate::engine::input::set_input_debounce_seconds(get().input_debounce_seconds);
}

pub(super) fn parse_bool_str(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn parse_loose_bool_str(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    parse_bool_str(raw).or_else(|| raw.parse::<u8>().ok().map(|n| n != 0))
}

fn normalize_additional_song_folders(raw: &str) -> String {
    let mut out = String::new();
    for path in raw
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(path);
    }
    out
}

fn load_additional_song_folders(conf: &SimpleIni) -> String {
    let read_only = conf
        .get("Options", "AdditionalSongFoldersReadOnly")
        .unwrap_or_default();
    let writable_raw = conf
        .get("Options", "AdditionalSongFoldersWritable")
        .unwrap_or_default();
    let deprecated = conf
        .get("Options", "AdditionalSongFolders")
        .unwrap_or_default();
    let writable = if writable_raw.trim().is_empty() {
        deprecated
    } else {
        writable_raw
    };

    if read_only.trim().is_empty() {
        return normalize_additional_song_folders(&writable);
    }
    if writable.trim().is_empty() {
        return normalize_additional_song_folders(&read_only);
    }

    let mut combined = String::with_capacity(read_only.len() + writable.len() + 1);
    combined.push_str(&read_only);
    combined.push(',');
    combined.push_str(&writable);
    normalize_additional_song_folders(&combined)
}
