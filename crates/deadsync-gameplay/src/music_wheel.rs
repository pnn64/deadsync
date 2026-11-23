//! Game-owned wheel entries, circular indexing, and random song selection.

use crate::TurnRng;
use deadsync_chart::SongData;
use deadsync_chart::song::{STANDARD_DIFFICULTY_COUNT, standard_difficulty_index};
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

struct RandomSongCandidate {
    entry: usize,
    section: Option<usize>,
    difficulties: u8,
    normal_song: bool,
}

/// Prepared metadata for the current song sort and style. The pool borrows its
/// immutable sort through a shared pointer and reuses storage when it changes.
#[derive(Default)]
pub struct RandomSongPool {
    entries: Arc<[MusicWheelEntry]>,
    chart_type: &'static str,
    candidates: Vec<RandomSongCandidate>,
}

impl RandomSongPool {
    pub fn sync(&mut self, entries: &Arc<[MusicWheelEntry]>, chart_type: &'static str) {
        if Arc::ptr_eq(&self.entries, entries) && self.chart_type == chart_type {
            return;
        }
        self.entries = Arc::clone(entries);
        self.chart_type = chart_type;
        self.candidates.clear();
        self.candidates.reserve(entries.len());
        let mut section = None;
        for (index, entry) in entries.iter().enumerate() {
            let song = match entry {
                MusicWheelEntry::Song(song) => song,
                MusicWheelEntry::PackHeader { .. } => {
                    section = entry.section_key().map(|_| index);
                    continue;
                }
                MusicWheelEntry::Random(_) => continue,
            };
            let mut difficulties = 0;
            let mut has_edit = false;
            let mut normal_song = false;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case("lights-cabinet")
                    && !chart.difficulty.eq_ignore_ascii_case("beginner")
                {
                    normal_song = true;
                }
                if chart.chart_type.eq_ignore_ascii_case(chart_type) {
                    if let Some(index) = standard_difficulty_index(&chart.difficulty) {
                        difficulties |= 1 << index;
                    } else if chart.difficulty.eq_ignore_ascii_case("edit") {
                        has_edit = true;
                    }
                }
            }
            if difficulties != 0 || has_edit {
                self.candidates.push(RandomSongCandidate {
                    entry: index,
                    section,
                    difficulties,
                    normal_song,
                });
            }
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Sample in the best eligible tier. Prefer normal songs, then the joined
    /// players' standard difficulties, then the open section. When no song
    /// has the preferred difficulties, any section is eligible.
    #[must_use]
    pub fn pick(
        &self,
        preferred: [Option<usize>; 2],
        preferred_section: Option<&str>,
        seed: u64,
    ) -> Option<&Arc<SongData>> {
        let required = preferred
            .into_iter()
            .flatten()
            .filter(|&difficulty| difficulty < STANDARD_DIFFICULTY_COUNT)
            .fold(0, |mask, difficulty| mask | (1 << difficulty));
        let mut rng = TurnRng::new(seed);
        let mut best = None;
        let mut best_rank = (false, false, false);
        let mut ties = 0;
        for candidate in &self.candidates {
            let preferred_steps = candidate.difficulties & required == required;
            let local = preferred_steps
                && preferred_section
                    .zip(candidate.section)
                    .is_some_and(|(name, index)| self.entries[index].section_key() == Some(name));
            let rank = (candidate.normal_song, preferred_steps, local);
            if rank > best_rank {
                best_rank = rank;
                ties = 0;
            }
            if rank == best_rank {
                ties += 1;
                if rng.gen_range(ties) == 0 {
                    best = Some(candidate.entry);
                }
            }
        }
        match self.entries.get(best?) {
            Some(MusicWheelEntry::Song(song)) => Some(song),
            _ => None,
        }
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
            assert_eq!(
                wheel_entry_index(1, offset, 3),
                ((offset + 10) % 3) as usize
            );
        }
    }
}
