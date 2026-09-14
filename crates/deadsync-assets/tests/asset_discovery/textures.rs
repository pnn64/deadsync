use super::*;
use crate::{asset_discovery_support::*, perf};
use std::hint::black_box;

#[path = "textures_baseline.rs"]
mod baseline;

fn graphic_old(roots: &[PathBuf], love: bool, multi: bool) -> Vec<DiscoveredTexture> {
    baseline::discover_graphic_textures_in_roots("judgements", roots.iter().cloned(), love, multi)
}
fn graphic_new(roots: &[PathBuf], love: bool, multi: bool) -> Vec<DiscoveredTexture> {
    discover_graphic_textures_in_roots("judgements", roots.iter().cloned(), love, multi)
}
fn snapshot(textures: Vec<DiscoveredTexture>) -> Vec<(String, String, PathBuf)> {
    textures
        .into_iter()
        .map(|t| (t.key, t.label, t.source_path))
        .collect()
}
fn noteskin_old(roots: &[PathBuf], assets: &[PathBuf]) -> Vec<(String, PathBuf)> {
    baseline::noteskin_png_texture_entries(roots, |p| {
        canonical_texture_key_with_asset_roots(p, assets)
    })
}
fn noteskin_new(roots: &[PathBuf], assets: &[PathBuf]) -> Vec<(String, PathBuf)> {
    noteskin_png_texture_entries(roots, |p| canonical_texture_key_with_asset_roots(p, assets))
}

#[test]
fn graphics_discovery_preserves_choices_and_overlay_precedence() {
    let tree = Tree::new();
    let first = tree.dir("first");
    let second = tree.dir("second");
    for name in [
        "Love 2x7.png",
        "Metal 2x7.PNG",
        "NONE.png",
        "plain.png",
        "sheet 2x7.txt",
        "one 1x1.png",
        "é 2x7.png",
        "junk.txt",
    ] {
        write(first.join(name));
    }
    write(second.join("love 2x7.PNG"));
    write(second.join("Other 2x7.png"));
    fs::create_dir(first.join("folder 2x7.png")).unwrap();
    #[cfg(any(windows, unix))]
    write(first.join(lossy_name(" 2x7.png")));
    let roots = [
        first.clone(),
        second.clone(),
        first.clone(),
        tree.path.join("missing"),
        first.join("plain.png"),
    ];
    for love in [false, true] {
        for multi in [false, true] {
            assert_eq!(
                snapshot(graphic_new(&roots, love, multi)),
                snapshot(graphic_old(&roots, love, multi))
            );
        }
    }
    let choices = graphic_new(&roots, true, true);
    assert_eq!(choices[0].label, "Love");
    assert_eq!(choices[0].source_path, first.join("Love 2x7.png"));
    assert!(choices.iter().any(|t| t.key.ends_with("sheet 2x7.txt")));
    assert!(!choices.iter().any(|t| t.label == "NONE"));
    populate(&first, 32, 128, "png");
    // Path conversion and metadata-query allocations are platform dependent.
    #[cfg(windows)]
    perf::assert_reduced_churn(
        || {
            black_box(graphic_old(&roots, true, true));
        },
        || {
            black_box(graphic_new(&roots, true, true));
        },
    );
}

#[test]
fn noteskin_discovery_preserves_traversal_exclusions_and_canonical_keys() {
    let tree = Tree::new();
    let a = tree.dir("first/noteskins");
    let b = tree.dir("second/noteskins");
    let outside = tree.dir("first/graphics");
    for (dir, names) in [
        (&a, &["tap.png", "TAP.PNG", "junk.txt"][..]),
        (&b, &["tap.png", "other.png"][..]),
        (&outside, &["outside.png"][..]),
    ] {
        for name in names {
            write(dir.join(name));
        }
    }
    for dir in [
        "first/noteskins/dance/nested",
        "first/noteskins/folder.png",
        "first/noteskins/.staging",
        "first/noteskins/native",
    ] {
        write(tree.dir(dir).join("inside.png"));
    }
    write(a.join("native/pack.json"));
    // A directory named pack.json does not hide its parent.
    tree.dir("first/noteskins/dance/pack.json");
    #[cfg(any(windows, unix))]
    write(a.join(lossy_name(".png")));
    let roots = [
        a.clone(),
        b.clone(),
        a.clone(),
        outside,
        tree.path.join("missing"),
        a.join("tap.png"),
    ];
    let assets = [tree.path.join("first"), tree.path.join("second")];
    let old = noteskin_old(&roots, &assets);
    let new = noteskin_new(&roots, &assets);
    assert_eq!(new, old);
    assert!(
        new.iter()
            .any(|(key, path)| key == "noteskins/tap.png" && *path == a.join("tap.png"))
    );
    assert!(
        new.iter()
            .any(|(key, _)| key == "noteskins/folder.png/inside.png")
    );
    assert!(!new.iter().any(|(key, _)| key.contains(".staging")
        || key.contains("native")
        || key.starts_with("graphics/")));
    populate(&a, 32, 128, "png");
    #[cfg(windows)]
    perf::assert_reduced_churn(
        || {
            black_box(noteskin_old(&roots, &assets));
        },
        || {
            black_box(noteskin_new(&roots, &assets));
        },
    );
    perf::assert_no_churn(|| {
        black_box(noteskin_new(&[], &assets));
    });
}

#[test]
#[cfg(any(windows, unix))]
fn texture_discovery_preserves_directory_and_broken_link_behavior() {
    let tree = Tree::new();
    let root = tree.dir("noteskins");
    let target = tree.dir("target");
    write(target.join("child.png"));
    directory_link(&target, &root.join("linked 2x7.png"));
    let gone = tree.dir("gone");
    directory_link(&gone, &root.join("broken 2x7.png"));
    fs::remove_dir(gone).unwrap();
    if file_link(&target.join("child.png"), &root.join("file 2x7.png")) {
        file_link(&target.join("missing"), &root.join("dangling 2x7.png"));
        let choices = graphic_new(std::slice::from_ref(&root), false, true);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].key, "judgements/file 2x7.png");
    }
    let roots = [root];
    let assets = [tree.path.clone()];
    assert_eq!(
        snapshot(graphic_new(&roots, false, true)),
        snapshot(graphic_old(&roots, false, true))
    );
    let new = noteskin_new(&roots, &assets);
    assert_eq!(new, noteskin_old(&roots, &assets));
    // The old PNG walker included non-directory PNG paths even when dangling.
    assert!(new.iter().any(|(key, _)| key == "noteskins/broken 2x7.png"));
    assert!(
        new.iter()
            .any(|(key, _)| key == "noteskins/linked 2x7.png/child.png")
    );
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_asset_discovery_textures() {
    let old_graphics =
        black_box(graphic_old as fn(&[PathBuf], bool, bool) -> Vec<DiscoveredTexture>);
    let new_graphics =
        black_box(graphic_new as fn(&[PathBuf], bool, bool) -> Vec<DiscoveredTexture>);
    let old_noteskin =
        black_box(noteskin_old as fn(&[PathBuf], &[PathBuf]) -> Vec<(String, PathBuf)>);
    let new_noteskin =
        black_box(noteskin_new as fn(&[PathBuf], &[PathBuf]) -> Vec<(String, PathBuf)>);
    for (label, accepted, rejected) in [
        ("empty", 0, 0),
        ("one", 1, 0),
        ("accepted128", 128, 0),
        ("mixed640", 128, 512),
        ("rejected512", 0, 512),
    ] {
        let tree = Tree::new();
        let root = tree.dir("noteskins");
        populate(&root, accepted, rejected, "png");
        let roots = [root];
        let assets = [tree.path.clone()];
        for multi in [false, true] {
            assert_eq!(
                snapshot(old_graphics(&roots, true, multi)),
                snapshot(new_graphics(&roots, true, multi))
            );
            pair(
                &format!("graphics/{label}/multi{multi}"),
                accepted + rejected,
                || old_graphics(black_box(&roots), true, black_box(multi)),
                || new_graphics(black_box(&roots), true, black_box(multi)),
            );
        }
        assert_eq!(old_noteskin(&roots, &assets), new_noteskin(&roots, &assets));
        pair(
            &format!("noteskin/{label}"),
            accepted + rejected,
            || old_noteskin(black_box(&roots), black_box(&assets)),
            || new_noteskin(black_box(&roots), black_box(&assets)),
        );
    }
    let tree = Tree::new();
    for i in 0..8 {
        populate(
            &tree.dir(&format!("noteskins/dance/skin{i}")),
            32,
            64,
            "png",
        );
    }
    let roots = [tree.path.join("noteskins")];
    let assets = [tree.path.clone()];
    assert_eq!(old_noteskin(&roots, &assets), new_noteskin(&roots, &assets));
    pair(
        "noteskin/nested768",
        768,
        || old_noteskin(black_box(&roots), black_box(&assets)),
        || new_noteskin(black_box(&roots), black_box(&assets)),
    );
}
