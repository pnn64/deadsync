//! App-facing ITG profile import orchestration.
//!
//! The import crate owns source reading and import flow. Callers supply the
//! root-level effects that actually create profiles and write scores.

use std::collections::HashSet;
use std::path::Path;

use deadsync_chart::SongPack;
use deadsync_profile::{ImportProfileData, PlayerOptionsData};
use deadsync_score::LocalScoreEntry;

use crate::itg::{self, ItgReadError, ItgSource};
pub use crate::pipeline::ImportSummary;
use crate::pipeline::run_import;

#[allow(clippy::too_many_arguments)]
pub fn import_itg_profile_dir<
    ExistingProfile,
    CreateProfile,
    ImportScores,
    DeleteProfile,
    WriteFavorites,
    WriteStats,
    ImportItl,
>(
    dir: &Path,
    base_singles: &PlayerOptionsData,
    base_doubles: &PlayerOptionsData,
    packs: &[SongPack],
    existing_profile_name: ExistingProfile,
    create_profile: CreateProfile,
    import_scores: ImportScores,
    delete_profile: DeleteProfile,
    write_favorites: WriteFavorites,
    write_stats: WriteStats,
    import_itl: ImportItl,
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
    let source = itg::read_profile_dir(dir)?;
    import_from_source(
        &source,
        base_singles,
        base_doubles,
        packs,
        existing_profile_name,
        create_profile,
        import_scores,
        delete_profile,
        write_favorites,
        write_stats,
        import_itl,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn import_from_source<
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
    existing_profile_name: ExistingProfile,
    create_profile: CreateProfile,
    import_scores: ImportScores,
    delete_profile: DeleteProfile,
    write_favorites: WriteFavorites,
    write_stats: WriteStats,
    import_itl: ImportItl,
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
    run_import(
        source,
        base_singles,
        base_doubles,
        packs,
        existing_profile_name,
        create_profile,
        import_scores,
        delete_profile,
        write_favorites,
        write_stats,
        import_itl,
    )
}
