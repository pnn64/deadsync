use crate::app_config::Config;
use crate::folders::AdditionalSongFolder;
use crate::runtime_state::{InputRoutingConfig, RuntimeConfigStore};
use crate::save::build_default_app_config_file;
use deadlib_platform::coalesced_write::CoalescedFileWriter;
use log::info;
use null_or_die::BiasCfg;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};
use std::time::Duration;

struct ConfigPaths {
    config: PathBuf,
    palettes: PathBuf,
}
static PATHS: OnceLock<ConfigPaths> = OnceLock::new();

/// Install persistence paths before loading settings or accepting save requests.
pub fn init_paths(config: PathBuf, palettes: PathBuf) -> Result<(), &'static str> {
    PATHS
        .set(ConfigPaths { config, palettes })
        .map_err(|_| "config paths already initialized")
}

pub(crate) fn config_path() -> &'static Path {
    &PATHS
        .get()
        .expect("config paths initialized at startup")
        .config
}

pub(crate) fn palette_path() -> &'static Path {
    &PATHS
        .get()
        .expect("config paths initialized at startup")
        .palettes
}

pub static RUNTIME_CONFIG: LazyLock<RuntimeConfigStore> = LazyLock::new(RuntimeConfigStore::new);

static SAVE_WRITER: LazyLock<CoalescedFileWriter> =
    LazyLock::new(|| CoalescedFileWriter::new("deadsync-config-save", config_path().to_path_buf()));

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
    let path = config_path();
    info!(
        "'{}' not found, creating with default values.",
        path.display()
    );
    let content = build_default_app_config_file();
    deadlib_platform::atomic_write::write_atomic(path, content.as_bytes())
}

pub fn get() -> Config {
    RUNTIME_CONFIG.config()
}

pub fn snapshot() -> (u64, Config) {
    RUNTIME_CONFIG.config_snapshot()
}

#[inline(always)]
pub fn snapshot_if_changed(generation: u64) -> Option<(u64, Config)> {
    RUNTIME_CONFIG.config_if_changed(generation)
}

#[inline(always)]
pub fn input_routing_config() -> InputRoutingConfig {
    RUNTIME_CONFIG.input_routing_config()
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
