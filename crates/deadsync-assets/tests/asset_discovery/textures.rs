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

#[test]
fn startup_skips_skins() {
    let tree = Tree::new();
    let dirs = deadsync_config::dirs::AppDirs {
        data_dir: tree.dir("user"),
        exe_dir: tree.dir("bundled"),
        cache_dir: tree.dir("cache"),
        portable: false,
    };
    let paths = dirs.asset_paths(None);
    let manifest = [deadlib_assets::texture_asset("menu.png")];
    let empty = initial_texture_jobs(manifest, &paths, |_| false);
    assert_eq!(empty.len(), 1);
    for i in 0..32 {
        for base in [&dirs.data_dir, &dirs.exe_dir] {
            let skin = base.join(format!("assets/noteskins/dance/skin{i}"));
            fs::create_dir_all(skin.join("nested")).unwrap();
            for name in ["tap.png", "receptor.PNG", "nested/mine.png"] {
                write(skin.join(name));
            }
        }
    }
    write(tree.dir("user/assets/noteskins/pack").join("pack.json"));
    write(tree.dir("user/assets/noteskins/pack/art").join("tap.png"));
    write(tree.dir("user/assets/noteskins/.staging").join("tap.png"));
    let installed = initial_texture_jobs(manifest, &paths, |_| false);
    assert_eq!(installed.len(), empty.len());
    for (actual, expected) in installed.iter().zip(&empty) {
        assert_eq!(actual.key, expected.key);
        assert_eq!(actual.path, expected.path);
        assert_eq!(actual.sampler, expected.sampler);
        assert_eq!(actual.hints, expected.hints);
    }
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
    assert_eq!(
        snapshot(graphic_new(&roots, false, true)),
        snapshot(graphic_old(&roots, false, true))
    );
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_asset_discovery_textures() {
    let old_graphics =
        black_box(graphic_old as fn(&[PathBuf], bool, bool) -> Vec<DiscoveredTexture>);
    let new_graphics =
        black_box(graphic_new as fn(&[PathBuf], bool, bool) -> Vec<DiscoveredTexture>);
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
    }
}
