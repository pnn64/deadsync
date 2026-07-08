//! Orchestration: turn an ITGmania `LocalProfiles/<id>/` directory into a brand
//! new DeadSync local profile (metadata, online keys, avatar, Simply Love player
//! options) plus its offline high-score history.

use std::path::Path;

use crate::game::profile::{
    create_local_profile_from_import, default_local_profile_options, write_imported_favorites,
    write_imported_profile_stats,
};
use crate::game::scores::{import_itl_json, import_local_scores};
use crate::game::song::get_song_cache;

use deadsync_import::itg::{self, ItgReadError, ItgSource};
pub use deadsync_import::pipeline::ImportSummary;
use deadsync_import::pipeline::run_import;

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
    let packs = get_song_cache();
    run_import(
        source,
        &base_singles,
        &base_doubles,
        &packs,
        |profile_guid| {
            crate::game::profile::scan_local_profiles()
                .into_iter()
                .find(|profile| profile.id == profile_guid)
                .map(|profile| profile.display_name)
        },
        |data| create_local_profile_from_import(data),
        |profile_id, initials, mut entries| {
            import_local_scores(
                profile_id,
                initials,
                &mut entries,
                |done, total| on_progress(done, total, ""),
                &should_cancel,
            )
        },
        |profile_id| {
            if let Err(e) = crate::game::profile::delete_local_profile(profile_id) {
                log::warn!("Failed to delete canceled import profile {profile_id}: {e}");
            }
        },
        write_imported_favorites,
        write_imported_profile_stats,
        import_itl_json,
    )
}
