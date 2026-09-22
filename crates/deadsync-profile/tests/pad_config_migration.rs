use deadsync_profile::{app_runtime as runtime, pad_config};

#[test]
fn failed_pad_config_migration_blocks_runtime_edits_until_retry_succeeds() {
    let root = std::env::temp_dir().join(format!(
        "deadsync-pad-config-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let profiles = root.join("profiles");
    let profile_dir = profiles.join("alice");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let legacy = pad_config::pad_config_path(&profile_dir);
    std::fs::write(&legacy, [0xff]).unwrap();
    let machine = root.join("save/padconfig.ini");
    runtime::init_paths(profiles, root.join("defaults.ini"), machine.clone()).unwrap();

    assert!(!runtime::migrate_pad_configs());
    assert!(runtime::load_pad_configs().is_none());
    let settings = vec![("DebounceMs".to_owned(), "4".to_owned())];
    assert!(!runtime::upsert_pad_config(
        "New",
        "smx",
        None,
        None,
        false,
        settings.clone()
    ));
    runtime::set_default_pad_config("S1", "Soft");
    runtime::rename_pad_config("Soft", "Renamed");
    runtime::delete_pad_config("Soft");
    assert!(
        !machine.exists(),
        "an edit must not mark failed migration complete"
    );

    pad_config::save_path(
        &legacy,
        &[pad_config::PadConfigProfile {
            name: "Soft".to_owned(),
            backend: "smx".to_owned(),
            pad_type: None,
            serial: Some("S1".to_owned()),
            default_for_serials: vec!["S1".to_owned()],
            global_default: false,
            settings: settings.clone(),
        }],
    )
    .unwrap();
    assert!(runtime::migrate_pad_configs());
    let migrated = runtime::load_pad_configs().unwrap();
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].name, "alice - Soft");
    assert_eq!(migrated[0].default_for_serials, ["S1"]);
    assert!(runtime::upsert_pad_config(
        "New",
        "smx",
        None,
        None,
        false,
        settings.clone()
    ));
    assert_eq!(runtime::load_pad_configs().unwrap().len(), 2);
    assert!(runtime::migrate_pad_configs());
    assert_eq!(runtime::load_pad_configs().unwrap().len(), 2);

    // A later read error is also distinct from a successfully loaded empty store.
    std::fs::write(&machine, [0xff]).unwrap();
    assert!(runtime::load_pad_configs().is_none());
    assert!(!runtime::upsert_pad_config(
        "Another", "smx", None, None, false, settings
    ));
    assert_eq!(std::fs::read(&machine).unwrap(), [0xff]);
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    std::fs::remove_dir_all(root).unwrap();
}
