use mlua::{Function, Lua, MultiValue, Table, Value};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::lua_util::method_arg;
use crate::runtime::note_song_lua_side_effect;
use crate::values::{read_boolish, read_string};

pub fn song_group_name(song_dir: &Path) -> String {
    song_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn song_lookup_matches(query: &str, song_dir: &str, group: &str, title: &str) -> bool {
    let query = query.trim().replace('\\', "/");
    !query.is_empty()
        && (query == song_dir
            || song_dir.contains(query.as_str())
            || query.eq_ignore_ascii_case(group)
            || query.eq_ignore_ascii_case(title))
}

pub fn theme_path(kind: &str, group: &str, name: &str) -> String {
    let group = group.trim_matches('/');
    let name = name.trim_start_matches('/');
    if group.is_empty() {
        format!("{}{kind}/{name}", crate::SONG_LUA_THEME_PATH_PREFIX)
    } else {
        format!("{}{kind}/{group}/{name}", crate::SONG_LUA_THEME_PATH_PREFIX)
    }
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

pub fn create_fileman_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let fileman = lua.create_table()?;
    let song_dir = song_dir.to_path_buf();
    let listing_song_dir = song_dir.clone();
    fileman.set(
        "GetDirListing",
        lua.create_function(move |lua, args: MultiValue| {
            fileman_dir_listing_table(lua, &listing_song_dir, &args).map(Value::Table)
        })?,
    )?;
    let file_song_dir = song_dir.clone();
    fileman.set(
        "DoesFileExist",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(resolve_compat_path(&file_song_dir, raw_path.as_str()).exists())
        })?,
    )?;
    let size_song_dir = song_dir.clone();
    fileman.set(
        "GetFileSizeBytes",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            let size = resolve_compat_path(&size_song_dir, raw_path.as_str())
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            Ok(size)
        })?,
    )?;
    fileman.set(
        "GetHashForFile",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    fileman.set(
        "Copy",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for (name, value) in [("CreateDir", true), ("Remove", true), ("Unzip", false)] {
        fileman.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(value)
            })?,
        )?;
    }
    fileman.set(
        "FlushDirCache",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(fileman)
}

fn fileman_dir_listing_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(table);
    };
    let only_dirs = method_arg(args, 1)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);
    let return_path_too = method_arg(args, 2)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);

    let entries = fileman_dir_listing(song_dir, raw_path.as_str(), only_dirs, return_path_too)
        .map_err(mlua::Error::external)?;
    for (idx, entry) in entries.into_iter().enumerate() {
        table.raw_set(idx + 1, entry)?;
    }
    Ok(table)
}

pub fn create_lua_compat_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let read_song_dir = song_dir.to_path_buf();
    table.set(
        "ReadFile",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let path = resolve_compat_path(&read_song_dir, &raw_path);
            match fs::read_to_string(path) {
                Ok(text) => Ok(Value::String(lua.create_string(&text)?)),
                Err(_) => Ok(Value::Nil),
            }
        })?,
    )?;
    table.set(
        "WriteFile",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(true)
        })?,
    )?;
    table.set(
        "ReportScriptError",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
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

pub fn create_find_files_function(lua: &Lua, song_dir: &Path) -> mlua::Result<Function> {
    let song_dir = song_dir.to_path_buf();
    lua.create_function(move |lua, args: MultiValue| find_compat_files_table(lua, &song_dir, &args))
}

fn find_compat_files_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let dir = args
        .front()
        .cloned()
        .and_then(read_string)
        .unwrap_or_default();
    let extension = args
        .get(1)
        .cloned()
        .and_then(read_string)
        .unwrap_or_else(|| "ogg".to_string())
        .to_string();
    let files = find_compat_files(song_dir, &dir, &extension).map_err(mlua::Error::external)?;
    let table = lua.create_table()?;
    for (index, file) in files.into_iter().enumerate() {
        table.raw_set(index + 1, file)?;
    }
    Ok(table)
}

pub fn create_actor_util_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let resolve_song_dir = song_dir.to_path_buf();
    table.set(
        "ResolvePath",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(path) = args.front().cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            let resolved = resolve_compat_path(&resolve_song_dir, &path);
            let out = if resolved.exists() {
                file_path_string(resolved.as_path())
            } else {
                path
            };
            Ok(Value::String(lua.create_string(&out)?))
        })?,
    )?;
    table.set(
        "GetFileType",
        lua.create_function(|lua, args: MultiValue| {
            let file_type = args
                .front()
                .cloned()
                .and_then(read_string)
                .map(|path| actor_util_file_type(&path))
                .unwrap_or("FileType_Unknown");
            Ok(Value::String(lua.create_string(file_type)?))
        })?,
    )?;
    table.set(
        "IsRegisteredClass",
        lua.create_function(|_, args: MultiValue| {
            let registered = args
                .front()
                .cloned()
                .and_then(read_string)
                .is_some_and(|name| actor_util_class_registered(&name));
            Ok(registered)
        })?,
    )?;
    for method in ["LoadAllCommands", "LoadAllCommandsFromName"] {
        table.set(
            method,
            lua.create_function(|lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(())
            })?,
        )?;
    }
    table.set(
        "LoadAllCommandsAndSetXY",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(table)
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

    #[test]
    fn song_lookup_matches_current_song_aliases() {
        let song_dir = "songs/pack/My Song/";

        assert!(song_lookup_matches(
            "songs\\pack\\My Song\\",
            song_dir,
            "pack",
            "My Song"
        ));
        assert!(song_lookup_matches("My Song", song_dir, "pack", "My Song"));
        assert!(song_lookup_matches("PACK", song_dir, "pack", "My Song"));
        assert!(song_lookup_matches("My Song", song_dir, "pack", "MY SONG"));
        assert!(!song_lookup_matches("", song_dir, "pack", "My Song"));
        assert!(!song_lookup_matches("other", song_dir, "pack", "My Song"));
    }

    #[test]
    fn theme_path_formats_virtual_theme_assets() {
        assert_eq!(
            theme_path("G", "Common", "/fallback banner"),
            "__songlua_theme_path/G/Common/fallback banner"
        );
        assert_eq!(
            theme_path("B", "", "ScreenGameplay/default"),
            "__songlua_theme_path/B/ScreenGameplay/default"
        );
        assert_eq!(
            theme_path("G", "/Banner/", "group fallback"),
            "__songlua_theme_path/G/Banner/group fallback"
        );
    }
}
