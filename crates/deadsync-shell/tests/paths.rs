use deadsync_config::dirs::AppDirs;
use std::path::PathBuf;

#[test]
fn startup_supplies_isolated_paths_to_persistence_assets_and_scanning() {
    let data = std::env::temp_dir().join(format!("deadsync-startup-paths-{}", std::process::id()));
    let dirs = AppDirs::resolve(Some(data.clone())).expect("resolve isolated application layout");
    dirs.ensure_dirs_exist();
    std::fs::write(
        dirs.config_path(),
        "[Options]\nLogToFile=0\nShowConsole=1\n",
    )
    .expect("write config fixture");
    let overlay = data.join("assets/startup-path-test.txt");
    std::fs::create_dir_all(overlay.parent().expect("asset parent")).expect("create overlay root");
    std::fs::write(&overlay, b"overlay").expect("write asset fixture");
    std::fs::create_dir_all(dirs.songs_dir()).expect("create isolated song root");

    deadsync_shell::app::init_paths(&dirs).expect("initialize application services");
    assert!(!deadsync_config::runtime_load::bootstrap_log_to_file());
    assert!(deadsync_config::runtime_load::bootstrap_show_console());
    assert_eq!(
        deadsync_profile::app_runtime::profiles_root(),
        data.join("save/profiles")
    );
    assert_eq!(
        deadsync_assets::resolve_asset_path("assets/startup-path-test.txt"),
        overlay
    );
    assert!(
        deadsync_simfile::app_runtime::collect_song_scan_roots(&dirs.songs_dir())
            .contains(&dirs.songs_dir())
    );

    deadsync_config::runtime::queue_save_write("[Options]\nShowConsole=0\n".to_owned());
    deadsync_config::runtime::flush_pending_saves();
    assert_eq!(
        std::fs::read_to_string(dirs.config_path()).expect("read saved settings"),
        "[Options]\nShowConsole=0\n"
    );
    assert!(
        deadsync_config::runtime::init_paths(
            PathBuf::from("other.ini"),
            PathBuf::from("other-palettes.ini")
        )
        .is_err()
    );
    assert!(!deadsync_config::runtime_load::bootstrap_show_console());
    std::fs::remove_dir_all(data).expect("remove isolated startup fixtures");
}
