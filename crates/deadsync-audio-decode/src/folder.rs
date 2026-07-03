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
}
