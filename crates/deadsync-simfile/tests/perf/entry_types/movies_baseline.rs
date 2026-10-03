// Frozen from 1958d87f1 (0.5.1701).
use super::*;

pub(super) fn list_bgchange_song_movies(song_dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = std::fs::read_dir(song_dir) else {
        return Vec::new();
    };
    let mut files = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !is_mac_resource_fork(path) && is_bgchange_movie_path(path) && path.is_file()
        })
        .collect::<Vec<_>>();
    files.sort_by_cached_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    files
}
