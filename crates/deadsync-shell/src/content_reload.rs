use deadsync_config::prelude as config;
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
/// one frame, and the focused worker-progress benchmark reports queue/frame
/// cost, allocation churn, and eventual drain behavior.
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
    pub(crate) fn start_initialization(&mut self, songs_root: PathBuf, courses_root: PathBuf) {
        self.start(move |tx| {
            scan_library(&tx, &songs_root, &courses_root);
            prewarm_artwork(&tx);
            compile_noteskins(&tx);
            analyze_replaygain(&tx, None);
            send_finished(&tx);
        });
    }

    pub(crate) fn start_library(&mut self, songs_root: PathBuf, courses_root: PathBuf) {
        self.start(move |tx| {
            scan_library(&tx, &songs_root, &courses_root);
            analyze_replaygain(&tx, None);
            send_finished(&tx);
        });
    }

    pub(crate) fn start_song_dirs(&mut self, songs_root: PathBuf, pack_dirs: Vec<PathBuf>) {
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
            analyze_replaygain(&tx, Some(&pack_dirs));
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
    let total = deadsync_assets::media_cache::artwork_cache_jobs(&banner_paths, &cdtitle_paths);
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
    deadsync_assets::media_cache::prewarm_artwork_cache_with_progress(
        &banner_paths,
        &cdtitle_paths,
        &mut on_artwork,
    );
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

/// Frontload ReplayGain (EBU R128 loudness) analysis before the menu appears,
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
) {
    if !config::get().enable_replaygain || !deadsync_audio_stream::is_initialized() {
        return;
    }
    let paths = replaygain_music_paths(restrict_to);
    if paths.is_empty() {
        return;
    }
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::ReplayGain,
    ));
    info!(
        "Init loading: analyzing ReplayGain loudness for {} song(s)...",
        paths.len()
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
    let mut paths = std::collections::BTreeSet::new();
    let cache = deadsync_simfile::runtime_cache::get_song_cache();
    for pack in cache.iter() {
        for song in &pack.songs {
            if let Some(path) = song.music_path.as_ref() {
                if let Some(dirs) = restrict_to
                    && !dirs.iter().any(|dir| path.starts_with(dir))
                {
                    continue;
                }
                paths.insert(path.clone());
            }
        }
    }
    paths.into_iter().collect()
}

#[cfg(feature = "bench-support")]
pub fn benchmark_sample_progress<T>(
    samples: &[(usize, usize, Duration)],
    mut make_event: impl FnMut(usize, usize) -> T,
) -> Vec<T> {
    let start = Instant::now();
    let mut gate = ProgressGate::default();
    samples
        .iter()
        .filter_map(|&(done, total, elapsed)| {
            gate.should_emit_at(done, total, start + elapsed)
                .then(|| make_event(done, total))
        })
        .collect()
}

#[cfg(feature = "bench-support")]
pub fn benchmark_receive_ready<T>(rx: &Receiver<T>) -> SmallVec<[T; PROGRESS_EVENTS_PER_FRAME]> {
    receive_ready(rx).events
}

fn artwork_cache_paths() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut banner = Vec::new();
    let mut cdtitle = Vec::new();
    {
        let cache = deadsync_simfile::runtime_cache::get_song_cache();
        for pack in cache.iter() {
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
    }
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

pub(crate) fn cache_progress_lines(path: Option<&Path>) -> (String, String) {
    let Some(path) = path else {
        return (String::new(), String::new());
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name)
        .to_owned();
    let parts: Vec<_> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    if let Some(index) = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("songs"))
        && let Some(pack) = parts.get(index + 1)
    {
        let song = parts
            .get(index + 2)
            .copied()
            .filter(|name| !name.eq_ignore_ascii_case(file_name))
            .map(str::to_owned)
            .unwrap_or(file_stem);
        return ((*pack).to_owned(), song);
    }
    if let Some(index) = parts
        .iter()
        .position(|part| part.eq_ignore_ascii_case("courses"))
        && let Some(group) = parts.get(index + 1)
    {
        return ((*group).to_owned(), file_stem);
    }
    let parent = path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_owned();
    (parent, file_stem)
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
    if !deadsync_config::prelude::song_path_is_writable(simfile_path) {
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
