use super::*;
use crate::asset_discovery_support::*;
#[cfg(windows)]
use crate::perf;
use std::hint::black_box;

#[path = "audio_baseline.rs"]
mod baseline;

#[test]
fn sound_discovery_preserves_filters_sorting_and_errors() {
    let tree = Tree::new();
    assert!(list_ogg_files(&tree.path).unwrap().is_empty());
    for name in [
        "z.OGG",
        "A.ogg",
        "_hidden.ogg",
        "_.ogg",
        "é.ogg",
        ".ogg",
        "plain",
        "file.png",
    ] {
        write(tree.path.join(name));
    }
    tree.dir("directory.ogg");
    #[cfg(any(windows, unix))]
    {
        write(tree.path.join(lossy_name(".ogg")));
        write(tree.path.join(lossy_name(".txt")));
    }
    let actual = list_ogg_files(&tree.path).unwrap();
    assert_eq!(actual, baseline::list_ogg_files(&tree.path).unwrap());
    assert!(actual.contains(&tree.path.join("A.ogg")));
    assert!(actual.contains(&tree.path.join("é.ogg")));
    assert!(!actual.contains(&tree.path.join("_hidden.ogg")));
    assert!(!actual.contains(&tree.path.join("directory.ogg")));
    for path in [tree.path.join("missing"), tree.path.join("A.ogg")] {
        assert_eq!(
            list_ogg_files(&path).unwrap_err().kind(),
            baseline::list_ogg_files(&path).unwrap_err().kind()
        );
    }
    populate(&tree.path, 32, 128, "ogg");
    // Path conversion and metadata-query allocations are platform dependent.
    #[cfg(windows)]
    perf::assert_reduced_churn(
        || {
            black_box(baseline::list_ogg_files(&tree.path).unwrap());
        },
        || {
            black_box(list_ogg_files(&tree.path).unwrap());
        },
    );
}

#[test]
#[cfg(any(windows, unix))]
fn sound_discovery_follows_links_and_excludes_directories() {
    let tree = Tree::new();
    let sounds = tree.dir("sounds");
    let target = tree.dir("target");
    directory_link(&target, &sounds.join("directory.ogg"));
    let gone = tree.dir("gone");
    directory_link(&gone, &sounds.join("broken.ogg"));
    std::fs::remove_dir(gone).unwrap();
    write(target.join("file"));
    if file_link(&target.join("file"), &sounds.join("linked.ogg")) {
        file_link(&target.join("missing"), &sounds.join("dangling.ogg"));
        assert_eq!(
            list_ogg_files(&sounds).unwrap(),
            [sounds.join("linked.ogg")]
        );
    }
    assert_eq!(
        list_ogg_files(&sounds).unwrap(),
        baseline::list_ogg_files(&sounds).unwrap()
    );
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_asset_discovery_audio() {
    let old = black_box(baseline::list_ogg_files as fn(&Path) -> std::io::Result<Vec<PathBuf>>);
    let new = black_box(list_ogg_files as fn(&Path) -> std::io::Result<Vec<PathBuf>>);
    for (label, accepted, rejected) in [
        ("empty", 0, 0),
        ("one", 1, 0),
        ("accepted128", 128, 0),
        ("mixed640", 128, 512),
        ("rejected512", 0, 512),
    ] {
        let tree = Tree::new();
        populate(&tree.path, accepted, rejected, "ogg");
        assert_eq!(old(&tree.path).unwrap(), new(&tree.path).unwrap());
        pair(
            &format!("audio/{label}"),
            accepted + rejected,
            || old(black_box(&tree.path)).unwrap(),
            || new(black_box(&tree.path)).unwrap(),
        );
    }
}
