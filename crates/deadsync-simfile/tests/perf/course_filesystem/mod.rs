use super::*;
use crate::perf;
use std::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering},
};
mod baseline;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Tree {
    path: PathBuf,
    root: PathBuf,
}
impl Tree {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "course-filesystem-{:010}-{:010}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, root }
    }
    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        // Only the absolute workspace child owned by this fixture is removed.
        assert_eq!(self.path.parent(), Some(self.root.as_path()));
        assert!(
            self.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("course-filesystem-")
        );
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn write(path: impl AsRef<Path>) {
    fs::write(path, b"#TITLE:Fixture;").unwrap();
}
fn scan_tree(courses: usize, junk: usize, dirs: usize) -> Tree {
    let tree = Tree::new();
    let folders: Vec<_> = (0..dirs)
        .map(|i| tree.dir(&format!("folder-{i:04}")))
        .collect();
    for i in 0..courses + junk {
        let folder = if folders.is_empty() {
            &tree.path
        } else {
            &folders[i % folders.len()]
        };
        write(folder.join(format!(
            "entry-{i:04}.{}",
            if i < courses {
                if i % 2 == 0 { "crs" } else { "CRS" }
            } else {
                "txt"
            }
        )));
    }
    tree
}
fn name_tree(dirs: usize, files: usize) -> Tree {
    let tree = Tree::new();
    for i in 0..dirs {
        tree.dir(&format!("Folder-{i:04}"));
    }
    for i in 0..files {
        write(tree.path.join(format!("file-{i:04}")));
    }
    tree
}
fn make_song(pack: &Path, song: &str) -> PathBuf {
    let dir = pack.join(song);
    fs::create_dir_all(&dir).unwrap();
    write(dir.join("song.sm"));
    dir
}
fn packs_tree(count: usize, placement: &str) -> Tree {
    let tree = Tree::new();
    for i in 0..count {
        make_song(&tree.dir(&format!("Pack-{i:04}")), "Other");
    }
    if count != 0 && placement != "missing" {
        let entries: Vec<_> = fs::read_dir(&tree.path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let pack = if placement == "first" {
            &entries[0]
        } else {
            entries.last().unwrap()
        };
        make_song(pack, "Needle");
    }
    tree
}
fn assert_resolve(roots: &[PathBuf], group: Option<&str>, song: &str) {
    let mut old_cache = HashMap::new();
    let mut new_cache = HashMap::new();
    for _ in 0..2 {
        assert_eq!(
            baseline::resolve_song_dir(roots, &mut old_cache, group, song),
            resolve_song_dir(roots, &mut new_cache, group, song),
            "{group:?}/{song:?}"
        );
        assert_eq!(old_cache, new_cache);
    }
}

#[test]
fn course_scan_preserves_paths_case_sorting_depth_and_missing_roots() {
    for (courses, junk, dirs) in [(0, 0, 0), (1, 0, 0), (128, 0, 0), (64, 128, 8)] {
        let tree = scan_tree(courses, junk, dirs);
        for name in [
            "a.crs",
            "Z.CRS",
            "not.crs.txt",
            ".crs",
            "song\u{130}.crs",
            "\u{1f3b5}.CRS",
        ] {
            write(tree.path.join(name));
        }
        write(tree.dir("directory.crs/deep/nested").join("inside.CrS"));
        assert_eq!(
            baseline::collect_course_paths(&tree.path),
            collect_course_paths(&tree.path)
        );
        assert!(!collect_course_paths(&tree.path).contains(&tree.path.join("directory.crs")));
        assert_eq!(
            baseline::collect_course_paths(&tree.path.join("absent")),
            collect_course_paths(&tree.path.join("absent"))
        );
        assert_eq!(
            baseline::collect_course_paths(&tree.path.join("a.crs")),
            collect_course_paths(&tree.path.join("a.crs"))
        );
    }
}
#[test]
fn directory_lookup_preserves_exact_preference_ascii_folding_and_file_rejection() {
    let tree = name_tree(128, 128);
    for name in ["MiXeD", "MiXeD\u{130}", "\u{c9}COLE", "\u{1f3b5}"] {
        tree.dir(name);
    }
    for query in [
        "",
        "  \u{2003}",
        "Folder-0000",
        "folder-0127",
        "missing",
        "MiXeD",
        "mixed",
        " MIXED ",
        "\u{2003}mixed\u{a0}",
        "mixed\u{130}",
        "mixedi",
        "\u{e9}cole",
        "\u{c9}cole",
        "\u{1f3b5}",
        "file-0000",
        "FILE-0127",
    ] {
        assert_eq!(
            baseline::is_dir_ci(&tree.path, query),
            is_dir_ci(&tree.path, query),
            "{query:?}"
        );
    }
    assert_eq!(is_dir_ci(&tree.path, "file-0000"), None);
    assert_eq!(is_dir_ci(&tree.path, "\u{e9}cole"), None);
    assert_eq!(
        is_dir_ci(&tree.path, "\u{c9}cole"),
        Some(tree.path.join("\u{c9}COLE"))
    );
    // Case-sensitive filesystems can hold multiple case variants. Case-insensitive
    // filesystems resolve all three spellings to the same existing directory.
    for name in ["casechoice", "CaseChoice", "CASECHOICE"] {
        tree.dir(name);
    }
    for query in ["casechoice", "CaseChoice", "CASECHOICE", "cAsEcHoIcE"] {
        assert_eq!(
            baseline::is_dir_ci(&tree.path, query),
            is_dir_ci(&tree.path, query)
        );
        if fs::read_dir(&tree.path)
            .unwrap()
            .any(|e| e.unwrap().file_name() == query)
        {
            assert_eq!(is_dir_ci(&tree.path, query), Some(tree.path.join(query)));
        }
    }
    perf::assert_reduced_churn(
        || {
            black_box(baseline::is_dir_ci(&tree.path, "missing"));
        },
        || {
            black_box(is_dir_ci(&tree.path, "missing"));
        },
    );
    perf::assert_no_churn(|| {
        black_box(is_dir_ci(black_box(&tree.path), black_box(" \u{2003} ")));
    });
}
#[test]
fn resolution_preserves_root_pack_series_precedence_and_cache_contents() {
    let tree = Tree::new();
    let base = tree.dir("base");
    let extra = tree.dir("extra");
    let base_song = make_song(&tree.dir("base/Direct"), "Needle");
    let extra_song = make_song(&tree.dir("extra/Direct"), "Needle");
    make_song(&tree.dir("extra/Series/Nested"), "Needle");
    make_song(&tree.dir("extra/Series/Nested"), "NestedOnly");
    make_song(&tree.dir("base/Series/Direct"), "Decoy");
    write(tree.path.join("not-a-root"));
    let roots = vec![
        tree.path.join("absent"),
        tree.path.join("not-a-root"),
        base.clone(),
        extra.clone(),
    ];
    for group in [
        None,
        Some(""),
        Some(" \u{2003}"),
        Some("Direct"),
        Some(" direct "),
        Some("Nested"),
        Some("missing"),
        Some("Series"),
    ] {
        for song in [
            "",
            "  ",
            "Needle",
            "needle",
            " Needle ",
            "NestedOnly",
            "Other",
            "missing",
            "Nested",
        ] {
            assert_resolve(&roots, group, song);
        }
    }
    assert_eq!(
        resolve_song_dir(&roots, &mut HashMap::new(), None, "Needle"),
        Some(extra_song)
    );
    assert_eq!(
        resolve_song_dir(&[extra, base], &mut HashMap::new(), None, "Needle"),
        Some(base_song)
    );
    // Direct packs must beat series even when the series is enumerated first.
    let first_series = Tree::new();
    make_song(&first_series.dir("000-Series/Nested"), "Needle");
    let direct = make_song(&first_series.dir("999-Direct"), "Needle");
    assert_resolve(std::slice::from_ref(&first_series.path), None, "Needle");
    assert_eq!(
        resolve_song_dir(
            std::slice::from_ref(&first_series.path),
            &mut HashMap::new(),
            None,
            "Needle"
        ),
        Some(direct)
    );
    let early = packs_tree(32, "first");
    perf::assert_reduced_churn(
        || {
            black_box(baseline::resolve_song_dir(
                std::slice::from_ref(&early.path),
                &mut HashMap::new(),
                None,
                "Needle",
            ));
        },
        || {
            black_box(resolve_song_dir(
                std::slice::from_ref(&early.path),
                &mut HashMap::new(),
                None,
                "Needle",
            ));
        },
    );
    perf::assert_no_churn(|| {
        black_box(resolve_song_dir(&[], &mut HashMap::new(), None, " "));
    });
}
#[test]
fn lossy_directory_names_match_the_baseline() {
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[b'N' as u16, 0xd800, b'M' as u16])
    };
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![b'N', 0xff, b'M'])
    };
    #[cfg(any(windows, unix))]
    {
        let tree = Tree::new();
        fs::create_dir(tree.path.join(&name)).unwrap();
        for query in ["N\u{fffd}M", "n\u{fffd}m", "missing"] {
            assert_eq!(
                baseline::is_dir_ci(&tree.path, query),
                is_dir_ci(&tree.path, query)
            );
        }
    }
}
#[cfg(any(windows, unix))]
#[test]
fn links_follow_directory_targets_and_keep_broken_course_paths() {
    #[cfg(unix)]
    use std::os::unix::fs::{symlink as symlink_dir, symlink as symlink_file};
    #[cfg(windows)]
    use std::os::windows::fs::{symlink_dir, symlink_file};
    let source = Tree::new();
    let tree = Tree::new();
    let song = make_song(&source.dir("Pack"), "Needle");
    write(source.path.join("real.crs"));
    if let Err(error) = symlink_dir(source.path.join("Pack"), tree.path.join("PackLink")) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || (cfg!(windows) && error.raw_os_error() == Some(1314))
        {
            eprintln!("symlink regression not exercised: {error}");
            return;
        }
        panic!("{error}");
    }
    symlink_dir(&song, tree.path.join("SongLink.crs")).unwrap();
    symlink_file(
        source.path.join("real.crs"),
        tree.path.join("file-link.CRS"),
    )
    .unwrap();
    symlink_file(source.path.join("absent"), tree.path.join("broken.crs")).unwrap();
    symlink_dir(
        source.path.join("absent-dir"),
        tree.path.join("BrokenDir.crs"),
    )
    .unwrap();
    assert_eq!(
        baseline::collect_course_paths(&tree.path),
        collect_course_paths(&tree.path)
    );
    for query in [
        "PackLink",
        "packlink",
        "SongLink.crs",
        "broken.crs",
        "BrokenDir.crs",
        "file-link.CRS",
    ] {
        assert_eq!(
            baseline::is_dir_ci(&tree.path, query),
            is_dir_ci(&tree.path, query)
        );
    }
    assert_resolve(std::slice::from_ref(&tree.path), None, "Needle");
    assert_resolve(std::slice::from_ref(&tree.path), Some("packlink"), "Needle");
}

#[test]
#[ignore = "manual before/after filesystem CPU, throughput, and allocation benchmark"]
fn benchmark_course_filesystem() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, courses, junk, dirs) in [
        ("empty", 0, 0, 0),
        ("one", 1, 0, 0),
        ("courses128", 128, 0, 0),
        ("mixed640", 128, 512, 0),
        ("nested", 128, 128, 8),
    ] {
        let tree = scan_tree(courses, junk, dirs);
        let mut variants = [
            ("old", baseline::collect_course_paths as fn(&Path) -> _),
            ("new", collect_course_paths as fn(&Path) -> _),
        ];
        if reverse {
            variants.reverse();
        }
        for (variant, f) in variants {
            let f = black_box(f);
            perf::measure_sampled(
                &format!("scan/{name}/{variant}"),
                16,
                (courses + junk + dirs).max(1),
                || f(black_box(&tree.path)),
            );
        }
    }
    for (name, dirs, files, query) in [
        ("empty", 0, 0, "missing"),
        ("one", 1, 0, "Folder-0000"),
        ("exact128", 128, 0, "Folder-0127"),
        ("case128", 128, 0, "folder-0127"),
        ("missing128", 128, 0, "missing"),
        ("files512", 0, 512, "missing"),
        ("blank", 128, 0, " \u{2003} "),
    ] {
        let tree = name_tree(dirs, files);
        let mut variants = [
            ("old", baseline::is_dir_ci as fn(&Path, &str) -> _),
            ("new", is_dir_ci as fn(&Path, &str) -> _),
        ];
        if reverse {
            variants.reverse();
        }
        for (variant, f) in variants {
            let f = black_box(f);
            perf::measure_sampled(&format!("lookup/{name}/{variant}"), 32, 1, || {
                f(black_box(&tree.path), black_box(query))
            });
        }
    }
    for (count, placement) in [
        (0, "missing"),
        (1, "first"),
        (32, "first"),
        (32, "last"),
        (32, "missing"),
    ] {
        let tree = packs_tree(count, placement);
        bench_resolve(
            &format!("packs{count}-{placement}"),
            std::slice::from_ref(&tree.path),
            None,
            "Needle",
            reverse,
        );
    }
    let nested = Tree::new();
    for i in 0..8 {
        make_song(&nested.dir(&format!("Series-{i:04}/Pack")), "Other");
    }
    make_song(&nested.dir("Series-0007/Pack"), "Needle");
    bench_resolve(
        "nested8",
        std::slice::from_ref(&nested.path),
        None,
        "Needle",
        reverse,
    );
    let group = name_tree(0, 0);
    let pack = group.dir("Pack");
    for i in 0..128 {
        make_song(&pack, &format!("Song-{i:04}"));
    }
    bench_resolve(
        "qualified128",
        std::slice::from_ref(&group.path),
        Some("pack"),
        "song-0127",
        reverse,
    );
}
fn bench_resolve(name: &str, roots: &[PathBuf], group: Option<&str>, song: &str, reverse: bool) {
    let mut variants = [
        (
            "old",
            baseline::resolve_song_dir
                as fn(&[PathBuf], &mut HashMap<String, PathBuf>, Option<&str>, &str) -> _,
        ),
        (
            "new",
            resolve_song_dir
                as fn(&[PathBuf], &mut HashMap<String, PathBuf>, Option<&str>, &str) -> _,
        ),
    ];
    if reverse {
        variants.reverse();
    }
    for (variant, f) in variants {
        let f = black_box(f);
        perf::measure_sampled(&format!("resolve/{name}/{variant}"), 8, 1, || {
            f(
                black_box(roots),
                black_box(&mut HashMap::new()),
                black_box(group),
                black_box(song),
            )
        });
        if group.is_some() {
            let mut cache = HashMap::new();
            black_box(f(roots, &mut cache, group, song));
            perf::measure_sampled(&format!("resolve/{name}-warm/{variant}"), 16, 1, || {
                f(
                    black_box(roots),
                    black_box(&mut cache),
                    black_box(group),
                    black_box(song),
                )
            });
        }
    }
}

#[cfg(windows)]
#[test]
fn directory_junctions_preserve_link_and_broken_target_behavior() {
    use std::os::windows::process::CommandExt;
    fn junction(target: &Path, link: &Path) {
        // Environment values are passed as data to a fixed PowerShell command.
        // Both paths belong to this test's absolute workspace fixtures.
        let plain = |path: &Path| {
            let text = path.to_string_lossy();
            text.strip_prefix(r"\\?\")
                .unwrap_or(text.as_ref())
                .to_owned()
        };
        let result = std::process::Command::new("powershell.exe")
            .args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:DEADSYNC_JUNCTION_LINK -Target $env:DEADSYNC_JUNCTION_TARGET | Out-Null"])
            .env("DEADSYNC_JUNCTION_LINK",plain(link))
            .env("DEADSYNC_JUNCTION_TARGET",plain(target))
            .creation_flags(0x0800_0000)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let source = Tree::new();
    let links = Tree::new();
    let pack = source.dir("Pack");
    make_song(&pack, "Needle");
    write(pack.join("included.crs"));
    junction(&pack, &links.path.join("PackLink"));
    let gone = source.dir("Gone");
    junction(&gone, &links.path.join("broken.crs"));
    fs::remove_dir(&gone).unwrap();
    assert_eq!(
        baseline::collect_course_paths(&links.path),
        collect_course_paths(&links.path)
    );
    assert!(collect_course_paths(&links.path).contains(&links.path.join("broken.crs")));
    for name in ["PackLink", "packlink", "broken.crs"] {
        assert_eq!(
            baseline::is_dir_ci(&links.path, name),
            is_dir_ci(&links.path, name)
        );
    }
    assert_resolve(std::slice::from_ref(&links.path), None, "Needle");
    assert_resolve(
        std::slice::from_ref(&links.path),
        Some("packlink"),
        "Needle",
    );
}
