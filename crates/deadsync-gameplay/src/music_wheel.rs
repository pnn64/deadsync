//! Game-owned wheel entries and circular selection indexing.

use deadsync_chart::SongData;
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
}

impl MusicWheelEntry {
    #[must_use]
    pub fn pack_key(&self) -> Option<&str> {
        match self {
            Self::PackHeader { pack_key, .. } => pack_key.as_deref(),
            Self::Song(_) => None,
        }
    }

    /// Virtual sections use their visible name, and series headers have no section key.
    #[must_use]
    pub fn section_key(&self) -> Option<&str> {
        match self {
            Self::PackHeader { pack_key: Some(key), .. } => Some(key),
            Self::PackHeader { name, parent_series: None, .. } => Some(name),
            Self::PackHeader { .. } | Self::Song(_) => None,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn is_series_header(&self) -> bool {
        matches!(self, Self::PackHeader { pack_key: None, parent_series: Some(_), .. })
    }
}

/// Offset from an index in a nonempty wheel. Offsets can span multiple turns.
#[inline(always)]
#[must_use]
pub fn wheel_entry_index(selected: usize, offset: isize, entry_count: usize) -> usize {
    let biased = selected as isize + offset + entry_count as isize;
    if biased >= 0 {
        biased as usize % entry_count
    } else {
        // An offset can cross several turns on a short wheel.
        biased.rem_euclid(entry_count as isize) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::wheel_entry_index;

    #[test]
    fn wheel_offsets_wrap_in_both_directions_on_short_lists() {
        for offset in -8..=8 {
            assert_eq!(wheel_entry_index(0, offset, 1), 0);
            assert_eq!(wheel_entry_index(1, offset, 3), ((offset + 10) % 3) as usize);
        }
    }
}
