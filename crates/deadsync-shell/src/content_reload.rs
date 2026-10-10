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
            match validated_pack_dir(dir, song_scan_roots, bundled_roots, PackDepth::Flat) {
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
    let pack = writable_pack(group_name, song_scan_roots, bundled_roots)?;
    write_pack_sync(&pack.dir, group_name, itg)?;
    Ok(pack.dir)
}

/// A pack that may be changed from inside the game.
pub(crate) struct WritablePack {
    pub(crate) dir: PathBuf,
    /// The simfile each of its songs loads from.
    pub(crate) simfiles: Vec<PathBuf>,
    /// What its `Pack.ini` declares, as the catalog read it.
    pub(crate) sync_pref: deadsync_chart::SyncPref,
}

/// Find a pack in the live catalog and prove it may be changed: one folder,
/// sitting inside a song root -- directly, or in a series folder there --
/// outside the program's own songs, and in a writable song folder. Located the
/// same way a delete locates it: from a song the catalog already holds,
/// resolved, never from a name.
pub(crate) fn writable_pack(
    group_name: &str,
    song_scan_roots: &[PathBuf],
    bundled_roots: &[PathBuf],
) -> Result<WritablePack, String> {
    let (simfiles, sync_pref) = {
        let cache = deadsync_simfile::runtime_cache::get_song_cache();
        let wanted = group_name.to_lowercase();
        let mut sync_pref = deadsync_chart::SyncPref::Default;
        let mut simfiles = Vec::new();
        for pack in cache
            .iter()
            .filter(|pack| pack.group_name.to_lowercase() == wanted)
        {
            sync_pref = pack.sync_pref;
            simfiles.extend(pack.songs.iter().map(|song| song.simfile_path.clone()));
        }
        (simfiles, sync_pref)
    };
    let simfile = simfiles
        .first()
        .ok_or_else(|| format!("no pack named '{group_name}' with songs in the live catalog"))?;
    // A pack the library merged from two song folders has a Pack.ini in each,
    // and the scan reads only one of them. Writing one would not stick.
    let mut folders: Vec<&Path> = simfiles
        .iter()
        .filter_map(|simfile| simfile.parent()?.parent())
        .collect();
    folders.sort_unstable();
    folders.dedup();
    // Spelled twice is not twice: a cached song can keep the spelling its root
    // had when it was cached. Resolved before they are counted.
    let mut folders: Vec<PathBuf> = folders
        .into_iter()
        .map(|folder| std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf()))
        .collect();
    folders.sort_unstable();
    folders.dedup();
    if folders.len() > 1 {
        return Err(format!(
            "'{group_name}' is spread over {} song folders",
            folders.len()
        ));
    }
    if !deadsync_config::runtime::song_path_is_writable(simfile) {
        return Err(format!("'{group_name}' is in a read-only song folder"));
    }
    let song_dir = validated_song_dir(simfile, song_scan_roots)?;
    let pack_dir = song_dir
        .parent()
        .ok_or_else(|| format!("song has no pack folder: {}", song_dir.display()))?;
    let dir = validated_pack_dir(
        pack_dir,
        song_scan_roots,
        bundled_roots,
        PackDepth::FlatOrSeries,
    )?;
    Ok(WritablePack {
        dir,
        simfiles,
        sync_pref,
    })
}

/// The pack's `Pack.ini`, found the way the scan finds it: by name, ignoring
/// case, skipping `._` files.
fn find_pack_ini(pack_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(pack_dir)
        .ok()?
        .flatten()
        .find(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            !name.starts_with("._")
                && name.eq_ignore_ascii_case("Pack.ini")
                && entry.path().is_file()
        })
        .map(|entry| entry.path())
}

/// What a pack's `Pack.ini` holds before a change, to put back if the change
/// it was made for does not happen.
pub(crate) struct PackIniBefore {
    path: PathBuf,
    /// Its bytes exactly, or `None` when there was no file.
    bytes: Option<Vec<u8>>,
}

/// Set `SyncOffset` in the `Pack.ini` of a folder [`writable_pack`] proved,
/// then read it back the way the scan will. Returns what was there before.
///
/// A file that is a link, or that is not text this can edit, is left exactly
/// as it is: rewriting it would lose what somebody put in it.
pub(crate) fn write_pack_sync(
    pack_dir: &Path,
    group_name: &str,
    itg: bool,
) -> Result<PackIniBefore, String> {
    let path = find_pack_ini(pack_dir).unwrap_or_else(|| pack_dir.join("Pack.ini"));
    if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(format!("its Pack.ini is a link: {}", path.display()));
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("could not read '{}': {error}", path.display())),
    };
    let existing = match bytes.as_deref().map(std::str::from_utf8) {
        None => "",
        Some(Ok(text)) => text,
        Some(Err(_)) => {
            return Err(format!(
                "its Pack.ini is not UTF-8 text, so it was left as it is: {}",
                path.display()
            ));
        }
    };
    let updated = pack_ini_with_sync(existing, group_name, itg);
    deadlib_platform::atomic_write::write_atomic(&path, updated.as_bytes())
        .map_err(|error| format!("could not write '{}': {error}", path.display()))?;
    let before = PackIniBefore { path, bytes };
    let wanted = if itg {
        deadsync_chart::SyncPref::Itg
    } else {
        deadsync_chart::SyncPref::Null
    };
    let read_back = std::fs::read_to_string(&before.path)
        .map(|text| pack_ini_sync(&text))
        .unwrap_or(deadsync_chart::SyncPref::Default);
    if read_back != wanted {
        let error = format!(
            "its Pack.ini did not take the new SyncOffset: {}",
            pack_dir.display()
        );
        return Err(match restore_pack_ini(before) {
            Ok(()) => error,
            Err(restore) => format!("{error}; {restore}"),
        });
    }
    Ok(before)
}

/// Put a `Pack.ini` back exactly as it was before [`write_pack_sync`],
/// removing one that did not exist.
pub(crate) fn restore_pack_ini(before: PackIniBefore) -> Result<(), String> {
    let restored = match &before.bytes {
        Some(bytes) => deadlib_platform::atomic_write::write_atomic(&before.path, bytes),
        None => std::fs::remove_file(&before.path),
    };
    restored.map_err(|error| {
        log::warn!("Could not put back '{}': {error}", before.path.display());
        format!(
            "its Pack.ini could not be put back as it was ({error}): {}",
            before.path.display()
        )
    })
}

/// The sync a `Pack.ini` declares, read exactly as the scan reads it: only
/// keys inside `[Group]` count, the last of each wins, the file counts only if
/// its `Version` is not empty, and only the exact words `ITG` and `NULL` mean
/// anything.
fn pack_ini_sync(text: &str) -> deadsync_chart::SyncPref {
    let mut in_group = false;
    let mut version = "";
    let mut sync = "";
    for raw in text.lines() {
        let line = raw.strip_prefix('\u{feff}').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line[1..line.len() - 1].trim().eq_ignore_ascii_case("group");
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("version") {
            version = value.trim();
        } else if key.trim().eq_ignore_ascii_case("syncoffset") {
            sync = value.trim();
        }
    }
    if version.is_empty() {
        return deadsync_chart::SyncPref::Default;
    }
    match sync {
        "ITG" => deadsync_chart::SyncPref::Itg,
        "NULL" => deadsync_chart::SyncPref::Null,
        _ => deadsync_chart::SyncPref::Default,
    }
}

/// The `Pack.ini` text for a pack, with its `SyncOffset` set.
///
/// Written as a small line editor rather than a template so an existing file
/// keeps everything else it had, in its own line endings. The scan reads keys
/// only inside `[Group]`, so that is where the value goes: every `SyncOffset`
/// there is set, and a group without one -- or without a non-empty `Version`,
/// without which the whole file is ignored -- gains it at the end of its last
/// `[Group]` section. A file with no `[Group]` at all gains one at the end.
fn pack_ini_with_sync(existing: &str, pack_name: &str, itg: bool) -> String {
    let wanted = if itg { "ITG" } else { "NULL" };
    if existing.trim().is_empty() {
        // A minimal file the parser will accept. `Version` is not decoration:
        // without it the whole file is discarded and the sync value with it.
        return format!("[Group]\nVersion=1\nDisplayTitle={pack_name}\nSyncOffset={wanted}\n");
    }
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let sync_line = if itg {
        "SyncOffset=ITG"
    } else {
        "SyncOffset=NULL"
    };

    let mut out: Vec<&str> = Vec::with_capacity(existing.lines().count() + 4);
    let mut in_group = false;
    // Where the last `[Group]` section's last line is, to add keys after.
    let mut group_end: Option<usize> = None;
    let mut has_sync = false;
    let mut version_counts = false;
    for raw in existing.lines() {
        let line = raw.strip_prefix('\u{feff}').unwrap_or(raw).trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line[1..line.len() - 1].trim().eq_ignore_ascii_case("group");
            out.push(raw);
            if in_group {
                group_end = Some(out.len() - 1);
            }
            continue;
        }
        if in_group && !line.starts_with(';') && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.eq_ignore_ascii_case("SyncOffset") {
                    out.push(sync_line);
                    has_sync = true;
                    group_end = Some(out.len() - 1);
                    continue;
                }
                if key.eq_ignore_ascii_case("Version") {
                    version_counts = !value.trim().is_empty();
                }
            }
            out.push(raw);
            if !line.is_empty() {
                group_end = Some(out.len() - 1);
            }
            continue;
        }
        out.push(raw);
    }

    let missing = ["Version=1", sync_line];
    let missing = match (version_counts, has_sync) {
        (false, false) => &missing[..],
        (false, true) => &missing[..1],
        (true, false) => &missing[1..],
        (true, true) => &missing[..0],
    };
    match group_end {
        Some(at) => {
            out.splice(at + 1..at + 1, missing.iter().copied());
        }
        None => {
            out.push("[Group]");
            out.extend_from_slice(missing);
        }
    }
    out.push("");
    out.join(newline)
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

    /// The scan reads keys only inside `[Group]`, so that is where the value
    /// goes -- not after a later section, where it would be ignored -- and
    /// the file keeps its own line endings.
    #[test]
    fn the_value_lands_inside_group() {
        let text = pack_ini_with_sync(
            "[Group]\r\nVersion=1\r\nDisplayTitle=X\r\n\r\n[Other]\r\nKey=1\r\n",
            "X",
            false,
        );
        assert_eq!(
            text,
            "[Group]\r\nVersion=1\r\nDisplayTitle=X\r\nSyncOffset=NULL\r\n\r\n[Other]\r\nKey=1\r\n"
        );
        assert_eq!(super::pack_ini_sync(&text), deadsync_chart::SyncPref::Null);

        // a SyncOffset outside [Group] is not the one that counts
        let text = pack_ini_with_sync("[Other]\nSyncOffset=ITG\n[Group]\nVersion=1\n", "X", false);
        assert!(text.starts_with("[Other]\nSyncOffset=ITG\n"), "left alone");
        assert_eq!(super::pack_ini_sync(&text), deadsync_chart::SyncPref::Null);

        // a file with no [Group] gains one, not keys the scan would ignore
        let text = pack_ini_with_sync("; notes\nTitle=Loose\n", "X", true);
        assert_eq!(
            text,
            "; notes\nTitle=Loose\n[Group]\nVersion=1\nSyncOffset=ITG\n"
        );
        assert_eq!(super::pack_ini_sync(&text), deadsync_chart::SyncPref::Itg);
    }

    /// Read back exactly as the scan reads it: a file whose `Version` is
    /// empty counts for nothing, and only the exact words mean anything.
    #[test]
    fn a_pack_ini_is_read_back_as_the_scan_reads_it() {
        use deadsync_chart::SyncPref;
        assert_eq!(
            super::pack_ini_sync("[Group]\nVersion=1\nSyncOffset=ITG\n"),
            SyncPref::Itg
        );
        assert_eq!(
            super::pack_ini_sync("[Group]\nVersion=\nSyncOffset=ITG\n"),
            SyncPref::Default
        );
        assert_eq!(
            super::pack_ini_sync("[Group]\nVersion=1\nSyncOffset=itg\n"),
            SyncPref::Default
        );
        assert_eq!(
            super::pack_ini_sync(
                "\u{feff}[gRoUp]\nVERSION = 2\nsyncoffset = NULL\n[Other]\nSyncOffset=ITG\n"
            ),
            SyncPref::Null
        );
    }
}

/// How deep below a song root a pack may sit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PackDepth {
    /// `root/pack`, and nothing else: what a delete may remove.
    Flat,
    /// `root/pack` or `root/series/pack`, both of which the scan loads.
    FlatOrSeries,
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
    depth: PackDepth,
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
            "a song folder root is not a pack: {}",
            pack_dir.display()
        ));
    }

    for root in &roots {
        let Ok(relative) = pack_dir.strip_prefix(root) else {
            continue;
        };
        // Exactly root/pack -- or root/series/pack where that is allowed --
        // and every step of it an ordinary name.
        let parts: Vec<_> = relative.components().collect();
        let deepest = match depth {
            PackDepth::Flat => 1,
            PackDepth::FlatOrSeries => 2,
        };
        if parts.is_empty()
            || parts.len() > deepest
            || !parts
                .iter()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
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

    /// A pack in a series folder is one the scan loads, so its sync may be
    /// changed -- but only a pack directly in a song folder may be deleted
    /// whole, and nothing deeper is a pack at all.
    #[test]
    fn a_series_pack_may_be_synced_but_not_deleted_whole() {
        let root = test_dir("series-depth");
        let flat = root.join("Pack");
        let nested = root.join("Series").join("Pack");
        let deeper = root.join("A").join("B").join("C");
        for dir in [&flat, &nested, &deeper] {
            std::fs::create_dir_all(dir).expect("create pack dir");
        }
        let roots = vec![root.clone()];
        assert!(validated_pack_dir(&flat, &roots, &[], PackDepth::Flat).is_ok());
        assert!(validated_pack_dir(&nested, &roots, &[], PackDepth::Flat).is_err());
        assert!(validated_pack_dir(&nested, &roots, &[], PackDepth::FlatOrSeries).is_ok());
        assert!(validated_pack_dir(&deeper, &roots, &[], PackDepth::FlatOrSeries).is_err());
        assert!(validated_pack_dir(&root, &roots, &[], PackDepth::FlatOrSeries).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A write is read back as the scan reads it and can be put back exactly;
    /// a file it cannot edit as text is left alone rather than replaced.
    #[test]
    fn a_pack_ini_write_reads_back_and_puts_back_exactly() {
        let pack = test_dir("pack-ini-write");
        std::fs::create_dir_all(&pack).expect("create pack dir");

        // none there: one is made, and putting it back removes it
        let before = write_pack_sync(&pack, "Pack", false).expect("write");
        let made = pack.join("Pack.ini");
        assert_eq!(
            pack_ini_sync(&std::fs::read_to_string(&made).unwrap()),
            deadsync_chart::SyncPref::Null
        );
        restore_pack_ini(before).expect("restore");
        assert!(!made.exists());

        // found whatever its case, edited, and put back byte for byte
        let odd = pack.join("PACK.INI");
        let original = b"[Group]\r\nVersion=1\r\nDisplayTitle=Kept\r\nSyncOffset=NULL\r\n";
        std::fs::write(&odd, original).expect("seed");
        let before = write_pack_sync(&pack, "Pack", true).expect("write");
        assert_eq!(
            pack_ini_sync(&std::fs::read_to_string(&odd).unwrap()),
            deadsync_chart::SyncPref::Itg
        );
        restore_pack_ini(before).expect("restore");
        assert_eq!(std::fs::read(&odd).unwrap(), original);

        // not UTF-8: refused, untouched
        let latin = b"[Group]\nVersion=1\nDisplayTitle=Caf\xe9\n";
        std::fs::write(&odd, latin).expect("seed");
        assert!(write_pack_sync(&pack, "Pack", true).is_err());
        assert_eq!(std::fs::read(&odd).unwrap(), latin);
        let _ = std::fs::remove_dir_all(pack);
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

#[cfg(test)]
mod perf_traversal {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/perf/pack_ini.rs"
    ));
}
