use log::{info, warn};
use std::path::PathBuf;

/// Single source of truth for all resolved application directories.
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
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("deadsync.ini")
    }

    pub fn log_path(&self) -> PathBuf {
        self.data_dir.join("deadsync.log")
    }

    pub fn profiles_root(&self) -> PathBuf {
        self.data_dir.join("save").join("profiles")
    }

    pub fn profile_dir(&self, id: &str) -> PathBuf {
        self.profiles_root().join(id)
    }

    pub fn screenshots_dir(&self) -> PathBuf {
        self.data_dir.join("save").join("screenshots")
    }

    pub fn songs_dir(&self) -> PathBuf {
        self.data_dir.join("songs")
    }

    pub fn courses_dir(&self) -> PathBuf {
        self.data_dir.join("courses")
    }

    pub fn song_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("songs")
    }

    pub fn banner_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("banner")
    }

    pub fn cdtitle_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("cdtitle")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir.join("downloads")
    }

    pub fn noteskin_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("noteskins")
    }

    pub fn unlock_cache_path(&self) -> PathBuf {
        self.cache_dir.join("unlocks-cache.json")
    }

    /// Returns additional song scan roots beyond the primary `songs_dir()`.
    /// In platform-native mode, also includes `{exe_dir}/songs` so bundled songs
    /// are found even when the data dir is elsewhere.
    pub fn extra_song_roots(&self) -> Vec<PathBuf> {
        if self.portable {
            return Vec::new();
        }
        let exe_songs = self.exe_dir.join("songs");
        if exe_songs.is_dir() && exe_songs != self.songs_dir() {
            vec![exe_songs]
        } else {
            Vec::new()
        }
    }

    /// Returns additional course roots beyond the primary `courses_dir()`.
    /// In platform-native mode, also includes `{exe_dir}/courses`.
    pub fn extra_course_roots(&self) -> Vec<PathBuf> {
        if self.portable {
            return Vec::new();
        }
        let exe_courses = self.exe_dir.join("courses");
        if exe_courses.is_dir() && exe_courses != self.courses_dir() {
            vec![exe_courses]
        } else {
            Vec::new()
        }
    }

    /// Resolves a relative asset path (e.g. `"assets/sounds/change.ogg"`) by
    /// checking the data dir overlay first. In platform-native mode, if the
    /// file or directory exists at `{data_dir}/{path}`, returns that absolute
    /// path; otherwise returns the original path (which resolves to
    /// `{exe_dir}/{path}` via CWD).
    pub fn resolve_asset_path(&self, path: &str) -> PathBuf {
        if !self.portable {
            let candidate = self.data_dir.join(path);
            if candidate.exists() {
                return candidate;
            }
        }
        PathBuf::from(path)
    }

    /// Strips the data-dir or exe-dir `assets/` prefix from an absolute path,
    /// returning the relative portion after `assets/`. Returns `None` if the
    /// path doesn't start with either prefix.
    pub fn strip_asset_prefix<'a>(&self, path: &'a std::path::Path) -> Option<&'a std::path::Path> {
        let data_assets = self.data_dir.join("assets");
        let exe_assets = self.exe_dir.join("assets");
        path.strip_prefix(&data_assets)
            .or_else(|_| path.strip_prefix(&exe_assets))
            .ok()
    }

    /// Returns all root directories where noteskins may be found.
    /// In platform-native mode the data-dir variant is listed first so that
    /// user-added skins take priority over bundled ones.
    pub fn noteskin_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::with_capacity(2);
        if !self.portable {
            let data_root = self.data_dir.join("assets").join("noteskins");
            if data_root.is_dir() {
                roots.push(data_root);
            }
        }
        roots.push(self.exe_dir.join("assets").join("noteskins"));
        roots
    }
}

static APP_DIRS: std::sync::LazyLock<AppDirs> = std::sync::LazyLock::new(AppDirs::resolve);

/// Returns the globally-resolved application directories.
#[inline(always)]
pub fn app_dirs() -> &'static AppDirs {
    &APP_DIRS
}

#[cfg(any(windows, test))]
fn native_cache_dir_for_data_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("cache")
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

    fn resolve() -> Self {
        let exe_path = std::env::current_exe().expect("cannot determine exe path");
        let exe_dir = Self::runtime_root_from_exe_path(&exe_path);

        if Self::has_portable_marker(&exe_dir) {
            return Self {
                data_dir: exe_dir.clone(),
                cache_dir: exe_dir.clone(),
                exe_dir,
                portable: true,
            };
        }

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            let home_dir = std::env::var_os("HOME")
                .map(PathBuf::from)
                .expect("cannot determine home directory");
            let data_dir = home_dir.join(".deadsync");
            Self {
                cache_dir: data_dir.join("cache"),
                data_dir,
                exe_dir,
                portable: false,
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        {
            let proj = directories::ProjectDirs::from("", "", "deadsync")
                .expect("cannot determine platform directories");

            // On Windows, `data_dir()` appends a `\data` subdirectory
            // (e.g. `%APPDATA%\deadsync\data`). We want `%APPDATA%\deadsync`
            // directly, so use `config_dir().parent()` which strips the suffix.
            // On macOS, `data_dir()` already gives the flat path we want.
            #[cfg(windows)]
            let data_dir = proj
                .config_dir()
                .parent()
                .expect("config_dir has no parent")
                .to_path_buf();
            #[cfg(not(windows))]
            let data_dir = proj.data_dir().to_path_buf();

            #[cfg(windows)]
            let cache_dir = native_cache_dir_for_data_dir(&data_dir);
            #[cfg(not(windows))]
            let cache_dir = proj.cache_dir().to_path_buf();

            Self {
                data_dir,
                cache_dir,
                exe_dir,
                portable: false,
            }
        }
    }
}

/// Creates the data and cache directories if they don't exist.
pub fn ensure_dirs_exist() {
    let dirs = app_dirs();
    for dir in [&dirs.data_dir, &dirs.cache_dir] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Failed to create directory {}: {e}", dir.display());
        }
    }
}

/// Attempts to migrate mutable data from the exe directory to platform-native
/// dirs. Only runs in platform-native mode when data exists at the old
/// location but not yet at the new location.
pub fn maybe_migrate_from_exe_dir() {
    let dirs = app_dirs();
    if dirs.portable {
        return;
    }

    let exe_config = dirs.exe_dir.join("deadsync.ini");
    let native_config = dirs.config_path();

    if !exe_config.exists() || native_config.exists() {
        return;
    }

    warn!(
        "Migrating data from exe directory ({}) to platform data directory ({})...",
        dirs.exe_dir.display(),
        dirs.data_dir.display()
    );

    copy_item(&exe_config, &native_config);
    copy_dir_if_exists(&dirs.exe_dir.join("save"), &dirs.data_dir.join("save"));

    // Migrate cache subdirectories.
    let exe_cache = dirs.exe_dir.join("cache");
    if exe_cache.is_dir() {
        copy_dir_if_exists(&exe_cache.join("songs"), &dirs.song_cache_dir());
        copy_dir_if_exists(&exe_cache.join("banner"), &dirs.banner_cache_dir());
        copy_dir_if_exists(&exe_cache.join("cdtitle"), &dirs.cdtitle_cache_dir());
    }

    warn!("Migration complete. Original files were NOT deleted.");
}

fn copy_item(src: &std::path::Path, dst: &std::path::Path) {
    if let Some(parent) = dst.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Failed to create parent dir for {}: {e}", dst.display());
            return;
        }
    }
    match std::fs::copy(src, dst) {
        Ok(_) => info!("  Copied {} -> {}", src.display(), dst.display()),
        Err(e) => warn!(
            "  Failed to copy {} -> {}: {e}",
            src.display(),
            dst.display()
        ),
    }
}

fn copy_dir_if_exists(src: &std::path::Path, dst: &std::path::Path) {
    if !src.is_dir() {
        return;
    }
    info!(
        "  Copying directory {} -> {} ...",
        src.display(),
        dst.display()
    );
    if let Err(e) = copy_dir_recursive(src, dst) {
        warn!(
            "  Failed to copy directory {} -> {}: {e}",
            src.display(),
            dst.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{AppDirs, native_cache_dir_for_data_dir};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn native_cache_dir_is_nested_under_data_dir() {
        assert_eq!(
            native_cache_dir_for_data_dir(Path::new("/tmp/deadsync")),
            Path::new("/tmp/deadsync/cache")
        );
    }

    #[test]
    fn runtime_root_uses_parent_profile_dir_for_cargo_test_binaries() {
        let unique = format!(
            "deadsync-dirs-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let exe_path = root.join("target/debug/deps/deadsync-test");
        let expected = root.join("target/debug");
        std::fs::create_dir_all(expected.join("assets")).expect("create mock assets dir");

        assert_eq!(AppDirs::runtime_root_from_exe_path(&exe_path), expected);

        std::fs::remove_dir_all(root).expect("cleanup mock target dir");
    }

    #[test]
    fn runtime_root_uses_parent_profile_dir_for_cargo_test_binaries_with_portable_ini() {
        let unique = format!(
            "deadsync-dirs-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
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
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
