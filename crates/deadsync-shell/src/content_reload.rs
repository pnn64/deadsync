use deadsync_config as config;
use deadsync_simfile::scan::SongScanMode;
use deadsync_theme_simply_love::views::{
    SimplyLoveContentReloadEvent, SimplyLoveContentReloadPhase,
};
use log::info;
use smallvec::SmallVec;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

/// Worker-to-game progress policy. Workers own production; the game thread owns
/// reception. The bounded channel lives for one reload, is warmed at job start,
/// and never grows. Progress updates are sampled and may be skipped when full;
/// phase and terminal events block until admitted and are never dropped. There
/// is no eviction or gameplay miss path. At most eight events are integrated in
/// one frame.
pub(crate) const PROGRESS_QUEUE_CAPACITY: usize = 32;
pub(crate) const PROGRESS_EVENTS_PER_FRAME: usize = 8;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(crate) struct ProgressGate {
    last_emit: Option<Instant>,
}

pub(crate) struct ReadyBatch<T> {
    pub events: SmallVec<[T; PROGRESS_EVENTS_PER_FRAME]>,
    pub disconnected: bool,
}

pub(crate) fn receive_ready<T>(rx: &Receiver<T>) -> ReadyBatch<T> {
    let mut events = SmallVec::new();
    let mut disconnected = false;
    for _ in 0..PROGRESS_EVENTS_PER_FRAME {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                disconnected = true;
                break;
            }
        }
    }
    ReadyBatch {
        events,
        disconnected,
    }
}

pub(crate) fn send_progress<T>(tx: &SyncSender<T>, done: usize, total: usize, event: T) {
    if done == total {
        let _ = tx.send(event);
    } else {
        let _ = tx.try_send(event);
    }
}

impl ProgressGate {
    pub(crate) fn should_emit(&mut self, done: usize, total: usize) -> bool {
        self.should_emit_at(done, total, Instant::now())
    }

    fn should_emit_at(&mut self, done: usize, total: usize, now: Instant) -> bool {
        let due = self.last_emit.is_none_or(|last| {
            now.checked_duration_since(last)
                .is_some_and(|elapsed| elapsed >= PROGRESS_MIN_INTERVAL)
        });
        if done == total || due {
            self.last_emit = Some(now);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
pub(crate) struct Service {
    rx: Option<Receiver<SimplyLoveContentReloadEvent>>,
}

impl Service {
    /// Whether a job is running. A new one is refused until it has finished
    /// and been read to the end, so a caller with work to hand over waits.
    pub(crate) const fn is_busy(&self) -> bool {
        self.rx.is_some()
    }
}

impl Service {
    pub(crate) fn start_initialization(
        &mut self,
        songs_root: PathBuf,
        courses_root: PathBuf,
        audio_available: bool,
    ) {
        self.start(move |tx| {
            scan_library(&tx, &songs_root, &courses_root, SongScanMode::Startup);
            prewarm_artwork(&tx);
            compile_noteskins(&tx);
            analyze_replaygain(&tx, None, audio_available);
            send_finished(&tx);
        });
    }

    pub(crate) fn start_library(
        &mut self,
        songs_root: PathBuf,
        courses_root: PathBuf,
        audio_available: bool,
    ) {
        self.start(move |tx| {
            scan_library(&tx, &songs_root, &courses_root, SongScanMode::Reload);
            analyze_replaygain(&tx, None, audio_available);
            send_finished(&tx);
        });
    }

    pub(crate) fn start_song_dirs(
        &mut self,
        songs_root: PathBuf,
        pack_dirs: Vec<PathBuf>,
        audio_available: bool,
    ) {
        self.start(move |tx| {
            let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
                SimplyLoveContentReloadPhase::Songs,
            ));
            let mut gate = ProgressGate::default();
            let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
                if !gate.should_emit(done, total) {
                    return;
                }
                send_progress(
                    &tx,
                    done,
                    total,
                    SimplyLoveContentReloadEvent::Song {
                        done,
                        total,
                        pack: pack.to_owned(),
                        song: song.to_owned(),
                    },
                );
            };
            deadsync_simfile::app_runtime::reload_song_dirs_with_progress_counts(
                &songs_root,
                &pack_dirs,
                &mut on_song,
            );
            analyze_replaygain(&tx, Some(&pack_dirs), audio_available);
            send_finished(&tx);
        });
    }

    fn start(
        &mut self,
        job: impl FnOnce(SyncSender<SimplyLoveContentReloadEvent>) + Send + 'static,
    ) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(PROGRESS_QUEUE_CAPACITY);
        self.rx = Some(rx);
        std::thread::spawn(move || job(tx));
    }

    pub(crate) fn poll(
        &mut self,
    ) -> SmallVec<[SimplyLoveContentReloadEvent; PROGRESS_EVENTS_PER_FRAME]> {
        let Some(rx) = self.rx.as_ref() else {
            return SmallVec::new();
        };
        let batch = receive_ready(rx);
        let mut events = batch.events;
        let mut finished = events
            .iter()
            .any(|event| matches!(event, SimplyLoveContentReloadEvent::Finished { .. }));
        if batch.disconnected {
            if !finished {
                events.push(finished_event());
            }
            finished = true;
        }
        if finished {
            self.rx = None;
        }
        events
    }
}

fn scan_library(
    tx: &SyncSender<SimplyLoveContentReloadEvent>,
    songs_root: &Path,
    courses_root: &Path,
    mode: SongScanMode,
) {
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Songs,
    ));
    let mut song_gate = ProgressGate::default();
    let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
        if !song_gate.should_emit(done, total) {
            return;
        }
        send_progress(
            tx,
            done,
            total,
            SimplyLoveContentReloadEvent::Song {
                done,
                total,
                pack: pack.to_owned(),
                song: song.to_owned(),
            },
        );
    };
    deadsync_simfile::app_runtime::scan_and_load_songs_with_progress_counts(
        songs_root,
        mode,
        &mut on_song,
    );

    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Courses,
    ));
    let mut course_gate = ProgressGate::default();
    let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
        if !course_gate.should_emit(done, total) {
            return;
        }
        send_progress(
            tx,
            done,
            total,
            SimplyLoveContentReloadEvent::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            },
        );
    };
    deadsync_simfile::app_runtime::scan_and_load_courses_with_progress_counts(
        courses_root,
        songs_root,
        &mut on_course,
    );
}

fn prewarm_artwork(tx: &SyncSender<SimplyLoveContentReloadEvent>) {
    let (banner_paths, cdtitle_paths) = artwork_cache_paths();
    let plan = deadsync_assets::media_cache::artwork_cache_plan(&banner_paths, &cdtitle_paths);
    let total = plan.job_count();
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Artwork,
    ));
    info!(
        "Init loading: caching artwork in one pass (banner={}, cdtitle={}, total jobs={})...",
        banner_paths.len(),
        cdtitle_paths.len(),
        total
    );
    let mut gate = ProgressGate::default();
    let mut on_artwork = |done: usize, _total: usize, path: Option<&Path>| {
        if !gate.should_emit(done, total) {
            return;
        }
        let (line2, line3) = cache_progress_lines(path);
        send_progress(
            tx,
            done,
            total,
            SimplyLoveContentReloadEvent::Artwork {
                done,
                total,
                line2,
                line3,
            },
        );
    };
    deadsync_assets::media_cache::prewarm_artwork_cache_with_progress(plan, &mut on_artwork);
    info!("Init loading: artwork cache prewarm complete.");
}

fn compile_noteskins(tx: &SyncSender<SimplyLoveContentReloadEvent>) {
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Noteskins,
    ));
    info!("Init loading: compiling noteskin cache before UI...");
    let mut gate = ProgressGate::default();
    let mut on_noteskin = |done: usize, total: usize, skin: &str, status: &str| {
        if !gate.should_emit(done, total) {
            return;
        }
        send_progress(
            tx,
            done,
            total,
            SimplyLoveContentReloadEvent::Noteskins {
                done,
                total,
                skin: skin.to_owned(),
                status: status.to_owned(),
            },
        );
    };
    let summary = deadsync_assets::noteskin::compile_all_itg_caches_with_progress(&mut on_noteskin);
    info!(
        "Init loading: noteskin cache compile complete (total={}, built={}, reused={}, failed={}).",
        summary.total, summary.built, summary.reused, summary.failed
    );
}

/// Frontload `ReplayGain` (EBU R128 loudness) analysis before the menu appears,
/// so the first play of any song doesn't audibly adjust loudness a few seconds
/// in. Runs synchronously with progress, populating the same cache the per-song
/// preview path uses. Unchanged songs resolve from the cache, so only new or
/// modified songs are actually recomputed.
///
/// When `restrict_to` is `Some`, only songs under those pack directories are
/// considered (used by targeted song-dir reloads); `None` covers the whole
/// library (boot and full reload).
fn analyze_replaygain(
    tx: &SyncSender<SimplyLoveContentReloadEvent>,
    restrict_to: Option<&[PathBuf]>,
    audio_available: bool,
) {
    if !config::runtime::get().enable_replaygain || !audio_available {
        return;
    }
    let paths = replaygain_music_paths(restrict_to);
    if paths.is_empty() {
        return;
    }
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::ReplayGain,
    ));
    let total = paths.len();
    // Publish the new phase's bounds before any decoder finishes.  Without an
    // explicit zero-progress event, reload overlays can keep displaying the
    // completed song-scan count while the first loudness analysis is running.
    let _ = tx.send(SimplyLoveContentReloadEvent::ReplayGain {
        done: 0,
        total,
        line2: String::new(),
        line3: String::new(),
    });
    info!(
        "Init loading: analyzing ReplayGain loudness for {} song(s)...",
        total
    );
    let mut gate = ProgressGate::default();
    let mut on_song = |done: usize, total: usize, path: &Path| {
        if !gate.should_emit(done, total) {
            return;
        }
        let (line2, line3) = cache_progress_lines(Some(path));
        send_progress(
            tx,
            done,
            total,
            SimplyLoveContentReloadEvent::ReplayGain {
                done,
                total,
                line2,
                line3,
            },
        );
    };
    deadsync_audio_replaygain::analyze_paths_blocking(paths, &mut on_song);
    info!("Init loading: ReplayGain analysis complete.");
}

/// Collects the deduplicated set of song music paths from the loaded song cache.
/// When `restrict_to` is `Some`, only songs whose music file lives under one of
/// those pack directories are included.
pub(crate) fn replaygain_music_paths(restrict_to: Option<&[PathBuf]>) -> Vec<PathBuf> {
    let cache = deadsync_simfile::runtime_cache::get_song_cache();
    replaygain_music_paths_from_packs(&cache, restrict_to)
}

fn replaygain_music_paths_from_packs(
    packs: &[deadsync_chart::SongPack],
    restrict_to: Option<&[PathBuf]>,
) -> Vec<PathBuf> {
    let song_count = packs.iter().map(|pack| pack.songs.len()).sum();
    let mut paths = Vec::<&Path>::with_capacity(song_count);
    match restrict_to {
        Some(dirs) => {
            for pack in packs {
                for song in &pack.songs {
                    if let Some(path) = song.music_path.as_ref()
                        && dirs.iter().any(|dir| path.starts_with(dir))
                    {
                        paths.push(path);
                    }
                }
            }
        }
        None => {
            for pack in packs {
                for song in &pack.songs {
                    if let Some(path) = song.music_path.as_ref() {
                        paths.push(path);
                    }
                }
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    paths.into_iter().map(Path::to_path_buf).collect()
}

fn artwork_cache_paths() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let course_capacity = deadsync_simfile::runtime_cache::get_course_cache().len();
    let (mut banner, cdtitle) = {
        let cache = deadsync_simfile::runtime_cache::get_song_cache();
        artwork_cache_paths_from_packs(&cache, course_capacity)
    };
    {
        let cache = deadsync_simfile::runtime_cache::get_course_cache();
        for (course_path, course) in cache.iter() {
            if let Some(path) =
                deadsync_simfile::course::resolve_course_banner_path(course_path, &course.banner)
            {
                banner.push(path);
            }
        }
    }
    (banner, cdtitle)
}

fn artwork_cache_paths_from_packs(
    packs: &[deadsync_chart::SongPack],
    extra_banner_capacity: usize,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let banner_count = extra_banner_capacity
        + packs
            .iter()
            .map(|pack| {
                usize::from(pack.banner_path.is_some())
                    + pack
                        .songs
                        .iter()
                        .filter(|song| song.banner_path.is_some())
                        .count()
            })
            .sum::<usize>();
    let cdtitle_count = packs
        .iter()
        .flat_map(|pack| &pack.songs)
        .filter(|song| song.cdtitle_path.is_some())
        .count();
    let mut banner = Vec::with_capacity(banner_count);
    let mut cdtitle = Vec::with_capacity(cdtitle_count);
    for pack in packs {
        if let Some(path) = pack.banner_path.as_ref() {
            banner.push(path.clone());
        }
        for song in &pack.songs {
            if let Some(path) = song.banner_path.as_ref() {
                banner.push(path.clone());
            }
            if let Some(path) = song.cdtitle_path.as_ref() {
                cdtitle.push(path.clone());
            }
        }
    }
    (banner, cdtitle)
}

pub(crate) fn cache_progress_lines(path: Option<&Path>) -> (String, String) {
    let Some(path) = path else {
        return (String::new(), String::new());
    };
    if let Some(names) = song_cache_progress_lines(path) {
        return names;
    }
    cache_progress_fallback(path)
}

fn cache_progress_fallback(path: &Path) -> (String, String) {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    match progress_path_parts(path) {
        Some(ProgressPathParts::Song { pack, song }) => {
            let song = song
                .filter(|name| !name.eq_ignore_ascii_case(file_name))
                .map(str::to_owned)
                .unwrap_or_else(|| file_stem_owned(path, file_name));
            return (pack.to_owned(), song);
        }
        Some(ProgressPathParts::Course { group }) => {
            return (group.to_owned(), file_stem_owned(path, file_name));
        }
        None => {}
    }
    let parent = path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_owned();
    (parent, file_stem_owned(path, file_name))
}

enum ProgressPathParts<'a> {
    Song {
        pack: &'a str,
        song: Option<&'a str>,
    },
    Course {
        group: &'a str,
    },
}

fn progress_path_parts(path: &Path) -> Option<ProgressPathParts<'_>> {
    let mut components = path.components();
    let mut course_group = None;
    let mut saw_courses = false;
    while let Some(component) = next_normal_component(&mut components) {
        if component.eq_ignore_ascii_case("songs") {
            if let Some(pack) = next_normal_component(&mut components) {
                return Some(ProgressPathParts::Song {
                    pack,
                    song: next_normal_component(&mut components),
                });
            }
            return course_group.map(|group| ProgressPathParts::Course { group });
        }
        if !saw_courses && component.eq_ignore_ascii_case("courses") {
            saw_courses = true;
            let mut tail = components.clone();
            course_group = next_normal_component(&mut tail);
        }
    }
    course_group.map(|group| ProgressPathParts::Course { group })
}

fn next_normal_component<'a>(components: &mut std::path::Components<'a>) -> Option<&'a str> {
    components.find_map(|component| match component {
        std::path::Component::Normal(name) => name.to_str(),
        _ => None,
    })
}

fn file_stem_owned(path: &Path, file_name: &str) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name)
        .to_owned()
}

fn song_cache_progress_lines(path: &Path) -> Option<(String, String)> {
    let cache = deadsync_simfile::runtime_cache::get_song_cache();
    for pack in cache.iter() {
        if pack.banner_path.as_deref() == Some(path) {
            let item = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned();
            return Some((pack.group_name.clone(), item));
        }
        for song in &pack.songs {
            let matches = [
                song.music_path.as_deref(),
                song.banner_path.as_deref(),
                song.background_path.as_deref(),
                song.cdtitle_path.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|candidate| candidate == path);
            if matches {
                let song_name = song
                    .simfile_path
                    .parent()
                    .and_then(|dir| dir.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_owned();
                return Some((pack.group_name.clone(), song_name));
            }
        }
    }
    None
}

fn send_finished(tx: &mpsc::SyncSender<SimplyLoveContentReloadEvent>) {
    let _ = tx.send(finished_event());
}

fn finished_event() -> SimplyLoveContentReloadEvent {
    SimplyLoveContentReloadEvent::Finished {
        song_packs: deadsync_simfile::runtime_cache::get_song_cache().clone(),
    }
}

pub(crate) fn reload_song(path: &Path) -> Result<Vec<deadsync_chart::SongPack>, String> {
    deadsync_simfile::app_runtime::reload_song_in_cache(path)?;
    Ok(deadsync_simfile::runtime_cache::get_song_cache().clone())
}

pub(crate) fn delete_song(
    simfile_path: &Path,
    song_scan_roots: &[PathBuf],
) -> Result<Vec<deadsync_chart::SongPack>, String> {
    if !deadsync_config::runtime::song_path_is_writable(simfile_path) {
        return Err(format!(
            "song is in a read-only additional song folder: {}",
            simfile_path.display()
        ));
    }
    if !deadsync_simfile::runtime_cache::song_is_cached(simfile_path) {
        return Err(format!(
            "song is no longer in the live catalog: {}",
            simfile_path.display()
        ));
    }

    let song_dir = validated_song_dir(simfile_path, song_scan_roots)?;
    std::fs::remove_dir_all(&song_dir).map_err(|error| {
        format!(
            "could not delete song directory '{}': {error}",
            song_dir.display()
        )
    })?;
    if !deadsync_simfile::runtime_cache::remove_song(simfile_path) {
        return Err(format!(
            "deleted '{}' but could not remove it from the live catalog",
            song_dir.display()
        ));
    }
    Ok(deadsync_simfile::runtime_cache::get_song_cache().clone())
}

/// What actually happened when a pack was deleted.
///
/// Deleting a pack is N directory removals, and any of them can fail on its
/// own -- a locked handle on the song being previewed is the ordinary case. So
/// this reports rather than pretending it is all-or-nothing.
pub(crate) struct PackDeletion {
    pub(crate) removed: usize,
    /// Songs that were left where they were, and why.
    pub(crate) kept: Vec<String>,
}

/// Permanently delete one installed pack.
///
/// Addressed by group name rather than by path: the caller is a theme screen,
/// and a request carrying a path would let a caller-supplied string reach
/// `remove_dir_all`. The name is resolved against the live catalog here, and
/// every directory removed is one the catalog already knew about.
///
/// Songs go first, each through the same guards a single-song delete uses --
/// so a read-only additional folder is refused per song, exactly as it is
/// there. The pack's own directory is only removed once its songs are gone,
/// and only if it passes a containment check of its own.
///
/// `bundled_roots` is the program's own song folder, which is scanned like
/// any other but is never the player's to delete from.
pub(crate) fn delete_pack(
    group_name: &str,
    song_scan_roots: &[PathBuf],
    bundled_roots: &[PathBuf],
) -> Result<PackDeletion, String> {
    let wanted = group_name.to_lowercase();
    let simfiles: Vec<PathBuf> = {
        let cache = deadsync_simfile::runtime_cache::get_song_cache();
        let pack = cache
            .iter()
            .find(|pack| pack.group_name.to_lowercase() == wanted)
            .ok_or_else(|| format!("no pack named '{group_name}' in the live catalog"))?;
        pack.songs
            .iter()
            .map(|song| song.simfile_path.clone())
            .collect()
    };

    let mut removed = 0usize;
    let mut kept: Vec<String> = Vec::new();
    let mut pack_dirs: Vec<PathBuf> = Vec::new();

    for simfile in &simfiles {
        if !deadsync_config::runtime::song_path_is_writable(simfile) {
            kept.push(format!("{} (read-only song folder)", simfile.display()));
            continue;
        }
        let song_dir = match validated_song_dir(simfile, song_scan_roots) {
            Ok(dir) => dir,
            Err(error) => {
                kept.push(error);
                continue;
            }
        };
        if let Err(error) = std::fs::remove_dir_all(&song_dir) {
            kept.push(format!("{} ({error})", song_dir.display()));
            continue;
        }
        // Only tell the catalog about what actually left the disk.
        deadsync_simfile::runtime_cache::remove_song(simfile);
        removed += 1;
        if let Some(parent) = song_dir.parent()
            && !pack_dirs.contains(&parent.to_path_buf())
        {
            pack_dirs.push(parent.to_path_buf());
        }
    }

    // The pack folder itself, and whatever else was loose in it -- a banner, a
    // Pack.ini. Only once every song under it is gone: a pack that kept a song
    // back is a pack that still exists.
    if kept.is_empty() {
        for dir in &pack_dirs {
            match validated_pack_dir(dir, song_scan_roots, bundled_roots) {
                Ok(dir) => {
                    if let Err(error) = std::fs::remove_dir_all(&dir) {
                        kept.push(format!("{} ({error})", dir.display()));
                    }
                }
                Err(error) => kept.push(error),
            }
        }
    }

    // No cache handed back: `remove_song` bumps the cache generation itself,
    // and the browser rebuilds its library list from that. Returning a
    // snapshot as well would give it two sources for one fact.
    Ok(PackDeletion { removed, kept })
}

/// Record how a pack was synced, by writing its `Pack.ini`.
///
/// The engine reads `SyncOffset` from there and shifts the pack's timing by
/// -9 ms for `ITG`. Two details are load-bearing and easy to get wrong:
/// `rssp` throws the whole file away unless `Version=` is non-empty, and it
/// matches only the exact strings `NULL` and `ITG`.
///
/// An existing file is edited rather than replaced -- it may carry a display
/// title or a series that somebody meant to keep.
pub(crate) fn set_pack_sync(
    group_name: &str,
    itg: bool,
    song_scan_roots: &[PathBuf],
    bundled_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    let simfile = {
        let cache = deadsync_simfile::runtime_cache::get_song_cache();
        let pack = cache
            .iter()
            .find(|pack| pack.group_name.to_lowercase() == group_name.to_lowercase())
            .ok_or_else(|| format!("no pack named '{group_name}' in the live catalog"))?;
        pack.songs
            .first()
            .map(|song| song.simfile_path.clone())
            .ok_or_else(|| format!("pack '{group_name}' has no songs to locate it by"))?
    };
    if !deadsync_config::runtime::song_path_is_writable(&simfile) {
        return Err(format!("'{group_name}' is in a read-only song folder"));
    }

    // Located the same way a delete locates it: from a song the catalog
    // already holds, resolved, and proved to sit directly inside a song root.
    let song_dir = validated_song_dir(&simfile, song_scan_roots)?;
    let pack_dir = song_dir
        .parent()
        .ok_or_else(|| format!("song has no pack folder: {}", song_dir.display()))?;
    let pack_dir = validated_pack_dir(pack_dir, song_scan_roots, bundled_roots)?;

    let path = pack_dir.join("Pack.ini");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = pack_ini_with_sync(existing.as_str(), group_name, itg);
    std::fs::write(&path, updated)
        .map_err(|error| format!("could not write '{}': {error}", path.display()))?;
    Ok(pack_dir)
}

/// The `Pack.ini` text for a pack, with its `SyncOffset` set.
///
/// Written as a small line editor rather than a template so an existing file
/// keeps everything else it had.
fn pack_ini_with_sync(existing: &str, pack_name: &str, itg: bool) -> String {
    let wanted = if itg { "ITG" } else { "NULL" };
    if existing.trim().is_empty() {
        // A minimal file the parser will accept. `Version` is not decoration:
        // without it the whole file is discarded and the sync value with it.
        return format!("[Group]\nVersion=1\nDisplayTitle={pack_name}\nSyncOffset={wanted}\n");
    }

    let mut out = String::with_capacity(existing.len() + 32);
    let mut wrote_sync = false;
    let mut has_version = false;
    for line in existing.lines() {
        let key = line.split('=').next().unwrap_or("").trim();
        if key.eq_ignore_ascii_case("SyncOffset") {
            out.push_str(format!("SyncOffset={wanted}").as_str());
            out.push('\n');
            wrote_sync = true;
            continue;
        }
        if key.eq_ignore_ascii_case("Version") {
            has_version = !line.split('=').nth(1).unwrap_or("").trim().is_empty();
        }
        out.push_str(line);
        out.push('\n');
    }
    if !has_version {
        out.push_str("Version=1\n");
    }
    if !wrote_sync {
        out.push_str(format!("SyncOffset={wanted}\n").as_str());
    }
    out
}

#[cfg(test)]
mod pack_sync_tests {
    use super::pack_ini_with_sync;

    /// `rssp` throws the whole file away unless `Version=` is non-empty, so a
    /// file written without one records the sync value and loses it.
    #[test]
    fn a_new_file_carries_the_version_the_parser_demands() {
        let text = pack_ini_with_sync("", "Some Pack", true);
        assert!(text.starts_with("[Group]\n"));
        assert!(text.contains("\nVersion=1\n"), "or the file is discarded");
        assert!(text.contains("\nSyncOffset=ITG\n"));
        assert!(text.contains("DisplayTitle=Some Pack"));
    }

    /// The parser matches only the exact strings `NULL` and `ITG`; anything
    /// else falls through to Default and the pack goes back to the machine
    /// setting without saying so.
    #[test]
    fn the_value_is_spelled_the_only_way_the_parser_accepts() {
        assert!(pack_ini_with_sync("", "P", false).contains("SyncOffset=NULL"));
        assert!(pack_ini_with_sync("", "P", true).contains("SyncOffset=ITG"));
    }

    /// An existing file is edited, not replaced: it may carry a display title
    /// or a series somebody meant to keep.
    #[test]
    fn an_existing_file_keeps_everything_but_its_sync_line() {
        let existing =
            "[Group]\nVersion=1\nDisplayTitle=Kept Title\nSeries=Kept Series\nSyncOffset=NULL\n";
        let text = pack_ini_with_sync(existing, "Ignored", true);
        assert!(text.contains("DisplayTitle=Kept Title"));
        assert!(text.contains("Series=Kept Series"));
        assert!(text.contains("SyncOffset=ITG"));
        assert!(!text.contains("SyncOffset=NULL"), "the old value is gone");
        assert_eq!(text.matches("SyncOffset=").count(), 1, "and not doubled");
    }

    /// A file that never declared a sync gains one; a file whose `Version` is
    /// empty gains that too, because without it nothing else in it counts.
    #[test]
    fn a_file_missing_either_key_gains_it() {
        let text = pack_ini_with_sync("[Group]\nVersion=1\nDisplayTitle=X\n", "X", false);
        assert!(text.contains("SyncOffset=NULL"));

        let text = pack_ini_with_sync("[Group]\nVersion=\nDisplayTitle=X\n", "X", true);
        assert!(
            text.contains("\nVersion=1\n"),
            "an empty Version is no Version"
        );
        assert!(text.contains("SyncOffset=ITG"));
    }

    /// The key is matched without regard to case, as the parser matches it.
    #[test]
    fn an_oddly_cased_key_is_replaced_rather_than_duplicated() {
        let text = pack_ini_with_sync("[Group]\nVersion=1\nsyncoffset=NULL\n", "X", true);
        assert_eq!(text.to_lowercase().matches("syncoffset=").count(), 1);
        assert!(text.contains("SyncOffset=ITG"));
    }
}

/// Resolve a pack directory and prove it is one, before anything is removed.
///
/// `strip_prefix` is purely lexical, which is the trap here: `{songs}/Pack/..`
/// strips to `Pack/..` -- two components, and it would sail through a naive
/// depth test while pointing at the songs root itself. So the path is resolved
/// first and a failure to resolve is fatal, never a fall back to the raw path.
fn validated_pack_dir(
    candidate: &Path,
    song_scan_roots: &[PathBuf],
    bundled_roots: &[PathBuf],
) -> Result<PathBuf, String> {
    // A link is not the thing it points at. Refused before resolving, because
    // resolving is exactly what would hide it.
    if std::fs::symlink_metadata(candidate)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(format!(
            "pack directory is a link, not a folder: {}",
            candidate.display()
        ));
    }
    let pack_dir = std::fs::canonicalize(candidate).map_err(|error| {
        format!(
            "could not resolve pack directory '{}': {error}",
            candidate.display()
        )
    })?;

    // App-bundled content. It sits inside a scan root and is writable, but it
    // belongs to the install rather than to the player's library, and "delete
    // my pack" never means "modify the installed program".
    for bundled in bundled_roots {
        if let Ok(bundled) = std::fs::canonicalize(bundled)
            && pack_dir.starts_with(&bundled)
        {
            return Err(format!(
                "pack is part of the installed program: {}",
                pack_dir.display()
            ));
        }
    }

    let mut roots: Vec<PathBuf> = Vec::with_capacity(song_scan_roots.len());
    for root in song_scan_roots {
        if let Ok(root) = std::fs::canonicalize(root) {
            roots.push(root);
        }
    }
    // A scan root is never a pack, however it is reached. Song folders may be
    // nested inside one another as configured roots, so this is checked
    // against every root rather than only the one it is measured from.
    if roots.contains(&pack_dir) {
        return Err(format!(
            "refusing to delete a song folder root: {}",
            pack_dir.display()
        ));
    }

    for root in &roots {
        let Ok(relative) = pack_dir.strip_prefix(root) else {
            continue;
        };
        // Exactly root/pack, and every step of it an ordinary name.
        let mut parts = relative.components();
        let Some(std::path::Component::Normal(_)) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }
        return Ok(pack_dir);
    }

    Err(format!(
        "pack directory is not directly inside a song folder: {}",
        pack_dir.display()
    ))
}

fn validated_song_dir(simfile_path: &Path, song_scan_roots: &[PathBuf]) -> Result<PathBuf, String> {
    let simfile = std::fs::canonicalize(simfile_path).map_err(|error| {
        format!(
            "could not resolve selected simfile '{}': {error}",
            simfile_path.display()
        )
    })?;
    if !simfile.is_file() {
        return Err(format!(
            "selected simfile is not a file: {}",
            simfile.display()
        ));
    }
    let song_dir = simfile
        .parent()
        .ok_or_else(|| format!("selected simfile has no parent: {}", simfile.display()))?;

    for root in song_scan_roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        let Ok(relative) = song_dir.strip_prefix(&root) else {
            continue;
        };
        // A valid song directory is at least root/pack/song. Refuse to remove
        // a scan root or whole pack even if malformed catalog data points there.
        if relative.components().count() >= 2 {
            return Ok(song_dir.to_path_buf());
        }
    }

    Err(format!(
        "song directory is not a safe child of a configured song root: {}",
        song_dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("deadsync-song-delete-{name}-{nonce}"))
    }

    #[test]
    fn completed_event_releases_worker_slot() {
        let (tx, rx) = mpsc::channel();
        tx.send(SimplyLoveContentReloadEvent::Finished {
            song_packs: Vec::new(),
        })
        .expect("test event should send");
        let mut service = Service { rx: Some(rx) };

        let events = service.poll();

        assert!(matches!(
            events.as_slice(),
            [SimplyLoveContentReloadEvent::Finished { .. }]
        ));
        assert!(service.rx.is_none());
    }

    #[test]
    fn progress_poll_is_bounded_and_preserves_terminal_order() {
        let (tx, rx) = mpsc::channel();
        for done in 1..=10 {
            tx.send(SimplyLoveContentReloadEvent::Song {
                done,
                total: 10,
                pack: "Pack".to_owned(),
                song: format!("Song {done}"),
            })
            .unwrap();
        }
        let mut service = Service { rx: Some(rx) };
        assert_eq!(service.poll().len(), PROGRESS_EVENTS_PER_FRAME);
        let second = service.poll();
        assert_eq!(second.len(), 10 - PROGRESS_EVENTS_PER_FRAME);
        assert!(matches!(
            second.last(),
            Some(SimplyLoveContentReloadEvent::Song { done: 10, .. })
        ));

        tx.send(SimplyLoveContentReloadEvent::Finished {
            song_packs: Vec::new(),
        })
        .unwrap();
        assert!(matches!(
            service.poll().as_slice(),
            [SimplyLoveContentReloadEvent::Finished { .. }]
        ));
        assert!(service.rx.is_none());
    }

    #[test]
    fn progress_gate_keeps_first_periodic_and_terminal_updates() {
        let start = Instant::now();
        let mut gate = ProgressGate::default();
        assert!(gate.should_emit_at(1, 100, start));
        assert!(!gate.should_emit_at(2, 100, start + Duration::from_millis(15)));
        assert!(gate.should_emit_at(3, 100, start + Duration::from_millis(16)));
        assert!(gate.should_emit_at(100, 100, start + Duration::from_millis(16)));
    }

    #[test]
    fn terminal_progress_waits_for_queue_capacity() {
        let (tx, rx) = mpsc::sync_channel(1);
        send_progress(&tx, 1, 3, 1);
        send_progress(&tx, 2, 3, 2);
        let terminal = std::thread::spawn(move || send_progress(&tx, 3, 3, 3));

        assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(1));
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(3));
        terminal
            .join()
            .expect("terminal progress sender should exit");
    }

    #[test]
    fn song_delete_path_requires_root_pack_song_depth() {
        let root = test_dir("depth");
        let song_dir = root.join("Pack").join("Song");
        let simfile = song_dir.join("song.ssc");
        std::fs::create_dir_all(&song_dir).unwrap();
        std::fs::write(&simfile, "#TITLE:Song;").unwrap();

        assert_eq!(
            validated_song_dir(&simfile, std::slice::from_ref(&root)).unwrap(),
            std::fs::canonicalize(&song_dir).unwrap()
        );

        let pack_simfile = root.join("Pack").join("pack.ssc");
        std::fs::write(&pack_simfile, "#TITLE:Pack;").unwrap();
        assert!(validated_song_dir(&pack_simfile, std::slice::from_ref(&root)).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn song_delete_path_rejects_files_outside_scan_roots() {
        let root = test_dir("root");
        let outside = test_dir("outside");
        let song_dir = outside.join("Pack").join("Song");
        let simfile = song_dir.join("song.ssc");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&song_dir).unwrap();
        std::fs::write(&simfile, "#TITLE:Song;").unwrap();

        assert!(validated_song_dir(&simfile, std::slice::from_ref(&root)).is_err());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn artwork_progress_preserves_song_and_course_labels() {
        assert_eq!(
            cache_progress_lines(Some(Path::new("Songs/Pack/Song/banner.png"))),
            ("Pack".to_owned(), "Song".to_owned())
        );
        assert_eq!(
            cache_progress_lines(Some(Path::new("Courses/Group/course-banner.png"))),
            ("Group".to_owned(), "course-banner".to_owned())
        );
        assert_eq!(
            cache_progress_lines(Some(Path::new("Cache/banner.png"))),
            ("Cache".to_owned(), "banner".to_owned())
        );
    }
}
