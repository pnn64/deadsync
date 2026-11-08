// Frozen from 361286535d5458eead4c0fa73f4f345559aea936; visibility only adapted for tests.
use super::*;

pub(super) fn overflow() -> Vec<u64> {
    let mut runtime = lock_runtime();
    let mut held: Vec<(u64, u64)> = runtime
        .slots
        .iter()
        .filter(|(_, slot)| **slot == Slot::Done)
        .map(|(id, _)| (*id, runtime.used.get(id).copied().unwrap_or(0)))
        .collect();
    if held.len() <= MAX_CACHED {
        return Vec::new();
    }
    // Least recently wanted first, which is what goes.
    held.sort_unstable_by_key(|(_, used)| *used);
    let excess = held.len() - MAX_CACHED;
    let evicted: Vec<u64> = held.into_iter().take(excess).map(|(id, _)| id).collect();
    for id in &evicted {
        // Forget the slot too, so coming back to it re-fetches rather than
        // showing nothing forever.
        runtime.slots.remove(id);
        runtime.used.remove(id);
        runtime.urls.remove(id);
    }
    evicted
}
