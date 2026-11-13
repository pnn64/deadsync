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

#[must_use]
pub fn song_lookup_matches(query: &str, song_dir: &str, group: &str, title: &str) -> bool {
    let query = query.trim();
    !query.is_empty()
        && (normalized_query_eq(query, song_dir, false)
            || normalized_query_contains(song_dir, query)
            || normalized_query_eq(query, group, true)
            || normalized_query_eq(query, title, true))
}

fn normalized_query_eq(query: &str, target: &str, ignore_ascii_case: bool) -> bool {
    query.len() == target.len()
        && query.bytes().zip(target.bytes()).all(|(query, target)| {
            let query = if query == b'\\' { b'/' } else { query };
            if ignore_ascii_case {
                query.eq_ignore_ascii_case(&target)
            } else {
                query == target
            }
        })
}

fn normalized_query_contains(target: &str, query: &str) -> bool {
    target.as_bytes().windows(query.len()).any(|candidate| {
        candidate
            .iter()
            .zip(query.bytes())
            .all(|(&target, query)| target == if query == b'\\' { b'/' } else { query })
    })
}

#[must_use]
pub fn theme_path(kind: &str, group: &str, name: &str) -> String {
    let group = group.trim_matches('/');
    let name = name.trim_start_matches('/');
    let name = if kind == "G" && group.is_empty() {
        crate::lua_util::native_gameplay_in_bitmap_filename(name).unwrap_or(name)
    } else {
        name
    };
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

#[must_use]
pub fn resolve_compat_path(song_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        song_dir.join(path)
    }
}

// ActorUtil::ResolvePath calls CollapsePath before filesystem lookup. Keep
// that lexical spelling for Lua chunk names; canonicalize would also resolve
// filesystem links, which native actor loading does not do.
pub(crate) fn collapse_actor_path(path: &Path) -> PathBuf {
    let text = file_path_string(path);
    let mut output = PathBuf::new();
    for component in Path::new(&text).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if output.file_name().is_some_and(|name| name != "..") {
                    output.pop();
                } else if !output.has_root() {
                    output.push("..");
                }
            }
            component => output.push(component.as_os_str()),
        }
    }
    output
}

#[must_use]
pub fn entry_file_path(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    if path.is_dir() {
        let default_lua = path.join("default.lua");
        if default_lua.is_file() {
            return Some(default_lua);
        }
    }
    None
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

#[must_use]
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

#[must_use]
pub fn path_basename(text: &str) -> &str {
    let trimmed = text.trim_end_matches(['/', '\\']);
    trimmed.rsplit(['/', '\\']).next().unwrap_or_default()
}

#[must_use]
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
    path.extension()
        .and_then(|extension| extension.to_str())
        .map_or("FileType_Unknown", actor_util_extension_type)
}

fn actor_util_extension_type(extension: &str) -> &'static str {
    if extension_matches(extension, &["png", "jpg", "jpeg", "bmp", "gif", "webp"]) {
        "FileType_Bitmap"
    } else if extension_matches(
        extension,
        &["mp4", "avi", "mov", "mkv", "webm", "mpeg", "mpg"],
    ) {
        "FileType_Movie"
    } else if extension_matches(extension, &["ogg", "oga", "mp3", "wav", "flac", "opus"]) {
        "FileType_Sound"
    } else if extension.eq_ignore_ascii_case("lua") {
        "FileType_Lua"
    } else if extension_matches(extension, &["xml", "ini", "txt", "json", "ssc", "sm"]) {
        "FileType_Text"
    } else {
        "FileType_Unknown"
    }
}

fn extension_matches(extension: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

#[must_use]
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
                    .is_some_and(|name| ascii_lowercase_contains(name, stem))
        }) {
            return Some(path.clone());
        }
    }
    None
}

fn ascii_lowercase_contains(haystack: &str, needle: &str) -> bool {
    needle.is_empty()
        || haystack.as_bytes().windows(needle.len()).any(|candidate| {
            candidate
                .iter()
                .zip(needle.bytes())
                .all(|(&haystack, needle)| haystack.to_ascii_lowercase() == needle)
        })
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
        .filter(|entry| match entry.file_type() {
            // Enumeration already reports each entry's type. Only a link needs
            // a second lookup, to classify the target it resolves to.
            Ok(kind) if !kind.is_symlink() => kind.is_file(),
            _ => entry.path().is_file(),
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    files.sort_by_cached_key(|path| file_path_string(path));
    files
}

fn path_stem_eq(path: &Path, stem: &str) -> bool {
    path.file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(stem))
}

#[inline(always)]
#[must_use]
pub fn song_dir_string(path: &Path) -> String {
    let mut text = file_path_string(path);
    if !text.ends_with('/') {
        text.push('/');
    }
    text
}

#[inline(always)]
#[must_use]
pub fn file_path_string(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    // canonicalize() returns verbatim paths on Windows. Lua paths use forward
    // slashes; spelling that prefix as //?/ breaks subsequent filesystem lookup.
    if let Some(verbatim) = text.strip_prefix("//?/") {
        if let Some(network) = verbatim.strip_prefix("UNC/") {
            return format!("//{network}");
        }
        if verbatim.as_bytes().get(1) == Some(&b':') {
            return verbatim.to_owned();
        }
    }
    text
}

#[inline(always)]
#[must_use]
pub fn is_song_lua_image_path(path: &Path) -> bool {
    path_has_extension(
        path,
        &[
            "png", "jpg", "jpeg", "bmp", "gif", "webp", "qoi", "tif", "tiff",
        ],
    )
}

#[inline(always)]
#[must_use]
pub fn is_song_lua_video_path(path: &Path) -> bool {
    path_has_extension(
        path,
        &["mp4", "avi", "webm", "mov", "mkv", "mpg", "mpeg", "ogv"],
    )
}

#[inline(always)]
#[must_use]
pub fn is_song_lua_audio_path(path: &Path) -> bool {
    path_has_extension(path, &["ogg", "mp3", "wav", "flac", "opus", "m4a", "aac"])
}

#[inline(always)]
#[must_use]
pub fn is_song_lua_simfile_path(path: &Path) -> bool {
    path_has_extension(path, &["sm", "ssc"])
}

#[inline(always)]
#[must_use]
pub fn is_song_lua_media_path(path: &Path) -> bool {
    is_song_lua_image_path(path) || is_song_lua_video_path(path) || is_song_lua_audio_path(path)
}

#[inline(always)]
fn path_has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
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
    #[test]
    fn path_strings_preserve_windows_canonical_drive_and_network_roots() {
        assert_eq!(
            file_path_string(Path::new(r"\\?\C:\Songs\fof\fof.lua")),
            "C:/Songs/fof/fof.lua"
        );
        assert_eq!(
            song_dir_string(Path::new(r"\\?\C:\Songs\fof")),
            "C:/Songs/fof/"
        );
        assert_eq!(
            file_path_string(Path::new(r"\\?\UNC\server\share\fof.lua")),
            "//server/share/fof.lua"
        );
    }

    #[cfg(windows)]
    #[test]
    fn actor_path_collapse_preserves_windows_roots() {
        assert_eq!(
            collapse_actor_path(Path::new(r"\\?\C:\Songs\fof\nested\..\FofGuys.lua")),
            PathBuf::from("C:/Songs/fof/FofGuys.lua")
        );
        assert_eq!(
            collapse_actor_path(Path::new(r"\\?\UNC\server\share\nested\..\FofGuys.lua")),
            PathBuf::from(r"\\server\share\FofGuys.lua")
        );
    }
}
