//! Progress reports for the game library and its derived asset caches.

use std::sync::Arc;
use std::time::Instant;

/// Current step of a library load or reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentReloadPhase {
    Songs,
    Courses,
    Artwork,
    Noteskins,
    ReplayGain,
}

/// Progress and the completed catalog snapshot from a library load or reload.
#[derive(Clone, Debug)]
pub enum ContentReloadEvent {
    Phase(ContentReloadPhase),
    Song {
        done: usize,
        total: usize,
        pack: String,
        song: String,
    },
    Course {
        done: usize,
        total: usize,
        group: String,
        course: String,
    },
    Artwork {
        done: usize,
        total: usize,
        line2: String,
        line3: String,
    },
    Noteskins {
        done: usize,
        total: usize,
        skin: String,
        status: String,
    },
    ReplayGain {
        done: usize,
        total: usize,
        line2: String,
        line3: String,
    },
    Finished {
        song_packs: Vec<deadsync_chart::SongPack>,
    },
}

/// Read-only progress for the active phase of the initial library load.
pub struct LibraryLoadView {
    pub phase: ContentReloadPhase,
    pub done: usize,
    pub total: usize,
    pub line2: Arc<str>,
    pub line3: Arc<str>,
    pub finished: bool,
    pub can_skip_replaygain: bool,
    pub started_at: Instant,
    pub phase_started_at: Instant,
}

/// The startup controller accumulates progress and lends its current view to the theme.
pub struct LibraryLoadProgress {
    view: LibraryLoadView,
    skip_requested: bool,
}

impl LibraryLoadProgress {
    pub fn new(now: Instant) -> Self {
        Self {
            view: LibraryLoadView {
                phase: ContentReloadPhase::Songs,
                done: 0,
                total: 0,
                line2: Arc::from(""),
                line3: Arc::from(""),
                finished: false,
                can_skip_replaygain: false,
                started_at: now,
                phase_started_at: now,
            },
            skip_requested: false,
        }
    }

    pub const fn view(&self) -> &LibraryLoadView {
        &self.view
    }

    pub fn apply(&mut self, event: ContentReloadEvent, now: Instant) {
        use ContentReloadPhase::{Artwork, Courses, Noteskins, ReplayGain, Songs};

        let (phase, done, total, line2, line3) = match event {
            ContentReloadEvent::Phase(phase) => (phase, 0, 0, String::new(), String::new()),
            ContentReloadEvent::Song {
                done,
                total,
                pack,
                song,
            } => (Songs, done, total, pack, song),
            ContentReloadEvent::Course {
                done,
                total,
                group,
                course,
            } => (Courses, done, total, group, course),
            ContentReloadEvent::Artwork {
                done,
                total,
                line2,
                line3,
            } => (Artwork, done, total, line2, line3),
            ContentReloadEvent::Noteskins {
                done,
                total,
                skin,
                status,
            } => (Noteskins, done, total, skin, status),
            ContentReloadEvent::ReplayGain {
                done,
                total,
                line2,
                line3,
            } => (ReplayGain, done, total, line2, line3),
            ContentReloadEvent::Finished { .. } => {
                self.view.finished = true;
                self.view.can_skip_replaygain = false;
                return;
            }
        };
        if self.view.phase != phase {
            self.view.phase = phase;
            self.view.phase_started_at = now;
        }
        self.view.done = done;
        self.view.total = total.max(done);
        self.view.line2 = Arc::from(line2);
        self.view.line3 = Arc::from(line3);
        self.view.can_skip_replaygain =
            phase == ReplayGain && !self.skip_requested && !self.view.finished;
    }

    pub fn request_replaygain_skip(&mut self) -> bool {
        if !self.view.can_skip_replaygain {
            return false;
        }
        self.skip_requested = true;
        self.view.can_skip_replaygain = false;
        true
    }
}

/// Progress and result of a user-requested library-wide loudness analysis.
#[derive(Clone, Debug)]
pub enum ApplyReplayGainEvent {
    /// The library has been enumerated and analysis is ready to begin.
    Started { total: usize },
    /// The most recently analyzed song with its pack and song labels.
    Progress {
        done: usize,
        total: usize,
        line2: String,
        line3: String,
    },
    /// Analysis finished or acknowledged cancellation.
    Finished {
        done: usize,
        total: usize,
        cancelled: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn startup_progress_reports_current_phase_counts_and_timings() {
        let started_at = Instant::now();
        let artwork_start = started_at + Duration::from_secs(2);
        let mut progress = LibraryLoadProgress::new(started_at);
        progress.apply(
            ContentReloadEvent::Phase(ContentReloadPhase::Artwork),
            artwork_start,
        );
        progress.apply(
            ContentReloadEvent::Artwork {
                done: 4,
                total: 9,
                line2: "Pack".to_owned(),
                line3: "Song".to_owned(),
            },
            artwork_start + Duration::from_secs(1),
        );
        let view = progress.view();
        assert_eq!((view.done, view.total), (4, 9));
        assert_eq!((view.line2.as_ref(), view.line3.as_ref()), ("Pack", "Song"));
        assert_eq!(view.started_at, started_at);
        assert_eq!(view.phase_started_at, artwork_start);

        progress.apply(
            ContentReloadEvent::Noteskins {
                done: 3,
                total: 2,
                skin: "metal".to_owned(),
                status: "built".to_owned(),
            },
            artwork_start + Duration::from_secs(2),
        );
        let view = progress.view();
        assert_eq!(view.phase, ContentReloadPhase::Noteskins);
        assert_eq!((view.done, view.total), (3, 3));
        assert_eq!(
            (view.line2.as_ref(), view.line3.as_ref()),
            ("metal", "built")
        );
        assert_eq!(
            view.phase_started_at,
            artwork_start + Duration::from_secs(2)
        );
    }

    #[test]
    fn startup_replaygain_skip_is_only_available_once_while_analyzing() {
        let now = Instant::now();
        let mut progress = LibraryLoadProgress::new(now);
        assert!(!progress.request_replaygain_skip());

        progress.apply(
            ContentReloadEvent::Phase(ContentReloadPhase::ReplayGain),
            now,
        );
        assert!(progress.view().can_skip_replaygain);
        assert!(progress.request_replaygain_skip());
        progress.apply(
            ContentReloadEvent::ReplayGain {
                done: 2,
                total: 100,
                line2: "Pack".to_owned(),
                line3: "Song".to_owned(),
            },
            now,
        );
        assert!(!progress.view().can_skip_replaygain);
        assert!(!progress.request_replaygain_skip());

        let mut progress = LibraryLoadProgress::new(now);
        progress.apply(
            ContentReloadEvent::Phase(ContentReloadPhase::ReplayGain),
            now,
        );
        progress.apply(
            ContentReloadEvent::Finished {
                song_packs: Vec::new(),
            },
            now,
        );
        assert!(progress.view().finished);
        assert!(!progress.view().can_skip_replaygain);
        assert!(!progress.request_replaygain_skip());
    }
}
