use super::*;
use deadlib_platform::dirs;
use std::fmt::Write as _;

#[path = "store/defaults.rs"]
mod defaults;
#[path = "store/save.rs"]
mod save;

pub(super) fn create_default_config_file() -> Result<(), std::io::Error> {
    let path = dirs::app_dirs().config_path();
    info!(
        "'{}' not found, creating with default values.",
        path.display()
    );
    std::fs::write(path, defaults::build_content())
}

pub(super) fn current_save_content() -> String {
    let cfg = *lock_config();
    let keymap = deadsync_input::get_keymap();
    let machine_default_noteskin = MACHINE_DEFAULT_NOTESKIN.lock().unwrap().clone();
    let additional_song_folders = ADDITIONAL_SONG_FOLDERS.lock().unwrap().clone();
    let never_cache_list = NEVER_CACHE_LIST.lock().unwrap().clone();
    let smx_p1_serial = SMX_P1_SERIAL.lock().unwrap().clone().unwrap_or_default();
    let smx_p2_serial = SMX_P2_SERIAL.lock().unwrap().clone().unwrap_or_default();
    let default_profile_p1 = DEFAULT_PROFILE_P1
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    let default_profile_p2 = DEFAULT_PROFILE_P2
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    save::build_content(
        &cfg,
        &keymap,
        &machine_default_noteskin,
        additional_song_folders.as_slice(),
        never_cache_list.as_slice(),
        &smx_p1_serial,
        &smx_p2_serial,
        &default_profile_p1,
        &default_profile_p2,
    )
}

pub(super) fn save_without_keymaps() {
    queue_save_write(current_save_content());
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
