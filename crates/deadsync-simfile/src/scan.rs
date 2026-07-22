use deadsync_chart::{SongData, SongPack, SyncPref};
use rssp::pack::{PackScan as RsspPackScan, SongScan as RsspSongScan};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::runtime_cache;

#[derive(Clone, Debug)]
pub struct SongScan {
    pub dir: PathBuf,
    pub simfile: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PackScan {
    pub dir: PathBuf,
    pub group_name: String,
    pub display_title: String,
    pub sort_title: String,
    pub translit_title: String,
    pub series: String,
    pub year: i32,
    pub sync_pref: SyncPref,
    pub banner_path: Option<PathBuf>,
    pub songs: Vec<SongScan>,
    version: i32,
    has_pack_ini: bool,
    background_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Clone, Copy, Debug)]
pub struct SongLoadOptions {
    pub fastload: bool,
    pub cachesongs: bool,
    pub global_offset_seconds: f32,
    pub song_parsing_threads: u32,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SongLoadStats {
    pub songs_cache_hits: usize,
    pub songs_parsed: usize,
    pub songs_failed: usize,
    pub used_parallel: bool,
    pub parse_threads: usize,
}

#[derive(Clone, Debug)]
pub struct RuntimeSongScanInput {
    pub base_root: PathBuf,
    pub song_roots: Vec<PathBuf>,
    pub cache_dir: PathBuf,
    pub load_options: SongLoadOptions,
    pub requested_threads: u8,
}

#[derive(Clone, Debug)]
pub struct RuntimeSongScanEnv {
    pub base_root: PathBuf,
    pub extra_song_roots: Vec<PathBuf>,
    pub additional_song_roots: Vec<(String, PathBuf)>,
    pub cache_dir: PathBuf,
    pub load_options: SongLoadOptions,
    pub requested_threads: u8,
}

#[derive(Clone, Debug)]
pub struct RuntimeCourseScanEnv {
    pub courses_root: PathBuf,
    pub songs_root: PathBuf,
    pub extra_course_roots: Vec<PathBuf>,
    pub extra_song_roots: Vec<PathBuf>,
    pub additional_song_roots: Vec<(String, PathBuf)>,
    pub autogen_courses_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeScanAdapterEvent {
    SongRoot(SongScanRootEvent),
    CourseRootMissing { root: PathBuf },
    Song(RuntimeSongScanEvent),
    Course(crate::course::RuntimeCourseScanEvent),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SongScanRootReport {
    pub roots: Vec<PathBuf>,
    pub events: Vec<SongScanRootEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongScanRootEvent {
    PrimaryMissing { root: PathBuf },
    AdditionalMissing { label: String, root: PathBuf },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeSongScanEvent {
    StartScan {
        base_root: PathBuf,
    },
    CacheDirError {
        cache_dir: PathBuf,
        error: String,
    },
    NoSongRoots,
    ScanFailure {
        kind: &'static str,
        path: PathBuf,
        error: String,
    },
    PackScan {
        name: String,
    },
    NeverCache {
        pack_display: String,
    },
    SongLoadFailed {
        simfile_path: PathBuf,
        error: String,
    },
    UsedParallel {
        parse_threads: usize,
        requested_threads: u8,
    },
    FinishedScan {
        packs: usize,
        songs: usize,
        stats: SongLoadStats,
        elapsed: Duration,
    },
    NoReloadSongRoots,
    NoReloadPackDirs,
    StartReload {
        packs: usize,
    },
    FinishedReload {
        reloaded_packs: usize,
        reloaded_songs: usize,
        stats: SongLoadStats,
        elapsed: Duration,
        total_packs: usize,
        total_songs: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeScanLogLevel {
    Debug,
    Info,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeScanLogEntry {
    pub level: RuntimeScanLogLevel,
    pub message: String,
}

impl RuntimeScanLogEntry {
    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            level: RuntimeScanLogLevel::Debug,
            message: message.into(),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: RuntimeScanLogLevel::Info,
            message: message.into(),
        }
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: RuntimeScanLogLevel::Warn,
            message: message.into(),
        }
    }
}

type SongParseMsg = (usize, PathBuf, Result<(Arc<SongData>, bool), String>);

pub fn push_unique_path(path: PathBuf, roots: &mut Vec<PathBuf>, keys: &mut Vec<String>) {
    let key = path_key(&path);
    if keys.iter().any(|existing| existing == &key) {
        return;
    }
    keys.push(key);
    roots.push(path);
}

pub fn collect_song_scan_roots(
    primary_root: &Path,
    extra_roots: impl IntoIterator<Item = PathBuf>,
    additional_roots: impl IntoIterator<Item = (String, PathBuf)>,
) -> SongScanRootReport {
    let mut report = SongScanRootReport {
        roots: Vec::with_capacity(4),
        events: Vec::new(),
    };
    let mut keys = Vec::with_capacity(4);

    if primary_root.is_dir() {
        push_unique_path(primary_root.to_path_buf(), &mut report.roots, &mut keys);
    } else {
        report.events.push(SongScanRootEvent::PrimaryMissing {
            root: primary_root.to_path_buf(),
        });
    }

    for extra in extra_roots {
        push_unique_path(extra, &mut report.roots, &mut keys);
    }

    for (label, root) in additional_roots {
        if root.is_dir() {
            push_unique_path(root, &mut report.roots, &mut keys);
        } else {
            report
                .events
                .push(SongScanRootEvent::AdditionalMissing { label, root });
        }
    }

    report
}

pub fn runtime_collect_song_scan_roots(
    env: &RuntimeSongScanEnv,
    mut event: impl FnMut(RuntimeScanAdapterEvent),
) -> Vec<PathBuf> {
    let report = collect_song_scan_roots(
        &env.base_root,
        env.extra_song_roots.clone(),
        env.additional_song_roots.clone(),
    );
    for root_event in report.events {
        event(RuntimeScanAdapterEvent::SongRoot(root_event));
    }
    report.roots
}

fn runtime_collect_course_scan_roots(
    env: &RuntimeCourseScanEnv,
    event: &mut impl FnMut(RuntimeScanAdapterEvent),
) -> Vec<PathBuf> {
    let report =
        crate::course::collect_course_scan_roots(&env.courses_root, env.extra_course_roots.clone());
    if report.primary_missing {
        event(RuntimeScanAdapterEvent::CourseRootMissing {
            root: env.courses_root.clone(),
        });
    }
    report.roots
}

fn runtime_song_scan_input(
    env: &RuntimeSongScanEnv,
    event: &mut impl FnMut(RuntimeScanAdapterEvent),
) -> RuntimeSongScanInput {
    RuntimeSongScanInput {
        base_root: env.base_root.clone(),
        song_roots: runtime_collect_song_scan_roots(env, event),
        cache_dir: env.cache_dir.clone(),
        load_options: env.load_options,
        requested_threads: env.requested_threads,
    }
}

fn runtime_course_scan_input(
    env: &RuntimeCourseScanEnv,
    event: &mut impl FnMut(RuntimeScanAdapterEvent),
) -> crate::course::RuntimeCourseScanInput {
    let song_env = RuntimeSongScanEnv {
        base_root: env.songs_root.clone(),
        extra_song_roots: env.extra_song_roots.clone(),
        additional_song_roots: env.additional_song_roots.clone(),
        cache_dir: PathBuf::new(),
        load_options: SongLoadOptions {
            fastload: false,
            cachesongs: false,
            global_offset_seconds: 0.0,
            song_parsing_threads: 0,
        },
        requested_threads: 0,
    };
    crate::course::RuntimeCourseScanInput {
        courses_root: env.courses_root.clone(),
        course_roots: runtime_collect_course_scan_roots(env, event),
        song_roots: runtime_collect_song_scan_roots(&song_env, event),
        autogen_courses_root: env.autogen_courses_root.clone(),
    }
}

pub fn scan_and_load_songs_with_progress_counts_runtime<Progress, Process, NeverCache>(
    env: RuntimeSongScanEnv,
    progress: &mut Progress,
    process_song: Process,
    group_is_never_cached: NeverCache,
    mut event: impl FnMut(RuntimeScanAdapterEvent),
) where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
{
    let input = runtime_song_scan_input(&env, &mut event);
    scan_and_load_songs_runtime(
        input,
        Some(progress),
        process_song,
        group_is_never_cached,
        |scan_event| event(RuntimeScanAdapterEvent::Song(scan_event)),
    );
}

pub fn reload_song_dirs_with_progress_counts_runtime<Progress, Process, NeverCache>(
    env: RuntimeSongScanEnv,
    pack_dirs: &[PathBuf],
    progress: &mut Progress,
    process_song: Process,
    group_is_never_cached: NeverCache,
    mut event: impl FnMut(RuntimeScanAdapterEvent),
) where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
{
    let input = runtime_song_scan_input(&env, &mut event);
    reload_song_dirs_runtime(
        input,
        pack_dirs,
        Some(progress),
        process_song,
        group_is_never_cached,
        |scan_event| event(RuntimeScanAdapterEvent::Song(scan_event)),
    );
}

pub fn scan_and_load_courses_with_progress_counts_runtime<Progress>(
    env: RuntimeCourseScanEnv,
    progress: &mut Progress,
    mut event: impl FnMut(RuntimeScanAdapterEvent),
) where
    Progress: FnMut(usize, usize, &str, &str),
{
    let input = runtime_course_scan_input(&env, &mut event);
    crate::course::scan_and_load_courses_runtime(input, Some(progress), |scan_event| {
        event(RuntimeScanAdapterEvent::Course(scan_event));
    });
}

pub fn fmt_scan_time(d: Duration) -> String {
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

pub fn runtime_song_scan_log_entry(event: RuntimeSongScanEvent) -> RuntimeScanLogEntry {
    match event {
        RuntimeSongScanEvent::StartScan { base_root } => RuntimeScanLogEntry::info(format!(
            "Starting simfile scan (base songs root '{}')...",
            base_root.display()
        )),
        RuntimeSongScanEvent::CacheDirError { cache_dir, error } => {
            RuntimeScanLogEntry::warn(format!(
                "Could not create cache directory '{}': {}. Caching will be disabled.",
                cache_dir.to_string_lossy(),
                error
            ))
        }
        RuntimeSongScanEvent::NoSongRoots => {
            RuntimeScanLogEntry::warn("No valid song roots found. No songs will be loaded.")
        }
        RuntimeSongScanEvent::ScanFailure { kind, path, error } => RuntimeScanLogEntry::warn(
            format!("Could not scan {kind} '{}': {}", path.display(), error),
        ),
        RuntimeSongScanEvent::PackScan { name } => {
            RuntimeScanLogEntry::debug(format!("Scanning pack: {name}"))
        }
        RuntimeSongScanEvent::NeverCache { pack_display } => RuntimeScanLogEntry::debug(format!(
            "Skipping song cache for pack '{pack_display}' (NeverCacheList)."
        )),
        RuntimeSongScanEvent::SongLoadFailed {
            simfile_path,
            error,
        } => RuntimeScanLogEntry::warn(format!("Failed to load '{simfile_path:?}': {error}")),
        RuntimeSongScanEvent::UsedParallel {
            parse_threads,
            requested_threads,
        } => RuntimeScanLogEntry::debug(format!(
            "Song parsing: used {parse_threads} threads for cache/parsing (SongParsingThreads={requested_threads})."
        )),
        RuntimeSongScanEvent::FinishedScan {
            packs,
            songs,
            stats,
            elapsed,
        } => RuntimeScanLogEntry::info(format!(
            "Finished scan. Found {} packs / {} songs (parsed {}, cache hits {}, failed {}) in {}.",
            packs,
            songs,
            stats.songs_parsed,
            stats.songs_cache_hits,
            stats.songs_failed,
            fmt_scan_time(elapsed)
        )),
        RuntimeSongScanEvent::NoReloadSongRoots => {
            RuntimeScanLogEntry::warn("No valid song roots found. No songs will be reloaded.")
        }
        RuntimeSongScanEvent::NoReloadPackDirs => RuntimeScanLogEntry::warn(
            "No valid song pack directories were requested for targeted reload.",
        ),
        RuntimeSongScanEvent::StartReload { packs } => RuntimeScanLogEntry::info(format!(
            "Starting targeted song reload for {packs} affected pack(s)..."
        )),
        RuntimeSongScanEvent::FinishedReload {
            reloaded_packs,
            reloaded_songs,
            stats,
            elapsed,
            total_packs,
            total_songs,
        } => RuntimeScanLogEntry::info(format!(
            "Finished targeted reload. Reloaded {} packs / {} songs (parsed {}, cache hits {}, failed {}) in {}. Song cache now has {} packs / {} songs.",
            reloaded_packs,
            reloaded_songs,
            stats.songs_parsed,
            stats.songs_cache_hits,
            stats.songs_failed,
            fmt_scan_time(elapsed),
            total_packs,
            total_songs,
        )),
    }
}

pub fn scan_song_roots(song_roots: &[PathBuf]) -> (Vec<PackScan>, Vec<ScanFailure>) {
    let mut packs = Vec::new();
    let mut failures = Vec::new();
    for songs_root in song_roots {
        match rssp::pack::scan_songs_dir(songs_root, rssp::pack::ScanOpt::default()) {
            Ok(found) => packs.extend(found.into_iter().map(PackScan::from)),
            Err(error) => failures.push(ScanFailure {
                path: songs_root.clone(),
                error: format!("{error:?}"),
            }),
        }
    }
    (merge_pack_scans(packs), failures)
}

pub fn scan_pack_dirs(pack_dirs: &[PathBuf]) -> (Vec<PackScan>, Vec<ScanFailure>) {
    let mut packs = Vec::new();
    let mut failures = Vec::new();
    for pack_dir in pack_dirs {
        match rssp::pack::scan_pack_dir(pack_dir, rssp::pack::ScanOpt::default()) {
            Ok(Some(pack)) => packs.push(PackScan::from(pack)),
            Ok(None) => {}
            Err(error) => failures.push(ScanFailure {
                path: pack_dir.clone(),
                error: format!("{error:?}"),
            }),
        }
    }
    (merge_pack_scans(packs), failures)
}

pub fn merge_pack_scans(mut packs: Vec<PackScan>) -> Vec<PackScan> {
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

pub fn collect_reload_pack_dirs(
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

pub fn empty_song_pack_from_scan(pack: &PackScan) -> SongPack {
    SongPack {
        group_name: pack.group_name.clone(),
        name: pack.display_title.clone(),
        sort_title: pack.sort_title.clone(),
        translit_title: pack.translit_title.clone(),
        series: pack.series.clone(),
        year: pack.year,
        sync_pref: pack.sync_pref,
        directory: pack.dir.clone(),
        banner_path: pack.banner_path.clone(),
        songs: Vec::new(),
    }
}

pub fn count_loaded_songs(packs: &[SongPack]) -> usize {
    packs.iter().map(|pack| pack.songs.len()).sum()
}

#[inline(always)]
pub fn song_pack_progress_name(pack: &SongPack) -> &str {
    pack.directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(pack.group_name.as_str())
}

#[inline(always)]
pub fn song_progress_name(path: &Path) -> &str {
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

pub fn finalize_loaded_packs(loaded_packs: &mut Vec<SongPack>) {
    loaded_packs.retain(|pack| !pack.songs.is_empty());
    for pack in loaded_packs.iter_mut() {
        sort_songs_itgmania(&mut pack.songs);
    }
    sort_song_packs(loaded_packs);
}

pub fn replace_song_packs(
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

pub fn load_pack_scans_with<Progress, Process, NeverCache, OnError, OnPack, OnNeverCache>(
    packs: Vec<PackScan>,
    options: SongLoadOptions,
    mut progress: Option<&mut Progress>,
    process_song: Process,
    group_is_never_cached: NeverCache,
    mut on_song_error: OnError,
    mut on_pack: OnPack,
    mut on_never_cache: OnNeverCache,
) -> (Vec<SongPack>, SongLoadStats)
where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
    OnError: FnMut(&Path, &str),
    OnPack: FnMut(&SongPack),
    OnNeverCache: FnMut(&str),
{
    let avail_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let mut parse_threads = match options.song_parsing_threads {
        0 => avail_threads,
        1 => 1,
        n => (n as usize).min(avail_threads).max(1),
    };
    if parse_threads < 1 {
        parse_threads = 1;
    }
    let parallel_parsing = parse_threads > 1;

    let mut loaded_packs = Vec::new();
    let mut stats = SongLoadStats {
        parse_threads,
        ..SongLoadStats::default()
    };
    let total_songs = packs.iter().map(|pack| pack.songs.len()).sum::<usize>();
    let mut songs_done = 0usize;
    report_load_progress(&mut progress, 0, total_songs, "", "");

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
        on_pack(&current_pack);
        let pack_idx = loaded_packs.len();
        loaded_packs.push(current_pack);

        let pack_never_cache =
            group_is_never_cached(&pack.group_name) || group_is_never_cached(&pack_display);
        if pack_never_cache {
            on_never_cache(&pack_display);
        }
        let pack_fastload = options.fastload && !pack_never_cache;
        let pack_cachesongs = options.cachesongs && !pack_never_cache;

        for song in pack.songs {
            let simfile_path = song.simfile;
            let song_display = song_progress_name(&simfile_path);

            if parallel_parsing {
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
                        &mut on_song_error,
                    );
                }

                let Some(tx) = tx_opt.as_ref() else {
                    process_song_sequential(
                        process_song,
                        simfile_path.clone(),
                        pack_fastload,
                        pack_cachesongs,
                        options.global_offset_seconds,
                        pack_idx,
                        &mut loaded_packs,
                        &mut stats,
                        &mut on_song_error,
                    );
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
                stats.used_parallel = true;
                std::thread::spawn(move || {
                    let out = catch_unwind(AssertUnwindSafe(|| {
                        process_song(
                            simfile_path_owned.clone(),
                            pack_fastload,
                            pack_cachesongs,
                            options.global_offset_seconds,
                        )
                        .map(|(data, is_hit)| (Arc::new(data), is_hit))
                    }))
                    .unwrap_or_else(|_| Err("Song parse panicked".to_string()));
                    let _ = tx.send((pack_idx, simfile_path_owned, out));
                });
                in_flight += 1;
            } else {
                process_song_sequential(
                    process_song,
                    simfile_path.clone(),
                    pack_fastload,
                    pack_cachesongs,
                    options.global_offset_seconds,
                    pack_idx,
                    &mut loaded_packs,
                    &mut stats,
                    &mut on_song_error,
                );
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
            &mut on_song_error,
        );
    }

    finalize_loaded_packs(&mut loaded_packs);
    (loaded_packs, stats)
}

fn ensure_runtime_song_cache_dir(cache_dir: &Path, event: &mut impl FnMut(RuntimeSongScanEvent)) {
    if let Err(error) = fs::create_dir_all(cache_dir) {
        event(RuntimeSongScanEvent::CacheDirError {
            cache_dir: cache_dir.to_path_buf(),
            error: error.to_string(),
        });
    }
}

fn emit_scan_failures(
    kind: &'static str,
    failures: &[ScanFailure],
    event: &mut impl FnMut(RuntimeSongScanEvent),
) {
    for failure in failures {
        event(RuntimeSongScanEvent::ScanFailure {
            kind,
            path: failure.path.clone(),
            error: failure.error.clone(),
        });
    }
}

fn load_runtime_pack_scans<Progress, Process, NeverCache>(
    packs: Vec<PackScan>,
    input: &RuntimeSongScanInput,
    progress: Option<&mut Progress>,
    process_song: Process,
    group_is_never_cached: NeverCache,
    event: &mut impl FnMut(RuntimeSongScanEvent),
) -> (Vec<SongPack>, SongLoadStats)
where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
{
    let pending_events = RefCell::new(Vec::new());
    let (loaded_packs, stats) = load_pack_scans_with(
        packs,
        input.load_options,
        progress,
        process_song,
        group_is_never_cached,
        |simfile_path, error| {
            pending_events
                .borrow_mut()
                .push(RuntimeSongScanEvent::SongLoadFailed {
                    simfile_path: simfile_path.to_path_buf(),
                    error: error.to_string(),
                });
        },
        |pack| {
            pending_events
                .borrow_mut()
                .push(RuntimeSongScanEvent::PackScan {
                    name: pack.name.clone(),
                });
        },
        |pack_display| {
            pending_events
                .borrow_mut()
                .push(RuntimeSongScanEvent::NeverCache {
                    pack_display: pack_display.to_string(),
                });
        },
    );
    for pending in pending_events.into_inner() {
        event(pending);
    }
    if stats.used_parallel {
        event(RuntimeSongScanEvent::UsedParallel {
            parse_threads: stats.parse_threads,
            requested_threads: input.requested_threads,
        });
    }
    (loaded_packs, stats)
}

pub fn scan_and_load_songs_runtime<Progress, Process, NeverCache>(
    input: RuntimeSongScanInput,
    progress: Option<&mut Progress>,
    process_song: Process,
    group_is_never_cached: NeverCache,
    mut event: impl FnMut(RuntimeSongScanEvent),
) where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
{
    event(RuntimeSongScanEvent::StartScan {
        base_root: input.base_root.clone(),
    });
    let started = Instant::now();
    ensure_runtime_song_cache_dir(&input.cache_dir, &mut event);

    if input.song_roots.is_empty() {
        event(RuntimeSongScanEvent::NoSongRoots);
        runtime_cache::set_song_cache(Vec::new());
        return;
    }

    let (packs, failures) = scan_song_roots(&input.song_roots);
    emit_scan_failures("songs dir", &failures, &mut event);
    let (loaded_packs, stats) = load_runtime_pack_scans(
        packs,
        &input,
        progress,
        process_song,
        group_is_never_cached,
        &mut event,
    );
    let songs_loaded = count_loaded_songs(&loaded_packs);
    event(RuntimeSongScanEvent::FinishedScan {
        packs: loaded_packs.len(),
        songs: songs_loaded,
        stats,
        elapsed: started.elapsed(),
    });
    runtime_cache::set_song_cache(loaded_packs);
}

pub fn reload_song_dirs_runtime<Progress, Process, NeverCache>(
    input: RuntimeSongScanInput,
    pack_dirs: &[PathBuf],
    progress: Option<&mut Progress>,
    process_song: Process,
    group_is_never_cached: NeverCache,
    mut event: impl FnMut(RuntimeSongScanEvent),
) where
    Progress: FnMut(usize, usize, &str, &str),
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>
        + Copy
        + Send
        + Sync
        + 'static,
    NeverCache: Fn(&str) -> bool,
{
    ensure_runtime_song_cache_dir(&input.cache_dir, &mut event);
    if input.song_roots.is_empty() {
        event(RuntimeSongScanEvent::NoReloadSongRoots);
        return;
    }

    let (scan_dirs, pack_keys) = collect_reload_pack_dirs(&input.song_roots, pack_dirs);
    if pack_keys.is_empty() {
        event(RuntimeSongScanEvent::NoReloadPackDirs);
        return;
    }

    event(RuntimeSongScanEvent::StartReload {
        packs: pack_keys.len(),
    });
    let started = Instant::now();
    let (packs, failures) = scan_pack_dirs(&scan_dirs);
    emit_scan_failures("pack dir", &failures, &mut event);
    let (reloaded_packs, stats) = load_runtime_pack_scans(
        packs,
        &input,
        progress,
        process_song,
        group_is_never_cached,
        &mut event,
    );
    let reloaded_pack_count = reloaded_packs.len();
    let reloaded_song_count = count_loaded_songs(&reloaded_packs);

    let (total_packs, total_songs) = {
        let mut song_cache = runtime_cache::get_song_cache();
        replace_song_packs(&mut song_cache, &pack_keys, reloaded_packs);
        (song_cache.len(), count_loaded_songs(&song_cache))
    };

    event(RuntimeSongScanEvent::FinishedReload {
        reloaded_packs: reloaded_pack_count,
        reloaded_songs: reloaded_song_count,
        stats,
        elapsed: started.elapsed(),
        total_packs,
        total_songs,
    });
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

fn process_song_sequential<Process, OnError>(
    process_song: Process,
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
    pack_idx: usize,
    loaded_packs: &mut [SongPack],
    stats: &mut SongLoadStats,
    on_song_error: &mut OnError,
) where
    Process: Fn(PathBuf, bool, bool, f32) -> Result<(SongData, bool), String>,
    OnError: FnMut(&Path, &str),
{
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
            on_song_error(&simfile_path, error.as_str());
        }
    }
}

fn reap_song_parse<F, OnError>(
    rx: Option<&std::sync::mpsc::Receiver<SongParseMsg>>,
    in_flight: &mut usize,
    loaded_packs: &mut Vec<SongPack>,
    stats: &mut SongLoadStats,
    songs_done: &mut usize,
    total_songs: usize,
    progress: &mut Option<&mut F>,
    on_song_error: &mut OnError,
) where
    F: FnMut(usize, usize, &str, &str),
    OnError: FnMut(&Path, &str),
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
                    on_song_error(&simfile_path, error.as_str());
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

fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    key
}

#[inline]
fn ascii_case_insensitive_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[inline]
fn itgmania_sort_bytes(text: &str) -> impl Iterator<Item = u8> + '_ {
    let bytes = text.as_bytes();
    let bytes = bytes.strip_prefix(b".").unwrap_or(bytes);
    let first = bytes.first().copied().map(|byte| byte.to_ascii_uppercase());
    let prefix = first
        .is_some_and(|byte| !byte.is_ascii_uppercase() && !byte.is_ascii_digit())
        .then_some(b'~');
    prefix
        .into_iter()
        .chain(bytes.iter().copied().map(|byte| byte.to_ascii_uppercase()))
}

#[inline]
fn song_itgmania_cmp(left: &SongData, right: &SongData) -> std::cmp::Ordering {
    let left_main = if left.translit_title.is_empty() {
        left.title.as_str()
    } else {
        left.translit_title.as_str()
    };
    let right_main = if right.translit_title.is_empty() {
        right.title.as_str()
    } else {
        right.translit_title.as_str()
    };

    let ordering = if left_main == right_main {
        let left_sub = if left.translit_subtitle.is_empty() {
            left.subtitle.as_str()
        } else {
            left.translit_subtitle.as_str()
        };
        let right_sub = if right.translit_subtitle.is_empty() {
            right.subtitle.as_str()
        } else {
            right.translit_subtitle.as_str()
        };
        itgmania_sort_bytes(left_sub).cmp(itgmania_sort_bytes(right_sub))
    } else {
        itgmania_sort_bytes(left_main)
            .cmp(itgmania_sort_bytes(right_main))
            // `MakeSortString`-equivalent titles must remain in contiguous raw-title groups.
            // Otherwise exact-title pairs compare by subtitle while folded-title pairs compare
            // by path, which is non-transitive and can make Rust's slice sort panic.
            .then_with(|| left_main.cmp(right_main))
    };
    if ordering != std::cmp::Ordering::Equal {
        return ordering;
    }

    let left_path = left.simfile_path.to_string_lossy();
    let right_path = right.simfile_path.to_string_lossy();
    ascii_case_insensitive_cmp(left_path.as_ref(), right_path.as_ref())
}

fn sort_songs_itgmania(songs: &mut [Arc<SongData>]) {
    songs.sort_by(|left, right| song_itgmania_cmp(left, right));
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn sort_songs_itgmania_for_bench(songs: &mut [Arc<SongData>]) {
    sort_songs_itgmania(songs);
}

#[cfg(feature = "bench-support")]
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

#[cfg(feature = "bench-support")]
struct ItgmaniaSongTitleKey {
    main_raw: Vec<u8>,
    main_sort: Vec<u8>,
    sub_sort: Vec<u8>,
    path_fold: Vec<u8>,
}

#[cfg(feature = "bench-support")]
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

#[cfg(feature = "bench-support")]
impl PartialEq for ItgmaniaSongTitleKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

#[cfg(feature = "bench-support")]
impl Eq for ItgmaniaSongTitleKey {}

#[cfg(feature = "bench-support")]
impl PartialOrd for ItgmaniaSongTitleKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(feature = "bench-support")]
impl Ord for ItgmaniaSongTitleKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.main_raw == other.main_raw {
            match self.sub_sort.cmp(&other.sub_sort) {
                std::cmp::Ordering::Equal => self.path_fold.cmp(&other.path_fold),
                ordering => ordering,
            }
        } else {
            match self.main_sort.cmp(&other.main_sort) {
                std::cmp::Ordering::Equal => self.main_raw.cmp(&other.main_raw),
                ordering => ordering,
            }
        }
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn sort_songs_itgmania_legacy(songs: &mut [Arc<SongData>]) {
    songs.sort_by_cached_key(|song| ItgmaniaSongTitleKey::new(song.as_ref()));
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

impl From<RsspSongScan> for SongScan {
    fn from(song: RsspSongScan) -> Self {
        Self {
            dir: song.dir,
            simfile: song.simfile,
        }
    }
}

impl From<RsspPackScan> for PackScan {
    fn from(pack: RsspPackScan) -> Self {
        Self {
            dir: pack.dir,
            group_name: pack.group_name,
            display_title: pack.display_title,
            sort_title: pack.sort_title,
            translit_title: pack.translit_title,
            series: pack.series,
            year: pack.year,
            sync_pref: sync_pref_from_rssp(pack.sync_pref),
            banner_path: pack.banner_path,
            songs: pack.songs.into_iter().map(SongScan::from).collect(),
            version: pack.version,
            has_pack_ini: pack.has_pack_ini,
            background_path: pack.background_path,
        }
    }
}

const fn sync_pref_from_rssp(pref: rssp::pack::SyncPref) -> SyncPref {
    match pref {
        rssp::pack::SyncPref::Default => SyncPref::Default,
        rssp::pack::SyncPref::Null => SyncPref::Null,
        rssp::pack::SyncPref::Itg => SyncPref::Itg,
    }
}

fn pack_dir_key(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ci_key)
        .filter(|key| !key.is_empty())
}

fn sort_song_packs(packs: &mut [SongPack]) {
    if packs.len() < 2 {
        return;
    }
    let mut order = (0..packs.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        ascii_case_insensitive_cmp(&packs[left].sort_title, &packs[right].sort_title).then_with(
            || ascii_case_insensitive_cmp(&packs[left].group_name, &packs[right].group_name),
        )
    });
    let mut destinations = vec![0; packs.len()];
    for (new_index, old_index) in order.into_iter().enumerate() {
        destinations[old_index] = new_index;
    }
    for index in 0..packs.len() {
        while destinations[index] != index {
            let destination = destinations[index];
            packs.swap(index, destination);
            destinations.swap(index, destination);
        }
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn sort_song_packs_for_bench(packs: &mut [SongPack]) {
    sort_song_packs(packs);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn sort_song_packs_legacy(packs: &mut [SongPack]) {
    packs.sort_by_cached_key(|pack| {
        (
            pack.sort_title.to_ascii_lowercase(),
            pack.group_name.to_ascii_lowercase(),
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn pack_scan(
        group_name: &str,
        display_title: &str,
        has_pack_ini: bool,
        banner_path: Option<&str>,
        songs: &[&str],
        root: &Path,
    ) -> PackScan {
        let dir = root.join(group_name);
        PackScan {
            dir: dir.clone(),
            group_name: group_name.to_string(),
            display_title: display_title.to_string(),
            sort_title: display_title.to_string(),
            translit_title: display_title.to_string(),
            series: String::new(),
            year: 0,
            version: i32::from(has_pack_ini),
            has_pack_ini,
            sync_pref: SyncPref::Default,
            banner_path: banner_path.map(PathBuf::from),
            background_path: None,
            songs: songs
                .iter()
                .map(|song| {
                    let song_dir = dir.join(song);
                    SongScan {
                        dir: song_dir.clone(),
                        simfile: song_dir.join("song.sm"),
                    }
                })
                .collect(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-scan-{name}-{}",
            std::process::id()
        ));
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
            sync_pref: SyncPref::Default,
            directory: root.join(group_name),
            banner_path: None,
            songs: Vec::new(),
        }
    }

    fn song_data(simfile_path: PathBuf, title: &str) -> SongData {
        SongData {
            simfile_path,
            title: title.to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    #[test]
    fn borrowed_itgmania_song_sort_preserves_special_and_subtitle_order() {
        let root = PathBuf::from("Songs/Pack");
        let make_song = |index: usize, title: &str, subtitle: &str| {
            let mut song = song_data(root.join(format!("Song{index}/song.ssc")), title);
            song.subtitle = subtitle.to_string();
            Arc::new(song)
        };
        let mut songs = vec![
            make_song(0, "!Bang", ""),
            make_song(1, "Same", "Zeta"),
            make_song(2, "Beta", ""),
            make_song(3, ".Alpha", ""),
            make_song(4, "Same", "alpha"),
            make_song(5, "9 Lives", ""),
        ];

        sort_songs_itgmania(&mut songs);

        let titles = songs
            .iter()
            .map(|song| (song.title.as_str(), song.subtitle.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            titles,
            [
                ("9 Lives", ""),
                (".Alpha", ""),
                ("Beta", ""),
                ("Same", "alpha"),
                ("Same", "Zeta"),
                ("!Bang", ""),
            ]
        );
    }

    #[test]
    fn borrowed_itgmania_song_sort_groups_folded_title_ties_by_raw_title() {
        let mut lower = song_data(PathBuf::from("Songs/Pack/A/song.ssc"), "alpha");
        lower.subtitle = "First".to_string();
        let mut upper = song_data(PathBuf::from("Songs/Pack/Z/song.ssc"), "ALPHA");
        upper.subtitle = "Last".to_string();
        let mut songs = vec![Arc::new(lower), Arc::new(upper)];

        sort_songs_itgmania(&mut songs);

        assert_eq!(
            songs[0].simfile_path,
            PathBuf::from("Songs/Pack/Z/song.ssc")
        );
        assert_eq!(
            songs[1].simfile_path,
            PathBuf::from("Songs/Pack/A/song.ssc")
        );
    }

    #[test]
    fn borrowed_itgmania_song_comparison_is_total_for_folded_title_collisions() {
        let make_song = |path: &str, title: &str, subtitle: &str| {
            let mut song = song_data(PathBuf::from(path), title);
            song.subtitle = subtitle.to_string();
            Arc::new(song)
        };
        let mut songs = vec![
            make_song("Songs/Pack/A/song.ssc", "Same", "Zeta"),
            make_song("Songs/Pack/Z/song.ssc", "Same", "alpha"),
            make_song("Songs/Pack/M/song.ssc", "same", "Middle"),
        ];

        for left in &songs {
            for right in &songs {
                assert_eq!(
                    song_itgmania_cmp(left, right),
                    song_itgmania_cmp(right, left).reverse()
                );
            }
        }
        for left in &songs {
            for middle in &songs {
                for right in &songs {
                    let left_middle = song_itgmania_cmp(left, middle);
                    let middle_right = song_itgmania_cmp(middle, right);
                    if left_middle != std::cmp::Ordering::Greater
                        && middle_right != std::cmp::Ordering::Greater
                    {
                        assert_ne!(song_itgmania_cmp(left, right), std::cmp::Ordering::Greater);
                    }
                }
            }
        }

        sort_songs_itgmania(&mut songs);

        assert_eq!(
            songs
                .iter()
                .map(|song| song.simfile_path.as_path())
                .collect::<Vec<_>>(),
            [
                Path::new("Songs/Pack/Z/song.ssc"),
                Path::new("Songs/Pack/A/song.ssc"),
                Path::new("Songs/Pack/M/song.ssc"),
            ]
        );
    }

    #[test]
    fn borrowed_pack_sort_preserves_case_insensitive_tuple_order() {
        let root = Path::new("Songs");
        let mut packs = vec![
            song_pack("Zulu", "alpha", root),
            song_pack("Beta", "beta", root),
            song_pack("Able", "ALPHA", root),
            song_pack("Same", "tie", root),
            song_pack("same", "TIE", root),
        ];

        sort_song_packs(&mut packs);

        assert_eq!(
            packs
                .iter()
                .map(|pack| pack.group_name.as_str())
                .collect::<Vec<_>>(),
            ["Able", "Zulu", "Beta", "Same", "same"]
        );
    }

    #[test]
    fn collect_song_scan_roots_dedupes_and_reports_missing_roots() {
        let root = test_dir("song-scan-roots");
        let primary = root.join("primary");
        let extra = root.join("extra");
        let additional = root.join("additional");
        let missing = root.join("missing");
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&additional).unwrap();

        let report = collect_song_scan_roots(
            &primary,
            vec![primary.clone(), extra.clone()],
            vec![
                ("missing-label".to_string(), missing.clone()),
                ("additional-label".to_string(), additional.clone()),
                ("additional-dupe".to_string(), additional.clone()),
            ],
        );

        assert_eq!(report.roots, vec![primary, extra, additional]);
        assert_eq!(
            report.events,
            vec![SongScanRootEvent::AdditionalMissing {
                label: "missing-label".to_string(),
                root: missing
            }]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collect_song_scan_roots_reports_missing_primary_root() {
        let root = test_dir("song-scan-root-missing-primary");
        let primary = root.join("missing-primary");

        let report = collect_song_scan_roots(&primary, Vec::new(), Vec::new());

        assert!(report.roots.is_empty());
        assert_eq!(
            report.events,
            vec![SongScanRootEvent::PrimaryMissing { root: primary }]
        );

        let _ = fs::remove_dir_all(root);
    }

    fn process_test_song(
        simfile_path: PathBuf,
        fastload: bool,
        cachesongs: bool,
        global_offset_seconds: f32,
    ) -> Result<(SongData, bool), String> {
        assert!(fastload);
        assert!(cachesongs);
        assert!((global_offset_seconds - 0.25).abs() < f32::EPSILON);
        let title = simfile_path
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if title == "Bad" {
            return Err("bad song".to_string());
        }
        let is_hit = title == "Cached";
        Ok((song_data(simfile_path, title.as_str()), is_hit))
    }

    fn process_never_cache_song(
        simfile_path: PathBuf,
        fastload: bool,
        cachesongs: bool,
        _global_offset_seconds: f32,
    ) -> Result<(SongData, bool), String> {
        assert!(!fastload);
        assert!(!cachesongs);
        Ok((song_data(simfile_path, "Never"), false))
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
    fn scan_song_roots_returns_owned_pack_scans_and_failures() {
        let root = test_dir("scan-song-roots");
        let pack = root.join("Pack");
        let song = pack.join("Song");
        fs::create_dir_all(&song).unwrap();
        fs::write(song.join("song.sm"), b"#TITLE:Song;").unwrap();

        let missing = root.join("Missing");
        let (packs, failures) = scan_song_roots(&[root.clone(), missing.clone()]);

        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].group_name, "Pack");
        assert_eq!(packs[0].songs.len(), 1);
        assert_eq!(packs[0].songs[0].simfile, song.join("song.sm"));
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].path, missing);

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

    #[test]
    fn load_pack_scans_with_tracks_stats_errors_and_progress() {
        let root = test_dir("load-pack-scans");
        let packs = vec![pack_scan(
            "Pack",
            "Pack",
            true,
            None,
            &["Parsed", "Cached", "Bad"],
            &root,
        )];
        let options = SongLoadOptions {
            fastload: true,
            cachesongs: true,
            global_offset_seconds: 0.25,
            song_parsing_threads: 1,
        };
        let mut progress = Vec::new();
        let mut errors = Vec::new();
        let mut scanned_packs = Vec::new();
        let mut never_cached = Vec::new();

        let (loaded, stats) = load_pack_scans_with(
            packs,
            options,
            Some(&mut |done, total, group, item| {
                progress.push((done, total, group.to_string(), item.to_string()));
            }),
            process_test_song,
            |_| false,
            |path, error| errors.push((path.to_path_buf(), error.to_string())),
            |pack| scanned_packs.push(pack.name.clone()),
            |pack| never_cached.push(pack.to_string()),
        );

        assert_eq!(stats.songs_cache_hits, 1);
        assert_eq!(stats.songs_parsed, 1);
        assert_eq!(stats.songs_failed, 1);
        assert!(!stats.used_parallel);
        assert_eq!(stats.parse_threads, 1);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].songs.len(), 2);
        assert_eq!(loaded[0].songs[0].title, "Cached");
        assert_eq!(loaded[0].songs[1].title, "Parsed");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].0.ends_with(Path::new("Bad").join("song.sm")));
        assert_eq!(scanned_packs, vec!["Pack"]);
        assert!(never_cached.is_empty());
        assert_eq!(
            progress.first(),
            Some(&(0, 3, String::new(), String::new()))
        );
        assert_eq!(
            progress.last().map(|event| (event.0, event.1)),
            Some((3, 3))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_pack_scans_with_disables_cache_for_never_cache_packs() {
        let root = test_dir("load-never-cache");
        let packs = vec![pack_scan("Never", "Never", true, None, &["Song"], &root)];
        let options = SongLoadOptions {
            fastload: true,
            cachesongs: true,
            global_offset_seconds: 0.0,
            song_parsing_threads: 1,
        };
        let mut never_cached = Vec::new();

        let (loaded, stats) = load_pack_scans_with(
            packs,
            options,
            None::<&mut fn(usize, usize, &str, &str)>,
            process_never_cache_song,
            |group| group.eq_ignore_ascii_case("Never"),
            |_path, _error| {},
            |_pack| {},
            |pack| never_cached.push(pack.to_string()),
        );

        assert_eq!(stats.songs_cache_hits, 0);
        assert_eq!(stats.songs_parsed, 1);
        assert_eq!(stats.songs_failed, 0);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].songs.len(), 1);
        assert_eq!(never_cached, vec!["Never"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn progress_names_prefer_display_dirs_with_fallbacks() {
        let root = test_dir("progress-names");
        let mut pack_without_dir = song_pack("Fallback Group", "Title", &root);
        pack_without_dir.directory = PathBuf::new();
        assert_eq!(song_pack_progress_name(&pack_without_dir), "Fallback Group");

        let mut pack_with_dir = song_pack("Fallback Group", "Title", &root);
        pack_with_dir.directory = root.join("Pack Dir");
        assert_eq!(song_pack_progress_name(&pack_with_dir), "Pack Dir");

        assert_eq!(
            song_progress_name(&root.join("Pack").join("Song").join("song.sm")),
            "Song"
        );
        assert_eq!(song_progress_name(Path::new("song.sm")), "song.sm");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fmt_scan_time_scales_units() {
        assert_eq!(fmt_scan_time(Duration::from_millis(999)), "999ms");
        assert_eq!(fmt_scan_time(Duration::from_millis(1500)), "1.50s");
        assert_eq!(fmt_scan_time(Duration::from_millis(61_200)), "1m1.2s");
    }
}
