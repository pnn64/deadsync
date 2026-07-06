use crate::app_config::Config;
use crate::folders::AdditionalSongFolder;
use crate::runtime_state::RuntimeConfigStore;
use crate::save::build_default_app_config_file;
use deadlib_platform::coalesced_write::CoalescedFileWriter;
use deadlib_platform::dirs;
use deadsync_audio::AudioMixLevels;
use log::info;
use null_or_die::BiasCfg;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

pub static RUNTIME_CONFIG: LazyLock<RuntimeConfigStore> = LazyLock::new(RuntimeConfigStore::new);

static SAVE_WRITER: LazyLock<CoalescedFileWriter> = LazyLock::new(|| {
    CoalescedFileWriter::new("deadsync-config-save", dirs::app_dirs().config_path())
});

#[inline(always)]
pub fn queue_save_write(content: String) {
    SAVE_WRITER.write(content);
}

pub fn flush_pending_saves() {
    SAVE_WRITER.flush(Duration::from_secs(5));
}

pub fn current_save_content() -> String {
    let keymap = deadsync_input::get_keymap();
    RUNTIME_CONFIG.save_content(&keymap)
}

pub fn save_without_keymaps() {
    queue_save_write(current_save_content());
}

pub fn create_default_config_file() -> Result<(), std::io::Error> {
    let path = dirs::app_dirs().config_path();
    info!(
        "'{}' not found, creating with default values.",
        path.display()
    );
    std::fs::write(path, build_default_app_config_file())
}

pub fn get() -> Config {
    RUNTIME_CONFIG.config()
}

pub fn audio_mix_levels() -> AudioMixLevels {
    deadsync_audio::audio_mix_levels()
}

pub fn machine_default_noteskin() -> String {
    RUNTIME_CONFIG.machine_default_noteskin()
}

pub fn smx_pad_assignment() -> (Option<String>, Option<String>) {
    RUNTIME_CONFIG.smx_pad_assignment()
}

pub fn default_profiles() -> (Option<String>, Option<String>) {
    RUNTIME_CONFIG.default_profiles()
}

pub fn additional_song_folder_roots() -> Vec<AdditionalSongFolder> {
    RUNTIME_CONFIG.additional_song_folder_roots()
}

pub fn never_cache_list() -> Vec<String> {
    RUNTIME_CONFIG.never_cache_list()
}

pub fn group_is_never_cached(group: &str) -> bool {
    RUNTIME_CONFIG.group_is_never_cached(group)
}

pub fn song_path_is_writable(path: &Path) -> bool {
    RUNTIME_CONFIG.song_path_is_writable(path)
}

pub fn null_or_die_bias_cfg() -> BiasCfg {
    RUNTIME_CONFIG.null_or_die_bias_cfg()
}
