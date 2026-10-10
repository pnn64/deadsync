// Frozen from db09a02d3; only module paths are adapted.
#![allow(dead_code)]

pub mod resolver {
    //! Resolves an `ITGmania` score key (`Song Dir` + `StepsType` + `Difficulty`) to a
    //! `DeadSync` `GrooveStats` `short_hash`, using the already-scanned song database.
    //!
    //! `ITGmania` `Stats.xml` does not store the `GrooveStats` hash for normal charts —
    //! it identifies a chart by its on-disk song directory plus the steps type and
    //! difficulty. `DeadSync` keys local scores by `short_hash`, so to import a score
    //! we must locate the same chart in `DeadSync`'s library and read its hash. Charts
    //! that aren't present in the library can't be resolved (we have no hash for
    //! them) and are reported as skipped.

    use hashbrown::{Equivalent, HashMap};
    use rustc_hash::FxBuildHasher;
    use std::hash::{Hash, Hasher};

    use deadsync_chart::{SongData, SongPack};

    #[derive(PartialEq, Eq)]
    struct SongKey<'a> {
        pack: &'a str,
        folder: &'a str,
    }

    impl Hash for SongKey<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            hash_ascii_case_insensitive(self.pack, state);
            hash_ascii_case_insensitive(self.folder, state);
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

    impl Equivalent<SongKey<'_>> for SongKeyRef<'_> {
        fn equivalent(&self, key: &SongKey<'_>) -> bool {
            self.pack.eq_ignore_ascii_case(key.pack) && self.folder.eq_ignore_ascii_case(key.folder)
        }
    }

    fn hash_ascii_case_insensitive<H: Hasher>(value: &str, state: &mut H) {
        value.len().hash(state);
        for byte in value.bytes() {
            byte.to_ascii_lowercase().hash(state);
        }
    }

    /// Builds a fast lookup over the scanned song library and resolves `ITGmania`
    /// score keys to `DeadSync` chart hashes.
    pub struct ChartResolver<'a> {
        /// Borrowed, ASCII-case-insensitive `(pack, song_folder)` lookup.
        by_song: HashMap<SongKey<'a>, &'a SongData, FxBuildHasher>,
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
        #[must_use]
        pub fn build(packs: &'a [SongPack]) -> Self {
            let mut by_song =
                HashMap::with_capacity_and_hasher(resolver_entry_capacity(packs), FxBuildHasher);
            for pack in packs {
                for pack_key in pack_keys(pack) {
                    for song in &pack.songs {
                        let song: &'a SongData = song.as_ref();
                        if let Some(folder) = song_folder_name(song) {
                            by_song
                                .entry(SongKey {
                                    pack: pack_key,
                                    folder,
                                })
                                .or_insert(song);
                        }
                    }
                }
            }
            Self { by_song }
        }

        /// Resolves an `ITGmania` song directory key (e.g. `"Pack/Song"` from
        /// `favorites.txt`, or a `Stats.xml` `Dir`) to the matching library song.
        /// Returns `None` when the song isn't in `DeadSync`'s scanned library.
        #[must_use]
        pub fn resolve_song(&self, song_dir: &str) -> Option<&'a SongData> {
            self.song_for_dir(song_dir)
        }

        /// Resolves a score key to a chart `short_hash`.
        #[must_use]
        /// # Panics
        ///
        /// Panics if an internal state invariant is violated.
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

    /// Borrowed keys a pack can be addressed by (group and folder name).
    fn pack_keys(pack: &SongPack) -> impl Iterator<Item = &str> {
        let group = (!pack.group_name.is_empty()).then_some(pack.group_name.as_str());
        let directory = pack
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|directory| group.is_none_or(|group| !directory.eq_ignore_ascii_case(group)));
        group.into_iter().chain(directory)
    }

    fn resolver_entry_capacity(packs: &[SongPack]) -> usize {
        packs.iter().fold(0usize, |capacity, pack| {
            let aliases = pack_keys(pack).count();
            let songs = pack
                .songs
                .iter()
                .filter(|song| song_folder_name(song).is_some())
                .count();
            capacity.saturating_add(aliases.saturating_mul(songs))
        })
    }

    /// The song's on-disk folder name (the parent directory of its simfile).
    fn song_folder_name(song: &SongData) -> Option<&str> {
        song.simfile_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
    }

    /// Normalizes an `ITGmania` `Dir` attribute (e.g. `"Songs/Pack/Song/"`) into a
    /// `(pack_lower, song_folder_lower)` pair. Leading `Songs/` / `AdditionalSongs/`
    /// roots and surrounding slashes are stripped. Returns `None` if the path
    /// doesn't have at least a pack and a song component.
    #[must_use]
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
        let pack = if first.eq_ignore_ascii_case("Songs")
            || first.eq_ignore_ascii_case("AdditionalSongs")
        {
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
}

pub mod pipeline {
    use std::collections::HashSet;

    use deadsync_chart::SongPack;
    use deadsync_profile::{
        ImportProfileData, PlayerOptionsData, initials_from_name, profile_guid_from_itgmania_guid,
        sanitize_player_initials,
    };
    use deadsync_score::{LocalScoreEntry, local_score_from_itg};

    use super::resolver::{ChartResolver, Resolution};
    use crate::itg::{ItgReadError, ItgSource};
    use crate::options::translate_player_options;

    /// Result of importing one `ITGmania` profile.
    #[derive(Debug, Default, Clone)]
    pub struct ImportSummary {
        /// New `DeadSync` local profile id.
        pub profile_id: String,
        pub display_name: String,
        /// Total high-score records found in `Stats.xml`.
        pub scores_total: usize,
        /// Plays successfully written to the new profile.
        pub scores_imported: usize,
        /// Records skipped because the song wasn't in `DeadSync`'s library.
        pub charts_song_not_found: usize,
        /// Records skipped because the chart (type/difficulty/edit) wasn't found.
        pub charts_chart_not_found: usize,
        /// Records whose grade/percent couldn't be mapped to a `DeadSync` play.
        pub scores_unmapped: usize,
        /// Total favorited songs found in `favorites.txt`.
        pub favorites_total: usize,
        /// Favorited songs matched to a library song and imported.
        pub favorites_imported: usize,
        /// Favorited songs skipped because the song wasn't in `DeadSync`'s library.
        pub favorites_song_not_found: usize,
        /// ITL `hashMap` entries imported from `ITL2026.json` (0 if absent).
        pub itl_entries_imported: usize,
        /// Whether the source had `ITL2026.json` event data at all.
        pub itl_present: bool,
        /// Whether Simply Love player-options preferences were found and translated
        /// (vs. falling back to `DeadSync` defaults for a profile that never ran it).
        pub simply_love_options_imported: bool,
        /// Whether a `GrooveStats` API key was carried across.
        pub groovestats_imported: bool,
        /// Whether an `ArrowCloud` API key was carried across.
        pub arrowcloud_imported: bool,
        /// Whether an avatar image was copied into the new profile.
        pub avatar_imported: bool,
        /// Whether the user canceled mid-import. When set, the partially-created
        /// profile was deleted (clean abort) and the count fields are not meaningful.
        pub canceled: bool,
        /// Set to the existing profile's display name when the import was refused
        /// because this `ITGmania` profile (matched by its derived GUID) was already
        /// imported. When set, no new profile was created.
        pub already_imported_as: Option<String>,
    }

    impl ImportSummary {
        /// Whether `GrooveStats` and/or `ArrowCloud` credentials were carried across (so
        /// the user can pull online scores via Score Import).
        #[must_use]
        pub const fn online_keys_imported(&self) -> bool {
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

    #[must_use]
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
        prepared.summary.profile_id.clone_from(&profile_id);

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
        let mut entries = Vec::with_capacity(source.total_high_scores());
        for song in &source.songs {
            for steps in &song.steps {
                if steps.high_scores.is_empty() {
                    continue;
                }
                let resolution = resolver.resolve(
                    &song.dir,
                    &steps.steps_type,
                    &steps.difficulty,
                    &steps.description,
                );
                for high_score in &steps.high_scores {
                    collect_resolved_score(&mut entries, summary, high_score, &resolution);
                }
            }
        }
        entries
    }

    fn collect_resolved_score(
        entries: &mut Vec<(String, LocalScoreEntry)>,
        summary: &mut ImportSummary,
        high_score: &deadsync_score::ImportedHighScore,
        resolution: &Resolution<'_>,
    ) {
        summary.scores_total += 1;
        match resolution {
            Resolution::Found(hash) => match local_score_from_itg(high_score) {
                Some(entry) => entries.push(((*hash).to_owned(), entry)),
                None => summary.scores_unmapped += 1,
            },
            Resolution::SongNotFound => summary.charts_song_not_found += 1,
            Resolution::ChartNotFound => summary.charts_chart_not_found += 1,
        }
    }

    fn collect_favorite_hashes(
        source: &ItgSource,
        resolver: &ChartResolver<'_>,
        summary: &mut ImportSummary,
    ) -> HashSet<String> {
        let mut resolved_songs = Vec::with_capacity(source.favorites.len());
        let mut chart_capacity = 0usize;
        for favorite in &source.favorites {
            summary.favorites_total += 1;
            match resolver.resolve_song(favorite) {
                Some(song) => {
                    summary.favorites_imported += 1;
                    chart_capacity = chart_capacity.saturating_add(song.charts.len());
                    resolved_songs.push(song);
                }
                None => summary.favorites_song_not_found += 1,
            }
        }
        let mut favorite_hashes = HashSet::with_capacity(chart_capacity);
        for song in resolved_songs {
            for chart in &song.charts {
                favorite_hashes.insert(chart.short_hash.to_string());
            }
        }
        favorite_hashes
    }
}
