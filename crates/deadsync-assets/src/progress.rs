//! Progress reports for the game library and its derived asset caches.

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
