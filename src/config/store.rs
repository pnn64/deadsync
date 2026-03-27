use super::*;
use std::fmt::Write as _;

#[path = "store/defaults.rs"]
mod defaults;
#[path = "store/save.rs"]
mod save;

const DEFAULT_KEYMAP_LINES: [(&str, &str); 26] = [
    ("P1_Back", "KeyCode::Escape"),
    ("P1_Down", "KeyCode::ArrowDown,KeyCode::KeyS"),
    ("P1_Left", "KeyCode::ArrowLeft,KeyCode::KeyA"),
    ("P1_MenuDown", ""),
    ("P1_MenuLeft", ""),
    ("P1_MenuRight", ""),
    ("P1_MenuUp", ""),
    ("P1_Operator", ""),
    ("P1_Restart", ""),
    ("P1_Right", "KeyCode::ArrowRight,KeyCode::KeyD"),
    ("P1_Select", "KeyCode::Slash"),
    ("P1_Start", "KeyCode::Enter"),
    ("P1_Up", "KeyCode::ArrowUp,KeyCode::KeyW"),
    ("P2_Back", "KeyCode::Numpad0"),
    ("P2_Down", "KeyCode::Numpad2"),
    ("P2_Left", "KeyCode::Numpad4"),
    ("P2_MenuDown", ""),
    ("P2_MenuLeft", ""),
    ("P2_MenuRight", ""),
    ("P2_MenuUp", ""),
    ("P2_Operator", ""),
    ("P2_Restart", ""),
    ("P2_Right", "KeyCode::Numpad6"),
    ("P2_Select", "KeyCode::NumpadDecimal"),
    ("P2_Start", "KeyCode::NumpadEnter"),
    ("P2_Up", "KeyCode::Numpad8"),
];

pub(super) fn normalize_machine_default_noteskin(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_MACHINE_NOTESKIN.to_string();
    }
    trimmed.to_ascii_lowercase()
}

pub(super) fn create_default_config_file() -> Result<(), std::io::Error> {
    info!("'{CONFIG_PATH}' not found, creating with default values.");
    std::fs::write(CONFIG_PATH, defaults::build_content())
}

pub(super) fn save_without_keymaps() {
    let cfg = *lock_config();
    let keymap = crate::engine::input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();
    let additional_song_folders = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();
    queue_save_write(save::build_content(
        &cfg,
        &keymap,
        &machine_default_noteskin,
        &additional_song_folders,
    ));
}

fn push_section(content: &mut String, name: &str) {
    content.push_str(name);
    content.push('\n');
}

fn push_line(content: &mut String, key: &str, value: impl std::fmt::Display) {
    writeln!(content, "{key}={value}").expect("writing into String cannot fail");
}

fn push_bool(content: &mut String, key: &str, enabled: bool) {
    push_line(content, key, if enabled { 1 } else { 0 });
}
