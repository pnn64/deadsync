use super::super::*;
use deadsync_theme::{PlatformRequest, RevealPathKind};

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
            HelpEntry::AppPath(AppPathKind::Data),
        ],
    },
    Item {
        id: ItemId::FldCacheDir,
        name: lookup_key("OptionsFolders", "CacheDir"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "CacheDirHelp")),
            HelpEntry::AppPath(AppPathKind::Cache),
        ],
    },
    Item {
        id: ItemId::FldSongs,
        name: lookup_key("OptionsFolders", "Songs"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "SongsHelp")),
            HelpEntry::AppPath(AppPathKind::Songs),
        ],
    },
    Item {
        id: ItemId::FldCourses,
        name: lookup_key("OptionsFolders", "Courses"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "CoursesHelp")),
            HelpEntry::AppPath(AppPathKind::Courses),
        ],
    },
    Item {
        id: ItemId::FldProfiles,
        name: lookup_key("OptionsFolders", "Profiles"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ProfilesHelp")),
            HelpEntry::AppPath(AppPathKind::Profiles),
        ],
    },
    Item {
        id: ItemId::FldScreenshots,
        name: lookup_key("OptionsFolders", "Screenshots"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ScreenshotsHelp")),
            HelpEntry::AppPath(AppPathKind::Screenshots),
        ],
    },
    Item {
        id: ItemId::FldLogFile,
        name: lookup_key("OptionsFolders", "LogFile"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "LogFileHelp")),
            HelpEntry::AppPath(AppPathKind::LogFile),
        ],
    },
    Item {
        id: ItemId::FldConfigFile,
        name: lookup_key("OptionsFolders", "ConfigFile"),
        help: &[
            HelpEntry::Paragraph(lookup_key("OptionsFoldersHelp", "ConfigFileHelp")),
            HelpEntry::AppPath(AppPathKind::ConfigFile),
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
fn folder_kind_for_row(id: SubRowId) -> Option<AppPathKind> {
    Some(match id {
        SubRowId::FoldersDataDir => AppPathKind::Data,
        SubRowId::FoldersCacheDir => AppPathKind::Cache,
        SubRowId::FoldersSongs => AppPathKind::Songs,
        SubRowId::FoldersCourses => AppPathKind::Courses,
        SubRowId::FoldersProfiles => AppPathKind::Profiles,
        SubRowId::FoldersScreenshots => AppPathKind::Screenshots,
        SubRowId::FoldersLogFile => AppPathKind::LogFile,
        SubRowId::FoldersConfigFile => AppPathKind::ConfigFile,
        _ => return None,
    })
}

pub(in crate::screens::options) fn folder_path_for_row(
    app_paths: &AppPathsView,
    id: SubRowId,
) -> Option<&std::path::Path> {
    Some(app_paths.get(folder_kind_for_row(id)?).path.as_path())
}

pub(in crate::screens::options) fn folder_reveal_request(
    app_paths: &AppPathsView,
    id: SubRowId,
) -> Option<PlatformRequest> {
    let path = folder_path_for_row(app_paths, id)?.to_path_buf();
    let kind = if matches!(id, SubRowId::FoldersLogFile | SubRowId::FoldersConfigFile) {
        RevealPathKind::File
    } else {
        RevealPathKind::Directory
    };
    Some(PlatformRequest::RevealPath { path, kind })
}
