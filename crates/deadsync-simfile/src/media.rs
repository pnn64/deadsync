use std::fs;
use std::path::{Path, PathBuf};

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

pub fn is_song_art_image(path: &Path) -> bool {
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
        assert_eq!(foreground_media_ext_rank(Path::new("notes.txt")), None);
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
}
