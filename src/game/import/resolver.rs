//! Resolves an ITGmania score key (`Song Dir` + `StepsType` + `Difficulty`) to a
//! DeadSync GrooveStats `short_hash`, using the already-scanned song database.
//!
//! ITGmania `Stats.xml` does not store the GrooveStats hash for normal charts —
//! it identifies a chart by its on-disk song directory plus the steps type and
//! difficulty. DeadSync keys local scores by `short_hash`, so to import a score
//! we must locate the same chart in DeadSync's library and read its hash. Charts
//! that aren't present in the library can't be resolved (we have no hash for
//! them) and are reported as skipped.

use std::collections::HashMap;

use deadsync_chart::{SongData, SongPack};

/// Builds a fast lookup over the scanned song library and resolves ITGmania
/// score keys to DeadSync chart hashes.
pub struct ChartResolver<'a> {
    /// `(pack_lower, song_folder_lower)` → song.
    by_song: HashMap<(String, String), &'a SongData>,
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
        let mut by_song: HashMap<(String, String), &'a SongData> = HashMap::new();
        for pack in packs {
            let pack_keys = pack_keys(pack);
            for song in &pack.songs {
                if let Some(folder) = song_folder_name(song) {
                    let folder_lc = folder.to_ascii_lowercase();
                    for pk in &pack_keys {
                        by_song
                            .entry((pk.clone(), folder_lc.clone()))
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
        let (pack, folder) = normalize_song_dir(song_dir)?;
        self.by_song.get(&(pack, folder)).copied()
    }

    /// Resolves a score key to a chart `short_hash`.
    pub fn resolve(
        &self,
        song_dir: &str,
        steps_type: &str,
        difficulty: &str,
        description: &str,
    ) -> Resolution<'a> {
        let Some((pack, folder)) = normalize_song_dir(song_dir) else {
            return Resolution::SongNotFound;
        };
        let Some(song) = self.by_song.get(&(pack, folder)).copied() else {
            return Resolution::SongNotFound;
        };

        let mut found: Option<&'a str> = None;
        let mut edit_candidates: Vec<&'a deadsync_chart::ChartData> = Vec::new();

        for chart in &song.charts {
            if !chart.chart_type.eq_ignore_ascii_case(steps_type) {
                continue;
            }
            if !chart.difficulty.eq_ignore_ascii_case(difficulty) {
                continue;
            }
            if difficulty.eq_ignore_ascii_case("Edit") {
                edit_candidates.push(chart);
            } else {
                found = Some(chart.short_hash.as_str());
                break;
            }
        }

        if !difficulty.eq_ignore_ascii_case("Edit") {
            return match found {
                Some(h) => Resolution::Found(h),
                None => Resolution::ChartNotFound,
            };
        }

        // Edit charts: disambiguate by description / chart name.
        match pick_edit(&edit_candidates, description) {
            Some(h) => Resolution::Found(h),
            None => Resolution::ChartNotFound,
        }
    }
}

/// Chooses the matching Edit chart by its description, with sensible fallbacks.
fn pick_edit<'a>(
    candidates: &[&'a deadsync_chart::ChartData],
    description: &str,
) -> Option<&'a str> {
    if candidates.is_empty() {
        return None;
    }
    let desc = description.trim();
    if !desc.is_empty() {
        if let Some(c) = candidates.iter().find(|c| {
            c.description.trim().eq_ignore_ascii_case(desc)
                || c.chart_name.trim().eq_ignore_ascii_case(desc)
        }) {
            return Some(c.short_hash.as_str());
        }
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
    let trimmed = dir.trim().replace('\\', "/");
    let mut parts: Vec<&str> = trimmed
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    // Strip a leading song-root component if present.
    if let Some(first) = parts.first() {
        if first.eq_ignore_ascii_case("Songs") || first.eq_ignore_ascii_case("AdditionalSongs") {
            parts.remove(0);
        }
    }

    if parts.len() < 2 {
        return None;
    }
    let pack = parts[0].to_ascii_lowercase();
    // The song folder is the last component; anything between pack and song is
    // unusual but we key on the final folder which is what holds the simfile.
    let song = parts[parts.len() - 1].to_ascii_lowercase();
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
        let charts = vec![
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
