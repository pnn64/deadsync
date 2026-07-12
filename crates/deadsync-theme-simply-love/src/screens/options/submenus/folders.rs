use super::super::*;
use deadlib_platform::dirs::app_dirs;
use deadsync_theme::{PlatformRequest, RevealPathKind};
use std::borrow::Cow;
use std::path::PathBuf;

/// Each folder row shows a single "Open" choice in the value column. Selecting
/// the row asks the shell to reveal the resolved path in the OS file explorer.
const OPEN_CHOICE: &[Choice] = &[localized_choice("Common", "Open")];

pub(in crate::screens::options) const FOLDERS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::FoldersDataDir,
        label: lookup_key("OptionsFolders", "DataDir"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersCacheDir,
        label: lookup_key("OptionsFolders", "CacheDir"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersSongs,
        label: lookup_key("OptionsFolders", "Songs"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersCourses,
        label: lookup_key("OptionsFolders", "Courses"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersProfiles,
        label: lookup_key("OptionsFolders", "Profiles"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersScreenshots,
        label: lookup_key("OptionsFolders", "Screenshots"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersLogFile,
        label: lookup_key("OptionsFolders", "LogFile"),
        choices: OPEN_CHOICE,
        inline: false,
    },
    SubRow {
        id: SubRowId::FoldersConfigFile,
        label: lookup_key("OptionsFolders", "ConfigFile"),
        choices: OPEN_CHOICE,
        inline: false,
    },
];

pub(in crate::screens::options) const FOLDERS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::FldDataDir,
        name: lookup_key("OptionsFolders", "DataDir"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "DataDirHelp")),
            HelpEntry::Dynamic(data_dir_path),
        ],
    },
    Item {
        id: ItemId::FldCacheDir,
        name: lookup_key("OptionsFolders", "CacheDir"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "CacheDirHelp")),
            HelpEntry::Dynamic(cache_dir_path),
        ],
    },
    Item {
        id: ItemId::FldSongs,
        name: lookup_key("OptionsFolders", "Songs"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "SongsHelp")),
            HelpEntry::Dynamic(songs_path),
        ],
    },
    Item {
        id: ItemId::FldCourses,
        name: lookup_key("OptionsFolders", "Courses"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "CoursesHelp")),
            HelpEntry::Dynamic(courses_path),
        ],
    },
    Item {
        id: ItemId::FldProfiles,
        name: lookup_key("OptionsFolders", "Profiles"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ProfilesHelp")),
            HelpEntry::Dynamic(profiles_path),
        ],
    },
    Item {
        id: ItemId::FldScreenshots,
        name: lookup_key("OptionsFolders", "Screenshots"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ScreenshotsHelp")),
            HelpEntry::Dynamic(screenshots_path),
        ],
    },
    Item {
        id: ItemId::FldLogFile,
        name: lookup_key("OptionsFolders", "LogFile"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "LogFileHelp")),
            HelpEntry::Dynamic(log_file_path),
        ],
    },
    Item {
        id: ItemId::FldConfigFile,
        name: lookup_key("OptionsFolders", "ConfigFile"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ConfigFileHelp")),
            HelpEntry::Dynamic(config_file_path),
        ],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

/// Maps a folder row id to the absolute path it represents. Returns `None`
/// for rows that don't belong to the Folders submenu.
pub(in crate::screens::options) fn folder_path_for_row(id: SubRowId) -> Option<PathBuf> {
    let dirs = app_dirs();
    let path = match id {
        SubRowId::FoldersDataDir => dirs.data_dir.clone(),
        SubRowId::FoldersCacheDir => dirs.cache_dir.clone(),
        SubRowId::FoldersSongs => dirs.songs_dir(),
        SubRowId::FoldersCourses => dirs.courses_dir(),
        SubRowId::FoldersProfiles => dirs.profiles_root(),
        SubRowId::FoldersScreenshots => dirs.screenshots_dir(),
        SubRowId::FoldersLogFile => dirs.log_path(),
        SubRowId::FoldersConfigFile => dirs.config_path(),
        _ => return None,
    };
    Some(path)
}

pub(in crate::screens::options) fn folder_reveal_request(id: SubRowId) -> Option<PlatformRequest> {
    let path = folder_path_for_row(id)?;
    let kind = if matches!(id, SubRowId::FoldersLogFile | SubRowId::FoldersConfigFile) {
        RevealPathKind::File
    } else {
        RevealPathKind::Directory
    };
    Some(PlatformRequest::RevealPath { path, kind })
}

fn data_dir_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().data_dir))
}

fn cache_dir_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().cache_dir))
}

fn songs_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().songs_dir()))
}

fn courses_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().courses_dir()))
}

fn profiles_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().profiles_root()))
}

fn screenshots_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().screenshots_dir()))
}

fn log_file_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().log_path()))
}

fn config_file_path() -> Cow<'static, str> {
    Cow::Owned(shorthand(&app_dirs().config_path()))
}

/// Returns a platform-friendly shorthand for an absolute path:
/// * Windows: `%APPDATA%\...`, `%LOCALAPPDATA%\...`, or `%USERPROFILE%\...`
///   when the path lives under one of those env-resolved roots.
/// * macOS / Linux / BSD: `~/...` when the path lives under the user's home.
///
/// Falls back to the full absolute path string when no shortening prefix
/// applies. Always lossless — the shorthand always points to the same
/// location as the original path.
fn shorthand(path: &std::path::Path) -> String {
    if let Some(short) = try_shorthand(path) {
        return short;
    }
    path.display().to_string()
}

#[cfg(target_os = "windows")]
fn try_shorthand(path: &std::path::Path) -> Option<String> {
    // Order matters: try the most specific env var first so we don't show
    // %USERPROFILE%\AppData\Roaming when %APPDATA% is shorter.
    for (var, label) in [
        ("APPDATA", "%APPDATA%"),
        ("LOCALAPPDATA", "%LOCALAPPDATA%"),
        ("USERPROFILE", "%USERPROFILE%"),
    ] {
        if let Some(replaced) = replace_prefix(path, &std::env::var_os(var)?, label) {
            return Some(replaced);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn try_shorthand(path: &std::path::Path) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    replace_prefix(path, &home, "~")
}

fn replace_prefix(path: &std::path::Path, prefix: &std::ffi::OsStr, label: &str) -> Option<String> {
    let prefix_path = std::path::Path::new(prefix);
    let rest = path.strip_prefix(prefix_path).ok()?;
    if rest.as_os_str().is_empty() {
        return Some(label.to_owned());
    }
    let sep = std::path::MAIN_SEPARATOR;
    Some(format!("{label}{sep}{}", rest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn replace_prefix_strips_matching_root() {
        let path = Path::new("/home/user/.deadsync/save");
        let result = replace_prefix(path, std::ffi::OsStr::new("/home/user"), "~");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(result, Some(format!("~{sep}.deadsync/save")));
    }

    #[test]
    fn replace_prefix_returns_label_for_exact_match() {
        let path = Path::new("/home/user");
        let result = replace_prefix(path, std::ffi::OsStr::new("/home/user"), "~");
        assert_eq!(result.as_deref(), Some("~"));
    }

    #[test]
    fn replace_prefix_returns_none_when_outside_root() {
        let path = Path::new("/var/log/deadsync.log");
        let result = replace_prefix(path, std::ffi::OsStr::new("/home/user"), "~");
        assert_eq!(result, None);
    }

    #[test]
    fn shorthand_falls_back_to_full_path() {
        // Path with no env var prefix should round-trip unchanged.
        let path = Path::new("/definitely/not/a/home/dir/x");
        let result = shorthand(path);
        assert_eq!(result, path.display().to_string());
    }
}
