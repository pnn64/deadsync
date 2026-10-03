// Frozen from 4499f127a (0.5.1700).
use super::*;

pub(super) fn resolve_foreground_media_dir(dir: &Path) -> Option<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return None;
    };
    let mut best: Option<(u8, PathBuf)> = None;
    for path in read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
    {
        let Some(rank) = foreground_media_ext_rank(&path) else {
            continue;
        };
        if best.as_ref().is_none_or(|(best_rank, best_path)| {
            foreground_media_candidate_cmp(rank, &path, *best_rank, best_path).is_lt()
        }) {
            best = Some((rank, path));
        }
    }
    best.map(|(_, path)| path)
}
