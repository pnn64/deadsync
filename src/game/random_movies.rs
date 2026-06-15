use deadsync_chart::SongData;
use deadsync_platform::dirs;
use deadsync_simfile::media::{
    RANDOM_MOVIES_DIR, collect_media_roots, random_movie_paths_for_song,
};
use std::path::PathBuf;

pub fn random_movie_paths(song: &SongData, random_movies: bool) -> Vec<PathBuf> {
    if !random_movies {
        return Vec::new();
    }
    let roots = random_movie_roots();
    random_movie_paths_for_song(song, &roots)
}

fn random_movie_roots() -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    let cwd = std::env::current_dir().ok();
    collect_media_roots(
        RANDOM_MOVIES_DIR,
        &dirs.data_dir,
        &dirs.exe_dir,
        cwd.as_deref(),
    )
}
