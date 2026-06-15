use deadsync_chart::background::expand_random_background_changes;
use deadsync_chart::{SongBackgroundChange, SongData};
use deadsync_platform::dirs;
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_simfile::media::{
    RANDOM_MOVIES_DIR, collect_media_roots, random_movie_paths_for_song,
};
use std::path::PathBuf;

pub fn build_background_changes(
    song: &SongData,
    timing: &TimingData,
    timing_segments: &TimingSegments,
    random_movies: bool,
) -> Vec<SongBackgroundChange> {
    if !random_movies {
        return song.background_changes.clone();
    }
    let roots = random_movie_roots();
    let paths = random_movie_paths_for_song(song, &roots);
    if paths.is_empty() {
        return song.background_changes.clone();
    }
    let seed_text = song
        .simfile_path
        .parent()
        .map(|path| path.to_string_lossy())
        .unwrap_or_else(|| song.simfile_path.to_string_lossy());
    expand_random_background_changes(song, timing, timing_segments, paths, seed_text.as_ref())
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
