use super::*;
use crate::test_directory::Directory;

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
    let dir = Directory::new("hash");
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
    let before = source_hash("dance", &data).unwrap();
    fs::write(dir.0.join("mixed.LUA"), b"changed").unwrap();
    let after = source_hash("dance", &data).unwrap();
    assert_ne!(before, after);
    // Duplicate roots and missing roots retain the original label ordering.
    data.search_dirs.push(dir.0.clone());
    data.search_dirs.push(dir.0.join("absent"));
    assert!(source_hash("dance", &data).is_ok());
    data.search_dirs.clear();
    assert!(source_hash("dance", &data).is_ok());
}
