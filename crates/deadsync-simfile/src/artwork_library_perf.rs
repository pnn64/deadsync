use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("artwork_original.rs");
}

fn fixture(label: &str, kind: &str) -> PathBuf {
    // Fixed-width unique suffixes keep path lengths identical across benchmark runs.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "deadsync-library-artwork-{label}-{kind}-{:010}-{nanos:039}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let images: &[(&str, u32, u32)] = match kind {
        "dimensions3" => &[("a.png", 640, 480), ("b.png", 256, 128), ("f.png", 64, 32)],
        "dimensions6" => &[
            ("a.png", 640, 480),
            ("b.png", 256, 128),
            ("c.png", 128, 128),
            ("d.png", 200, 100),
            ("e.png", 128, 128),
            ("f.png", 64, 32),
        ],
        "hints" | "tagged" => &[
            ("banner.png", 256, 128),
            ("background.png", 640, 480),
            ("cdtitle.png", 64, 32),
        ],
        _ => &[],
    };
    for &(name, w, h) in images {
        image::RgbaImage::new(w, h).save(root.join(name)).unwrap();
    }
    if kind == "corrupt" {
        fs::write(root.join("broken.png"), b"invalid image bytes").unwrap();
    }
    root
}

fn resolve(root: &Path, kind: &str, old: bool) -> ResolvedSongArtwork {
    let [banner, background, cdtitle] = if kind == "tagged" {
        ["banner.png", "background.png", "cdtitle.png"]
    } else {
        ["", "", ""]
    };
    if old {
        original::resolve_song_artwork_like_itg(
            root,
            b"#CDIMAGE:;#DISCIMAGE:;",
            banner,
            background,
            cdtitle,
            "",
        )
    } else {
        resolve_song_artwork_like_itg(
            root,
            b"#CDIMAGE:;#DISCIMAGE:;",
            banner,
            background,
            cdtitle,
            "",
        )
    }
}

fn selected(art: &ResolvedSongArtwork) -> (&Option<PathBuf>, &Option<PathBuf>, &Option<PathBuf>) {
    (&art.banner_path, &art.background_path, &art.cdtitle_path)
}

#[test]
fn consuming_artwork_paths_preserves_hints_dimensions_and_failure_results() {
    for kind in [
        "empty",
        "corrupt",
        "dimensions3",
        "dimensions6",
        "hints",
        "tagged",
    ] {
        let root = fixture("parity", kind);
        let old = resolve(&root, kind, true);
        let new = resolve(&root, kind, false);
        assert_eq!(selected(&old), selected(&new), "{kind}");
        if kind.starts_with("dimensions") {
            assert_eq!(new.banner_path, Some(root.join("b.png")));
            assert_eq!(new.background_path, Some(root.join("a.png")));
            assert_eq!(new.cdtitle_path, Some(root.join("f.png")));
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn consuming_artwork_paths_removes_each_dimension_selection_clone() {
    for (kind, removed) in [("dimensions3", 3), ("dimensions6", 6)] {
        let root = fixture("allocations", kind);
        let (old, a) = measure(|| resolve(&root, kind, true));
        let (new, b) = measure(|| resolve(&root, kind, false));
        assert_eq!(selected(&old), selected(&new));
        assert_eq!(a.allocs, b.allocs + removed);
        assert_eq!(a.reallocs, b.reallocs);
        assert!(a.allocated_bytes > b.allocated_bytes);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_library_artwork() {
    for kind in [
        "empty",
        "corrupt",
        "dimensions3",
        "dimensions6",
        "hints",
        "tagged",
    ] {
        let root = fixture("bench", kind);
        let (_, a) = measure(|| resolve(&root, kind, true));
        let (_, b) = measure(|| resolve(&root, kind, false));
        println!("ALLOC artwork/{kind}: original {a:?}, current {b:?}");
        compare(
            &format!("artwork/{kind}"),
            8,
            || {
                black_box(resolve(black_box(&root), kind, true));
            },
            || {
                black_box(resolve(black_box(&root), kind, false));
            },
        );
        fs::remove_dir_all(root).unwrap();
    }
}
