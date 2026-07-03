//! Folder-based random sound effect helpers.
//!
//! Mirrors the Simply Love / Zmod "drop ogg files in a folder, play a random
//! one" convention. The directory contents are listed once per resolved path
//! and cached for the life of the process. Files whose stem starts with an
//! underscore are excluded (matches the `_silent.redir` / theme override
//! convention used by SL/SM5).
//!
//! Resolution goes through [`deadlib_platform::dirs::app_dirs`], so a user-supplied
//! `{data_dir}/assets/sounds/<folder>/...` overlay is automatically picked up
//! on top of the bundled `assets/` directory.

use crate::config;
use deadlib_platform::dirs;
use deadsync_audio_decode::folder as audio_folder;
use deadsync_audio_stream as audio;
use log::{debug, warn};
use std::path::{Path, PathBuf};

/// Returns true when the folder feature is enabled in config.
#[inline(always)]
fn enabled() -> bool {
    config::get().custom_sounds_enabled
}

/// Picks a random `.ogg` file from the directory referenced by `rel_dir`
/// (an `assets/`-relative path, e.g. `"assets/sounds/evaluation_pass"`).
/// Pure resolver: ignores the `custom_sounds_enabled` flag so the caller can
/// distinguish "no files" from "feature disabled". Returns `None` when the
/// directory is missing or contains no eligible `.ogg` files.
pub fn random_sfx_in(rel_dir: &str) -> Option<PathBuf> {
    pick_random_in(&dirs::app_dirs().resolve_asset_path(rel_dir))
}

/// Same as [`random_sfx_in`] but takes a fully resolved directory.
pub fn pick_random_in(dir: &Path) -> Option<PathBuf> {
    audio_folder::pick_random_ogg(dir)
}

/// Picks an indexed `.ogg` file (`{index}.ogg`) from the directory referenced
/// by `rel_dir`, falling back to `fallback_name` (e.g. `"restart.ogg"`) when
/// the indexed file is missing. Returns `None` if neither exists.
pub fn indexed_sfx_in(rel_dir: &str, index: u32, fallback_name: &str) -> Option<PathBuf> {
    let dir = dirs::app_dirs().resolve_asset_path(rel_dir);
    pick_indexed_in(&dir, index, fallback_name)
}

/// Same as [`indexed_sfx_in`] but takes a fully resolved directory.
pub fn pick_indexed_in(dir: &Path, index: u32, fallback_name: &str) -> Option<PathBuf> {
    audio_folder::pick_indexed_ogg(dir, index, fallback_name)
}

fn play_random_sfx_with(rel_dir: &str, play: fn(&str)) {
    if !enabled() {
        return;
    }
    if let Some(path) = random_sfx_in(rel_dir) {
        let path_str = path.to_string_lossy().into_owned();
        play(&path_str);
    } else {
        debug!("No custom SFX picked for {rel_dir}");
    }
}

/// Plays a random `.ogg` from `rel_dir` via [`audio::play_sfx`]. No-op when
/// the [`config::Config::custom_sounds_enabled`] flag is off or the folder
/// is empty.
pub fn play_random_sfx(rel_dir: &str) {
    play_random_sfx_with(rel_dir, audio::play_sfx);
}

/// Plays a random `.ogg` from `rel_dir` as screen-owned SFX.
pub fn play_random_screen_sfx(rel_dir: &str) {
    play_random_sfx_with(rel_dir, audio::play_screen_sfx);
}

/// Plays the indexed `.ogg` (or fallback) from `rel_dir` via [`audio::play_sfx`].
/// No-op when [`config::Config::custom_sounds_enabled`] is off.
pub fn play_indexed_sfx(rel_dir: &str, index: u32, fallback_name: &str) {
    if !enabled() {
        return;
    }
    if let Some(path) = indexed_sfx_in(rel_dir, index, fallback_name) {
        let path_str = path.to_string_lossy().into_owned();
        audio::play_sfx(&path_str);
    } else {
        debug!("No custom SFX for {rel_dir} index {index} (fallback {fallback_name})");
    }
}

/// Resolves a music path from a folder (or single file). If `rel_path` points
/// to a directory containing one or more eligible `.ogg` files, a random one
/// is returned; if it points to a file, that file is returned as-is;
/// otherwise returns `None`. Independent of `custom_sounds_enabled` because
/// it powers the per-visual-style menu music selection, not the SFX folder
/// feature.
pub fn random_music_path(rel_path: &str) -> Option<PathBuf> {
    let resolved = dirs::app_dirs().resolve_asset_path(rel_path);
    if resolved.is_dir() {
        let picked = pick_random_in(&resolved);
        if picked.is_none() {
            warn!(
                "Menu music folder {} is empty; falling back to no music",
                resolved.display()
            );
        }
        picked
    } else if resolved.is_file() {
        Some(resolved)
    } else {
        None
    }
}
