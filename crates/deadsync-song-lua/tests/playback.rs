// Compile the production playback source with asset fixtures in an integration
// crate. This avoids the library-unit-test identity cycle through deadsync-assets.
pub use deadsync_song_lua::*;
pub mod playback {
    include!("../src/playback.rs");
    mod tests {
        include!("playback/cases.rs");
    }
}
mod tests {
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
            let mut assets = dirs.asset_paths(None);
            if let Some(root) = std::env::var_os("DEADSYNC_WORKSHOP_PACK") {
                let root = std::path::PathBuf::from(root)
                    .canonicalize()
                    .expect("Workshop fixture exists");
                assets.noteskin_pack_roots =
                    vec![root.parent().expect("pack has a parent").to_path_buf()];
            }
            deadsync_assets::init_paths(assets).expect("initialize fixture assets");
            deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())
                .expect("initialize fixture config");
            deadsync_profile::app_runtime::init_paths(
                dirs.profiles_root(),
                dirs.default_player_options_path(),
            )
            .expect("initialize fixture profiles");
            deadsync_simfile::app_runtime::init_paths(deadsync_simfile::app_runtime::ScanPaths {
                song_cache: dirs.song_cache_dir(),
                extra_songs: Vec::new(),
                extra_courses: Vec::new(),
                autogen_courses: dirs.courses_dir(),
                song_movies: Vec::new(),
                random_movies: Vec::new(),
                bg_animations: Vec::new(),
            })
            .expect("initialize fixture scanning");
        });
    }
}
