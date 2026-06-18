//! Orchestration: turn an ITGmania `LocalProfiles/<id>/` directory into a brand
//! new DeadSync local profile (metadata, online keys, avatar, Simply Love player
//! options) plus its offline high-score history.

use std::collections::HashSet;
use std::path::Path;

use deadsync_profile::{
    initials_from_name, profile_guid_from_itgmania_guid, sanitize_player_initials,
};
use deadsync_score::{LocalScoreEntry, local_score_from_itg};

use crate::game::profile::{
    ImportProfileData, create_local_profile_from_import, default_local_profile_options,
    write_imported_favorites, write_imported_profile_stats,
};
use crate::game::scores::{import_itl_json, import_local_scores};
use crate::game::song::get_song_cache;

use super::itg::{self, ItgReadError, ItgSource};
use super::options::translate_player_options;
use super::resolver::{ChartResolver, Resolution};

/// Result of importing one ITGmania profile.
#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    /// New DeadSync local profile id.
    pub profile_id: String,
    pub display_name: String,
    /// Total high-score records found in `Stats.xml`.
    pub scores_total: usize,
    /// Plays successfully written to the new profile.
    pub scores_imported: usize,
    /// Records skipped because the song wasn't in DeadSync's library.
    pub charts_song_not_found: usize,
    /// Records skipped because the chart (type/difficulty/edit) wasn't found.
    pub charts_chart_not_found: usize,
    /// Records whose grade/percent couldn't be mapped to a DeadSync play.
    pub scores_unmapped: usize,
    /// Total favorited songs found in `favorites.txt`.
    pub favorites_total: usize,
    /// Favorited songs matched to a library song and imported.
    pub favorites_imported: usize,
    /// Favorited songs skipped because the song wasn't in DeadSync's library.
    pub favorites_song_not_found: usize,
    /// ITL `hashMap` entries imported from `ITL2026.json` (0 if absent).
    pub itl_entries_imported: usize,
}

/// Read an ITGmania profile directory and import it into a new local profile.
///
/// `on_progress(done, total, label)` is invoked once per song as the offline
/// scores are processed, so a caller can drive a progress bar. Pass a no-op
/// closure when progress isn't needed.
pub fn import_itg_profile_dir<F: FnMut(usize, usize, &str)>(
    dir: &Path,
    on_progress: F,
) -> Result<ImportSummary, ItgReadError> {
    let source = itg::read_profile_dir(dir)?;
    import_from_source(&source, on_progress)
}

/// Import an already-read [`ItgSource`] into a new local profile. See
/// [`import_itg_profile_dir`] for the `on_progress` contract.
pub fn import_from_source<F: FnMut(usize, usize, &str)>(
    source: &ItgSource,
    mut on_progress: F,
) -> Result<ImportSummary, ItgReadError> {
    let (base_singles, base_doubles) = default_local_profile_options();
    let options_singles = translate_player_options(&source.simply_love, &base_singles);
    let options_doubles = translate_player_options(&source.simply_love, &base_doubles);

    // Derive a stable DeadSync identity from the ITGmania profile GUID so
    // re-importing the same profile maps to the same GUID. Falls back to a fresh
    // GUID (handled by `create_local_profile_from_import`) when the source has no
    // `Stats.xml`/`Guid`.
    let profile_guid = profile_guid_from_itgmania_guid(&source.guid).unwrap_or_default();

    let data = ImportProfileData {
        display_name: &source.editable.display_name,
        weight_pounds: source.editable.weight_pounds,
        birth_year: source.editable.birth_year,
        initials: &source.editable.last_used_high_score_name,
        groovestats_api_key: &source.online.groovestats_api_key,
        groovestats_username: &source.online.groovestats_username,
        groovestats_is_pad_player: source.online.groovestats_is_pad_player,
        arrowcloud_api_key: &source.online.arrowcloud_api_key,
        ignore_step_count_calories: source.editable.ignore_step_count_calories,
        avatar_src: source.avatar_path.as_deref(),
        options_singles: &options_singles,
        options_doubles: &options_doubles,
        guid: &profile_guid,
    };
    let profile_id = create_local_profile_from_import(&data).map_err(ItgReadError::Io)?;

    let initials = {
        let sanitized = sanitize_player_initials(&source.editable.last_used_high_score_name);
        if sanitized.is_empty() {
            initials_from_name(source.editable.display_name.trim())
        } else {
            sanitized
        }
    };

    let mut summary = ImportSummary {
        profile_id: profile_id.clone(),
        display_name: source.editable.display_name.clone(),
        ..Default::default()
    };

    let mut entries: Vec<(String, LocalScoreEntry)> = Vec::new();
    let mut favorite_hashes: HashSet<String> = HashSet::new();
    {
        let packs = get_song_cache();
        let resolver = ChartResolver::build(&packs);
        let total_songs = source.songs.len();
        for (song_idx, song) in source.songs.iter().enumerate() {
            on_progress(song_idx, total_songs, song_label(&song.dir));
            for steps in &song.steps {
                for hs in &steps.high_scores {
                    summary.scores_total += 1;
                    match resolver.resolve(
                        &song.dir,
                        &steps.steps_type,
                        &steps.difficulty,
                        &steps.description,
                    ) {
                        Resolution::Found(hash) => match local_score_from_itg(hs) {
                            Some(entry) => entries.push((hash.to_string(), entry)),
                            None => summary.scores_unmapped += 1,
                        },
                        Resolution::SongNotFound => summary.charts_song_not_found += 1,
                        Resolution::ChartNotFound => summary.charts_chart_not_found += 1,
                    }
                }
            }
        }
        // Final tick so the bar reads 100% once every song is processed.
        on_progress(total_songs, total_songs, "");

        // Favorites are per-song in Simply Love but per-chart in DeadSync, so a
        // resolved song favorites all of its charts' hashes.
        for fav in &source.favorites {
            summary.favorites_total += 1;
            match resolver.resolve_song(fav) {
                Some(song) => {
                    summary.favorites_imported += 1;
                    for chart in &song.charts {
                        favorite_hashes.insert(chart.short_hash.to_string());
                    }
                }
                None => summary.favorites_song_not_found += 1,
            }
        }
    }

    summary.scores_imported = import_local_scores(&profile_id, &initials, &mut entries);
    write_imported_favorites(&profile_id, &favorite_hashes);
    write_imported_profile_stats(&profile_id, source.current_combo);
    if let Some(itl_json) = &source.itl_json {
        summary.itl_entries_imported = import_itl_json(&profile_id, itl_json);
    }
    Ok(summary)
}

/// Derives a short, human-readable label for a song from its ITGmania `Dir`
/// attribute (e.g. `"Songs/My Pack/Cool Song/"` → `"My Pack/Cool Song"`), for
/// display in the import progress bar.
fn song_label(dir: &str) -> &str {
    let trimmed = dir.trim().trim_matches(['/', '\\']);
    // Strip a leading song-root component so the label is pack/song.
    let after_root = trimmed
        .strip_prefix("Songs/")
        .or_else(|| trimmed.strip_prefix("AdditionalSongs/"))
        .unwrap_or(trimmed);
    if after_root.is_empty() {
        trimmed
    } else {
        after_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_label_strips_root_and_slashes() {
        assert_eq!(song_label("Songs/My Pack/Cool Song/"), "My Pack/Cool Song");
        assert_eq!(song_label("AdditionalSongs/Pack/Song"), "Pack/Song");
        assert_eq!(song_label("Pack/Song/"), "Pack/Song");
        assert_eq!(song_label(""), "");
    }
}
