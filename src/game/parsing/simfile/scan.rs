use super::process_song;
use crate::config::dirs;
use crate::game::song::{get_song_cache, set_song_cache};
use deadsync_chart::{SongData, SongPack};
use deadsync_simfile::scan::{
    PackScan, ScanFailure, collect_reload_pack_dirs, count_loaded_songs, empty_song_pack_from_scan,
    finalize_loaded_packs, fmt_scan_time, push_unique_path, replace_song_packs, scan_pack_dirs,
    scan_song_roots,
};
use log::{debug, info, warn};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn scan_and_load_songs(root_path: &Path) {
    scan_and_load_songs_impl::<fn(usize, usize, &str, &str)>(root_path, None);
}

pub fn scan_and_load_songs_with_progress<F>(root_path: &Path, progress: &mut F)
where
    F: FnMut(&str, &str),
{
    let mut with_counts = |_: usize, _: usize, pack: &str, song: &str| progress(pack, song);
    scan_and_load_songs_impl(root_path, Some(&mut with_counts));
}

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

#[derive(Default)]
struct SongLoadStats {
    songs_cache_hits: usize,
    songs_parsed: usize,
    songs_failed: usize,
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

#[inline(always)]
fn report_load_progress<F>(
    progress: &mut Option<&mut F>,
    done: usize,
    total: usize,
    group: &str,
    item: &str,
) where
    F: FnMut(usize, usize, &str, &str),
{
    if let Some(cb) = progress.as_mut() {
        cb(done, total, group, item);
    }
}

#[inline(always)]
fn song_pack_progress_name(pack: &SongPack) -> &str {
    pack.directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(pack.group_name.as_str())
}

#[inline(always)]
fn song_progress_name(path: &Path) -> &str {
    path.parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
        })
}

type SongParseMsg = (usize, PathBuf, Result<(Arc<SongData>, bool), String>);

fn reap_song_parse<F>(
    rx: Option<&std::sync::mpsc::Receiver<SongParseMsg>>,
    in_flight: &mut usize,
    loaded_packs: &mut Vec<SongPack>,
    stats: &mut SongLoadStats,
    songs_done: &mut usize,
    total_songs: usize,
    progress: &mut Option<&mut F>,
) where
    F: FnMut(usize, usize, &str, &str),
{
    let Some(rx) = rx else {
        return;
    };
    match rx.recv() {
        Ok((pack_idx, simfile_path, result)) => {
            *in_flight = in_flight.saturating_sub(1);
            match result {
                Ok((song_data, is_hit)) => {
                    if is_hit {
                        stats.songs_cache_hits += 1;
                    } else {
                        stats.songs_parsed += 1;
                    }
                    if let Some(pack) = loaded_packs.get_mut(pack_idx) {
                        pack.songs.push(song_data);
                    }
                }
                Err(error) => {
                    stats.songs_failed += 1;
                    warn!("Failed to load '{simfile_path:?}': {error}")
                }
            }
            *songs_done = songs_done.saturating_add(1);
            let pack_display = loaded_packs
                .get(pack_idx)
                .map_or("", song_pack_progress_name);
            report_load_progress(
                progress,
                *songs_done,
                total_songs,
                pack_display,
                song_progress_name(&simfile_path),
            );
        }
        Err(_) => {
            *in_flight = 0;
        }
    }
}

fn load_pack_scans<F>(
    packs: Vec<PackScan>,
    mut progress: Option<&mut F>,
) -> (Vec<SongPack>, SongLoadStats)
where
    F: FnMut(usize, usize, &str, &str),
{
    let config = crate::config::get();
    let fastload = config.fastload;
    let cachesongs = config.cachesongs;
    let global_offset_seconds = config.global_offset_seconds;

    let avail_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let mut parse_threads = match config.song_parsing_threads {
        0 => avail_threads,
        1 => 1,
        n => (n as usize).min(avail_threads).max(1),
    };
    if parse_threads < 1 {
        parse_threads = 1;
    }
    let parallel_parsing = parse_threads > 1;

    let mut loaded_packs = Vec::new();
    let mut stats = SongLoadStats::default();
    let total_songs = packs.iter().map(|pack| pack.songs.len()).sum::<usize>();
    let mut songs_done = 0usize;
    report_load_progress(&mut progress, 0, total_songs, "", "");

    let mut runtime: Option<tokio::runtime::Runtime> = None;
    let mut tx_opt: Option<std::sync::mpsc::Sender<SongParseMsg>> = None;
    let mut rx_opt: Option<std::sync::mpsc::Receiver<SongParseMsg>> = None;
    let mut in_flight = 0usize;

    for pack in packs {
        let pack_display = pack
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(pack.group_name.as_str())
            .to_owned();

        let current_pack = empty_song_pack_from_scan(&pack);
        debug!("Scanning pack: {}", current_pack.name);
        let pack_idx = loaded_packs.len();
        loaded_packs.push(current_pack);

        for song in pack.songs {
            let simfile_path = song.simfile;
            let song_display = song_progress_name(&simfile_path);

            if parallel_parsing {
                let rt = runtime.get_or_insert_with(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .max_blocking_threads(parse_threads)
                        .build()
                        .unwrap()
                });
                if tx_opt.is_none() || rx_opt.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel::<SongParseMsg>();
                    tx_opt = Some(tx);
                    rx_opt = Some(rx);
                }

                while in_flight >= parse_threads {
                    reap_song_parse(
                        rx_opt.as_ref(),
                        &mut in_flight,
                        &mut loaded_packs,
                        &mut stats,
                        &mut songs_done,
                        total_songs,
                        &mut progress,
                    );
                }

                let Some(tx) = tx_opt.as_ref() else {
                    match process_song(
                        simfile_path.clone(),
                        fastload,
                        cachesongs,
                        global_offset_seconds,
                    ) {
                        Ok((song_data, is_hit)) => {
                            if is_hit {
                                stats.songs_cache_hits += 1;
                            } else {
                                stats.songs_parsed += 1;
                            }
                            loaded_packs[pack_idx].songs.push(Arc::new(song_data));
                        }
                        Err(error) => {
                            stats.songs_failed += 1;
                            warn!("Failed to load '{simfile_path:?}': {error}")
                        }
                    }
                    songs_done = songs_done.saturating_add(1);
                    report_load_progress(
                        &mut progress,
                        songs_done,
                        total_songs,
                        pack_display.as_str(),
                        song_display,
                    );
                    continue;
                };

                let tx = tx.clone();
                let simfile_path_owned = simfile_path.clone();
                rt.handle().spawn_blocking(move || {
                    let out = catch_unwind(AssertUnwindSafe(|| {
                        process_song(
                            simfile_path_owned.clone(),
                            fastload,
                            cachesongs,
                            global_offset_seconds,
                        )
                        .map(|(data, is_hit)| (Arc::new(data), is_hit))
                    }))
                    .unwrap_or_else(|_| Err("Song parse panicked".to_string()));
                    let _ = tx.send((pack_idx, simfile_path_owned, out));
                });
                in_flight += 1;
            } else {
                match process_song(
                    simfile_path.clone(),
                    fastload,
                    cachesongs,
                    global_offset_seconds,
                ) {
                    Ok((song_data, is_hit)) => {
                        if is_hit {
                            stats.songs_cache_hits += 1;
                        } else {
                            stats.songs_parsed += 1;
                        }
                        loaded_packs[pack_idx].songs.push(Arc::new(song_data));
                    }
                    Err(error) => {
                        stats.songs_failed += 1;
                        warn!("Failed to load '{simfile_path:?}': {error}")
                    }
                }
                songs_done = songs_done.saturating_add(1);
                report_load_progress(
                    &mut progress,
                    songs_done,
                    total_songs,
                    pack_display.as_str(),
                    song_display,
                );
            }
        }
    }

    while in_flight > 0 {
        reap_song_parse(
            rx_opt.as_ref(),
            &mut in_flight,
            &mut loaded_packs,
            &mut stats,
            &mut songs_done,
            total_songs,
            &mut progress,
        );
    }

    if runtime.is_some() {
        debug!(
            "Song parsing: used {} threads for cache/parsing (SongParsingThreads={}).",
            parse_threads, config.song_parsing_threads
        );
    }

    finalize_loaded_packs(&mut loaded_packs);
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
