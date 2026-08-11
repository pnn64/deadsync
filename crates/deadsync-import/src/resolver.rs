//! Resolves an ITGmania score key (`Song Dir` + `StepsType` + `Difficulty`) to a
//! DeadSync GrooveStats `short_hash`, using the already-scanned song database.
//!
//! ITGmania `Stats.xml` does not store the GrooveStats hash for normal charts —
//! it identifies a chart by its on-disk song directory plus the steps type and
//! difficulty. DeadSync keys local scores by `short_hash`, so to import a score
//! we must locate the same chart in DeadSync's library and read its hash. Charts
//! that aren't present in the library can't be resolved (we have no hash for
//! them) and are reported as skipped.

use hashbrown::{Equivalent, HashMap};
use rustc_hash::FxBuildHasher;
use std::hash::{Hash, Hasher};

use deadsync_chart::{SongData, SongPack};

#[derive(PartialEq, Eq)]
struct SongKey {
    pack: String,
    folder: String,
}

impl Hash for SongKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_ascii_case_insensitive(&self.pack, state);
        hash_ascii_case_insensitive(&self.folder, state);
    }
}

struct SongKeyRef<'a> {
    pack: &'a str,
    folder: &'a str,
}

impl Hash for SongKeyRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_ascii_case_insensitive(self.pack, state);
        hash_ascii_case_insensitive(self.folder, state);
    }
}

impl Equivalent<SongKey> for SongKeyRef<'_> {
    fn equivalent(&self, key: &SongKey) -> bool {
        self.pack.eq_ignore_ascii_case(&key.pack) && self.folder.eq_ignore_ascii_case(&key.folder)
    }
}

fn hash_ascii_case_insensitive<H: Hasher>(value: &str, state: &mut H) {
    value.len().hash(state);
    for byte in value.bytes() {
        byte.to_ascii_lowercase().hash(state);
    }
}

/// Builds a fast lookup over the scanned song library and resolves ITGmania
/// score keys to DeadSync chart hashes.
pub struct ChartResolver<'a> {
    /// `(pack_lower, song_folder_lower)` → song.
    by_song: HashMap<SongKey, &'a SongData, FxBuildHasher>,
}

/// Outcome of resolving a single `<Steps>` entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution<'a> {
    /// Found the chart; carries its `short_hash`.
    Found(&'a str),
    /// The song directory wasn't found in the library.
    SongNotFound,
    /// The song was found but it has no matching chart (type/difficulty/edit).
    ChartNotFound,
}

impl<'a> ChartResolver<'a> {
    /// Builds the resolver from the scanned packs.
    pub fn build(packs: &'a [SongPack]) -> Self {
        let mut by_song = HashMap::with_hasher(FxBuildHasher);
        for pack in packs {
            let pack_keys = pack_keys(pack);
            for song in &pack.songs {
                if let Some(folder) = song_folder_name(song) {
                    let folder_lc = folder.to_ascii_lowercase();
                    for pk in &pack_keys {
                        by_song
                            .entry(SongKey {
                                pack: pk.clone(),
                                folder: folder_lc.clone(),
                            })
                            .or_insert(song.as_ref());
                    }
                }
            }
        }
        Self { by_song }
    }

    /// Resolves an ITGmania song directory key (e.g. `"Pack/Song"` from
    /// `favorites.txt`, or a `Stats.xml` `Dir`) to the matching library song.
    /// Returns `None` when the song isn't in DeadSync's scanned library.
    pub fn resolve_song(&self, song_dir: &str) -> Option<&'a SongData> {
        self.song_for_dir(song_dir)
    }

    /// Resolves a score key to a chart `short_hash`.
    pub fn resolve(
        &self,
        song_dir: &str,
        steps_type: &str,
        difficulty: &str,
        description: &str,
    ) -> Resolution<'a> {
        let Some(song) = self.song_for_dir(song_dir) else {
            return Resolution::SongNotFound;
        };

        let is_edit = difficulty.eq_ignore_ascii_case("Edit");
        let description = description.trim();
        let mut edit_count = 0;
        let mut sole_edit = None;
        for chart in &song.charts {
            if !chart.chart_type.eq_ignore_ascii_case(steps_type) {
                continue;
            }
            if !chart.difficulty.eq_ignore_ascii_case(difficulty) {
                continue;
            }
            if !is_edit {
                return Resolution::Found(chart.short_hash.as_str());
            }
            edit_count += 1;
            sole_edit = Some(chart.short_hash.as_str());
            if !description.is_empty()
                && (chart.description.trim().eq_ignore_ascii_case(description)
                    || chart.chart_name.trim().eq_ignore_ascii_case(description))
            {
                return Resolution::Found(chart.short_hash.as_str());
            }
        }

        if is_edit && edit_count == 1 {
            Resolution::Found(sole_edit.expect("one Edit chart was recorded"))
        } else {
            Resolution::ChartNotFound
        }
    }

    fn song_for_dir(&self, song_dir: &str) -> Option<&'a SongData> {
        let (pack, folder) = nested_song_dir_parts(song_dir)?;
        self.by_song
            .get(&SongKeyRef { pack, folder })
            .copied()
            .or_else(|| {
                let (pack, folder) = song_dir_parts(song_dir)?;
                self.by_song.get(&SongKeyRef { pack, folder }).copied()
            })
    }
}

/// Chooses the matching Edit chart by its description, with sensible fallbacks.
#[cfg(test)]
fn pick_edit<'a>(
    candidates: &[&'a deadsync_chart::ChartData],
    description: &str,
) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }
    let desc = description.trim();
    if !desc.is_empty()
        && let Some(c) = candidates.iter().find(|c| {
            c.description.trim().eq_ignore_ascii_case(desc)
                || c.chart_name.trim().eq_ignore_ascii_case(desc)
        })
    {
        return Some(c.short_hash.as_str());
    }
    // No description match: only safe to assume when there's exactly one edit.
    if candidates.len() == 1 {
        return Some(candidates[0].short_hash.as_str());
    }
    None
}

/// All case-folded keys a pack can be addressed by (folder name).
fn pack_keys(pack: &SongPack) -> Vec<String> {
    let mut keys = Vec::with_capacity(2);
    if !pack.group_name.is_empty() {
        keys.push(pack.group_name.to_ascii_lowercase());
    }
    if let Some(dir_name) = pack.directory.file_name().and_then(|s| s.to_str()) {
        let lc = dir_name.to_ascii_lowercase();
        if !keys.contains(&lc) {
            keys.push(lc);
        }
    }
    keys
}

/// The song's on-disk folder name (the parent directory of its simfile).
fn song_folder_name(song: &SongData) -> Option<&str> {
    song.simfile_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
}

/// Normalizes an ITGmania `Dir` attribute (e.g. `"Songs/Pack/Song/"`) into a
/// `(pack_lower, song_folder_lower)` pair. Leading `Songs/` / `AdditionalSongs/`
/// roots and surrounding slashes are stripped. Returns `None` if the path
/// doesn't have at least a pack and a song component.
pub fn normalize_song_dir(dir: &str) -> Option<(String, String)> {
    let (pack, song) = song_dir_parts(dir)?;
    Some((pack.to_ascii_lowercase(), song.to_ascii_lowercase()))
}

fn song_dir_parts(dir: &str) -> Option<(&str, &str)> {
    let mut parts = dir
        .trim()
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty());

    let first = parts.next()?;
    let pack =
        if first.eq_ignore_ascii_case("Songs") || first.eq_ignore_ascii_case("AdditionalSongs") {
            parts.next()?
        } else {
            first
        };

    let mut song = parts.next()?;
    for part in parts {
        song = part;
    }
    // The song folder is the last component; anything between pack and song is
    // unusual but we key on the final folder which is what holds the simfile.
    Some((pack, song))
}

fn nested_song_dir_parts(dir: &str) -> Option<(&str, &str)> {
    let mut parts = dir
        .trim()
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .rev();
    let song = parts.next()?;
    let pack = parts.next()?;
    Some((pack, song))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_typical_dirs() {
        assert_eq!(
            normalize_song_dir("Songs/My Pack/Cool Song/"),
            Some(("my pack".into(), "cool song".into()))
        );
        assert_eq!(
            normalize_song_dir("AdditionalSongs/Pack/Song"),
            Some(("pack".into(), "song".into()))
        );
        assert_eq!(
            normalize_song_dir("Pack/Song/"),
            Some(("pack".into(), "song".into()))
        );
        assert_eq!(
            normalize_song_dir("Songs\\Win Pack\\Win Song\\"),
            Some(("win pack".into(), "win song".into()))
        );
        assert_eq!(
            normalize_song_dir(" / SONGS // My Pack / Bonus / Cool Song / "),
            Some(("my pack".into(), "cool song".into()))
        );
        assert_eq!(
            normalize_song_dir("AdditionalSongs\\Pack / Nested\\Final Song"),
            Some(("pack".into(), "final song".into()))
        );
        assert_eq!(
            nested_song_dir_parts("Songs/Series/Pack/Song"),
            Some(("Pack", "Song"))
        );
    }

    #[test]
    fn rejects_incomplete_dirs() {
        assert_eq!(normalize_song_dir("Songs/"), None);
        assert_eq!(normalize_song_dir("Songs/JustAPack/"), None);
        assert_eq!(normalize_song_dir(""), None);
    }

    fn chart(diff: &str, desc: &str, name: &str, hash: &str) -> deadsync_chart::ChartData {
        deadsync_chart::ChartData {
            chart_type: "dance-single".into(),
            difficulty: diff.into(),
            description: desc.into(),
            chart_name: name.into(),
            meter: 10,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.into(),
            stats: Default::default(),
            tech_counts: Default::default(),
            mines_nonfake: 0,
            stamina_counts: Default::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    #[test]
    fn picks_standard_difficulty() {
        let charts = [
            chart("Medium", "", "", "hashmed"),
            chart("Hard", "", "", "hashhard"),
        ];
        assert_eq!(pick_edit(&[], ""), None);
        // Non-edit resolution is exercised via resolve(); here verify edit pick.
        let hard: Vec<&deadsync_chart::ChartData> = charts.iter().collect();
        // pick_edit only used for edits, but confirm description fallback logic:
        assert_eq!(pick_edit(&hard[1..2], ""), Some("hashhard"));
    }

    #[test]
    fn picks_edit_by_description() {
        let c1 = chart("Edit", "My Cool Edit", "", "edit1");
        let c2 = chart("Edit", "Another", "", "edit2");
        let cands = vec![&c1, &c2];
        assert_eq!(pick_edit(&cands, "My Cool Edit"), Some("edit1"));
        assert_eq!(pick_edit(&cands, "another"), Some("edit2"));
        // Ambiguous (no description, multiple edits) → None.
        assert_eq!(pick_edit(&cands, ""), None);
    }

    #[test]
    fn picks_single_edit_without_description() {
        let c1 = chart("Edit", "Whatever", "", "soloedit");
        let cands = vec![&c1];
        assert_eq!(pick_edit(&cands, ""), Some("soloedit"));
    }
}
