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
        crate::engine::space::set_overscan(
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
    crate::engine::input::set_keymap(km);
}

fn load_defaults_after_error() {
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = DEFAULT_MACHINE_NOTESKIN.to_string();
    ADDITIONAL_SONG_FOLDERS.lock().unwrap().clear();
    *SMX_P1_SERIAL.lock().unwrap() = None;
    *SMX_P2_SERIAL.lock().unwrap() = None;
    pad_order::reset();
}

fn load_runtime_state(conf: &SimpleIni) {
    let noteskin = conf
        .get("Options", "DefaultNoteSkin")
        .map(|v| normalize_machine_default_noteskin(&v))
        .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(conf);
    // SMX pad assignment serials: missing/blank means "no assignment" (jumper).
    let serial = |key| {
        conf.get("Options", key)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    *SMX_P1_SERIAL.lock().unwrap() = serial("SmxP1Serial");
    *SMX_P2_SERIAL.lock().unwrap() = serial("SmxP2Serial");
    pad_order::load_order_from_ini(conf);
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

fn push_additional_song_folders(raw: &str, writable: bool, out: &mut Vec<AdditionalSongFolder>) {
    out.extend(
        raw.split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| AdditionalSongFolder {
                path: path.to_string(),
                writable,
            }),
    );
}

fn load_additional_song_folders(conf: &SimpleIni) -> Vec<AdditionalSongFolder> {
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

    let mut folders = Vec::new();
    push_additional_song_folders(&read_only, false, &mut folders);
    push_additional_song_folders(&writable, true, &mut folders);
    folders
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

    fn folder(path: &str, writable: bool) -> AdditionalSongFolder {
        AdditionalSongFolder {
            path: path.to_string(),
            writable,
        }
    }

    #[test]
    fn additional_song_folders_keeps_read_only_when_deprecated_key_empty() {
        let conf = ini("[Options]\n\
AdditionalSongFolders=\n\
AdditionalSongFoldersReadOnly=G:\\itgmania\\songs\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("G:\\itgmania\\songs", false)]
        );
    }

    #[test]
    fn additional_song_folders_migrates_deprecated_key_to_writable() {
        let conf = ini("[Options]\nAdditionalSongFolders=D:\\songs\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("D:\\songs", true)]
        );
    }

    #[test]
    fn additional_song_folders_prefers_writable_key_over_deprecated_key() {
        let conf = ini("[Options]\n\
AdditionalSongFolders=D:\\old\n\
AdditionalSongFoldersWritable=D:\\new\n\
AdditionalSongFoldersReadOnly=G:\\readonly\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("G:\\readonly", false), folder("D:\\new", true),]
        );
    }

    #[test]
    fn additional_song_folders_trims_empty_entries() {
        let conf = ini("[Options]\n\
AdditionalSongFoldersWritable= D:\\a ,, D:\\b \n\
AdditionalSongFoldersReadOnly= , G:\\ro , \n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![
                folder("G:\\ro", false),
                folder("D:\\a", true),
                folder("D:\\b", true),
            ]
        );
    }
}
