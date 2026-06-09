use crate::config::dirs;
use crate::engine::audio::decode;
use crate::game::song::get_song_cache;
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_simfile::cache::{
    SerializableSongData, build_requested_gameplay_charts, build_song_meta,
    update_precise_song_bounds,
};
use deadsync_simfile::song::{ParseSongOptions, SONG_ANALYSIS_MONO_THRESHOLD, parse_song_file};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::time::{Duration, Instant};

mod cache;
mod scan;

pub(crate) use scan::collect_song_scan_roots;
pub use scan::{
    reload_song_dirs_with_progress_counts, scan_and_load_songs, scan_and_load_songs_with_progress,
    scan_and_load_songs_with_progress_counts,
};

const RANDOM_MOVIES_DIR: &str = "RandomMovies";
const SONG_MOVIES_DIR: &str = "SongMovies";
const BG_ANIMATIONS_DIR: &str = "BGAnimations";

// --- CACHING HELPER FUNCTIONS ---

pub(crate) fn fmt_scan_time(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        return format!("{:.2}s", ms as f64 / 1000.0);
    }
    let total_s = ms as f64 / 1000.0;
    let m = (total_s / 60.0).floor() as u64;
    let s = (m as f64).mul_add(-60.0, total_s);
    format!("{m}m{s:.1}s")
}

/// Helper to load a song from cache OR parse it if needed.
/// Returns (`SongData`, `is_cache_hit`).
fn process_song(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_path = if fastload || cachesongs {
        cache::compute_song_cache_path(&simfile_path)
    } else {
        None
    };

    let allow_cache_read = fastload || cachesongs;
    if allow_cache_read
        && let Some(cp) = cache_path.as_deref()
        && let Some(song_data) = cache::load_song_from_cache(&simfile_path, cp, !fastload)
    {
        return Ok((song_data, true));
    }

    let song_data = parse_song_and_maybe_write_cache(
        &simfile_path,
        fastload,
        cachesongs,
        cache_path.as_deref(),
        global_offset_seconds,
    )?;
    Ok((song_data, false))
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(simfile_path: &Path) -> Result<Arc<SongData>, String> {
    let config = crate::config::get();
    let global_offset_seconds = config.global_offset_seconds;
    let cachesongs = config.cachesongs;
    let cache_path = cachesongs
        .then(|| cache::compute_song_cache_path(simfile_path))
        .flatten();
    let song_data = parse_song_and_maybe_write_cache(
        simfile_path,
        false,
        cachesongs,
        cache_path.as_deref(),
        global_offset_seconds,
    )?;
    let updated = Arc::new(song_data);

    let mut song_cache = get_song_cache();
    let mut replaced = false;
    for pack in song_cache.iter_mut() {
        for song in &mut pack.songs {
            if song.simfile_path == simfile_path {
                *song = updated.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(format!(
            "Song '{}' not found in song cache",
            simfile_path.display()
        ));
    }
    Ok(updated)
}

fn load_gameplay_song_data(
    simfile_path: &Path,
    allow_cache_write: bool,
    global_offset_seconds: f32,
) -> Result<SerializableSongData, String> {
    let started = Instant::now();
    let cache_path = allow_cache_write
        .then(|| cache::compute_song_cache_path(simfile_path))
        .flatten();
    let parse_started = Instant::now();
    let mut song_data = parse_and_process_song_file(simfile_path)?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;
    update_precise_song_bounds(&mut song_data, global_offset_seconds);
    let write_started = Instant::now();
    if allow_cache_write && let Some(cp) = cache_path.as_deref() {
        cache::write_song_cache(cp, &song_data, global_offset_seconds);
    }
    let write_ms = write_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 25.0 {
        info!(
            "Gameplay song data load: source=parse file={:?} parse_ms={parse_ms:.3} write_ms={write_ms:.3} elapsed_ms={total_ms:.3}",
            simfile_path.file_name().unwrap_or_default()
        );
    } else {
        debug!(
            "Gameplay song data load: source=parse file={:?} parse_ms={parse_ms:.3} write_ms={write_ms:.3} elapsed_ms={total_ms:.3}",
            simfile_path.file_name().unwrap_or_default()
        );
    }
    Ok(song_data)
}

pub fn load_gameplay_charts(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let started = Instant::now();
    let config = crate::config::get();
    let allow_cache_read = config.fastload || config.cachesongs;
    let allow_cache_write = config.cachesongs;
    let verify_cache_freshness = !config.fastload;
    let load_started = Instant::now();
    if allow_cache_read
        && let Some(charts) = cache::load_gameplay_charts_from_cache(
            song,
            requested_chart_ixs,
            global_offset_seconds,
            verify_cache_freshness,
        )
    {
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        if total_ms >= 25.0 {
            info!(
                "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms=0.000 elapsed_ms={total_ms:.3}",
                song.title,
                requested_chart_ixs.len()
            );
        } else {
            debug!(
                "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms=0.000 elapsed_ms={total_ms:.3}",
                song.title,
                requested_chart_ixs.len()
            );
        }
        return Ok(charts);
    }

    let song_data =
        load_gameplay_song_data(&song.simfile_path, allow_cache_write, global_offset_seconds)?;
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    let build_started = Instant::now();
    let charts =
        build_requested_gameplay_charts(&song_data, requested_chart_ixs, global_offset_seconds)?;
    let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 25.0 {
        info!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms={build_ms:.3} elapsed_ms={total_ms:.3}",
            song.title,
            requested_chart_ixs.len()
        );
    } else {
        debug!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms={build_ms:.3} elapsed_ms={total_ms:.3}",
            song.title,
            requested_chart_ixs.len()
        );
    }
    Ok(charts)
}

pub fn load_sync_analysis_chart(
    song: &SongData,
    chart_ix: usize,
) -> Result<GameplayChartData, String> {
    let config = crate::config::get();
    let allow_cache_read = config.fastload || config.cachesongs;
    let verify_cache_freshness = !config.fastload;
    if allow_cache_read
        && let Some(mut charts) =
            cache::load_gameplay_charts_from_cache(song, &[chart_ix], 0.0, verify_cache_freshness)
        && let Some(chart) = charts.pop()
    {
        return Ok(chart);
    }

    let song_data = load_gameplay_song_data(&song.simfile_path, false, 0.0)?;
    let mut charts = build_requested_gameplay_charts(&song_data, &[chart_ix], 0.0)?;
    charts
        .pop()
        .ok_or_else(|| format!("Chart index {chart_ix} out of range"))
}

fn parse_song_and_maybe_write_cache(
    path: &Path,
    fastload: bool,
    cachesongs: bool,
    cache_path: Option<&Path>,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    if fastload {
        debug!("Cache miss for: {:?}", path.file_name().unwrap_or_default());
    } else {
        debug!(
            "Parsing (fastload disabled): {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    let mut song_data = parse_and_process_song_file(path)?;
    update_precise_song_bounds(&mut song_data, global_offset_seconds);
    if cachesongs && let Some(cp) = cache_path {
        cache::write_song_cache(cp, &song_data, global_offset_seconds);
    }
    Ok(build_song_meta(song_data, global_offset_seconds))
}

#[cfg(test)]
pub(crate) fn parse_song_for_test(
    path: &Path,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    let mut song_data = parse_and_process_song_file(path)?;
    update_precise_song_bounds(&mut song_data, global_offset_seconds);
    Ok(build_song_meta(song_data, global_offset_seconds))
}

fn bgchange_asset_roots(dirname: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    let mut roots = Vec::with_capacity(4);
    push_existing_unique_dir(&mut roots, dirs.data_dir.join(dirname));
    push_existing_unique_dir(&mut roots, dirs.exe_dir.join(dirname));
    if let Ok(cwd) = std::env::current_dir() {
        push_existing_unique_dir(&mut roots, cwd.join(dirname));
        push_existing_unique_dir(&mut roots, cwd.join("deadsync").join(dirname));
    }
    roots
}

fn push_existing_unique_dir(out: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() && !out.iter().any(|existing| existing == &path) {
        out.push(path);
    }
}

fn parse_song_options() -> ParseSongOptions {
    ParseSongOptions {
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        song_movie_roots: bgchange_asset_roots(SONG_MOVIES_DIR),
        random_movie_roots: bgchange_asset_roots(RANDOM_MOVIES_DIR),
        bg_animation_roots: bgchange_asset_roots(BG_ANIMATIONS_DIR),
    }
}

/// Parse and normalize a simfile on a cache miss.
fn parse_and_process_song_file(path: &Path) -> Result<SerializableSongData, String> {
    parse_song_file(path, &parse_song_options(), compute_music_length_seconds)
}

/// Computes the length of the music file in seconds when the decode layer supports it.
/// Returns 0.0 on failure or if no music path is provided.
fn compute_music_length_seconds(music_path: Option<&Path>) -> f32 {
    let Some(path) = music_path else {
        return 0.0;
    };
    match decode::file_length_seconds(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute audio length for {path:?}: {e}");
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-simfile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_song_records_itg_first_second_from_first_step() {
        let root = test_dir("first-second");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let simfile = song_dir.join("song.sm");
        fs::write(
            &simfile,
            b"#TITLE:First Second;\n\
              #BPMS:0.000=60.000;\n\
              #OFFSET:0.000;\n\
              #NOTES:\n\
              dance-single:\n\
              :\n\
              Challenge:\n\
              1:\n\
              0.000,0.000,0.000,0.000,0.000:\n\
              0000\n\
              ,\n\
              1000\n\
              ,\n\
              0001\n\
              ;",
        )
        .unwrap();

        let song = super::parse_song_for_test(&simfile, 0.0).unwrap();

        assert!((song.precise_first_second() - 4.0).abs() <= 1e-6);
        assert!((song.precise_last_second() - 8.0).abs() <= 1e-6);
    }
}
