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
            send_finished(&tx);
        });
    }

    pub(crate) fn start_library(&mut self, songs_root: PathBuf, courses_root: PathBuf) {
        self.start(move |tx| {
            scan_library(&tx, &songs_root, &courses_root);
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

fn cache_progress_lines(path: Option<&Path>) -> (String, String) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
