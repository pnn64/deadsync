// Frozen from 7754dc3f7 (0.5.1163); only imports/visibility/formatting differ.
use super::super::*;

pub(super) fn find_child_dir_case_insensitive(parent: &Path, name: &str) -> Option<PathBuf> {
    let cache = CHILD_DIR_CACHE.get_or_init(|| Mutex::new(BorrowMap::new()));
    if let Some(cached) = cached_path_lookup(cache, parent, name) {
        return cached;
    }
    let entries = fs::read_dir(parent).ok()?;
    let mut found = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let matches = entry
            .file_name()
            .to_str()
            .is_some_and(|entry_name| entry_name.eq_ignore_ascii_case(name));
        if matches {
            found = Some(path);
            break;
        }
    }
    cache_path_lookup(cache, parent, name, found.clone());
    found
}

pub(super) fn find_file_with_prefix(dir: &Path, prefix: &str, png_only: bool) -> Option<PathBuf> {
    let caches =
        FILE_PREFIX_CACHE.get_or_init(|| std::array::from_fn(|_| Mutex::new(BorrowMap::new())));
    let cache = &caches[usize::from(png_only)];
    if let Some(cached) = cached_path_lookup(cache, dir, prefix) {
        return cached;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut match_count = 0usize;
    let mut chosen: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if (png_only
            && !name
                .get(name.len().saturating_sub(4)..)
                .is_some_and(|ext| ext.eq_ignore_ascii_case(".png")))
            || !name
                .get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match_count += 1;
        let comes_first = chosen.as_ref().is_none_or(|current| {
            let current = current
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.bytes()
                .map(|byte| byte.to_ascii_lowercase())
                .lt(current.bytes().map(|byte| byte.to_ascii_lowercase()))
        });
        if comes_first {
            chosen = Some(path);
        }
    }

    if let Some(chosen) = chosen.as_ref().filter(|_| !png_only && match_count > 1) {
        warn!(
            "multiple noteskin files matched prefix '{}' in '{}'; using '{}', ignoring {} others",
            prefix,
            dir.display(),
            chosen.display(),
            match_count - 1
        );
    }
    cache_path_lookup(cache, dir, prefix, chosen.clone());
    chosen
}
