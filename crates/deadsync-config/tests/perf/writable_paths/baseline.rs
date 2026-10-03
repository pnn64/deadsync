// Frozen from 462b31ec5 (0.5.1705).
use super::*;
use std::path::PathBuf;

pub(super) fn song_path_is_writable_for_roots(path: &Path, roots: &[AdditionalSongFolder]) -> bool {
    let path = canonical_or_raw(path);
    let mut best: Option<(usize, bool)> = None;
    for root in roots {
        let root_path = canonical_or_raw(Path::new(root.path.as_str()));
        let Some(len) = root_prefix_len(path.as_path(), root_path.as_path()) else {
            continue;
        };
        if best.is_none_or(|(best_len, _)| len >= best_len) {
            best = Some((len, root.writable));
        }
    }
    best.is_none_or(|(_, writable)| writable)
}

pub(super) fn canonical_or_raw(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
