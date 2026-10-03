// Frozen from 1958d87f1 (0.5.1701).
use super::*;

pub(super) fn list_song_art_images(song_dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(song_dir) else {
        return Vec::new();
    };
    let mut paths = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_mac_resource_fork(path) && is_song_art_image(path) && path.is_file())
        .collect::<Vec<_>>();
    sort_song_art_paths(&mut paths);
    paths
}
