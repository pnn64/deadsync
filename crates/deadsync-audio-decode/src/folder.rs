use std::path::{Path, PathBuf};

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
}
