//! Folder-based random sound effect helpers.
//!
//! Mirrors the Simply Love / Zmod "drop ogg files in a folder, play a random
//! one" convention. The directory contents are listed once per resolved path
//! and cached for the life of the process. Files whose stem starts with an
//! underscore are excluded (matches the `_silent.redir` / theme override
//! convention used by SL/SM5).
//!
//! Resolution goes through [`crate::paths`], so a user-supplied
//! `{data_dir}/assets/sounds/<folder>/...` overlay is automatically picked up
//! on top of the bundled `assets/` directory.

use log::{debug, warn};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::SystemTime,
};

/// Returns true when the folder feature is enabled in config.
#[inline(always)]
fn enabled() -> bool {
    deadsync_config::prelude::get().custom_sounds_enabled
}

/// Resolves an enabled custom sound from eligible, non-underscore `.ogg` files.
#[must_use]
pub fn random_sfx(rel_dir: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let path = pick_random_in(&crate::paths().resolve_asset_path(rel_dir));
    if path.is_none() {
        debug!("No custom SFX picked for {rel_dir}");
    }
    path
}

/// Resolves an enabled `{index}.ogg` sound, using `fallback_name` when absent.
#[must_use]
pub fn indexed_sfx(rel_dir: &str, index: u32, fallback_name: &str) -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let path = pick_indexed_in(
        &crate::paths().resolve_asset_path(rel_dir),
        index,
        fallback_name,
    );
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
#[must_use]
pub fn random_music_path(rel_path: &str) -> Option<PathBuf> {
    pick_music_path(&crate::paths().resolve_asset_path(rel_path))
}

type SharedOggListing = Arc<Vec<PathBuf>>;

// Application-thread, session-lifetime listings shared behind a mutex. Each
// resolved sound directory is scanned on first selection at a screen transition;
// hits clone only the Arc. Entries are retained without pruning for the session,
// so playback of already prepared sounds never scans or destroys these listings.
static OGG_LISTINGS: OnceLock<Mutex<HashMap<PathBuf, SharedOggListing>>> = OnceLock::new();

#[inline(always)]
fn listings() -> &'static Mutex<HashMap<PathBuf, SharedOggListing>> {
    OGG_LISTINGS.get_or_init(|| Mutex::new(HashMap::new()))
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

#[inline(always)]
fn is_eligible_ogg(path: &Path) -> bool {
    path.is_file() && is_ogg(path) && !is_skipped_stem(path)
}

fn list_ogg_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_eligible_ogg(path))
        .collect();
    out.sort();
    Ok(out)
}

fn cached_ogg_listing_shared(dir: &Path) -> SharedOggListing {
    {
        let map = listings()
            .lock()
            .expect("sound-folder listing cache poisoned");
        if let Some(files) = map.get(dir) {
            return Arc::clone(files);
        }
    }
    let files = Arc::new(list_ogg_files(dir).unwrap_or_default());
    let mut map = listings()
        .lock()
        .expect("sound-folder listing cache poisoned");
    Arc::clone(map.entry(dir.to_path_buf()).or_insert(files))
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

#[must_use]
fn pick_random_in(dir: &Path) -> Option<PathBuf> {
    let listing = cached_ogg_listing_shared(dir);
    if listing.is_empty() {
        return None;
    }
    listing.get(time_based_index(listing.len())).cloned()
}

#[must_use]
fn pick_indexed_in(dir: &Path, index: u32, fallback_name: &str) -> Option<PathBuf> {
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

#[must_use]
fn pick_music_path(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        let picked = pick_random_in(path);
        if picked.is_none() {
            warn!(
                "Menu music folder {} is empty; falling back to no music",
                path.display()
            );
        }
        picked
    } else if path.is_file() {
        Some(path.to_path_buf())
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
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "deadsync-audio-folder-{label}-{n:x}-{}",
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
    fn list_ogg_files_returns_sorted_eligible_files() {
        let dir = TmpDir::new("sorted");
        let b = write(dir.path(), "b.ogg");
        let a = write(dir.path(), "a.ogg");
        write(dir.path(), "_silent.ogg");
        write(dir.path(), "ignored.wav");
        fs::create_dir(dir.path().join("folder.ogg")).expect("create excluded directory");

        let files = list_ogg_files(dir.path()).expect("list");

        assert_eq!(files, [a, b]);
    }

    #[test]
    fn list_ogg_files_extension_check_is_case_insensitive() {
        let dir = TmpDir::new("case");
        let upper = write(dir.path(), "upper.OGG");

        let files = list_ogg_files(dir.path()).expect("list");

        assert_eq!(files, [upper]);
    }

    #[test]
    fn list_ogg_files_errors_for_missing_dir() {
        let dir = TmpDir::new("missing");
        let missing = dir.path().join("does_not_exist");

        assert!(list_ogg_files(&missing).is_err());
    }

    #[test]
    fn pick_indexed_ogg_returns_indexed_when_present() {
        let dir = TmpDir::new("indexed");
        write(dir.path(), "1.ogg");
        write(dir.path(), "restart.ogg");

        let picked = pick_indexed_in(dir.path(), 1, "restart.ogg").expect("pick");

        assert_eq!(picked, dir.path().join("1.ogg"));
    }

    #[test]
    fn pick_indexed_ogg_falls_back_when_index_missing() {
        let dir = TmpDir::new("fallback");
        write(dir.path(), "restart.ogg");

        let picked = pick_indexed_in(dir.path(), 5, "restart.ogg").expect("pick");

        assert_eq!(picked, dir.path().join("restart.ogg"));
    }

    #[test]
    fn pick_indexed_ogg_none_when_nothing_matches() {
        let dir = TmpDir::new("none");
        write(dir.path(), "other.ogg");

        assert!(pick_indexed_in(dir.path(), 5, "restart.ogg").is_none());
    }

    #[test]
    fn pick_random_ogg_returns_none_for_missing_dir() {
        let dir = TmpDir::new("missing");
        let missing = dir.path().join("does_not_exist");

        assert!(pick_random_in(&missing).is_none());
    }

    #[test]
    fn pick_random_ogg_returns_none_for_empty_dir() {
        let dir = TmpDir::new("empty");

        assert!(pick_random_in(dir.path()).is_none());
    }

    #[test]
    fn pick_random_ogg_single_file_returns_it() {
        let dir = TmpDir::new("single");
        let only = write(dir.path(), "alpha.ogg");

        assert_eq!(pick_random_in(dir.path()), Some(only));
    }

    #[test]
    fn pick_random_ogg_ignores_non_ogg() {
        let dir = TmpDir::new("nonogg");
        write(dir.path(), "ignored.wav");
        write(dir.path(), "ignored.txt");
        let ogg = write(dir.path(), "kept.ogg");

        assert_eq!(pick_random_in(dir.path()), Some(ogg));
    }

    #[test]
    fn pick_random_ogg_ignores_underscore_prefixed() {
        let dir = TmpDir::new("underscore");
        write(dir.path(), "_silent.ogg");
        let kept = write(dir.path(), "kept.ogg");

        assert_eq!(pick_random_in(dir.path()), Some(kept));
    }

    #[test]
    fn pick_random_ogg_extension_check_is_case_insensitive() {
        let dir = TmpDir::new("case");
        let upper = write(dir.path(), "upper.OGG");

        assert_eq!(pick_random_in(dir.path()), Some(upper));
    }

    #[test]
    fn shared_cached_ogg_listing_reuses_first_allocation() {
        let dir = TmpDir::new("shared-cache");
        write(dir.path(), "a.ogg");

        let first = cached_ogg_listing_shared(dir.path());
        write(dir.path(), "b.ogg");
        let second = cached_ogg_listing_shared(dir.path());

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.as_slice(), [dir.path().join("a.ogg")]);
    }

    #[test]
    fn pick_music_path_uses_random_ogg_from_directory() {
        let dir = TmpDir::new("music-dir");
        let kept = write(dir.path(), "track.ogg");

        assert_eq!(pick_music_path(dir.path()), Some(kept));
    }

    #[test]
    fn pick_music_path_returns_none_for_empty_directory() {
        let dir = TmpDir::new("music-empty");
        assert_eq!(pick_music_path(dir.path()), None);
    }

    #[test]
    fn pick_music_path_accepts_direct_file() {
        let dir = TmpDir::new("music-file");
        let file = write(dir.path(), "loop.ogg");

        assert_eq!(pick_music_path(&file), Some(file));
    }

    #[test]
    fn pick_music_path_returns_none_for_missing_path() {
        let dir = TmpDir::new("music-missing");

        assert_eq!(pick_music_path(&dir.path().join("missing.ogg")), None);
    }
}
