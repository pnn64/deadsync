// Frozen from 0342a1a4e (0.5.1225). Function bodies are unchanged.
use super::super::*;

pub(super) fn collect_course_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("crs"))
            {
                out.push(path);
            }
        }
    }
    out.sort_by_cached_key(|p| p.to_string_lossy().to_ascii_lowercase());
    out
}

pub(super) fn is_dir_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }
    let want_ci = want.to_ascii_lowercase();
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    let mut ci_match = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let got = entry.file_name();
        let got = got.to_string_lossy();
        if got == want {
            return Some(path);
        }
        if ci_match.is_none() && got.to_ascii_lowercase() == want_ci {
            ci_match = Some(path);
        }
    }
    ci_match
}

pub fn resolve_song_dir(
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    group: Option<&str>,
    song: &str,
) -> Option<PathBuf> {
    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = resolve_course_group_dir(song_roots, group_dirs, group)?;
        return is_dir_ci(&group_dir, song);
    }

    for songs_root in song_roots.iter().rev() {
        let Ok(entries) = fs::read_dir(songs_root) else {
            continue;
        };
        let group_dirs = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .map(|path| {
                let is_pack = is_pack_dir(&path);
                (path, is_pack)
            })
            .collect::<Vec<_>>();
        for (group_dir, is_pack) in &group_dirs {
            if !is_pack {
                continue;
            }
            if let Some(found) = is_dir_ci(group_dir, song) {
                return Some(found);
            }
        }
        for (series_dir, is_pack) in group_dirs {
            if is_pack {
                continue;
            }
            let Ok(pack_dirs) = fs::read_dir(series_dir) else {
                continue;
            };
            for pack_dir in pack_dirs.flatten().map(|entry| entry.path()) {
                if pack_dir.is_dir()
                    && let Some(found) = is_dir_ci(&pack_dir, song)
                {
                    return Some(found);
                }
            }
        }
    }
    None
}

pub fn resolve_course_group_dir(
    song_roots: &[PathBuf],
    group_dirs: &mut HashMap<String, PathBuf>,
    group: &str,
) -> Option<PathBuf> {
    let key = group.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Some(path) = group_dirs.get(&key) {
        return Some(path.clone());
    }
    let mut path = None;
    for songs_root in song_roots.iter().rev() {
        if let Some(candidate) = is_dir_ci(songs_root, group)
            && is_pack_dir(&candidate)
        {
            path = Some(candidate);
            break;
        }
    }
    if path.is_none() {
        'roots: for songs_root in song_roots.iter().rev() {
            let Ok(series_dirs) = fs::read_dir(songs_root) else {
                continue;
            };
            for series_dir in series_dirs.flatten().map(|entry| entry.path()) {
                if !series_dir.is_dir() || is_pack_dir(&series_dir) {
                    continue;
                }
                path = is_dir_ci(&series_dir, group);
                if path.is_some() {
                    break 'roots;
                }
            }
        }
    }
    let path = path?;
    group_dirs.insert(key, path.clone());
    Some(path)
}
