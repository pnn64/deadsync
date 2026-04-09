use super::{fmt_scan_time, process_song};
use crate::config::dirs;
use crate::game::song::{SongData, SongPack, get_song_cache, set_song_cache};
use log::{debug, info, warn};
use rssp::pack::{PackScan, SongScan};
use std::collections::HashMap;
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

fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    key
}

fn push_unique_path(path: PathBuf, roots: &mut Vec<PathBuf>, keys: &mut Vec<String>) {
    let key = path_key(&path);
    if keys.iter().any(|existing| existing == &key) {
        return;
    }
    keys.push(key);
    roots.push(path);
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

    let additional_folders = crate::config::additional_song_folders();
    for raw in additional_folders.split(',') {
        let path = raw.trim();
        if path.is_empty() {
            continue;
        }
        let extra_root = PathBuf::from(path);
        if extra_root.is_dir() {
            push_unique_path(extra_root, &mut roots, &mut keys);
        } else {
            warn!(
                "AdditionalSongFolders entry '{}' is not a directory; skipping.",
                path
            );
        }
    }
    roots
}

fn itgmania_make_sort_bytes(text: &str) -> Vec<u8> {
    let mut out = text.as_bytes().to_vec();
    out.make_ascii_uppercase();

    if matches!(out.first(), Some(b'.')) {
        out.remove(0);
    }

    if let Some(&byte) = out.first() {
        let is_alpha = byte.is_ascii_uppercase();
        let is_digit = byte.is_ascii_digit();
        if !is_alpha && !is_digit {
            out.insert(0, b'~');
        }
    }

    out
}

struct ItgmaniaSongTitleKey {
    main_raw: Vec<u8>,
    main_sort: Vec<u8>,
    sub_sort: Vec<u8>,
    path_fold: Vec<u8>,
}

impl ItgmaniaSongTitleKey {
    fn new(song: &SongData) -> Self {
        let main_raw_str = if song.translit_title.is_empty() {
            song.title.as_str()
        } else {
            song.translit_title.as_str()
        };
        let sub_raw_str = if song.translit_subtitle.is_empty() {
            song.subtitle.as_str()
        } else {
            song.translit_subtitle.as_str()
        };

        let mut path_fold = song
            .simfile_path
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        path_fold.make_ascii_lowercase();

        Self {
            main_raw: main_raw_str.as_bytes().to_vec(),
            main_sort: itgmania_make_sort_bytes(main_raw_str),
            sub_sort: itgmania_make_sort_bytes(sub_raw_str),
            path_fold,
        }
    }
}

impl PartialEq for ItgmaniaSongTitleKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for ItgmaniaSongTitleKey {}

impl PartialOrd for ItgmaniaSongTitleKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ItgmaniaSongTitleKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.main_raw == other.main_raw {
            match self.sub_sort.cmp(&other.sub_sort) {
                std::cmp::Ordering::Equal => self.path_fold.cmp(&other.path_fold),
                ordering => ordering,
            }
        } else {
            match self.main_sort.cmp(&other.main_sort) {
                std::cmp::Ordering::Equal => self.path_fold.cmp(&other.path_fold),
                ordering => ordering,
            }
        }
    }
}

fn ci_key(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}

fn song_scan_key(song: &SongScan) -> String {
    song.dir
        .file_name()
        .and_then(|name| name.to_str())
        .map(ci_key)
        .filter(|key| !key.is_empty())
        .unwrap_or_else(|| song.dir.to_string_lossy().to_ascii_lowercase())
}

fn merge_pack_scan(dst: &mut PackScan, mut src: PackScan) {
    dst.dir.clone_from(&src.dir);
    if src.has_pack_ini {
        dst.display_title.clone_from(&src.display_title);
        dst.sort_title.clone_from(&src.sort_title);
        dst.translit_title.clone_from(&src.translit_title);
        dst.series.clone_from(&src.series);
        dst.year = src.year;
        dst.version = src.version;
        dst.has_pack_ini = true;
        dst.sync_pref = src.sync_pref;
    }
    if src.banner_path.is_some() {
        dst.banner_path.clone_from(&src.banner_path);
    }
    if src.background_path.is_some() {
        dst.background_path.clone_from(&src.background_path);
    }

    let mut song_slots = HashMap::with_capacity(dst.songs.len() + src.songs.len());
    for (idx, song) in dst.songs.iter().enumerate() {
        song_slots.insert(song_scan_key(song), idx);
    }
    for song in src.songs.drain(..) {
        let key = song_scan_key(&song);
        if let Some(slot) = song_slots.get(&key).copied() {
            dst.songs[slot] = song;
        } else {
            let slot = dst.songs.len();
            song_slots.insert(key, slot);
            dst.songs.push(song);
        }
    }
}

fn merge_pack_scans(mut packs: Vec<PackScan>) -> Vec<PackScan> {
    let mut merged = Vec::with_capacity(packs.len());
    let mut pack_slots = HashMap::with_capacity(packs.len());

    for pack in packs.drain(..) {
        let key = ci_key(&pack.group_name);
        if key.is_empty() {
            merged.push(pack);
            continue;
        }
        if let Some(slot) = pack_slots.get(&key).copied() {
            merge_pack_scan(&mut merged[slot], pack);
        } else {
            let slot = merged.len();
            pack_slots.insert(key, slot);
            merged.push(pack);
        }
    }

    merged
}

fn pack_dir_key(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ci_key)
        .filter(|key| !key.is_empty())
}

fn collect_reload_pack_dirs(
    song_roots: &[PathBuf],
    dirs: &[PathBuf],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut pack_dirs = Vec::with_capacity(dirs.len());
    let mut pack_dir_keys = Vec::with_capacity(dirs.len());
    let mut pack_keys = Vec::with_capacity(dirs.len());

    for dir in dirs {
        let Some(key) = pack_dir_key(dir) else {
            continue;
        };
        if !pack_keys.iter().any(|existing| existing == &key) {
            pack_keys.push(key);
        }

        if dir.is_dir() {
            push_unique_path(dir.to_path_buf(), &mut pack_dirs, &mut pack_dir_keys);
        }

        let Some(file_name) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for root in song_roots {
            let candidate = root.join(file_name);
            if candidate.is_dir() {
                push_unique_path(candidate, &mut pack_dirs, &mut pack_dir_keys);
            }
        }
    }

    (pack_dirs, pack_keys)
}

fn scan_song_roots(song_roots: &[PathBuf]) -> Vec<PackScan> {
    let mut packs = Vec::new();
    for songs_root in song_roots {
        match rssp::pack::scan_songs_dir(songs_root, rssp::pack::ScanOpt::default()) {
            Ok(mut found) => packs.append(&mut found),
            Err(error) => warn!(
                "Could not scan songs dir '{}': {error:?}",
                songs_root.display()
            ),
        }
    }
    merge_pack_scans(packs)
}

fn scan_pack_dirs(pack_dirs: &[PathBuf]) -> Vec<PackScan> {
    let mut packs = Vec::new();
    for pack_dir in pack_dirs {
        match rssp::pack::scan_pack_dir(pack_dir, rssp::pack::ScanOpt::default()) {
            Ok(Some(pack)) => packs.push(pack),
            Ok(None) => {}
            Err(error) => warn!(
                "Could not scan pack dir '{}': {error:?}",
                pack_dir.display()
            ),
        }
    }
    merge_pack_scans(packs)
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

fn count_loaded_songs(packs: &[SongPack]) -> usize {
    packs.iter().map(|pack| pack.songs.len()).sum()
}

fn sort_song_packs(packs: &mut Vec<SongPack>) {
    packs.sort_by_cached_key(|pack| {
        (
            pack.sort_title.to_ascii_lowercase(),
            pack.group_name.to_ascii_lowercase(),
        )
    });
}

fn finalize_loaded_packs(loaded_packs: &mut Vec<SongPack>) {
    loaded_packs.retain(|pack| !pack.songs.is_empty());
    for pack in loaded_packs.iter_mut() {
        pack.songs
            .sort_by_cached_key(|song| ItgmaniaSongTitleKey::new(song.as_ref()));
    }
    sort_song_packs(loaded_packs);
}

fn replace_song_packs(
    song_cache: &mut Vec<SongPack>,
    pack_keys: &[String],
    mut reloaded: Vec<SongPack>,
) {
    if pack_keys.is_empty() {
        return;
    }
    song_cache.retain(|pack| {
        let key = ci_key(&pack.group_name);
        !pack_keys.iter().any(|existing| existing == &key)
    });
    song_cache.append(&mut reloaded);
    sort_song_packs(song_cache);
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

        let current_pack = SongPack {
            group_name: pack.group_name,
            name: pack.display_title,
            sort_title: pack.sort_title,
            translit_title: pack.translit_title,
            series: pack.series,
            year: pack.year,
            sync_pref: pack.sync_pref,
            directory: pack.dir,
            banner_path: pack.banner_path,
            songs: Vec::new(),
        };
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

    let packs = scan_song_roots(&song_roots);
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
    let packs = scan_pack_dirs(&scan_dirs);
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

#[cfg(test)]
mod tests {
    use super::{collect_reload_pack_dirs, merge_pack_scans, replace_song_packs};
    use crate::game::song::SongPack;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn pack_scan(
        group_name: &str,
        display_title: &str,
        has_pack_ini: bool,
        banner_path: Option<&str>,
        songs: &[&str],
        root: &Path,
    ) -> rssp::pack::PackScan {
        let dir = root.join(group_name);
        rssp::pack::PackScan {
            dir: dir.clone(),
            group_name: group_name.to_string(),
            display_title: display_title.to_string(),
            sort_title: display_title.to_string(),
            translit_title: display_title.to_string(),
            series: String::new(),
            year: 0,
            version: i32::from(has_pack_ini),
            has_pack_ini,
            sync_pref: rssp::pack::SyncPref::Default,
            banner_path: banner_path.map(PathBuf::from),
            background_path: None,
            songs: songs
                .iter()
                .map(|song| {
                    let song_dir = dir.join(song);
                    rssp::pack::SongScan {
                        dir: song_dir.clone(),
                        simfile: song_dir.join("song.sm"),
                        extension: "sm",
                    }
                })
                .collect(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deadsync-scan-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn song_pack(group_name: &str, sort_title: &str, root: &Path) -> SongPack {
        SongPack {
            group_name: group_name.to_string(),
            name: sort_title.to_string(),
            sort_title: sort_title.to_string(),
            translit_title: sort_title.to_string(),
            series: String::new(),
            year: 0,
            sync_pref: rssp::pack::SyncPref::Default,
            directory: root.join(group_name),
            banner_path: None,
            songs: Vec::new(),
        }
    }

    #[test]
    fn merge_pack_scans_collapses_case_insensitive_groups() {
        let root = test_dir("merge-pack-scans");
        let base = root.join("base");
        let extra = root.join("extra");
        let packs = vec![
            pack_scan(
                "Pack",
                "Fancy Pack",
                true,
                Some("base-banner.png"),
                &["Alpha", "Dupe"],
                &base,
            ),
            pack_scan("pack", "pack", false, None, &["Beta", "dupe"], &extra),
        ];

        let merged = merge_pack_scans(packs);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].display_title, "Fancy Pack");
        assert_eq!(
            merged[0].banner_path,
            Some(PathBuf::from("base-banner.png"))
        );

        let mut names = merged[0]
            .songs
            .iter()
            .map(|song| {
                song.dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap()
                    .to_ascii_lowercase()
            })
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta", "dupe"]);
        assert!(
            merged[0]
                .songs
                .iter()
                .any(|song| song.dir.starts_with(&extra))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_reload_pack_dirs_includes_matching_pack_dirs_across_roots() {
        let root = test_dir("reload-pack-dirs");
        let base = root.join("base");
        let extra = root.join("extra");
        let base_pack = base.join("Pack");
        let extra_pack = extra.join("Pack");
        fs::create_dir_all(&base_pack).unwrap();
        fs::create_dir_all(&extra_pack).unwrap();
        fs::create_dir_all(base.join("Other")).unwrap();

        let (dirs, keys) = collect_reload_pack_dirs(
            &[base.clone(), extra.clone()],
            std::slice::from_ref(&base_pack),
        );

        let mut actual_dirs = dirs
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        actual_dirs.sort();
        let mut expected_dirs = vec![
            base_pack.to_string_lossy().into_owned(),
            extra_pack.to_string_lossy().into_owned(),
        ];
        expected_dirs.sort();

        assert_eq!(actual_dirs, expected_dirs);
        assert_eq!(keys, vec!["pack".to_string()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn replace_song_packs_only_updates_targeted_group() {
        let root = test_dir("replace-song-packs");
        let before_root = root.join("before");
        let after_root = root.join("after");
        let mut cache = vec![
            song_pack("Alpha", "Bravo", &before_root),
            song_pack("Pack", "Zulu", &before_root),
            song_pack("Beta", "Alpha", &before_root),
        ];

        replace_song_packs(
            &mut cache,
            &["pack".to_string()],
            vec![song_pack("Pack", "Charlie", &after_root)],
        );

        let group_names = cache
            .iter()
            .map(|pack| pack.group_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(group_names, vec!["Beta", "Alpha", "Pack"]);
        assert_eq!(cache.len(), 3);
        assert_eq!(cache[2].directory, after_root.join("Pack"));

        let _ = fs::remove_dir_all(root);
    }
}
