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

use deadsync_import::itg::{self, ItgReadError, ItgSource};
use deadsync_import::options::translate_player_options;
use deadsync_import::resolver::{ChartResolver, Resolution};

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
    /// Whether the source had `ITL2026.json` event data at all.
    pub itl_present: bool,
    /// Whether Simply Love player-options preferences were found and translated
    /// (vs. falling back to DeadSync defaults for a profile that never ran it).
    pub simply_love_options_imported: bool,
    /// Whether a GrooveStats API key was carried across.
    pub groovestats_imported: bool,
    /// Whether an ArrowCloud API key was carried across.
    pub arrowcloud_imported: bool,
    /// Whether an avatar image was copied into the new profile.
    pub avatar_imported: bool,
    /// Whether the user canceled mid-import. When set, the partially-created
    /// profile was deleted (clean abort) and the count fields are not meaningful.
    pub canceled: bool,
    /// Set to the existing profile's display name when the import was refused
    /// because this ITGmania profile (matched by its derived GUID) was already
    /// imported. When set, no new profile was created.
    pub already_imported_as: Option<String>,
}

impl ImportSummary {
    /// Whether GrooveStats and/or ArrowCloud credentials were carried across (so
    /// the user can pull online scores via Score Import).
    pub fn online_keys_imported(&self) -> bool {
        self.groovestats_imported || self.arrowcloud_imported
    }
}

/// Read an ITGmania profile directory and import it into a new local profile.
///
/// `on_progress(done, total, label)` is invoked as imported scores are written to
/// disk (the phase that dominates import time), so a caller can drive a progress
/// bar. `label` is currently empty. Pass a no-op closure when progress isn't
/// needed.
///
/// `should_cancel()` is polled before each score write; when it returns `true`
/// the import is cleanly aborted: the partially-created profile is deleted and a
/// summary with `canceled = true` is returned.
pub fn import_itg_profile_dir<F, C>(
    dir: &Path,
    on_progress: F,
    should_cancel: C,
) -> Result<ImportSummary, ItgReadError>
where
    F: FnMut(usize, usize, &str),
    C: Fn() -> bool,
{
    let source = itg::read_profile_dir(dir)?;
    import_from_source(&source, on_progress, should_cancel)
}

/// Import an already-read [`ItgSource`] into a new local profile. See
/// [`import_itg_profile_dir`] for the `on_progress` / `should_cancel` contract.
pub fn import_from_source<F, C>(
    source: &ItgSource,
    mut on_progress: F,
    should_cancel: C,
) -> Result<ImportSummary, ItgReadError>
where
    F: FnMut(usize, usize, &str),
    C: Fn() -> bool,
{
    let (base_singles, base_doubles) = default_local_profile_options();
    let options_singles = translate_player_options(&source.simply_love, &base_singles);
    let options_doubles = translate_player_options(&source.simply_love, &base_doubles);

    // Derive a stable DeadSync identity from the ITGmania profile GUID so
    // re-importing the same profile maps to the same GUID. Falls back to a fresh
    // GUID (handled by `create_local_profile_from_import`) when the source has no
    // `Stats.xml`/`Guid`.
    let profile_guid = profile_guid_from_itgmania_guid(&source.guid).unwrap_or_default();

    // Refuse to import a profile we've already imported: if the derived GUID
    // already identifies a local profile, report it instead of creating a
    // duplicate (which would corrupt GUID-keyed lookups).
    if !profile_guid.is_empty()
        && let Some(existing) = crate::game::profile::scan_local_profiles()
            .into_iter()
            .find(|p| p.id == profile_guid)
    {
        return Ok(ImportSummary {
            display_name: source.editable.display_name.clone(),
            already_imported_as: Some(existing.display_name),
            ..Default::default()
        });
    }

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
        simply_love_options_imported: !source.simply_love.is_empty(),
        groovestats_imported: !source.online.groovestats_api_key.trim().is_empty(),
        arrowcloud_imported: !source.online.arrowcloud_api_key.trim().is_empty(),
        avatar_imported: source.avatar_path.is_some(),
        itl_present: source.itl_json.is_some(),
        ..Default::default()
    };

    let mut entries: Vec<(String, LocalScoreEntry)> = Vec::new();
    let mut favorite_hashes: HashSet<String> = HashSet::new();
    {
        let packs = get_song_cache();
        let resolver = ChartResolver::build(&packs);
        for song in &source.songs {
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

    // Resolution above is in-memory and quick; the disk writes below dominate the
    // import time, so the progress bar tracks the per-score save phase and the
    // cancel check is polled here.
    let (written, canceled) = import_local_scores(
        &profile_id,
        &initials,
        &mut entries,
        |done, total| on_progress(done, total, ""),
        &should_cancel,
    );
    if canceled {
        // Clean abort: remove the partially-created profile so canceling leaves
        // no trace. Best-effort — a failed delete still reports as canceled.
        if let Err(e) = crate::game::profile::delete_local_profile(&profile_id) {
            log::warn!("Failed to delete canceled import profile {profile_id}: {e}");
        }
        summary.canceled = true;
        return Ok(summary);
    }
    summary.scores_imported = written;
    write_imported_favorites(&profile_id, &favorite_hashes);
    write_imported_profile_stats(&profile_id, source.current_combo);
    if let Some(itl_json) = &source.itl_json {
        summary.itl_entries_imported = import_itl_json(&profile_id, itl_json);
    }
    Ok(summary)
}
