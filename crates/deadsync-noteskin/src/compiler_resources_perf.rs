use super::*;
use crate::resource_perf_support as support;
use std::hint::black_box;
#[path = "compiler_resources_original.rs"]
mod original;

fn fixture(root: &Path, count: usize, bytes: usize) -> noteskin_itg::NoteskinData {
    let payload: Vec<_> = (0..bytes).map(|i| (i % 251) as u8).collect();
    for i in 0..count {
        fs::write(root.join(format!("actor-{i:04}.lua")), &payload).unwrap();
    }
    noteskin_itg::NoteskinData {
        name: "fixture".into(),
        overrides: vec![],
        metrics: Default::default(),
        search_dirs: vec![root.to_owned()],
    }
}

#[test]
fn reusable_hash_buffer_preserves_keys_across_file_sizes_and_changes() {
    let dir = support::Directory::new("hash");
    let mut data = fixture(&dir.0, 1, 4096);
    for (name, size) in [
        ("metrics.ini", 0),
        ("NoteSkin.lua", 65537),
        ("mixed.LUA", 7),
        ("\u{96ea}.lua", 131072),
        ("ignored.png", 4096),
    ] {
        fs::write(dir.0.join(name), vec![size as u8; size]).unwrap();
    }
    fs::create_dir(dir.0.join("directory.lua")).unwrap();
    for game in ["dance", "pump", "", "\u{96ea}"] {
        assert_eq!(source_hash(game, &data), original::source_hash(game, &data));
    }
    let before = source_hash("dance", &data).unwrap();
    fs::write(dir.0.join("mixed.LUA"), b"changed").unwrap();
    let after = source_hash("dance", &data).unwrap();
    assert_ne!(before, after);
    assert_eq!(Ok(after), original::source_hash("dance", &data));
    // Duplicate roots and missing roots retain the original label ordering.
    data.search_dirs.push(dir.0.clone());
    data.search_dirs.push(dir.0.join("absent"));
    assert_eq!(
        source_hash("dance", &data),
        original::source_hash("dance", &data)
    );
    data.search_dirs.clear();
    assert_eq!(
        source_hash("dance", &data),
        original::source_hash("dance", &data)
    );
}

#[test]
fn source_hash_reuses_file_buffer_across_actor_files() {
    let dir = support::Directory::new("hash-alloc");
    let data = fixture(&dir.0, 32, 32768);
    let (expected, before) = support::measure(|| original::source_hash("dance", &data));
    let (actual, after) = support::measure(|| source_hash("dance", &data));
    assert_eq!(actual, expected);
    assert!(after.allocs < before.allocs, "{before:?} -> {after:?}");
    assert!(
        after.allocated_bytes * 2 < before.allocated_bytes,
        "{before:?} -> {after:?}"
    );
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_resource_traversal() {
    for (count, bytes) in [
        (0, 0),
        (1, 0),
        (1, 4096),
        (16, 4096),
        (64, 4096),
        (16, 262144),
    ] {
        let dir = support::Directory::new("hash-bench");
        let data = fixture(&dir.0, count, bytes);
        support::compare(
            &format!("hash/{count}/{bytes}"),
            || {
                black_box(original::source_hash(black_box("dance"), black_box(&data)).unwrap());
            },
            || {
                black_box(source_hash(black_box("dance"), black_box(&data)).unwrap());
            },
        );
    }
}
