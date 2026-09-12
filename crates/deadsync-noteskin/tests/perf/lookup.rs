use super::*;
use crate::perf::measure_sampled;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "lookup/baseline.rs"]
mod baseline;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(entries: usize) -> Self {
        let path = std::env::temp_dir().join(format!(
            "deadsync-noteskin-prep-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for i in 0..entries {
            fs::write(path.join(format!("unused-{i:04}.png")), []).unwrap();
        }
        fs::create_dir(path.join("zzSkinTarget")).unwrap();
        fs::create_dir(path.join("Down Tap Note directory.png")).unwrap();
        fs::write(path.join("Down Tap Note B.png"), []).unwrap();
        fs::write(path.join("Down Tap Note a.PNG"), []).unwrap();
        fs::write(path.join("Down Tap Note 0.redir"), []).unwrap();
        fs::write(path.join("ü Down Tap Note.png"), []).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        evict(&self.0);
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// In cold benchmarks this identical cache eviction is included on both sides.
// Only this fixture's cache entries are touched; parallel tests remain isolated.
fn evict(path: &Path) {
    let parent = path.to_string_lossy();
    if let Some(cache) = CHILD_DIR_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&IniKeyRef(&parent));
    }
    if let Some(caches) = FILE_PREFIX_CACHE.get() {
        for cache in caches {
            cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&IniKeyRef(&parent));
        }
    }
}

fn compare_lookup(path: &Path, name: &str, directory: bool, png_only: bool) -> Option<PathBuf> {
    evict(path);
    let old = if directory {
        baseline::find_child_dir_case_insensitive(path, name)
    } else {
        baseline::find_file_with_prefix(path, name, png_only)
    };
    evict(path);
    let new = if directory {
        find_child_dir_case_insensitive(path, name)
    } else {
        find_file_with_prefix(path, name, png_only)
    };
    assert_eq!(
        new, old,
        "name={name}, directory={directory}, png_only={png_only}"
    );
    new
}

#[test]
fn directory_lookups_match_old_case_prefix_type_and_miss_behavior() {
    let fixture = Fixture::new(127);
    for directory in [false, true] {
        for png_only in [false, true] {
            for name in [
                "",
                "zzskintarget",
                "ZZSKINTARGET",
                "Down Tap Note",
                "DOWN TAP NOTE a",
                "ü",
                "missing",
                "unused-0001.png",
            ] {
                compare_lookup(&fixture.0, name, directory, png_only);
            }
        }
    }
    assert_eq!(
        compare_lookup(&fixture.0, "Down Tap Note", false, true),
        Some(fixture.0.join("Down Tap Note a.PNG"))
    );
    assert_eq!(
        compare_lookup(&fixture.0, "Down Tap Note", false, false),
        Some(fixture.0.join("Down Tap Note 0.redir"))
    );
    compare_lookup(&fixture.0.join("absent"), "", false, false);
    compare_lookup(&fixture.0.join("unused-0001.png"), "", true, false);
}

#[test]
fn cached_lookups_keep_hits_and_misses_until_evicted() {
    let _guard = super::tests::LOOKUP_CACHE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let fixture = Fixture::new(0);
    assert!(find_child_dir_case_insensitive(&fixture.0, "Later").is_none());
    fs::create_dir(fixture.0.join("Later")).unwrap();
    assert!(find_child_dir_case_insensitive(&fixture.0, "Later").is_none());
    evict(&fixture.0);
    assert_eq!(
        find_child_dir_case_insensitive(&fixture.0, "Later"),
        Some(fixture.0.join("Later"))
    );
    let path = fixture.0.join("new.png");
    fs::write(&path, []).unwrap();
    assert_eq!(
        find_file_with_prefix(&fixture.0, "new", true),
        Some(path.clone())
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(find_file_with_prefix(&fixture.0, "new", true), Some(path));
    evict(&fixture.0);
    assert!(find_file_with_prefix(&fixture.0, "new", true).is_none());
}

#[test]
fn directory_links_and_file_links_follow_the_old_targets() {
    #[cfg(unix)]
    {
        let fixture = Fixture::new(0);
        std::os::unix::fs::symlink(fixture.0.join("zzSkinTarget"), fixture.0.join("linked"))
            .unwrap();
        std::os::unix::fs::symlink(
            fixture.0.join("Down Tap Note a.PNG"),
            fixture.0.join("linked.png"),
        )
        .unwrap();
        std::os::unix::fs::symlink(fixture.0.join("absent"), fixture.0.join("broken.png")).unwrap();
        assert!(compare_lookup(&fixture.0, "linked", true, false).is_some());
        assert!(compare_lookup(&fixture.0, "linked.png", false, true).is_some());
        assert!(compare_lookup(&fixture.0, "broken.png", false, true).is_none());
    }
    #[cfg(windows)]
    {
        // Windows junctions can be supplied without Developer Mode privileges.
        if let Some(path) = std::env::var_os("DEADSYNC_NOTESKIN_LINK_DIR") {
            let path = PathBuf::from(path);
            assert!(compare_lookup(&path, "linked", true, false).is_some());
            assert!(compare_lookup(&path, "broken", true, false).is_none());
            assert!(compare_lookup(&path, "linked", false, false).is_none());
        } else {
            eprintln!("junction fixture not supplied; junction checks skipped");
        }
        let fixture = Fixture::new(0);
        match std::os::windows::fs::symlink_file(
            fixture.0.join("Down Tap Note a.PNG"),
            fixture.0.join("linked.png"),
        ) {
            Ok(()) => {
                assert!(compare_lookup(&fixture.0, "linked.png", false, true).is_some());
            }
            Err(error) if error.raw_os_error() == Some(1314) => {
                eprintln!("file symlink check requires Windows symlink privilege")
            }
            Err(error) => panic!("symlink_file: {error}"),
        }
    }
}

#[test]
#[ignore = "manual release old/new benchmark"]
fn preparation_bench_lookup() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, entries, directory, query, png_only, cold) in [
        ("lookup_dir_small", 0, true, "zzskintarget", false, true),
        ("lookup_dir_128", 128, true, "zzskintarget", false, true),
        ("lookup_dir_1024", 1024, true, "missing", false, true),
        ("lookup_prefix_128", 128, false, "Down Tap Note", true, true),
        ("lookup_prefix_many", 128, false, "unused", true, true),
        ("lookup_prefix_miss", 128, false, "missing", true, true),
        ("lookup_dir_warm", 128, true, "zzskintarget", false, false),
        (
            "lookup_prefix_warm",
            128,
            false,
            "Down Tap Note",
            true,
            false,
        ),
    ] {
        let fixture = Fixture::new(entries);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("{name}_{}", if old { "old" } else { "new" });
            measure_sampled(&label, if cold { 8 } else { 4096 }, 1, || {
                if cold {
                    evict(&fixture.0);
                }
                if directory {
                    if old {
                        baseline::find_child_dir_case_insensitive(
                            black_box(&fixture.0),
                            black_box(query),
                        )
                    } else {
                        find_child_dir_case_insensitive(black_box(&fixture.0), black_box(query))
                    }
                } else if old {
                    baseline::find_file_with_prefix(
                        black_box(&fixture.0),
                        black_box(query),
                        png_only,
                    )
                } else {
                    find_file_with_prefix(black_box(&fixture.0), black_box(query), png_only)
                }
            });
        }
    }
}
