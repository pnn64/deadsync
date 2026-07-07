use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

static OGG_LISTINGS: OnceLock<Mutex<HashMap<PathBuf, Vec<PathBuf>>>> = OnceLock::new();

#[inline(always)]
fn listings() -> &'static Mutex<HashMap<PathBuf, Vec<PathBuf>>> {
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

pub fn list_ogg_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| is_eligible_ogg(path))
        .collect();
    out.sort();
    Ok(out)
}

fn list_ogg_files_or_empty(dir: &Path) -> Vec<PathBuf> {
    list_ogg_files(dir).unwrap_or_default()
}

pub fn cached_ogg_listing(dir: &Path) -> Vec<PathBuf> {
    let key = dir.to_path_buf();
    {
        let map = listings().lock().unwrap();
        if let Some(files) = map.get(&key) {
            return files.clone();
        }
    }
    let files = list_ogg_files_or_empty(dir);
    let mut map = listings().lock().unwrap();
    map.entry(key).or_insert(files).clone()
}

/// Invalidates a cached listing. This is test-only because sound-folder
/// listings are process-stable by design in production.
#[cfg(test)]
fn invalidate_ogg_listing_cache(dir: &Path) {
    listings().lock().unwrap().remove(dir);
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

pub fn pick_random_ogg(dir: &Path) -> Option<PathBuf> {
    let listing = cached_ogg_listing(dir);
    if listing.is_empty() {
        return None;
    }
    listing.get(time_based_index(listing.len())).cloned()
}

pub fn pick_indexed_ogg(dir: &Path, index: u32, fallback_name: &str) -> Option<PathBuf> {
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

pub fn pick_music_path(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        pick_random_ogg(path)
    } else if path.is_file() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

pub fn random_sfx_path(
    rel_dir: &str,
    resolve_asset_path: impl FnOnce(&str) -> PathBuf,
) -> Option<PathBuf> {
    pick_random_ogg(&resolve_asset_path(rel_dir))
}

pub fn indexed_sfx_path(
    rel_dir: &str,
    index: u32,
    fallback_name: &str,
    resolve_asset_path: impl FnOnce(&str) -> PathBuf,
) -> Option<PathBuf> {
    let dir = resolve_asset_path(rel_dir);
    pick_indexed_ogg(&dir, index, fallback_name)
}

pub enum MusicPathResult {
    Picked(PathBuf),
    EmptyDirectory(PathBuf),
    Missing,
}

impl MusicPathResult {
    pub fn path(self) -> Option<PathBuf> {
        match self {
            Self::Picked(path) => Some(path),
            Self::EmptyDirectory(_) | Self::Missing => None,
        }
    }
}

pub fn music_path_result(
    rel_path: &str,
    resolve_asset_path: impl FnOnce(&str) -> PathBuf,
) -> MusicPathResult {
    let resolved = resolve_asset_path(rel_path);
    if resolved.is_dir() {
        return pick_music_path(&resolved)
            .map(MusicPathResult::Picked)
            .unwrap_or(MusicPathResult::EmptyDirectory(resolved));
    }
    pick_music_path(&resolved)
        .map(MusicPathResult::Picked)
        .unwrap_or(MusicPathResult::Missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TmpDir {
        path: PathBuf,
    }

    impl TmpDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!(
                "deadsync-audio-folder-{label}-{nanos:x}-{n:x}-{}",
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

        let picked = pick_indexed_ogg(dir.path(), 1, "restart.ogg").expect("pick");

        assert_eq!(picked, dir.path().join("1.ogg"));
    }

    #[test]
    fn pick_indexed_ogg_falls_back_when_index_missing() {
        let dir = TmpDir::new("fallback");
        write(dir.path(), "restart.ogg");

        let picked = pick_indexed_ogg(dir.path(), 5, "restart.ogg").expect("pick");

        assert_eq!(picked, dir.path().join("restart.ogg"));
    }

    #[test]
    fn pick_indexed_ogg_none_when_nothing_matches() {
        let dir = TmpDir::new("none");
        write(dir.path(), "other.ogg");

        assert!(pick_indexed_ogg(dir.path(), 5, "restart.ogg").is_none());
    }

    #[test]
    fn pick_random_ogg_returns_none_for_missing_dir() {
        let dir = TmpDir::new("missing");
        let missing = dir.path().join("does_not_exist");
        invalidate_ogg_listing_cache(&missing);

        assert!(pick_random_ogg(&missing).is_none());
    }

    #[test]
    fn pick_random_ogg_returns_none_for_empty_dir() {
        let dir = TmpDir::new("empty");
        invalidate_ogg_listing_cache(dir.path());

        assert!(pick_random_ogg(dir.path()).is_none());
    }

    #[test]
    fn pick_random_ogg_single_file_returns_it() {
        let dir = TmpDir::new("single");
        let only = write(dir.path(), "alpha.ogg");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(pick_random_ogg(dir.path()), Some(only));
    }

    #[test]
    fn pick_random_ogg_returns_one_of_listed_oggs() {
        let dir = TmpDir::new("multi");
        let a = write(dir.path(), "a.ogg");
        let b = write(dir.path(), "b.ogg");
        invalidate_ogg_listing_cache(dir.path());

        let picked = pick_random_ogg(dir.path()).expect("pick");
        assert!(picked == a || picked == b, "{picked:?} not in fixture");
    }

    #[test]
    fn pick_random_ogg_ignores_non_ogg() {
        let dir = TmpDir::new("nonogg");
        write(dir.path(), "ignored.wav");
        write(dir.path(), "ignored.txt");
        let ogg = write(dir.path(), "kept.ogg");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(pick_random_ogg(dir.path()), Some(ogg));
    }

    #[test]
    fn pick_random_ogg_ignores_underscore_prefixed() {
        let dir = TmpDir::new("underscore");
        write(dir.path(), "_silent.ogg");
        let kept = write(dir.path(), "kept.ogg");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(pick_random_ogg(dir.path()), Some(kept));
    }

    #[test]
    fn pick_random_ogg_extension_check_is_case_insensitive() {
        let dir = TmpDir::new("case");
        let upper = write(dir.path(), "upper.OGG");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(pick_random_ogg(dir.path()), Some(upper));
    }

    #[test]
    fn cached_ogg_listing_reuses_first_result() {
        let dir = TmpDir::new("cache");
        write(dir.path(), "a.ogg");
        invalidate_ogg_listing_cache(dir.path());

        let first = cached_ogg_listing(dir.path());
        write(dir.path(), "b.ogg");
        let second = cached_ogg_listing(dir.path());

        assert_eq!(first, second);
    }

    #[test]
    fn pick_music_path_uses_random_ogg_from_directory() {
        let dir = TmpDir::new("music-dir");
        let kept = write(dir.path(), "track.ogg");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(pick_music_path(dir.path()), Some(kept));
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

    #[test]
    fn random_sfx_path_resolves_relative_directory() {
        let dir = TmpDir::new("random-resolved");
        let kept = write(dir.path(), "picked.ogg");
        invalidate_ogg_listing_cache(dir.path());

        assert_eq!(
            random_sfx_path("assets/sounds/test", |_| dir.path().to_path_buf()),
            Some(kept)
        );
    }

    #[test]
    fn indexed_sfx_path_resolves_relative_directory() {
        let dir = TmpDir::new("indexed-resolved");
        let indexed = write(dir.path(), "2.ogg");

        assert_eq!(
            indexed_sfx_path("assets/sounds/test", 2, "fallback.ogg", |_| {
                dir.path().to_path_buf()
            }),
            Some(indexed)
        );
    }

    #[test]
    fn music_path_result_reports_empty_directory() {
        let dir = TmpDir::new("music-empty-result");
        invalidate_ogg_listing_cache(dir.path());

        match music_path_result("assets/music/menu/test", |_| dir.path().to_path_buf()) {
            MusicPathResult::EmptyDirectory(path) => assert_eq!(path, dir.path()),
            MusicPathResult::Picked(_) | MusicPathResult::Missing => {
                panic!("empty directory should be reported")
            }
        }
    }

    #[test]
    fn music_path_result_accepts_direct_file() {
        let dir = TmpDir::new("music-direct-result");
        let file = write(dir.path(), "loop.ogg");

        assert_eq!(
            music_path_result("assets/music/loop.ogg", |_| file.clone()).path(),
            Some(file)
        );
    }
}
