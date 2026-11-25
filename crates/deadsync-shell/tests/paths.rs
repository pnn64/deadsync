use deadsync_config::dirs::AppDirs;
use std::path::PathBuf;

#[test]
fn startup_supplies_isolated_paths_to_persistence_assets_and_scanning() {
    let data = std::env::temp_dir().join(format!("deadsync-startup-paths-{}", std::process::id()));
    let dirs = AppDirs::resolve(Some(data.clone())).expect("resolve isolated application layout");
    dirs.ensure_dirs_exist();
    std::fs::write(
        dirs.config_path(),
        "[Options]\nLogToFile=0\nShowConsole=1\nDefaultNoteSkin=Cyber\nSmxDefaultLightBrightness=67\n\
         CenterImageTranslateX=-8\nCenterImageTranslateY=6\nCenterImageAddWidth=14\nCenterImageAddHeight=-2\n",
    )
    .expect("write config fixture");
    let overlay = data.join("assets/startup-path-test.txt");
    std::fs::create_dir_all(overlay.parent().expect("asset parent")).expect("create overlay root");
    std::fs::write(&overlay, b"overlay").expect("write asset fixture");
    std::fs::create_dir_all(dirs.songs_dir()).expect("create isolated song root");
    let additional_songs = data.join("additional-songs");
    std::fs::create_dir_all(&additional_songs).expect("create additional song root");

    deadsync_shell::app::init_paths(&dirs).expect("initialize application services");
    assert!(!deadsync_config::runtime_load::bootstrap_log_to_file());
    assert!(deadsync_config::runtime_load::bootstrap_show_console());
    assert_eq!(
        deadsync_profile::app_runtime::profiles_root(),
        data.join("save/profiles")
    );
    let config_effects = deadsync_shell::app::load_config();
    assert_eq!(config_effects.overscan, (-8, 6, 14, -2));
    assert_eq!(deadlib_present::space::overscan(), config_effects.overscan);
    deadsync_shell::app::load_profiles();
    let guest = deadsync_profile::app_runtime::guest_player_options();
    assert_eq!(guest.noteskin.as_str(), "cyber");
    assert_eq!(guest.pad_light_brightness, 67);
    let defaults = std::fs::read_to_string(dirs.default_player_options_path())
        .expect("read seeded player defaults");
    assert!(defaults.contains("NoteSkin=cyber\n"));
    assert!(defaults.contains("PadLightBrightness=67\n"));

    assert_eq!(
        deadsync_assets::resolve_asset_path("assets/startup-path-test.txt"),
        overlay
    );
    let song_roots = deadsync_simfile::app_runtime::collect_song_scan_roots(
        &dirs.songs_dir(),
        vec![("additional-songs".into(), additional_songs.clone())],
    );
    assert_eq!(song_roots.first(), Some(&dirs.songs_dir()));
    assert!(song_roots.contains(&additional_songs));

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
