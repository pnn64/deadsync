use std::collections::HashSet;

use deadsync_chart::SongPack;
use deadsync_profile::{
    ImportProfileData, PlayerOptionsData, initials_from_name, profile_guid_from_itgmania_guid,
    sanitize_player_initials,
};
use deadsync_score::{LocalScoreEntry, local_score_from_itg};

use crate::itg::{ItgReadError, ItgSource};
use crate::options::translate_player_options;
use crate::resolver::{ChartResolver, Resolution};

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

pub struct PreparedImport {
    pub profile_guid: String,
    pub initials: String,
    pub options_singles: PlayerOptionsData,
    pub options_doubles: PlayerOptionsData,
    pub summary: ImportSummary,
    pub score_entries: Vec<(String, LocalScoreEntry)>,
    pub favorite_hashes: HashSet<String>,
}

pub fn prepare_import(
    source: &ItgSource,
    base_singles: &PlayerOptionsData,
    base_doubles: &PlayerOptionsData,
    packs: &[SongPack],
) -> PreparedImport {
    let options_singles = translate_player_options(&source.simply_love, base_singles);
    let options_doubles = translate_player_options(&source.simply_love, base_doubles);
    let profile_guid = profile_guid_from_itgmania_guid(&source.guid).unwrap_or_default();
    let initials = import_initials(source);
    let mut summary = ImportSummary {
        display_name: source.editable.display_name.clone(),
        simply_love_options_imported: !source.simply_love.is_empty(),
        groovestats_imported: !source.online.groovestats_api_key.trim().is_empty(),
        arrowcloud_imported: !source.online.arrowcloud_api_key.trim().is_empty(),
        avatar_imported: source.avatar_path.is_some(),
        itl_present: source.itl_json.is_some(),
        ..Default::default()
    };

    let resolver = ChartResolver::build(packs);
    let score_entries = collect_score_entries(source, &resolver, &mut summary);
    let favorite_hashes = collect_favorite_hashes(source, &resolver, &mut summary);

    PreparedImport {
        profile_guid,
        initials,
        options_singles,
        options_doubles,
        summary,
        score_entries,
        favorite_hashes,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_import<
    ExistingProfile,
    CreateProfile,
    ImportScores,
    DeleteProfile,
    WriteFavorites,
    WriteStats,
    ImportItl,
>(
    source: &ItgSource,
    base_singles: &PlayerOptionsData,
    base_doubles: &PlayerOptionsData,
    packs: &[SongPack],
    mut existing_profile_name: ExistingProfile,
    mut create_profile: CreateProfile,
    mut import_scores: ImportScores,
    mut delete_profile: DeleteProfile,
    mut write_favorites: WriteFavorites,
    mut write_stats: WriteStats,
    mut import_itl: ImportItl,
) -> Result<ImportSummary, ItgReadError>
where
    ExistingProfile: FnMut(&str) -> Option<String>,
    CreateProfile: FnMut(&ImportProfileData<'_>) -> Result<String, std::io::Error>,
    ImportScores: FnMut(&str, &str, Vec<(String, LocalScoreEntry)>) -> (usize, bool),
    DeleteProfile: FnMut(&str),
    WriteFavorites: FnMut(&str, &HashSet<String>),
    WriteStats: FnMut(&str, u32),
    ImportItl: FnMut(&str, &str) -> usize,
{
    let mut prepared = prepare_import(source, base_singles, base_doubles, packs);

    if !prepared.profile_guid.is_empty()
        && let Some(existing) = existing_profile_name(&prepared.profile_guid)
    {
        return Ok(ImportSummary {
            display_name: source.editable.display_name.clone(),
            already_imported_as: Some(existing),
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
        options_singles: &prepared.options_singles,
        options_doubles: &prepared.options_doubles,
        guid: &prepared.profile_guid,
    };
    let profile_id = create_profile(&data).map_err(ItgReadError::Io)?;
    prepared.summary.profile_id = profile_id.clone();

    let (written, canceled) = import_scores(
        &profile_id,
        &prepared.initials,
        std::mem::take(&mut prepared.score_entries),
    );
    if canceled {
        delete_profile(&profile_id);
        prepared.summary.canceled = true;
        return Ok(prepared.summary);
    }

    prepared.summary.scores_imported = written;
    write_favorites(&profile_id, &prepared.favorite_hashes);
    write_stats(&profile_id, source.current_combo);
    if let Some(itl_json) = &source.itl_json {
        prepared.summary.itl_entries_imported = import_itl(&profile_id, itl_json);
    }
    Ok(prepared.summary)
}

fn import_initials(source: &ItgSource) -> String {
    let sanitized = sanitize_player_initials(&source.editable.last_used_high_score_name);
    if sanitized.is_empty() {
        initials_from_name(source.editable.display_name.trim())
    } else {
        sanitized
    }
}

fn collect_score_entries(
    source: &ItgSource,
    resolver: &ChartResolver<'_>,
    summary: &mut ImportSummary,
) -> Vec<(String, LocalScoreEntry)> {
    let mut entries = Vec::new();
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
    entries
}

fn collect_favorite_hashes(
    source: &ItgSource,
    resolver: &ChartResolver<'_>,
    summary: &mut ImportSummary,
) -> HashSet<String> {
    let mut favorite_hashes = HashSet::new();
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
    favorite_hashes
}
