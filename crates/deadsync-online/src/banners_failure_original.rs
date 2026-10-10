// Frozen starting implementations for differential tests and paired benchmarks.
use super::*;

pub(super) fn forget_failure(runtime: &mut RuntimeState, pack_id: u64) {
    if !runtime.failed.contains(&pack_id) {
        return;
    }
    let mut next = (*runtime.failed).clone();
    next.remove(&pack_id);
    runtime.failed = Arc::new(next);
}

pub(super) fn mark_failed(pack_id: u64, settled: bool) {
    let mut runtime = lock_runtime();
    let attempts = match runtime.slots.get(&pack_id) {
        Some(Slot::Failed { attempts, .. } | Slot::Pending { attempts }) => {
            attempts.saturating_add(1)
        }
        _ => 1,
    };
    let spent = attempts >= MAX_ATTEMPTS;
    let retry_at = (!settled).then(|| {
        let gap = if spent { LONG_RETRY } else { RETRY_AFTER };
        Instant::now() + gap
    });
    runtime
        .slots
        .insert(pack_id, Slot::Failed { attempts, retry_at });

    if (settled || spent) && !runtime.failed.contains(&pack_id) {
        let mut next = (*runtime.failed).clone();
        next.insert(pack_id);
        runtime.failed = Arc::new(next);
    }
}
