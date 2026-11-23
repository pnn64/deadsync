//! Shared catalog entries and selectable actions for a song wheel.

use crate::SongData;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum MusicWheelEntry {
    PackHeader {
        name: Arc<str>,
        original_index: usize,
        banner_path: Option<Arc<Path>>,
        song_count: usize,
        pack_key: Option<Arc<str>>,
        /// Set on both a series header and its child packs. A series has no pack key.
        parent_series: Option<Arc<str>>,
    },
    Song(Arc<SongData>),
    Random(WheelRandomAction),
}

/// A catalog action can reveal a song or confirm the song previewed by a portal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelRandomAction {
    Reveal,
    Portal,
}

impl MusicWheelEntry {
    #[must_use]
    pub fn pack_key(&self) -> Option<&str> {
        match self {
            Self::PackHeader { pack_key, .. } => pack_key.as_deref(),
            Self::Song(_) | Self::Random(_) => None,
        }
    }

    /// Virtual sections use their visible name, and series headers have no section key.
    #[must_use]
    pub fn section_key(&self) -> Option<&str> {
        match self {
            Self::PackHeader {
                pack_key: Some(key),
                ..
            } => Some(key),
            Self::PackHeader {
                name,
                parent_series: None,
                ..
            } => Some(name),
            Self::PackHeader { .. } | Self::Song(_) | Self::Random(_) => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_series_header(&self) -> bool {
        matches!(
            self,
            Self::PackHeader {
                pack_key: None,
                parent_series: Some(_),
                ..
            }
        )
    }
}
