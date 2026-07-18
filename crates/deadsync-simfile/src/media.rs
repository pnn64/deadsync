use deadsync_chart::SongData;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const BG_ANIMATIONS_DIR: &str = "BGAnimations";
pub const RANDOM_MOVIES_DIR: &str = "RandomMovies";
pub const SONG_MOVIES_DIR: &str = "SongMovies";

const BACKGROUND_MAPPING_FILE: &str = "BackgroundMapping.ini";
const BGCHANGE_MOVIE_EXTENSIONS: [&str; 12] = [
    "avi", "f4v", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "ogv", "webm", "wmv",
];
const SONG_ART_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "bmp"];

#[inline]
fn extension_matches(ext: &str, extensions: &[&str]) -> bool {
    extensions
        .iter()
        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
}

pub fn collect_media_roots(
    dirname: &str,
    data_dir: &Path,
    exe_dir: &Path,
    cwd: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(4);
    push_media_root(&mut roots, data_dir.join(dirname));
    push_media_root(&mut roots, exe_dir.join(dirname));
    if let Some(cwd) = cwd {
        push_media_root(&mut roots, cwd.join(dirname));
        push_media_root(&mut roots, cwd.join("deadsync").join(dirname));
    }
    roots
}

fn push_media_root(out: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() && !out.iter().any(|existing| existing == &path) {
        out.push(path);
    }
}

pub fn collapse_song_asset_path(path: &str) -> String {
    let has_root = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::with_capacity(path.split('/').count());
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push("..");
            }
            continue;
        }
        parts.push(part);
    }
    let collapsed = parts.join("/");
    if has_root {
        if collapsed.is_empty() {
            "/".to_string()
        } else {
            format!("/{collapsed}")
        }
    } else {
        collapsed
    }
}

pub fn resolve_song_dir_entry_ci(base: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().to_ascii_lowercase() == want {
            return Some(entry.path());
        }
    }
    None
}

pub fn resolve_song_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    let asset_tag = asset_tag.trim();
    if asset_tag.is_empty() {
        return None;
    }

    let collapsed = collapse_song_asset_path(&asset_tag.replace('\\', "/"));
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.starts_with('/') {
        let path = PathBuf::from(&collapsed);
        return path.exists().then_some(path);
    }

    let direct = song_dir.join(&collapsed);
    if direct.exists() {
        return Some(direct);
    }

    let mut path = song_dir.to_path_buf();
    let mut parts = collapsed
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    while let Some(part) = parts.next() {
        if part == "." {
            continue;
        }
        if part == ".." {
            if !path.pop() {
                return None;
            }
            continue;
        }
        let next = resolve_song_dir_entry_ci(&path, part).or_else(|| {
            let next = path.join(part);
            next.exists().then_some(next)
        })?;
        if parts.peek().is_some() && !next.is_dir() {
            return None;
        }
        path = next;
    }
    Some(path)
}

pub fn resolve_song_asset_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    resolve_song_path_like_itg(song_dir, asset_tag).filter(|path| path.is_file())
}

pub fn resolve_dir_default_lua_like_itg(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("default.lua");
    resolve_song_dir_entry_ci(dir, "default.lua")
        .filter(|path| path.is_file())
        .or_else(|| direct.is_file().then_some(direct))
}

pub fn list_song_dir_rel_entries(song_dir: &Path) -> Vec<String> {
    let mut dirs = vec![song_dir.to_path_buf()];
    let mut entries = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(song_dir) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                dirs.push(path);
                entries.push(rel);
                continue;
            }
            if path.is_file() {
                entries.push(rel);
            }
        }
    }
    entries.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    entries
}

pub fn path_uses_lua_like_itg(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        return true;
    }
    path.is_dir() && resolve_dir_default_lua_like_itg(path).is_some()
}

pub fn song_lua_entry_path_like_itg(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        resolve_dir_default_lua_like_itg(&path).unwrap_or_else(|| path.join("default.lua"))
    } else {
        path
    }
}

pub fn resolve_foreground_media_dir(dir: &Path) -> Option<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return None;
    };
    let mut media = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let rank = foreground_media_ext_rank(&path)?;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            Some(((rank, name), path))
        })
        .collect::<Vec<_>>();
    media.sort_by(|left, right| left.0.cmp(&right.0));
    media.into_iter().next().map(|(_, path)| path)
}

pub fn resolve_foreground_media_path(song_dir: &Path, target: &str) -> Option<PathBuf> {
    let path = resolve_song_path_like_itg(song_dir, target)?;
    if path_uses_lua_like_itg(&path) {
        return None;
    }
    if path.is_dir() {
        return resolve_foreground_media_dir(&path);
    }
    foreground_media_ext_rank(&path).is_some().then_some(path)
}

pub fn foreground_media_ext_rank(path: &Path) -> Option<u8> {
    let ext = path.extension()?.to_str()?;
    if extension_matches(ext, &BGCHANGE_MOVIE_EXTENSIONS) {
        Some(0)
    } else {
        SONG_ART_EXTENSIONS
            .iter()
            .position(|candidate| ext.eq_ignore_ascii_case(candidate))
            .map(|index| index as u8 + 1)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn foreground_media_ext_rank_legacy(path: &Path) -> Option<u8> {
    let ext = path.extension()?.to_str()?;
    if matches!(
        ext.to_ascii_lowercase().as_str(),
        "avi"
            | "f4v"
            | "flv"
            | "m4v"
            | "mkv"
            | "mov"
            | "mp4"
            | "mpeg"
            | "mpg"
            | "ogv"
            | "webm"
            | "wmv"
    ) {
        Some(0)
    } else if ext.eq_ignore_ascii_case("png") {
        Some(1)
    } else if ext.eq_ignore_ascii_case("jpg") {
        Some(2)
    } else if ext.eq_ignore_ascii_case("jpeg") {
        Some(3)
    } else if ext.eq_ignore_ascii_case("gif") {
        Some(4)
    } else if ext.eq_ignore_ascii_case("bmp") {
        Some(5)
    } else {
        None
    }
}

pub fn is_bgchange_movie_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extension_matches(ext, &BGCHANGE_MOVIE_EXTENSIONS))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn is_bgchange_movie_path_legacy(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "avi"
                    | "f4v"
                    | "flv"
                    | "m4v"
                    | "mkv"
                    | "mov"
                    | "mp4"
                    | "mpeg"
                    | "mpg"
                    | "ogv"
                    | "webm"
                    | "wmv"
            )
        })
}

pub fn random_movie_paths_for_song(song: &SongData, roots: &[PathBuf]) -> Vec<PathBuf> {
    let group = song_group_name(song);
    let genre_whitelist = group
        .as_deref()
        .filter(|_| !song.genre.trim().is_empty())
        .and_then(|group| {
            roots
                .iter()
                .find_map(|root| genre_movie_whitelist(&root.join(group), &song.genre))
        });

    for root in roots {
        if let Some(group) = group.as_deref() {
            let paths = filtered_random_movie_paths(&root.join(group), genre_whitelist.as_ref());
            if !paths.is_empty() {
                return paths;
            }
        }
        let paths = filtered_random_movie_paths(root, genre_whitelist.as_ref());
        if !paths.is_empty() {
            return paths;
        }
    }
    Vec::new()
}

fn filtered_random_movie_paths(dir: &Path, whitelist: Option<&HashSet<String>>) -> Vec<PathBuf> {
    let paths = list_random_movie_paths(dir);
    let Some(whitelist) = whitelist else {
        return paths;
    };
    let filtered = paths
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| whitelist.contains(name))
        })
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() { paths } else { filtered }
}

fn list_random_movie_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            !is_mac_resource_fork(path) && path.is_file() && is_bgchange_movie_path(path)
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| {
        a.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .cmp(
                &b.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase()),
            )
    });
    paths
}

fn song_group_name(song: &SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn genre_movie_whitelist(group_dir: &Path, genre: &str) -> Option<HashSet<String>> {
    let path = group_dir.join(BACKGROUND_MAPPING_FILE);
    let sections = parse_ini_sections(&fs::read_to_string(path).ok()?);
    let genre_section = sections
        .get("GenreToSection")?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(genre.trim()))?
        .1
        .trim()
        .to_owned();
    let section = sections.get(genre_section.as_str())?;
    let out = section
        .iter()
        .map(|(key, _)| key.trim().to_owned())
        .filter(|key| !key.is_empty())
        .collect::<HashSet<_>>();
    (!out.is_empty()).then_some(out)
}

fn parse_ini_sections(text: &str) -> HashMap<String, Vec<(String, String)>> {
    let mut sections = HashMap::<String, Vec<(String, String)>>::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_owned();
            sections.entry(current.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        sections
            .entry(current.clone())
            .or_default()
            .push((key.trim().to_owned(), value.trim().to_owned()));
    }
    sections
}

pub fn is_song_art_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extension_matches(ext, &SONG_ART_EXTENSIONS))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn is_song_art_image_legacy(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "bmp"
            )
        })
}

pub fn is_mac_resource_fork(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("._"))
}

pub fn song_art_file_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

pub fn song_art_file_stem(path: &Path) -> Option<String> {
    Some(path.file_stem()?.to_string_lossy().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::SongBackgroundChange;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn collapses_relative_song_asset_paths() {
        assert_eq!(collapse_song_asset_path("a//b/./c"), "a/b/c");
        assert_eq!(collapse_song_asset_path("a/b/../c"), "a/c");
        assert_eq!(collapse_song_asset_path("../a/./b"), "../a/b");
    }

    #[test]
    fn collapses_rooted_song_asset_paths() {
        assert_eq!(collapse_song_asset_path("/a/../b"), "/b");
        assert_eq!(collapse_song_asset_path("/../b"), "/../b");
        assert_eq!(collapse_song_asset_path("/."), "/");
    }

    #[test]
    fn ranks_foreground_media_by_itg_preference() {
        assert_eq!(foreground_media_ext_rank(Path::new("clip.MP4")), Some(0));
        assert_eq!(foreground_media_ext_rank(Path::new("image.png")), Some(1));
        assert_eq!(foreground_media_ext_rank(Path::new("image.jpeg")), Some(3));
        assert_eq!(foreground_media_ext_rank(Path::new("image.BMP")), Some(5));
        assert_eq!(foreground_media_ext_rank(Path::new("notes.txt")), None);
        assert_eq!(foreground_media_ext_rank(Path::new("legacy.m2v")), None);
    }

    #[test]
    fn identifies_bgchange_movies() {
        assert!(is_bgchange_movie_path(Path::new("movie.mpg")));
        assert!(is_bgchange_movie_path(Path::new("movie.WEBM")));
        assert!(!is_bgchange_movie_path(Path::new("still.png")));
    }

    #[test]
    fn identifies_art_images_and_resource_forks() {
        assert!(is_song_art_image(Path::new("banner.JPG")));
        assert!(!is_song_art_image(Path::new("music.ogg")));
        assert!(is_mac_resource_fork(Path::new("._banner.png")));
        assert!(!is_mac_resource_fork(Path::new("banner.png")));
    }

    #[test]
    fn normalizes_song_art_keys_and_stems() {
        let path = PathBuf::from("Visuals").join("Banner.PNG");
        assert_eq!(song_art_file_key(&path), "visuals/banner.png");
        assert_eq!(song_art_file_stem(&path), Some("banner".to_string()));
    }

    #[test]
    fn collect_media_roots_skips_missing_and_dedupes() {
        let root = test_dir("media-roots");
        let shared = root.join("shared");
        let cwd = root.join("work");
        let cwd_deadsync = cwd.join("deadsync");
        let shared_movies = shared.join(RANDOM_MOVIES_DIR);
        let cwd_movies = cwd.join(RANDOM_MOVIES_DIR);
        let cwd_deadsync_movies = cwd_deadsync.join(RANDOM_MOVIES_DIR);
        fs::create_dir_all(&shared_movies).unwrap();
        fs::create_dir_all(&cwd_deadsync_movies).unwrap();

        let roots = collect_media_roots(RANDOM_MOVIES_DIR, &shared, &shared, Some(&cwd));

        assert_eq!(roots, vec![shared_movies, cwd_deadsync_movies]);
        assert!(!roots.contains(&cwd_movies));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolves_song_paths_case_insensitively() {
        let root = test_dir("case-paths");
        let song_dir = root.join("Song");
        let nested = song_dir.join("Visuals").join("Intro");
        fs::create_dir_all(&nested).unwrap();
        let target = nested.join("Movie.MP4");
        fs::write(&target, b"movie").unwrap();

        let resolved = resolve_song_path_like_itg(&song_dir, "visuals\\intro\\movie.mp4").unwrap();
        assert_eq!(
            fs::canonicalize(resolved).unwrap(),
            fs::canonicalize(&target).unwrap()
        );
        let resolved =
            resolve_song_asset_path_like_itg(&song_dir, "./Visuals/../Visuals/Intro/Movie.MP4")
                .unwrap();
        assert_eq!(
            fs::canonicalize(resolved).unwrap(),
            fs::canonicalize(target).unwrap()
        );
        assert!(resolve_song_asset_path_like_itg(&song_dir, "Visuals").is_none());
    }

    #[test]
    fn resolves_default_lua_case_insensitively() {
        let root = test_dir("default-lua");
        let dir = root.join("Visuals");
        fs::create_dir_all(&dir).unwrap();
        let default_lua = dir.join("Default.lua");
        fs::write(&default_lua, b"return Def.ActorFrame{}").unwrap();

        assert_eq!(resolve_dir_default_lua_like_itg(&dir), Some(default_lua));
    }

    #[test]
    fn lists_song_dir_entries_by_longest_first() {
        let root = test_dir("entries");
        let song_dir = root.join("Song");
        fs::create_dir_all(song_dir.join("BG").join("Layer")).unwrap();
        fs::write(song_dir.join("BG").join("Layer").join("flash.lua"), b"lua").unwrap();
        fs::write(song_dir.join("banner.png"), b"png").unwrap();

        let entries = list_song_dir_rel_entries(&song_dir);

        assert_eq!(
            entries,
            vec![
                "BG/Layer/flash.lua".to_string(),
                "banner.png".to_string(),
                "BG/Layer".to_string(),
                "BG".to_string(),
            ]
        );
    }

    #[test]
    fn random_movie_paths_prefer_group_and_genre_whitelist() {
        let root = test_dir("random-movies-group-genre");
        let movies = root.join("RandomMovies");
        let group = movies.join("Pack");
        fs::create_dir_all(&group).unwrap();
        let ambient = group.join("ambient.mp4");
        let bright = group.join("bright.ogv");
        let root_movie = movies.join("root.mp4");
        fs::write(&ambient, b"movie").unwrap();
        fs::write(&bright, b"movie").unwrap();
        fs::write(&root_movie, b"movie").unwrap();
        fs::write(group.join("._fork.mp4"), b"fork").unwrap();
        fs::write(
            group.join(BACKGROUND_MAPPING_FILE),
            "\
[GenreToSection]
Tech=Bright

[Bright]
bright.ogv=1
",
        )
        .unwrap();
        let song = test_song(
            root.join("Songs")
                .join("Pack")
                .join("Song")
                .join("song.ssc"),
            "tech",
        );

        let paths = random_movie_paths_for_song(&song, &[movies]);

        assert_eq!(paths, vec![bright]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn random_movie_paths_fall_back_to_root_movies() {
        let root = test_dir("random-movies-root");
        let movies = root.join("RandomMovies");
        fs::create_dir_all(&movies).unwrap();
        let clip = movies.join("clip.webm");
        fs::write(&clip, b"movie").unwrap();
        fs::write(movies.join("still.png"), b"png").unwrap();
        let song = test_song(
            root.join("Songs")
                .join("MissingPack")
                .join("Song")
                .join("song.ssc"),
            "",
        );

        let paths = random_movie_paths_for_song(&song, &[movies]);

        assert_eq!(paths, vec![clip]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_lua_paths_and_resolves_entry_path() {
        let root = test_dir("lua-entry");
        let dir = root.join("Visuals");
        fs::create_dir_all(&dir).unwrap();
        let default_lua = dir.join("Default.lua");
        fs::write(&default_lua, b"return Def.ActorFrame{}").unwrap();
        let direct_lua = root.join("effect.lua");
        fs::write(&direct_lua, b"lua").unwrap();

        assert!(path_uses_lua_like_itg(&dir));
        assert!(path_uses_lua_like_itg(&direct_lua));
        assert_eq!(song_lua_entry_path_like_itg(dir), default_lua);
        assert_eq!(song_lua_entry_path_like_itg(direct_lua.clone()), direct_lua);
    }

    #[test]
    fn resolves_foreground_media_by_rank_and_skips_lua_dirs() {
        let root = test_dir("fg-media");
        let song_dir = root.join("Song");
        let media_dir = song_dir.join("Visuals");
        fs::create_dir_all(&media_dir).unwrap();
        fs::write(media_dir.join("still.png"), b"png").unwrap();
        let movie = media_dir.join("clip.mp4");
        fs::write(&movie, b"mp4").unwrap();

        assert_eq!(
            resolve_foreground_media_dir(&media_dir),
            Some(movie.clone())
        );
        assert_eq!(
            resolve_foreground_media_path(&song_dir, "Visuals"),
            Some(movie)
        );

        let lua_dir = song_dir.join("LuaVisuals");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(lua_dir.join("default.lua"), b"lua").unwrap();
        fs::write(lua_dir.join("fallback.png"), b"png").unwrap();

        assert!(resolve_foreground_media_path(&song_dir, "LuaVisuals").is_none());
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-media-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_song(simfile_path: PathBuf, genre: &str) -> SongData {
        SongData {
            simfile_path,
            title: String::new(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: genre.to_string(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::<SongBackgroundChange>::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }
}
