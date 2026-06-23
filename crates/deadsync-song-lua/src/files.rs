use std::path::{Path, PathBuf};
use std::{fs, io};

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

pub fn resolve_compat_path(song_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        song_dir.join(path)
    }
}

pub fn fileman_dir_listing(
    song_dir: &Path,
    raw_path: &str,
    only_dirs: bool,
    return_path_too: bool,
) -> io::Result<Vec<String>> {
    let path = resolve_compat_path(song_dir, raw_path);
    let mut entries = Vec::new();
    if path.is_dir() {
        entries = fileman_read_dir(&path, None, only_dirs, return_path_too)?;
    } else if let Some(pattern) = path.file_name().and_then(|name| name.to_str())
        && pattern.bytes().any(|byte| matches!(byte, b'*' | b'?'))
        && let Some(parent) = path.parent()
    {
        entries = fileman_read_dir(parent, Some(pattern), only_dirs, return_path_too)?;
    } else if path.exists() && (!only_dirs || path.is_dir()) {
        entries.push(fileman_entry_name(&path, return_path_too));
    }

    entries.sort_unstable();
    Ok(entries)
}

pub fn find_compat_files(song_dir: &Path, dir: &str, extension: &str) -> io::Result<Vec<String>> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    let path = resolve_compat_path(song_dir, dir);
    let mut files = if path.is_dir() {
        fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case(&extension))
            })
            .map(|path| file_path_string(path.as_path()))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    files.sort_unstable();
    Ok(files)
}

fn fileman_read_dir(
    path: &Path,
    pattern: Option<&str>,
    only_dirs: bool,
    return_path_too: bool,
) -> io::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)?.filter_map(Result::ok) {
        let entry_path = entry.path();
        if only_dirs && !entry_path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if pattern.is_some_and(|pattern| !wildcard_matches(pattern, &name)) {
            continue;
        }
        entries.push(if return_path_too {
            file_path_string(entry_path.as_path())
        } else {
            name
        });
    }
    Ok(entries)
}

fn fileman_entry_name(path: &Path, return_path_too: bool) -> String {
    if return_path_too {
        return file_path_string(path);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_idx, mut text_idx) = (0, 0);
    let (mut star_idx, mut star_text_idx) = (None, 0);
    while text_idx < text.len() {
        if pattern_idx < pattern.len()
            && (pattern[pattern_idx] == b'?' || pattern[pattern_idx] == text[text_idx])
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            star_text_idx = text_idx;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            star_text_idx += 1;
            text_idx = star_text_idx;
        } else {
            return false;
        }
    }
    pattern[pattern_idx..].iter().all(|byte| *byte == b'*')
}

pub fn path_basename(text: &str) -> &str {
    let trimmed = text.trim_end_matches(|c| c == '/' || c == '\\');
    trimmed
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or_default()
}

pub fn actor_util_class_registered(name: &str) -> bool {
    matches!(
        name,
        "Actor"
            | "ActorFrame"
            | "Sprite"
            | "Banner"
            | "ActorMultiVertex"
            | "Sound"
            | "BitmapText"
            | "RollingNumbers"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "CourseContentsList"
            | "DeviceList"
            | "InputList"
            | "Model"
            | "Quad"
            | "ActorProxy"
            | "ActorFrameTexture"
    )
}

pub fn actor_util_file_type(path: &str) -> &'static str {
    let path = Path::new(path);
    if path.is_dir() {
        return "FileType_Directory";
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp") => "FileType_Bitmap",
        Some("mp4" | "avi" | "mov" | "mkv" | "webm" | "mpeg" | "mpg") => "FileType_Movie",
        Some("ogg" | "oga" | "mp3" | "wav" | "flac" | "opus") => "FileType_Sound",
        Some("lua") => "FileType_Lua",
        Some("xml" | "ini" | "txt" | "json" | "ssc" | "sm") => "FileType_Text",
        _ => "FileType_Unknown",
    }
}

pub fn strip_sprite_hints(filename: &str) -> String {
    let mut text = filename.replace(" (doubleres)", "");
    if text
        .as_bytes()
        .get(text.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".png"))
    {
        text.truncate(text.len() - 4);
    }
    if let Some(space) = text.rfind(' ')
        && frame_hint(&text[space + 1..])
    {
        text.truncate(space);
    }
    text
}

fn frame_hint(text: &str) -> bool {
    let Some((wide, tall)) = text.split_once('x') else {
        return false;
    };
    !wide.is_empty()
        && !tall.is_empty()
        && wide.bytes().all(|byte| byte.is_ascii_digit())
        && tall.bytes().all(|byte| byte.is_ascii_digit())
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

    #[test]
    fn wildcard_matching_supports_fileman_patterns() {
        assert!(wildcard_matches("*.lua", "default.lua"));
        assert!(wildcard_matches(
            "Screen?ameplay*",
            "ScreenGameplay underlay"
        ));
        assert!(!wildcard_matches("*.lua", "banner.png"));
    }

    #[test]
    fn compat_path_resolves_relative_to_song_dir() {
        let song_dir = Path::new("songs/pack/song");
        assert_eq!(
            resolve_compat_path(song_dir, "BG/default.lua"),
            Path::new("songs/pack/song").join("BG/default.lua")
        );
    }

    #[test]
    fn path_basename_handles_both_separators() {
        assert_eq!(path_basename("songs/pack/song/file.lua"), "file.lua");
        assert_eq!(path_basename("songs\\pack\\song\\"), "song");
    }

    #[test]
    fn strip_sprite_hints_removes_stepmania_suffixes() {
        assert_eq!(
            strip_sprite_hints("Tap Note 4x2 (doubleres).png"),
            "Tap Note"
        );
        assert_eq!(strip_sprite_hints("mine.png"), "mine");
    }
}
