use deadsync_config::prelude as config;
use deadsync_theme_simply_love::views::{
    SimplyLoveContentReloadEvent, SimplyLoveContentReloadPhase,
};
use log::info;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

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
            let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
                let _ = tx.send(SimplyLoveContentReloadEvent::Song {
                    done,
                    total,
                    pack: pack.to_owned(),
                    song: song.to_owned(),
                });
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

    fn start(&mut self, job: impl FnOnce(Sender<SimplyLoveContentReloadEvent>) + Send + 'static) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || job(tx));
    }

    pub(crate) fn poll(&mut self) -> Vec<SimplyLoveContentReloadEvent> {
        let Some(rx) = self.rx.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    finished |= matches!(event, SimplyLoveContentReloadEvent::Finished { .. });
                    events.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !finished {
                        events.push(finished_event());
                    }
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            self.rx = None;
        }
        events
    }
}

fn scan_library(tx: &Sender<SimplyLoveContentReloadEvent>, songs_root: &Path, courses_root: &Path) {
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Songs,
    ));
    let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
        let _ = tx.send(SimplyLoveContentReloadEvent::Song {
            done,
            total,
            pack: pack.to_owned(),
            song: song.to_owned(),
        });
    };
    deadsync_simfile::app_runtime::scan_and_load_songs_with_progress_counts(
        songs_root,
        &mut on_song,
    );

    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Courses,
    ));
    let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
        let _ = tx.send(SimplyLoveContentReloadEvent::Course {
            done,
            total,
            group: group.to_owned(),
            course: course.to_owned(),
        });
    };
    deadsync_simfile::app_runtime::scan_and_load_courses_with_progress_counts(
        courses_root,
        songs_root,
        &mut on_course,
    );
}

fn prewarm_artwork(tx: &Sender<SimplyLoveContentReloadEvent>) {
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
    let mut on_artwork = |done: usize, _total: usize, path: Option<&Path>| {
        let (line2, line3) = cache_progress_lines(path);
        let _ = tx.send(SimplyLoveContentReloadEvent::Artwork {
            done,
            total,
            line2,
            line3,
        });
    };
    deadsync_assets::media_cache::prewarm_artwork_cache_with_progress(
        &banner_paths,
        &cdtitle_paths,
        &mut on_artwork,
    );
    info!("Init loading: artwork cache prewarm complete.");
}

fn compile_noteskins(tx: &Sender<SimplyLoveContentReloadEvent>) {
    let _ = tx.send(SimplyLoveContentReloadEvent::Phase(
        SimplyLoveContentReloadPhase::Noteskins,
    ));
    info!("Init loading: compiling noteskin cache before UI...");
    let mut on_noteskin = |done: usize, total: usize, skin: &str, status: &str| {
        let _ = tx.send(SimplyLoveContentReloadEvent::Noteskins {
            done,
            total,
            skin: skin.to_owned(),
            status: status.to_owned(),
        });
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
fn analyze_replaygain(tx: &Sender<SimplyLoveContentReloadEvent>, restrict_to: Option<&[PathBuf]>) {
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
    let mut on_song = |done: usize, total: usize, path: &Path| {
        let (line2, line3) = cache_progress_lines(Some(path));
        let _ = tx.send(SimplyLoveContentReloadEvent::ReplayGain {
            done,
            total,
            line2,
            line3,
        });
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

fn send_finished(tx: &mpsc::Sender<SimplyLoveContentReloadEvent>) {
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
