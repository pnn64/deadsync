use std::fs;
use std::path::{Path, PathBuf};

pub fn song_group_name(song_dir: &Path) -> String {
    song_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn song_music_path(song_dir: &Path) -> Option<PathBuf> {
    song_named_file_path(
        song_dir,
        &["music", "song", "audio"],
        is_song_lua_audio_path,
    )
    .or_else(|| song_first_file_path(song_dir, is_song_lua_audio_path))
}

pub fn song_named_image_path(song_dir: &Path, stems: &[&str]) -> Option<PathBuf> {
    song_named_file_path(song_dir, stems, is_song_lua_image_path)
}

pub fn song_simfile_path(song_dir: &Path) -> Option<PathBuf> {
    song_first_file_path(song_dir, is_song_lua_simfile_path)
}

fn song_named_file_path(
    song_dir: &Path,
    stems: &[&str],
    predicate: fn(&Path) -> bool,
) -> Option<PathBuf> {
    let files = song_dir_files(song_dir);
    for stem in stems {
        if let Some(path) = files
            .iter()
            .find(|path| predicate(path) && path_stem_eq(path, stem))
        {
            return Some(path.clone());
        }
    }
    for stem in stems {
        if let Some(path) = files.iter().find(|path| {
            predicate(path)
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.to_ascii_lowercase().contains(stem))
        }) {
            return Some(path.clone());
        }
    }
    None
}

fn song_first_file_path(song_dir: &Path, predicate: fn(&Path) -> bool) -> Option<PathBuf> {
    song_dir_files(song_dir)
        .into_iter()
        .find(|path| predicate(path))
}

fn song_dir_files(song_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(song_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort_by_key(|path| file_path_string(path));
    files
}

fn path_stem_eq(path: &Path, stem: &str) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(stem))
}

#[inline(always)]
pub fn song_dir_string(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    if !text.ends_with('/') {
        text.push('/');
    }
    text
}

#[inline(always)]
pub fn file_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[inline(always)]
pub fn is_song_lua_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "qoi" | "tif" | "tiff"
            )
        })
}

#[inline(always)]
pub fn is_song_lua_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp4" | "avi" | "webm" | "mov" | "mkv" | "mpg" | "mpeg" | "ogv"
            )
        })
}

#[inline(always)]
pub fn is_song_lua_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "ogg" | "mp3" | "wav" | "flac" | "opus" | "m4a" | "aac"
            )
        })
}

#[inline(always)]
pub fn is_song_lua_simfile_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "sm" | "ssc"))
}

#[inline(always)]
pub fn is_song_lua_media_path(path: &Path) -> bool {
    is_song_lua_image_path(path) || is_song_lua_video_path(path) || is_song_lua_audio_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_path_detection_accepts_known_extensions() {
        assert!(is_song_lua_image_path(Path::new("banner.PNG")));
        assert!(is_song_lua_video_path(Path::new("bg.webm")));
        assert!(is_song_lua_audio_path(Path::new("song.ogg")));
        assert!(is_song_lua_simfile_path(Path::new("chart.ssc")));
        assert!(is_song_lua_media_path(Path::new("music.flac")));
        assert!(!is_song_lua_media_path(Path::new("chart.ssc")));
    }

    #[test]
    fn path_strings_use_forward_slashes() {
        assert_eq!(
            file_path_string(Path::new("songs\\pack\\song.ogg")),
            "songs/pack/song.ogg"
        );
        assert_eq!(
            song_dir_string(Path::new("songs\\pack\\song")),
            "songs/pack/song/"
        );
    }
}
