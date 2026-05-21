use super::super::*;
use crate::config::dirs::app_dirs;
use crate::engine::open_path;
use std::borrow::Cow;
use std::path::PathBuf;

/// Each folder row shows a single "Open" choice in the value column. Selecting
/// the row triggers `open_folder_for_row`, which reveals the resolved path in
/// the OS file explorer.
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

/// Returns `true` if the given row id belongs to the Folders submenu.
#[inline]
pub(in crate::screens::options) fn is_folder_row(id: SubRowId) -> bool {
    folder_path_for_row(id).is_some()
}

/// Reveals the folder corresponding to `id` in the OS file explorer. For
/// directory rows the directory itself is opened; for file rows the parent
/// directory is opened and the file is highlighted on platforms that support
/// it. Missing directories are created lazily so we never point the user at
/// a nonexistent path.
pub(in crate::screens::options) fn open_folder_for_row(id: SubRowId) {
    let Some(path) = folder_path_for_row(id) else {
        return;
    };
    crate::config::dirs::ensure_dirs_exist();
    let is_file_row = matches!(
        id,
        SubRowId::FoldersLogFile | SubRowId::FoldersConfigFile
    );
    if !is_file_row && !path.exists() && let Err(e) = std::fs::create_dir_all(&path) {
        log::warn!(
            "Failed to create folder before opening '{}': {e}",
            path.display()
        );
    }
    if let Err(e) = open_path::reveal(&path) {
        log::warn!("Failed to open '{}' in file explorer: {e}", path.display());
    }
}

fn data_dir_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().data_dir.display().to_string())
}

fn cache_dir_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().cache_dir.display().to_string())
}

fn songs_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().songs_dir().display().to_string())
}

fn courses_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().courses_dir().display().to_string())
}

fn profiles_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().profiles_root().display().to_string())
}

fn screenshots_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().screenshots_dir().display().to_string())
}

fn log_file_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().log_path().display().to_string())
}

fn config_file_path() -> Cow<'static, str> {
    Cow::Owned(app_dirs().config_path().display().to_string())
}
