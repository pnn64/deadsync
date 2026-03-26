use super::*;

pub(super) fn bootstrap_log_to_file() -> bool {
    let mut conf = SimpleIni::new();
    let default = Config::default().log_to_file;
    if conf.load(CONFIG_PATH).is_err() {
        return default;
    }
    conf.get("Options", "LogToFile")
        .and_then(|v| parse_bool_str(&v))
        .unwrap_or(default)
}

pub(super) fn load_runtime_state(conf: &SimpleIni) {
    let noteskin = conf
        .get("Options", "DefaultNoteSkin")
        .map(|v| normalize_machine_default_noteskin(&v))
        .unwrap_or_else(|| DEFAULT_MACHINE_NOTESKIN.to_string());
    *MACHINE_DEFAULT_NOTESKIN.lock().unwrap() = noteskin;
    *ADDITIONAL_SONG_FOLDERS.lock().unwrap() = load_additional_song_folders(conf);
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
