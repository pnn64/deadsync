// Frozen from 462b31ec5 (0.5.1705).
use super::*;
use crate::writer::push_line;

fn never_cache_list_value(list: &[String]) -> String {
    list.join(",")
}

pub(super) fn additional_song_folder_paths(
    folders: &[AdditionalSongFolder],
    writable: bool,
) -> String {
    let mut out = String::new();
    for folder in folders.iter().filter(|folder| folder.writable == writable) {
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(folder.path.as_str());
    }
    out
}

pub(super) fn push_additional_song_folder_option_lines(
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

pub(super) fn push_never_cache_list_option_line(content: &mut String, list: &[String]) {
    push_line(content, "NeverCacheList", never_cache_list_value(list));
}
