// Frozen from 071bd24d7 (0.5.1160); imports/visibility only adapted.
use super::*;

pub(super) fn collapse_song_asset_path_with(path: &str, backslash_separator: bool) -> String {
    let has_root = path.starts_with('/') || (backslash_separator && path.starts_with('\\'));
    let content_start = usize::from(has_root);
    let mut collapsed = String::with_capacity(path.len());
    if has_root {
        collapsed.push('/');
    }
    for part in path.split(|ch| ch == '/' || (backslash_separator && ch == '\\')) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            let last_start = collapsed
                .rfind('/')
                .map_or(content_start, |separator| separator + 1);
            if last_start < collapsed.len() && &collapsed[last_start..] != ".." {
                let new_len = if last_start > content_start {
                    last_start - 1
                } else {
                    content_start
                };
                collapsed.truncate(new_len);
            } else {
                if collapsed.len() > content_start {
                    collapsed.push('/');
                }
                collapsed.push_str("..");
            }
            continue;
        }
        if collapsed.len() > content_start {
            collapsed.push('/');
        }
        collapsed.push_str(part);
    }
    collapsed
}

pub(super) fn collapse_song_asset_path_like_itg(path: &str) -> String {
    collapse_song_asset_path_with(path, true)
}

pub fn resolve_song_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    let asset_tag = asset_tag.trim();
    if asset_tag.is_empty() {
        return None;
    }

    let collapsed = collapse_song_asset_path_like_itg(asset_tag);
    if collapsed.is_empty() {
        let is_current_dir = asset_tag
            .split(['/', '\\'])
            .all(|part| part.is_empty() || part == ".");
        return (is_current_dir && song_dir.is_dir()).then(|| song_dir.to_path_buf());
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
