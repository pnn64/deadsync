// Shared asset/config initialization for playback integration tests.
pub(crate) fn init_paths() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let bundle = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate is under the workspace crates directory")
            .to_path_buf();
        let data = std::env::temp_dir().join(format!(
            "deadsync-song-lua-playback-paths-{}",
            std::process::id()
        ));
        let dirs = deadsync_config::dirs::AppDirs {
            cache_dir: data.join("cache"),
            data_dir: data,
            exe_dir: bundle,
            portable: false,
        };
        dirs.ensure_dirs_exist();
        let assets = dirs.asset_paths(None);
        deadsync_assets::init_paths(assets).expect("initialize fixture assets");
        deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())
            .expect("initialize fixture config");
        deadsync_profile::app_runtime::init_paths(
            dirs.profiles_root(),
            dirs.default_player_options_path(),
            dirs.pad_config_path(),
        )
        .expect("initialize fixture profiles");
        deadsync_simfile::app_runtime::init_paths(
            deadsync_simfile::app_runtime::ScanPaths {
                song_cache: dirs.song_cache_dir(),
                extra_songs: Vec::new(),
                extra_courses: Vec::new(),
                autogen_courses: dirs.courses_dir(),
                song_movies: Vec::new(),
                random_movies: Vec::new(),
                bg_animations: Vec::new(),
            },
            |path| {
                deadlib_audio_decode::file_length_seconds(
                    path,
                    deadlib_audio_decode::DecodeOptions {
                        mp3_gapless: false,
                        mp3_info_silence: true,
                    },
                )
            },
        )
        .expect("initialize fixture scanning");
    });
}

#[allow(
    dead_code,
    reason = "some fixture suites only need path initialization"
)]
pub(crate) fn load_gameplay_charts(
    song: &deadsync_chart::SongData,
    chart_indices: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<deadsync_chart::GameplayChartData>, String> {
    let cfg = deadsync_config::runtime::get();
    deadsync_simfile::app_runtime::load_gameplay_charts(
        song,
        chart_indices,
        deadsync_simfile::runtime::RuntimeSongConfig::new(
            cfg.fastload,
            cfg.cachesongs,
            global_offset_seconds,
        ),
        deadsync_config::runtime::group_is_never_cached,
    )
}
