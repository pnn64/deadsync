#[cfg(windows)]
#[test]
fn migration_retries_an_unreadable_legacy_file_without_writing_a_partial_store() {
    use std::os::windows::fs::OpenOptionsExt;

    let root = unique_temp_dir("locked-migration");
    let profiles = root.join("profiles");
    let legacy_dir = profiles.join("alice");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy = pad_config_path(&legacy_dir);
    save_path(
        &legacy,
        &[sample("Soft", "smx", Some("fsr"), Some("S1"), &["S1"])],
    )
    .unwrap();
    let other = profiles.join("bob");
    std::fs::create_dir_all(&other).unwrap();
    save_path(
        &pad_config_path(&other),
        &[sample("Hard", "smx", Some("fsr"), Some("S2"), &["S2"])],
    )
    .unwrap();
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&legacy)
        .unwrap();
    assert!(load_path(&legacy).is_err(), "fixture must deny reads");
    let machine = root.join("save").join(PAD_CONFIG_FILE);
    let first = migrate_machine_store(&machine, &profiles);
    let marked_complete = machine.exists();
    drop(locked);
    let retry = migrate_machine_store(&machine, &profiles);
    let saved = load_path(&machine).unwrap_or_default();
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        first.is_err(),
        "unreadable source was treated as migrated: first={first:?}, retry={retry:?}, saved={saved:?}"
    );
    assert!(!marked_complete);
    assert_eq!(retry.unwrap(), Some(2));
    assert_eq!(saved[0].name, "alice - Soft");
    assert_eq!(saved[0].default_for_serials, ["S1"]);
    assert_eq!(saved[1].name, "bob - Hard");
    assert_eq!(saved[1].default_for_serials, ["S2"]);
}

#[test]
fn migration_retries_after_a_legacy_read_error() {
    let root = unique_temp_dir("invalid-legacy");
    let profiles = root.join("profiles");
    let legacy_dir = profiles.join("alice");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy = pad_config_path(&legacy_dir);
    std::fs::write(&legacy, [0xff]).unwrap();
    let machine = root.join("save").join(PAD_CONFIG_FILE);
    let error = migrate_machine_store(&machine, &profiles).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains(&legacy.display().to_string()));
    assert!(!machine.exists());

    save_path(&legacy, &[sample("Soft", "smx", None, Some("S1"), &["S1"])]).unwrap();
    assert_eq!(migrate_machine_store(&machine, &profiles).unwrap(), Some(1));
    let saved = load_path(&machine).unwrap();
    assert_eq!(saved[0].name, "alice - Soft");
    assert_eq!(saved[0].default_for_serials, ["S1"]);
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_does_not_mark_a_failed_directory_read_complete() {
    let root = unique_temp_dir("directory-error");
    let profiles = root.join("profiles");
    std::fs::write(&profiles, "not a directory").unwrap();
    let machine = root.join("save").join(PAD_CONFIG_FILE);
    assert!(migrate_machine_store(&machine, &profiles).is_err());
    assert!(!machine.exists());

    std::fs::remove_file(&profiles).unwrap();
    std::fs::create_dir_all(profiles.join("empty-profile")).unwrap();
    std::fs::write(profiles.join("README.txt"), "not a profile").unwrap();
    assert_eq!(migrate_machine_store(&machine, &profiles).unwrap(), Some(0));
    assert!(load_path(&machine).unwrap().is_empty());
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_retries_a_failed_destination_write() {
    let root = unique_temp_dir("write-error");
    let profiles = root.join("profiles");
    let legacy_dir = profiles.join("alice");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy = pad_config_path(&legacy_dir);
    let configs = [sample("Soft", "smx", None, Some("S1"), &["S1"])];
    save_path(&legacy, &configs).unwrap();
    let save_dir = root.join("save");
    std::fs::write(&save_dir, "not a directory").unwrap();
    let machine = save_dir.join(PAD_CONFIG_FILE);
    assert!(migrate_machine_store(&machine, &profiles).is_err());
    assert_eq!(load_path(&legacy).unwrap(), configs);

    std::fs::remove_file(&save_dir).unwrap();
    assert_eq!(migrate_machine_store(&machine, &profiles).unwrap(), Some(1));
    assert_eq!(load_path(&machine).unwrap()[0].name, "alice - Soft");
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pad_config_edits_preserve_a_store_that_cannot_be_read() {
    let root = unique_temp_dir("unreadable-store");
    let path = root.join(PAD_CONFIG_FILE);
    let bytes = [0xff, 0xfe];
    std::fs::write(&path, bytes).unwrap();
    assert!(
        upsert_path(
            &path,
            "New",
            "smx",
            None,
            None,
            false,
            vec![("DebounceMs".into(), "4".into())]
        )
        .is_err()
    );
    assert!(set_default_path(&path, "S1", "New").is_err());
    assert!(rename_path(&path, "Old", "New").is_err());
    assert!(delete_path(&path, "Old").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}
