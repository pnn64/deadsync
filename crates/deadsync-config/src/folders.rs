use crate::ini::SimpleIni;

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
}
