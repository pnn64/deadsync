// Frozen from 5d780220d (0.5.1708).
use super::*;

pub(super) fn finish_catalog_request(
    generation: u64,
    result: Result<Vec<PackInfo>, StepManiaOnlineError>,
) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        log::debug!("Discarding stale StepManiaOnline catalog generation {generation}.");
        return;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    match result {
        Ok(packs) => {
            let count = packs.len();
            snapshot.phase = CatalogPhase::Ready;
            snapshot.catalog = Arc::from(packs);
            snapshot.revision = snapshot.revision.wrapping_add(1);
            snapshot.message = None;
            log::info!("Loaded {count} StepManiaOnline packs.");
        }
        Err(error) => {
            log::warn!("StepManiaOnline catalog request failed: {error}");
            snapshot.phase = CatalogPhase::Error;
            snapshot.message = Some(error.to_string());
        }
    }
    runtime.snapshot = Arc::new(snapshot);
}
