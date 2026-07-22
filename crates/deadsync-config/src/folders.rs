use crate::ini::SimpleIni;
use crate::writer::push_line;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditionalSongFolder {
    pub path: String,
    pub writable: bool,
}

pub fn load_additional_song_folders(conf: &SimpleIni) -> Vec<AdditionalSongFolder> {
    let read_only = conf
        .get("Options", "AdditionalSongFoldersReadOnly")
        .unwrap_or_default();
    let writable_raw = conf
        .get("Options", "AdditionalSongFoldersWritable")
        .unwrap_or_default();
    let deprecated = conf
        .get("Options", "AdditionalSongFolders")
        .unwrap_or_default();
    let writable = if writable_raw.trim().is_empty() {
        deprecated
    } else {
        writable_raw
    };

    let mut folders = Vec::new();
    push_additional_song_folders(&read_only, false, &mut folders);
    push_additional_song_folders(&writable, true, &mut folders);
    folders
}

fn push_additional_song_folders(raw: &str, writable: bool, out: &mut Vec<AdditionalSongFolder>) {
    out.extend(
        raw.split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| AdditionalSongFolder {
                path: path.to_string(),
                writable,
            }),
    );
}

pub fn additional_song_folder_paths(folders: &[AdditionalSongFolder], writable: bool) -> String {
    let mut out = String::new();
    for folder in folders.iter().filter(|folder| folder.writable == writable) {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(folder.path.as_str());
    }
    out
}

pub fn push_additional_song_folder_option_lines(
    content: &mut String,
    folders: &[AdditionalSongFolder],
) {
    push_line(content, "AdditionalSongFolders", "");
    push_line(
        content,
        "AdditionalSongFoldersWritable",
        additional_song_folder_paths(folders, true),
    );
    push_line(
        content,
        "AdditionalSongFoldersReadOnly",
        additional_song_folder_paths(folders, false),
    );
}

pub fn song_path_is_writable_for_roots(path: &Path, roots: &[AdditionalSongFolder]) -> bool {
    let path = canonical_or_raw(path);
    let mut best: Option<(usize, bool)> = None;
    for root in roots {
        let root_path = canonical_or_raw(Path::new(root.path.as_str()));
        let Some(len) = root_prefix_len(path.as_path(), root_path.as_path()) else {
            continue;
        };
        if best.is_none_or(|(best_len, _)| len >= best_len) {
            best = Some((len, root.writable));
        }
    }
    best.is_none_or(|(_, writable)| writable)
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn root_prefix_len(path: &Path, root: &Path) -> Option<usize> {
    let mut path_components = path.components();
    let mut len = 0usize;
    for root_component in root.components() {
        let path_component = path_components.next()?;
        if !path_components_equal(path_component.as_os_str(), root_component.as_os_str()) {
            return None;
        }
        len += 1;
    }
    Some(len)
}

#[cfg(windows)]
fn path_components_equal(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(not(windows))]
fn path_components_equal(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

    fn folder(path: &str, writable: bool) -> AdditionalSongFolder {
        AdditionalSongFolder {
            path: path.to_string(),
            writable,
        }
    }

    #[test]
    fn keeps_read_only_when_deprecated_key_empty() {
        let conf = ini("[Options]\n\
AdditionalSongFolders=\n\
AdditionalSongFoldersReadOnly=G:\\itgmania\\songs\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("G:\\itgmania\\songs", false)]
        );
    }

    #[test]
    fn migrates_deprecated_key_to_writable() {
        let conf = ini("[Options]\nAdditionalSongFolders=D:\\songs\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("D:\\songs", true)]
        );
    }

    #[test]
    fn prefers_writable_key_over_deprecated_key() {
        let conf = ini("[Options]\n\
AdditionalSongFolders=D:\\old\n\
AdditionalSongFoldersWritable=D:\\new\n\
AdditionalSongFoldersReadOnly=G:\\readonly\n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![folder("G:\\readonly", false), folder("D:\\new", true),]
        );
    }

    #[test]
    fn trims_empty_entries() {
        let conf = ini("[Options]\n\
AdditionalSongFoldersWritable= D:\\a ,, D:\\b \n\
AdditionalSongFoldersReadOnly= , G:\\ro , \n");

        assert_eq!(
            load_additional_song_folders(&conf),
            vec![
                folder("G:\\ro", false),
                folder("D:\\a", true),
                folder("D:\\b", true),
            ]
        );
    }

    #[test]
    fn paths_split_writable_and_read_only() {
        let folders = [
            folder("G:\\readonly", false),
            folder("D:\\writable-a", true),
            folder("E:\\writable-b", true),
        ];

        assert_eq!(
            additional_song_folder_paths(&folders, false),
            "G:\\readonly"
        );
        assert_eq!(
            additional_song_folder_paths(&folders, true),
            "D:\\writable-a,E:\\writable-b"
        );
    }

    #[test]
    fn writes_additional_song_folder_option_lines() {
        let folders = [
            folder("G:\\readonly", false),
            folder("D:\\writable-a", true),
            folder("E:\\writable-b", true),
        ];
        let mut content = String::new();

        push_additional_song_folder_option_lines(&mut content, &folders);

        assert_eq!(
            content,
            concat!(
                "AdditionalSongFolders=\n",
                "AdditionalSongFoldersWritable=D:\\writable-a,E:\\writable-b\n",
                "AdditionalSongFoldersReadOnly=G:\\readonly\n",
            ),
        );
    }

    #[test]
    fn song_path_writable_defaults_to_true_outside_additional_roots() {
        assert!(song_path_is_writable_for_roots(
            Path::new("Songs/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }

    #[test]
    fn song_path_writable_rejects_read_only_additional_root() {
        assert!(!song_path_is_writable_for_roots(
            Path::new("ExtraSongs/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }

    #[test]
    fn song_path_writable_prefers_longest_matching_root() {
        assert!(song_path_is_writable_for_roots(
            Path::new("ExtraSongs/WritablePack/song.ssc"),
            &[
                folder("ExtraSongs", false),
                folder("ExtraSongs/WritablePack", true),
            ]
        ));
    }

    #[test]
    fn root_prefix_does_not_match_partial_directory_names() {
        assert!(song_path_is_writable_for_roots(
            Path::new("ExtraSongs2/Pack/song.ssc"),
            &[folder("ExtraSongs", false)]
        ));
    }
}
