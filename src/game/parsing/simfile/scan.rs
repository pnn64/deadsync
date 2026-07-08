use super::{compute_song_cache_path, load_song_from_cache, parse_song_and_maybe_write_cache};
use crate::game::song::{get_song_cache, set_song_cache};
use deadlib_platform::dirs;
use deadsync_chart::{SongData, SongPack};
use deadsync_simfile::scan::{
    PackScan, ScanFailure, SongLoadOptions, collect_reload_pack_dirs, count_loaded_songs,
    fmt_scan_time, load_pack_scans_with, push_unique_path, replace_song_packs, scan_pack_dirs,
    scan_song_roots,
};
use log::{debug, info, warn};
use std::fs;
use std::path::{Path, PathBuf};

pub fn scan_and_load_songs_with_progress_counts<F>(root_path: &Path, progress: &mut F)
where
    F: FnMut(usize, usize, &str, &str),
{
    scan_and_load_songs_impl(root_path, Some(progress));
}

pub fn reload_song_dirs_with_progress_counts<F>(
    root_path: &Path,
    dirs: &[PathBuf],
    progress: &mut F,
) where
    F: FnMut(usize, usize, &str, &str),
{
    reload_song_dirs_impl(root_path, dirs, Some(progress));
}

pub(crate) fn collect_song_scan_roots(root_path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(4);
    let mut keys: Vec<String> = Vec::with_capacity(4);
    if root_path.is_dir() {
        push_unique_path(root_path.to_path_buf(), &mut roots, &mut keys);
    } else {
        warn!("Songs directory '{}' not found.", root_path.display());
    }

    // In platform-native mode, also include exe-dir songs.
    for extra in dirs::app_dirs().extra_song_roots() {
        push_unique_path(extra, &mut roots, &mut keys);
    }

    for folder in crate::config::additional_song_folder_roots() {
        let extra_root = PathBuf::from(folder.path.as_str());
        if extra_root.is_dir() {
            push_unique_path(extra_root, &mut roots, &mut keys);
        } else {
            warn!(
                "AdditionalSongFolders entry '{}' is not a directory; skipping.",
                folder.path
            );
        }
    }
    roots
}

fn ensure_song_cache_dir() {
    let cache_dir = dirs::app_dirs().song_cache_dir();
    if let Err(error) = fs::create_dir_all(&cache_dir) {
        warn!(
            "Could not create cache directory '{}': {}. Caching will be disabled.",
            cache_dir.to_string_lossy(),
            error
        );
    }
}

fn warn_scan_failures(kind: &str, failures: &[ScanFailure]) {
    for failure in failures {
        warn!(
            "Could not scan {kind} '{}': {}",
            failure.path.display(),
            failure.error
        );
    }
}

fn process_song(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_path = if fastload || cachesongs {
        compute_song_cache_path(&simfile_path)
    } else {
        None
    };

    let allow_cache_read = fastload || cachesongs;
    if allow_cache_read
        && let Some(cp) = cache_path.as_deref()
        && let Some(song_data) = load_song_from_cache(&simfile_path, cp, !fastload)
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

fn load_pack_scans<F>(
    packs: Vec<PackScan>,
    progress: Option<&mut F>,
) -> (Vec<SongPack>, deadsync_simfile::scan::SongLoadStats)
where
    F: FnMut(usize, usize, &str, &str),
{
    let config = crate::config::get();
    let options = SongLoadOptions {
        fastload: config.fastload,
        cachesongs: config.cachesongs,
        global_offset_seconds: config.global_offset_seconds,
        song_parsing_threads: u32::from(config.song_parsing_threads),
    };
    let requested_threads = config.song_parsing_threads;
    let (loaded_packs, stats) = load_pack_scans_with(
        packs,
        options,
        progress,
        process_song,
        crate::config::group_is_never_cached,
        |simfile_path, error| warn!("Failed to load '{simfile_path:?}': {error}"),
        |pack| debug!("Scanning pack: {}", pack.name),
        |pack_display| debug!("Skipping song cache for pack '{pack_display}' (NeverCacheList)."),
    );
    if stats.used_parallel {
        debug!(
            "Song parsing: used {} threads for cache/parsing (SongParsingThreads={}).",
            stats.parse_threads, requested_threads
        );
    }
    (loaded_packs, stats)
}

fn scan_and_load_songs_impl<F>(root_path: &Path, progress: Option<&mut F>)
where
    F: FnMut(usize, usize, &str, &str),
{
    info!(
        "Starting simfile scan (base songs root '{}')...",
        root_path.display()
    );

    let started = std::time::Instant::now();
    ensure_song_cache_dir();

    let song_roots = collect_song_scan_roots(root_path);
    if song_roots.is_empty() {
        warn!("No valid song roots found. No songs will be loaded.");
        set_song_cache(Vec::new());
        return;
    }

    let (packs, failures) = scan_song_roots(&song_roots);
    warn_scan_failures("songs dir", &failures);
    let (loaded_packs, stats) = load_pack_scans(packs, progress);
    let songs_loaded = count_loaded_songs(&loaded_packs);
    info!(
        "Finished scan. Found {} packs / {} songs (parsed {}, cache hits {}, failed {}) in {}.",
        loaded_packs.len(),
        songs_loaded,
        stats.songs_parsed,
        stats.songs_cache_hits,
        stats.songs_failed,
        fmt_scan_time(started.elapsed())
    );
    set_song_cache(loaded_packs);
}

fn reload_song_dirs_impl<F>(root_path: &Path, pack_dirs: &[PathBuf], progress: Option<&mut F>)
where
    F: FnMut(usize, usize, &str, &str),
{
    ensure_song_cache_dir();

    let song_roots = collect_song_scan_roots(root_path);
    if song_roots.is_empty() {
        warn!("No valid song roots found. No songs will be reloaded.");
        return;
    }

    let (scan_dirs, pack_keys) = collect_reload_pack_dirs(&song_roots, pack_dirs);
    if pack_keys.is_empty() {
        warn!("No valid song pack directories were requested for targeted reload.");
        return;
    }

    info!(
        "Starting targeted song reload for {} affected pack(s)...",
        pack_keys.len()
    );
    let started = std::time::Instant::now();
    let (packs, failures) = scan_pack_dirs(&scan_dirs);
    warn_scan_failures("pack dir", &failures);
    let (reloaded_packs, stats) = load_pack_scans(packs, progress);
    let reloaded_pack_count = reloaded_packs.len();
    let reloaded_song_count = count_loaded_songs(&reloaded_packs);

    let (total_packs, total_songs) = {
        let mut song_cache = get_song_cache();
        replace_song_packs(&mut song_cache, &pack_keys, reloaded_packs);
        (song_cache.len(), count_loaded_songs(&song_cache))
    };

    info!(
        "Finished targeted reload. Reloaded {} packs / {} songs (parsed {}, cache hits {}, failed {}) in {}. Song cache now has {} packs / {} songs.",
        reloaded_pack_count,
        reloaded_song_count,
        stats.songs_parsed,
        stats.songs_cache_hits,
        stats.songs_failed,
        fmt_scan_time(started.elapsed()),
        total_packs,
        total_songs,
    );
}
