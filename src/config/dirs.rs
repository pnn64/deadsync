use log::{info, warn};
use std::path::PathBuf;

/// Single source of truth for all resolved application directories.
pub struct AppDirs {
    /// Root for user data (config, saves, songs, courses, log).
    pub data_dir: PathBuf,
    /// Root for regenerable cache data.
    pub cache_dir: PathBuf,
    /// Directory containing the executable.
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
}

static APP_DIRS: std::sync::LazyLock<AppDirs> = std::sync::LazyLock::new(AppDirs::resolve);

/// Returns the globally-resolved application directories.
#[inline(always)]
pub fn app_dirs() -> &'static AppDirs {
    &APP_DIRS
}

impl AppDirs {
    fn resolve() -> Self {
        let exe_dir = std::env::current_exe()
            .expect("cannot determine exe path")
            .parent()
            .expect("exe has no parent dir")
            .to_path_buf();

        if exe_dir.join("portable.txt").exists() {
            return Self {
                data_dir: exe_dir.clone(),
                cache_dir: exe_dir.clone(),
                exe_dir,
                portable: true,
            };
        }

        let proj = directories::ProjectDirs::from("", "", "deadsync")
            .expect("cannot determine platform directories");

        // On Windows, `data_dir()` appends a `\data` subdirectory
        // (e.g. `%APPDATA%\deadsync\data`). We want `%APPDATA%\deadsync`
        // directly, so use `config_dir().parent()` which strips the suffix.
        // On Linux/macOS, `data_dir()` already gives the flat path we want.
        #[cfg(windows)]
        let data_dir = proj
            .config_dir()
            .parent()
            .expect("config_dir has no parent")
            .to_path_buf();
        #[cfg(not(windows))]
        let data_dir = proj.data_dir().to_path_buf();

        Self {
            data_dir,
            cache_dir: proj.cache_dir().to_path_buf(),
            exe_dir,
            portable: false,
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

/// Attempts to migrate data from the exe directory to platform-native dirs.
/// Only runs in platform-native mode when data exists at the old location but
/// not yet at the new location.
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
    copy_dir_if_exists(&dirs.exe_dir.join("songs"), &dirs.songs_dir());
    copy_dir_if_exists(&dirs.exe_dir.join("courses"), &dirs.courses_dir());

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
            warn!(
                "Failed to create parent dir for {}: {e}",
                dst.display()
            );
            return;
        }
    }
    match std::fs::copy(src, dst) {
        Ok(_) => info!("  Copied {} -> {}", src.display(), dst.display()),
        Err(e) => warn!("  Failed to copy {} -> {}: {e}", src.display(), dst.display()),
    }
}

fn copy_dir_if_exists(src: &std::path::Path, dst: &std::path::Path) {
    if !src.is_dir() {
        return;
    }
    info!("  Copying directory {} -> {} ...", src.display(), dst.display());
    if let Err(e) = copy_dir_recursive(src, dst) {
        warn!("  Failed to copy directory {} -> {}: {e}", src.display(), dst.display());
    }
}

fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> Result<(), std::io::Error> {
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
