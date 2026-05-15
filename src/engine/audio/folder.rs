//! Folder-based random sound effect helpers.
//!
//! Mirrors the Simply Love / Zmod "drop ogg files in a folder, play a random
//! one" convention. The directory contents are listed once per resolved path
//! and cached for the life of the process. Files whose stem starts with an
//! underscore are excluded (matches the `_silent.redir` / theme override
//! convention used by SL/SM5).
//!
//! Resolution goes through [`crate::config::dirs::app_dirs`], so a user-supplied
//! `{data_dir}/assets/sounds/<folder>/...` overlay is automatically picked up
//! on top of the bundled `assets/` directory.

use crate::config::{self, dirs};
use log::{debug, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

static FOLDER_LISTINGS: OnceLock<Mutex<HashMap<PathBuf, Vec<PathBuf>>>> = OnceLock::new();

#[inline(always)]
fn listings() -> &'static Mutex<HashMap<PathBuf, Vec<PathBuf>>> {
    FOLDER_LISTINGS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline(always)]
fn is_ogg(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ogg"))
}

#[inline(always)]
fn is_skipped_stem(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.starts_with('_'))
}

fn list_ogg_files_uncached(dir: &Path) -> Vec<PathBuf> {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            // Missing directory is normal (the user hasn't dropped anything in
            // yet) so we log at debug, not warn.
            debug!("Custom sound dir unavailable {}: {e}", dir.display());
            return Vec::new();
        }
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_ogg(path) && !is_skipped_stem(path))
        .collect();
    out.sort();
    out
}

fn cached_listing(dir: &Path) -> Vec<PathBuf> {
    let key: PathBuf = dir.to_path_buf();
    {
        let map = listings().lock().unwrap();
        if let Some(v) = map.get(&key) {
            return v.clone();
        }
    }
    let files = list_ogg_files_uncached(dir);
    let mut map = listings().lock().unwrap();
    map.entry(key).or_insert(files).clone()
}

/// Invalidates the cached listing for a resolved directory. Tests use this to
/// avoid leakage between cases. Not currently exposed to the rest of the app
/// because the listing is assumed to be stable for the life of the process.
#[cfg(test)]
fn invalidate_cache(dir: &Path) {
    let mut map = listings().lock().unwrap();
    map.remove(dir);
}

#[inline(always)]
fn time_based_index(len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = nanos.wrapping_add(0x9E37_79B9_7F4A_7C15);
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    (state as usize) % len
}

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
    let listing = cached_listing(dir);
    if listing.is_empty() {
        return None;
    }
    let idx = time_based_index(listing.len());
    listing.get(idx).cloned()
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
    let indexed = dir.join(format!("{index}.ogg"));
    if indexed.is_file() {
        return Some(indexed);
    }
    let fallback = dir.join(fallback_name);
    if fallback.is_file() {
        return Some(fallback);
    }
    None
}

/// Plays a random `.ogg` from `rel_dir` via [`super::play_sfx`]. No-op when
/// the [`config::Config::custom_sounds_enabled`] flag is off or the folder
/// is empty.
pub fn play_random_sfx(rel_dir: &str) {
    if !enabled() {
        return;
    }
    if let Some(path) = random_sfx_in(rel_dir) {
        let path_str = path.to_string_lossy().into_owned();
        super::play_sfx(&path_str);
    } else {
        debug!("No custom SFX picked for {rel_dir}");
    }
}

/// Plays the indexed `.ogg` (or fallback) from `rel_dir` via [`super::play_sfx`].
/// No-op when [`config::Config::custom_sounds_enabled`] is off.
pub fn play_indexed_sfx(rel_dir: &str, index: u32, fallback_name: &str) {
    if !enabled() {
        return;
    }
    if let Some(path) = indexed_sfx_in(rel_dir, index, fallback_name) {
        let path_str = path.to_string_lossy().into_owned();
        super::play_sfx(&path_str);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TmpDir {
        path: PathBuf,
    }

    impl TmpDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "deadsync-folder-{label}-{nanos:x}-{n:x}-{}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write(path: &Path, name: &str) -> PathBuf {
        let p = path.join(name);
        fs::write(&p, b"").expect("write fixture");
        p
    }

    #[test]
    fn pick_random_in_returns_none_for_missing_dir() {
        let dir = TmpDir::new("missing");
        let missing = dir.path().join("does_not_exist");
        invalidate_cache(&missing);
        assert!(pick_random_in(&missing).is_none());
    }

    #[test]
    fn pick_random_in_returns_none_for_empty_dir() {
        let dir = TmpDir::new("empty");
        invalidate_cache(dir.path());
        assert!(pick_random_in(dir.path()).is_none());
    }

    #[test]
    fn pick_random_in_single_file_returns_it() {
        let dir = TmpDir::new("single");
        let only = write(dir.path(), "alpha.ogg");
        invalidate_cache(dir.path());
        let picked = pick_random_in(dir.path()).expect("pick");
        assert_eq!(picked, only);
    }

    #[test]
    fn pick_random_in_returns_one_of_listed_oggs() {
        let dir = TmpDir::new("multi");
        let a = write(dir.path(), "a.ogg");
        let b = write(dir.path(), "b.ogg");
        invalidate_cache(dir.path());
        let picked = pick_random_in(dir.path()).expect("pick");
        assert!(picked == a || picked == b, "{picked:?} not in fixture");
    }

    #[test]
    fn pick_random_in_ignores_non_ogg() {
        let dir = TmpDir::new("nonogg");
        write(dir.path(), "ignored.wav");
        write(dir.path(), "ignored.txt");
        let ogg = write(dir.path(), "kept.ogg");
        invalidate_cache(dir.path());
        let picked = pick_random_in(dir.path()).expect("pick");
        assert_eq!(picked, ogg);
    }

    #[test]
    fn pick_random_in_ignores_underscore_prefixed() {
        let dir = TmpDir::new("underscore");
        write(dir.path(), "_silent.ogg");
        let kept = write(dir.path(), "kept.ogg");
        invalidate_cache(dir.path());
        let picked = pick_random_in(dir.path()).expect("pick");
        assert_eq!(picked, kept);
    }

    #[test]
    fn pick_random_in_extension_check_is_case_insensitive() {
        let dir = TmpDir::new("case");
        let upper = write(dir.path(), "upper.OGG");
        invalidate_cache(dir.path());
        let picked = pick_random_in(dir.path()).expect("pick");
        assert_eq!(picked, upper);
    }

    #[test]
    fn pick_indexed_in_returns_indexed_when_present() {
        let dir = TmpDir::new("indexed");
        write(dir.path(), "1.ogg");
        write(dir.path(), "2.ogg");
        write(dir.path(), "restart.ogg");
        let picked = pick_indexed_in(dir.path(), 1, "restart.ogg").expect("pick");
        assert_eq!(picked, dir.path().join("1.ogg"));
        let picked = pick_indexed_in(dir.path(), 2, "restart.ogg").expect("pick");
        assert_eq!(picked, dir.path().join("2.ogg"));
    }

    #[test]
    fn pick_indexed_in_falls_back_when_index_missing() {
        let dir = TmpDir::new("fallback");
        write(dir.path(), "1.ogg");
        write(dir.path(), "restart.ogg");
        let picked = pick_indexed_in(dir.path(), 5, "restart.ogg").expect("pick");
        assert_eq!(picked, dir.path().join("restart.ogg"));
    }

    #[test]
    fn pick_indexed_in_none_when_nothing_matches() {
        let dir = TmpDir::new("none");
        write(dir.path(), "other.ogg");
        assert!(pick_indexed_in(dir.path(), 5, "restart.ogg").is_none());
    }

    #[test]
    fn cached_listing_reuses_first_result() {
        let dir = TmpDir::new("cache");
        write(dir.path(), "a.ogg");
        invalidate_cache(dir.path());

        let first = cached_listing(dir.path());
        // Add a new file after caching; the cached value should still be
        // returned because the listing is process-stable by design.
        write(dir.path(), "b.ogg");
        let second = cached_listing(dir.path());
        assert_eq!(first, second);
    }
}

