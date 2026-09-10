//! DeadSync filenames, portable layouts, and asset overlay conventions.

use log::warn;
use std::path::{Path, PathBuf};

/// Single source of truth for all resolved application directories.
#[derive(Clone, Debug)]
pub struct AppDirs {
    /// Root for user data (config, saves, songs, courses, log).
    pub data_dir: PathBuf,
    /// Root for regenerable cache data.
    pub cache_dir: PathBuf,
    /// Directory containing bundled runtime data.
    /// Usually this is the executable directory. For test binaries built under
    /// `target/<profile>/deps`, this is normalized back to `target/<profile>`
    /// so copied assets remain discoverable.
    pub exe_dir: PathBuf,
    /// Whether running in portable mode.
    pub portable: bool,
}

impl AppDirs {
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("deadsync.ini")
    }

    #[must_use]
    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join("deadsync.log")
    }

    #[must_use]
    pub fn profiles_root(&self) -> PathBuf {
        self.data_dir.join("save").join("profiles")
    }

    /// Machine-global saved pad configs. Pad thresholds describe the physical
    /// pads wired to this machine, so they live beside the other machine-level
    /// save data rather than inside any player profile.
    #[must_use]
    pub fn pad_config_path(&self) -> PathBuf {
        self.data_dir.join("save").join("padconfig.ini")
    }

    #[must_use]
    pub fn screenshots_dir(&self) -> PathBuf {
        self.data_dir.join("save").join("screenshots")
    }

    #[must_use]
    pub fn current_screen_path(&self) -> PathBuf {
        self.data_dir.join("save").join("current_screen.txt")
    }

    #[must_use]
    pub fn default_player_options_path(&self) -> PathBuf {
        self.data_dir
            .join("save")
            .join("default_player_options.ini")
    }

    #[must_use]
    pub fn judgment_palettes_path(&self) -> PathBuf {
        self.data_dir.join("save").join("judgment_palettes.ini")
    }

    #[must_use]
    pub fn songs_dir(&self) -> PathBuf {
        self.data_dir.join("songs")
    }

    #[must_use]
    pub fn courses_dir(&self) -> PathBuf {
        self.data_dir.join("courses")
    }

    #[must_use]
    pub fn song_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("songs")
    }

    #[must_use]
    pub fn banner_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("banner")
    }

    #[must_use]
    pub fn cdtitle_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("cdtitle")
    }

    #[must_use]
    pub fn replaygain_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("replaygain")
    }

    /// Single-file consolidated `ReplayGain` cache. Replaces the legacy
    /// per-song `replaygain/<hash>.bin` layout, which doesn't scale to
    /// libraries of 10k+ songs.
    #[must_use]
    pub fn replaygain_cache_file(&self) -> PathBuf {
        self.cache_dir.join("replaygain.bin")
    }

    #[must_use]
    pub fn null_or_die_cache_file(&self) -> PathBuf {
        self.cache_dir.join("null-or-die-sync.json")
    }

    #[must_use]
    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }

    #[must_use]
    pub fn noteskin_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("noteskins")
    }

    #[must_use]
    pub fn unlock_cache_path(&self) -> PathBuf {
        self.cache_dir.join("unlocks-cache.json")
    }

    /// Returns additional song scan roots beyond the primary `songs_dir()`.
    /// In platform-native mode, also includes `{exe_dir}/songs` so bundled songs
    /// are found even when the data dir is elsewhere.
    #[must_use]
    pub fn extra_song_roots(&self) -> Vec<PathBuf> {
        if self.portable {
            return Vec::new();
        }
        let exe_songs = self.exe_dir.join("songs");
        if exe_songs != self.songs_dir() {
            vec![exe_songs]
        } else {
            Vec::new()
        }
    }

    /// Returns additional course roots beyond the primary `courses_dir()`.
    /// In platform-native mode, also includes `{exe_dir}/courses`.
    #[must_use]
    pub fn extra_course_roots(&self) -> Vec<PathBuf> {
        if self.portable {
            return Vec::new();
        }
        let exe_courses = self.exe_dir.join("courses");
        if exe_courses != self.courses_dir() {
            vec![exe_courses]
        } else {
            Vec::new()
        }
    }

    /// Returns all root directories where noteskins may be found.
    /// In platform-native mode the data-dir variant is listed first so that
    /// user-added skins take priority over bundled ones.
    #[must_use]
    pub fn noteskin_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::with_capacity(2);
        if !self.portable {
            let data_root = self.data_dir.join("assets").join("noteskins");
            roots.push(data_root);
        }
        roots.push(self.exe_dir.join("assets").join("noteskins"));
        roots
    }
}

impl AppDirs {
    fn has_portable_marker(dir: &std::path::Path) -> bool {
        dir.join("portable.txt").exists() || dir.join("portable.ini").exists()
    }

    fn runtime_root_from_exe_path(exe_path: &std::path::Path) -> PathBuf {
        let exe_dir = exe_path
            .parent()
            .expect("exe has no parent dir")
            .to_path_buf();
        let in_cargo_deps_dir = exe_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("deps"));
        if !in_cargo_deps_dir {
            return exe_dir;
        }

        let Some(parent) = exe_dir.parent() else {
            return exe_dir;
        };
        let parent = parent.to_path_buf();
        let looks_like_bundle_root = parent.join("assets").is_dir()
            || Self::has_portable_marker(&parent)
            || parent.join("songs").is_dir()
            || parent.join("courses").is_dir();
        if looks_like_bundle_root {
            parent
        } else {
            exe_dir
        }
    }

    fn portable_layout(exe_dir: PathBuf) -> Self {
        let cache_dir = exe_dir.join("cache");
        Self {
            data_dir: exe_dir.clone(),
            cache_dir,
            exe_dir,
            portable: true,
        }
    }

    fn isolated_layout(data_dir: PathBuf, exe_dir: PathBuf) -> Self {
        Self {
            cache_dir: data_dir.join("cache"),
            data_dir,
            exe_dir,
            portable: false,
        }
    }

    /// Resolve DeadSync's layout once, optionally isolating writable data below an explicit root.
    pub fn resolve(data_override: Option<PathBuf>) -> std::io::Result<Self> {
        if let Some(root) = data_override.as_ref()
            && !root.is_absolute()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "application data directory must be absolute: '{}'",
                    root.display()
                ),
            ));
        }
        let exe_path = std::env::current_exe()?;
        let exe_dir = Self::runtime_root_from_exe_path(&exe_path);
        if let Some(data_dir) = data_override {
            return Ok(Self::isolated_layout(data_dir, exe_dir));
        }
        if Self::has_portable_marker(&exe_dir) {
            return Ok(Self::portable_layout(exe_dir));
        }
        let native = deadlib_platform::dirs::native_dirs(deadlib_platform::dirs::AppIdentity {
            qualifier: "",
            organization: "",
            application: "deadsync",
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot determine native application directories",
            )
        })?;
        Ok(Self::native_layout(exe_dir, native))
    }

    fn native_layout(exe_dir: PathBuf, native: deadlib_platform::dirs::NativeDirs) -> Self {
        // Preserve the established Windows layout, without ProjectDirs' `config` suffix.
        #[cfg(windows)]
        let data_dir = native
            .config_dir
            .parent()
            .expect("native config directory has an application parent")
            .to_path_buf();
        #[cfg(not(windows))]
        let data_dir = native.data_dir;
        #[cfg(windows)]
        let cache_dir = data_dir.join("cache");
        #[cfg(not(windows))]
        let cache_dir = native.cache_dir;
        Self {
            data_dir,
            cache_dir,
            exe_dir,
            portable: false,
        }
    }

    /// Create writable roots before starting subsystem workers.
    pub fn ensure_dirs_exist(&self) {
        for dir in [&self.data_dir, &self.cache_dir] {
            if let Err(error) = std::fs::create_dir_all(dir) {
                warn!("Failed to create directory {}: {error}", dir.display());
            }
        }
    }

    /// Candidate media directories, including roots created after startup.
    pub fn media_roots(&self, dirname: &str, cwd: Option<&Path>) -> Vec<PathBuf> {
        let mut roots = Vec::with_capacity(4);
        let candidates = [
            Some(self.data_dir.join(dirname)),
            Some(self.exe_dir.join(dirname)),
            cwd.map(|path| path.join(dirname)),
            cwd.map(|path| path.join("deadsync").join(dirname)),
        ];
        for root in candidates.into_iter().flatten() {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        roots
    }

    /// Locate Workshop content inside the game assets, including portable installs.
    pub fn workshop_dir(&self, cwd: Option<&Path>) -> PathBuf {
        let roots = cwd
            .into_iter()
            .flat_map(|cwd| [cwd.to_path_buf(), cwd.join("deadsync")])
            .chain(std::iter::once(self.exe_dir.clone()));
        roots
            .map(|root| root.join("assets/noteskins"))
            .find(|root| root.is_dir())
            .unwrap_or_else(|| self.exe_dir.join("assets/noteskins"))
            .join("hurg")
    }

    /// Prepare the asset subsystem's overlay and cache paths at startup.
    pub fn asset_paths(&self, cwd: Option<&Path>) -> AssetPaths {
        // The install destination wins over older copies (including cargo's
        // copied assets), so a successful download activates that exact pack.
        let mut pack_root = self.workshop_dir(cwd);
        pack_root.pop();
        let mut noteskin_pack_roots = vec![pack_root];
        for root in self.media_roots("assets/noteskins", cwd) {
            if !noteskin_pack_roots.contains(&root) {
                noteskin_pack_roots.push(root);
            }
        }
        let mut search_roots = Vec::with_capacity(4);
        let mut graphic_roots = Vec::with_capacity(3);
        if !self.portable {
            search_roots.push(self.data_dir.clone());
            graphic_roots.push(self.data_dir.join("assets/graphics"));
        }
        if let Some(cwd) = cwd {
            search_roots.extend([cwd.to_path_buf(), cwd.join("deadsync")]);
            graphic_roots.push(cwd.join("assets/graphics"));
        }
        search_roots.push(self.exe_dir.clone());
        graphic_roots.push(self.exe_dir.join("assets/graphics"));
        AssetPaths {
            search_roots,
            graphic_roots,
            texture_roots: [self.data_dir.join("assets"), self.exe_dir.join("assets")],
            noteskin_roots: self.noteskin_roots(),
            noteskin_pack_roots,
            noteskin_cache: self.noteskin_cache_dir(),
            banner_cache: self.banner_cache_dir(),
            cdtitle_cache: self.cdtitle_cache_dir(),
        }
    }

    pub fn bookkeeping_path(&self) -> PathBuf {
        self.data_dir.join("save/bookkeeping.json")
    }
    pub fn fsr_dump_path(&self) -> PathBuf {
        self.data_dir.join("fsrdump.txt")
    }
}

/// Paths retained by the game asset subsystem, without config/profile/song layout.
#[derive(Clone, Debug)]
pub struct AssetPaths {
    pub search_roots: Vec<PathBuf>,
    pub graphic_roots: Vec<PathBuf>,
    pub texture_roots: [PathBuf; 2],
    pub noteskin_roots: Vec<PathBuf>,
    pub noteskin_pack_roots: Vec<PathBuf>,
    pub noteskin_cache: PathBuf,
    pub banner_cache: PathBuf,
    pub cdtitle_cache: PathBuf,
}

impl AssetPaths {
    /// Resolve an asset against the startup overlay order; absolute paths pass through.
    pub fn resolve_asset_path(&self, path: &str) -> PathBuf {
        let original = PathBuf::from(path);
        if original.is_absolute() {
            return original;
        }
        for root in &self.search_roots {
            let candidate = root.join(path);
            if candidate.exists() {
                return candidate;
            }
        }
        original
    }

    pub fn strip_asset_prefix<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        self.texture_roots
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workshop_uses_game_assets_in_portable_and_installed_layouts() {
        let root =
            std::env::temp_dir().join(format!("deadsync-workshop-paths-{}", std::process::id()));
        let game = root.join("game");
        let checkout = root.join("checkout");
        std::fs::create_dir_all(game.join("assets/noteskins")).unwrap();
        std::fs::create_dir_all(checkout.join("deadsync/assets/noteskins")).unwrap();
        for portable in [false, true] {
            let dirs = AppDirs {
                data_dir: root.join("user-data"),
                cache_dir: root.join("cache"),
                exe_dir: game.clone(),
                portable,
            };
            assert_eq!(dirs.workshop_dir(None), game.join("assets/noteskins/hurg"));
            assert_eq!(
                dirs.workshop_dir(Some(&checkout)),
                checkout.join("deadsync/assets/noteskins/hurg")
            );
            assert_eq!(
                dirs.workshop_dir(Some(&root.join("unrelated"))),
                game.join("assets/noteskins/hurg")
            );
            assert!(
                dirs.asset_paths(None)
                    .noteskin_pack_roots
                    .contains(&game.join("assets/noteskins"))
            );
            assert_eq!(
                dirs.asset_paths(Some(&checkout)).noteskin_pack_roots[0],
                checkout.join("deadsync/assets/noteskins"),
                "new installs must take precedence over older copied assets"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn runtime_root_uses_parent_profile_dir_for_cargo_test_binaries() {
        let root =
            std::env::temp_dir().join(format!("deadsync-dirs-cargo-assets-{}", std::process::id()));
        let exe_path = root.join("target/debug/deps/deadsync-test");
        let expected = root.join("target/debug");
        std::fs::create_dir_all(expected.join("assets")).expect("create mock assets dir");

        assert_eq!(AppDirs::runtime_root_from_exe_path(&exe_path), expected);

        std::fs::remove_dir_all(root).expect("cleanup mock target dir");
    }

    #[test]
    fn runtime_root_uses_parent_profile_dir_for_cargo_test_binaries_with_portable_ini() {
        let root = std::env::temp_dir().join(format!(
            "deadsync-dirs-cargo-portable-{}",
            std::process::id()
        ));
        let exe_path = root.join("target/debug/deps/deadsync-test");
        let expected = root.join("target/debug");
        std::fs::create_dir_all(&expected).expect("create mock target dir");
        std::fs::write(expected.join("portable.ini"), "").expect("create portable.ini");

        assert_eq!(AppDirs::runtime_root_from_exe_path(&exe_path), expected);

        std::fs::remove_dir_all(root).expect("cleanup mock target dir");
    }

    #[test]
    fn runtime_root_keeps_regular_executable_dir() {
        let exe_path = PathBuf::from("/tmp/deadsync/bin/deadsync");
        assert_eq!(
            AppDirs::runtime_root_from_exe_path(&exe_path),
            PathBuf::from("/tmp/deadsync/bin")
        );
    }

    #[test]
    fn portable_layout_keeps_song_cache_under_cache_dir() {
        let dirs = AppDirs::portable_layout(PathBuf::from("/tmp/deadsync-portable"));
        assert_eq!(dirs.songs_dir(), Path::new("/tmp/deadsync-portable/songs"));
        assert_eq!(
            dirs.default_player_options_path(),
            Path::new("/tmp/deadsync-portable/save/default_player_options.ini")
        );
        assert_eq!(
            dirs.song_cache_dir(),
            Path::new("/tmp/deadsync-portable/cache/songs")
        );
        assert_eq!(
            dirs.null_or_die_cache_file(),
            Path::new("/tmp/deadsync-portable/cache/null-or-die-sync.json")
        );
    }

    #[test]
    fn isolated_layout_keeps_bundled_assets_separate_from_case_data() {
        let dirs = AppDirs::isolated_layout(
            PathBuf::from("/tmp/deadsync-case"),
            PathBuf::from("/opt/deadsync"),
        );

        assert_eq!(dirs.data_dir, Path::new("/tmp/deadsync-case"));
        assert_eq!(dirs.cache_dir, Path::new("/tmp/deadsync-case/cache"));
        assert_eq!(dirs.exe_dir, Path::new("/opt/deadsync"));
        assert!(!dirs.portable);
    }

    #[test]
    fn native_layout_preserves_platform_compatibility() {
        let base = std::env::temp_dir().join("deadsync-native-layout");
        let dirs = AppDirs::native_layout(
            base.join("bundle"),
            deadlib_platform::dirs::NativeDirs {
                data_dir: base.join("native-data"),
                cache_dir: base.join("native-cache"),
                config_dir: base.join("application/config"),
            },
        );
        #[cfg(windows)]
        {
            assert_eq!(dirs.data_dir, base.join("application"));
            assert_eq!(dirs.cache_dir, base.join("application/cache"));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(dirs.data_dir, base.join("native-data"));
            assert_eq!(dirs.cache_dir, base.join("native-cache"));
        }
        assert_eq!(dirs.config_path(), dirs.data_dir.join("deadsync.ini"));
        assert!(!dirs.portable);
    }

    #[test]
    fn explicit_data_root_is_validated_and_isolates_writes() {
        let invalid = AppDirs::resolve(Some(PathBuf::from("relative-data")))
            .expect_err("relative override is rejected");
        assert_eq!(invalid.kind(), std::io::ErrorKind::InvalidInput);
        let data = std::env::temp_dir().join("deadsync-explicit-layout");
        let dirs = AppDirs::resolve(Some(data.clone())).expect("resolve isolated layout");
        assert_eq!(dirs.data_dir, data);
        assert_eq!(dirs.cache_dir, data.join("cache"));
        assert_eq!(dirs.profiles_root(), data.join("save/profiles"));
        assert_eq!(
            dirs.replaygain_cache_file(),
            data.join("cache/replaygain.bin")
        );
        assert_eq!(dirs.replaygain_cache_dir(), data.join("cache/replaygain"));
        assert!(!dirs.portable);
    }

    #[test]
    fn asset_overlay_preserves_search_order_and_portable_policy() {
        let root =
            std::env::temp_dir().join(format!("deadsync-overlay-order-{}", std::process::id()));
        let data = root.join("data");
        let exe = root.join("bundle");
        let cwd = root.join("workspace");
        let relative = "assets/graphics/overlay.png";
        let sources = [&data, &cwd, &cwd.join("deadsync"), &exe];
        for source in sources {
            std::fs::create_dir_all(source.join("assets/graphics"))
                .expect("create overlay directory");
            std::fs::write(source.join(relative), b"fixture").expect("create overlay asset");
        }
        let dirs = AppDirs::isolated_layout(data.clone(), exe.clone());
        let assets = dirs.asset_paths(Some(&cwd));
        assert_eq!(assets.resolve_asset_path(relative), data.join(relative));
        assert_eq!(
            assets.resolve_asset_path("assets/graphics"),
            data.join("assets/graphics")
        );
        assert_eq!(
            assets.strip_asset_prefix(&data.join(relative)),
            Some(Path::new("graphics/overlay.png"))
        );
        assert_eq!(
            assets.resolve_asset_path(exe.join(relative).to_str().expect("fixture path")),
            exe.join(relative)
        );
        assert_eq!(
            assets.resolve_asset_path("missing/asset.png"),
            Path::new("missing/asset.png")
        );
        let portable = AppDirs::portable_layout(exe.clone()).asset_paths(Some(&cwd));
        assert_eq!(
            portable.search_roots,
            [cwd.clone(), cwd.join("deadsync"), exe.clone()]
        );
        assert_eq!(portable.resolve_asset_path(relative), cwd.join(relative));
        for (source, next) in sources.windows(2).map(|pair| (pair[0], pair[1])) {
            std::fs::remove_file(source.join(relative)).expect("remove higher priority asset");
            assert_eq!(assets.resolve_asset_path(relative), next.join(relative));
        }
        std::fs::remove_dir_all(root).expect("remove overlay fixtures");
    }

    #[test]
    fn content_candidates_keep_missing_roots_for_later_reload() {
        let root = std::env::temp_dir().join("deadsync-missing-content-candidates");
        let cwd = root.join("work");
        let dirs = AppDirs::isolated_layout(root.join("data"), root.join("bundle"));
        assert!(dirs.extra_song_roots().contains(&root.join("bundle/songs")));
        assert!(
            dirs.extra_course_roots()
                .contains(&root.join("bundle/courses"))
        );
        assert_eq!(dirs.noteskin_roots()[0], root.join("data/assets/noteskins"));
        let portable = AppDirs::portable_layout(root.join("bundle"));
        assert!(portable.extra_song_roots().is_empty());
        assert!(portable.extra_course_roots().is_empty());
        assert_eq!(
            portable.media_roots("RandomMovies", Some(&cwd)),
            [
                root.join("bundle/RandomMovies"),
                cwd.join("RandomMovies"),
                cwd.join("deadsync/RandomMovies"),
            ]
        );
    }
}
