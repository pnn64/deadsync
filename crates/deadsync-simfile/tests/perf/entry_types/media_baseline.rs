// Frozen from 1958d87f1 (0.5.1701).
use super::*;

pub(super) fn list_random_movie_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            !is_mac_resource_fork(path) && is_bgchange_movie_path(path) && path.is_file()
        })
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| random_movie_path_cmp(left, right));
    paths
}

pub(super) fn resolve_foreground_media_dir(dir: &Path) -> Option<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return None;
    };
    let mut best: Option<(u8, PathBuf)> = None;
    for path in read_dir.flatten().map(|entry| entry.path()) {
        let Some(rank) = foreground_media_ext_rank(&path) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        if best.as_ref().is_none_or(|(best_rank, best_path)| {
            foreground_media_candidate_cmp(rank, &path, *best_rank, best_path).is_lt()
        }) {
            best = Some((rank, path));
        }
    }
    best.map(|(_, path)| path)
}
