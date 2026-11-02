use super::*;
use std::hint::black_box;
#[path = "resource_paths_original.rs"]
mod original;
#[path = "../../../tests/perf/resource_support.rs"]
mod support;

fn fixture(root: &Path, count: usize) -> AssetPaths {
    let search_roots: Vec<_> = (0..count)
        .map(|i| {
            let path = root.join(format!("root-{i:02}"));
            std::fs::create_dir(&path).unwrap();
            path
        })
        .collect();
    AssetPaths {
        search_roots,
        graphic_roots: vec![],
        texture_roots: Default::default(),
        noteskin_roots: vec![],
        noteskin_pack_roots: vec![],
        noteskin_cache: PathBuf::new(),
        banner_cache: PathBuf::new(),
        cdtitle_cache: PathBuf::new(),
    }
}

#[test]
fn resource_paths_preserve_overlay_order_fallbacks_and_live_changes() {
    let dir = support::Directory::new("paths");
    let paths = fixture(&dir.0, 4);
    for (i, root) in paths.search_roots.iter().enumerate() {
        std::fs::write(root.join("shared.png"), [i as u8]).unwrap();
        std::fs::create_dir(root.join("directory.png")).unwrap();
    }
    std::fs::write(paths.search_roots[3].join("last.png"), []).unwrap();
    std::fs::write(paths.search_roots[2].join("\u{96ea}.png"), []).unwrap();
    let absolute = dir.0.join("absent.png");
    for path in [
        "shared.png",
        "last.png",
        "\u{96ea}.png",
        "directory.png",
        "missing",
        "",
        ".",
        "..",
        "directory.png/../shared.png",
        "\0",
        absolute.to_str().unwrap(),
    ] {
        assert_eq!(
            paths.resolve_asset_path(path),
            original::resolve_asset_path(&paths, path),
            "{path:?}"
        );
    }
    for root in &paths.search_roots {
        assert_eq!(
            paths.resolve_asset_path("shared.png"),
            root.join("shared.png")
        );
        std::fs::remove_file(root.join("shared.png")).unwrap();
    }
    assert_eq!(
        paths.resolve_asset_path("shared.png"),
        Path::new("shared.png")
    );
    let empty = fixture(&dir.0, 0);
    for path in ["missing.png", "", absolute.to_str().unwrap()] {
        assert_eq!(
            empty.resolve_asset_path(path),
            original::resolve_asset_path(&empty, path)
        );
    }
}

#[test]
fn resource_path_search_reuses_temporary_storage() {
    let dir = support::Directory::new("path-alloc");
    let paths = fixture(&dir.0, 8);
    std::fs::write(paths.search_roots[7].join("last.png"), []).unwrap();
    for path in ["last.png", "missing.png"] {
        let (expected, before) = support::measure(|| original::resolve_asset_path(&paths, path));
        let (actual, after) = support::measure(|| paths.resolve_asset_path(path));
        assert_eq!(actual, expected);
        assert!(
            after.allocs + after.reallocs < before.allocs + before.reallocs,
            "{before:?} -> {after:?}"
        );
        assert!(
            after.allocated_bytes < before.allocated_bytes,
            "{before:?} -> {after:?}"
        );
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_resource_traversal() {
    for count in [1, 4, 16] {
        let dir = support::Directory::new("path-bench");
        let paths = fixture(&dir.0, count);
        std::fs::write(paths.search_roots[0].join("first.png"), []).unwrap();
        std::fs::write(paths.search_roots[count - 1].join("last.png"), []).unwrap();
        let absolute = paths.search_roots[0].join("first.png");
        for path in [
            "first.png",
            "last.png",
            "missing.png",
            absolute.to_str().unwrap(),
        ] {
            let kind = if Path::new(path).is_absolute() {
                "absolute"
            } else {
                path
            };
            support::compare(
                &format!("path/{count}/{kind}"),
                || {
                    black_box(original::resolve_asset_path(
                        black_box(&paths),
                        black_box(path),
                    ));
                },
                || {
                    black_box(paths.resolve_asset_path(black_box(path)));
                },
            );
        }
    }
}
