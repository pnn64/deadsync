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

use deadlib_platform::dirs;
use deadsync_audio_decode::folder as audio_folder;
use log::{debug, warn};
use std::path::{Path, PathBuf};

/// Returns true when the folder feature is enabled in config.
#[inline(always)]
fn enabled() -> bool {
    deadsync_config::prelude::get().custom_sounds_enabled
}

/// Picks a random `.ogg` file from the directory referenced by `rel_dir`
/// (an `assets/`-relative path, e.g. `"assets/sounds/evaluation_pass"`).
/// Pure resolver: ignores the `custom_sounds_enabled` flag so the caller can
/// distinguish "no files" from "feature disabled". Returns `None` when the
/// directory is missing or contains no eligible `.ogg` files.
pub fn random_sfx_in(rel_dir: &str) -> Option<PathBuf> {
    audio_folder::random_sfx_path(rel_dir, |path| dirs::app_dirs().resolve_asset_path(path))
}

/// Same as [`random_sfx_in`] but takes a fully resolved directory.
pub fn pick_random_in(dir: &Path) -> Option<PathBuf> {
    audio_folder::pick_random_ogg(dir)
}

/// Picks an indexed `.ogg` file (`{index}.ogg`) from the directory referenced
/// by `rel_dir`, falling back to `fallback_name` (e.g. `"restart.ogg"`) when
/// the indexed file is missing. Returns `None` if neither exists.
pub fn indexed_sfx_in(rel_dir: &str, index: u32, fallback_name: &str) -> Option<PathBuf> {
    audio_folder::indexed_sfx_path(rel_dir, index, fallback_name, |path| {
        dirs::app_dirs().resolve_asset_path(path)
    })
}

/// Same as [`indexed_sfx_in`] but takes a fully resolved directory.
pub fn pick_indexed_in(dir: &Path, index: u32, fallback_name: &str) -> Option<PathBuf> {
    audio_folder::pick_indexed_ogg(dir, index, fallback_name)
}

/// Resolves one enabled custom sound without executing audio work.
pub fn random_sfx(rel_dir: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let path = random_sfx_in(rel_dir);
    if path.is_none() {
        debug!("No custom SFX picked for {rel_dir}");
    }
    path
}

/// Resolves one enabled indexed custom sound without executing audio work.
pub fn indexed_sfx(rel_dir: &str, index: u32, fallback_name: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let path = indexed_sfx_in(rel_dir, index, fallback_name);
    if path.is_none() {
        debug!("No custom SFX for {rel_dir} index {index} (fallback {fallback_name})");
    }
    path
}

/// Resolves a music path from a folder (or single file). If `rel_path` points
/// to a directory containing one or more eligible `.ogg` files, a random one
/// is returned; if it points to a file, that file is returned as-is;
/// otherwise returns `None`. Independent of `custom_sounds_enabled` because
/// it powers the per-visual-style menu music selection, not the SFX folder
/// feature.
pub fn random_music_path(rel_path: &str) -> Option<PathBuf> {
    match audio_folder::music_path_result(rel_path, |path| {
        dirs::app_dirs().resolve_asset_path(path)
    }) {
        audio_folder::MusicPathResult::Picked(path) => Some(path),
        audio_folder::MusicPathResult::EmptyDirectory(path) => {
            warn!(
                "Menu music folder {} is empty; falling back to no music",
                path.display()
            );
            None
        }
        audio_folder::MusicPathResult::Missing => None,
    }
}
