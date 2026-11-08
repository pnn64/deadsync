use super::*;
use crate::perf::{assert_no_churn, measure};
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

struct Restore(RuntimeState);

impl Restore {
    fn save() -> Self {
        Self(std::mem::take(&mut *lock_runtime()))
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        *lock_runtime() = std::mem::take(&mut self.0);
    }
}

fn fixture(done: usize, pending: usize, failed: usize, ties: bool) -> RuntimeState {
    let mut state = RuntimeState {
        clock: u64::MAX,
        ..RuntimeState::default()
    };
    for id in 0..(done + pending + failed) as u64 {
        let slot = if id < done as u64 {
            Slot::Done
        } else if id < (done + pending) as u64 {
            Slot::Pending { attempts: 2 }
        } else {
            Slot::Failed {
                attempts: 3,
                retry_at: None,
            }
        };
        state.slots.insert(id, slot);
        if id % 5 != 0 {
            state.used.insert(id, if ties { 7 } else { id });
        }
        state
            .urls
            .insert(id, format!("https://example.test/{id}.png"));
    }
    state.failed = Arc::new(((done + pending) as u64..(done + pending + failed) as u64).collect());
    state
}

fn copy_state(state: &RuntimeState) -> RuntimeState {
    // Cloning preserves each map's seed and iteration order, including LRU ties.
    RuntimeState {
        slots: state.slots.clone(),
        used: state.used.clone(),
        clock: state.clock,
        failed: Arc::clone(&state.failed),
        urls: state.urls.clone(),
        sender: None,
        ready: None,
    }
}

fn assert_same(left: &RuntimeState, right: &RuntimeState) {
    assert_eq!(left.slots, right.slots);
    assert_eq!(left.used, right.used);
    assert_eq!(left.urls, right.urls);
    assert_eq!(left.clock, right.clock);
    assert!(Arc::ptr_eq(&left.failed, &right.failed));
}

#[test]
fn overflow_matches_original_at_bounds_with_pending_failures_and_lru_ties() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for done in [
        0,
        1,
        24,
        80,
        MAX_CACHED - 1,
        MAX_CACHED,
        MAX_CACHED + 1,
        512,
    ] {
        for (pending, failed) in [(0, 0), (4, 7), (200, 200)] {
            for ties in [false, true] {
                let state = fixture(done, pending, failed, ties);
                *lock_runtime() = copy_state(&state);
                let expected = original::overflow();
                let after = std::mem::replace(&mut *lock_runtime(), copy_state(&state));
                assert_eq!(overflow(), expected);
                assert_same(&lock_runtime(), &after);
                assert_eq!(expected.len(), done.saturating_sub(MAX_CACHED));
                assert!(overflow().is_empty());
            }
        }
    }
}

#[test]
fn bounded_cache_keeps_channels_failure_snapshot_and_clock() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    let mut state = fixture(80, 40, 20, true);
    let expected = copy_state(&state);
    let (sender, jobs) = sync_channel(1);
    let (finished, ready) = sync_channel(1);
    state.sender = Some(sender);
    state.ready = Some(ready);
    *lock_runtime() = state;
    assert!(overflow().is_empty());
    let state = lock_runtime();
    assert_same(&state, &expected);
    assert!(
        state
            .sender
            .as_ref()
            .unwrap()
            .try_send(Job {
                pack_id: 123,
                url: "kept".into(),
            })
            .is_ok()
    );
    assert_eq!(jobs.try_recv().unwrap().pack_id, 123);
    finished
        .try_send(FetchedBanner {
            pack_id: 321,
            bytes: Arc::from([1_u8, 2]),
        })
        .unwrap();
    assert_eq!(
        state.ready.as_ref().unwrap().try_recv().unwrap().pack_id,
        321
    );
}

#[test]
fn bounded_cache_has_no_allocation_churn_even_with_sparse_capacity() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for done in [0, 24, 80, MAX_CACHED] {
        let mut state = fixture(done, 0, 0, false);
        state.slots.reserve(4096);
        *lock_runtime() = state;
        assert_no_churn(|| assert!(black_box(overflow()).is_empty()));
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_banner_overflow() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for (label, done, pending, failed) in [
        ("empty", 0, 0, 0),
        ("24", 24, 0, 0),
        ("80", 80, 0, 0),
        ("192", 192, 0, 0),
        ("mixed-192", 80, 56, 56),
        ("pending-400", 0, 200, 200),
        ("done-80-total-480", 80, 200, 200),
    ] {
        *lock_runtime() = fixture(done, pending, failed, false);
        let (before, old) = measure(original::overflow);
        let (after, new) = measure(overflow);
        assert_eq!(before, after);
        println!("banner-{label} churn: original {old:?}, current {new:?}");
        paired_bench::compare(&format!("banner-{label}"), 1000, |current| {
            black_box(if current {
                overflow()
            } else {
                original::overflow()
            });
        });
    }
    // Actual eviction consumes its state. Reset outside each timed call so the
    // reported control includes the same timer overhead, but no fixture cloning.
    for done in [193, 512] {
        let state = fixture(done, 30, 20, true);
        *lock_runtime() = copy_state(&state);
        let (before, old) = measure(original::overflow);
        *lock_runtime() = copy_state(&state);
        let (after, new) = measure(overflow);
        assert_eq!(before, after);
        assert_eq!(old, new);
        println!("banner-evict-{done} churn: original {old:?}, current {new:?}");
        let mut samples = [Vec::new(), Vec::new()];
        for sample in 0..10 {
            for variant in [sample % 2, 1 - sample % 2] {
                let mut elapsed = Duration::ZERO;
                for _ in 0..200 {
                    *lock_runtime() = copy_state(&state);
                    let start = Instant::now();
                    black_box(if black_box(variant == 1) {
                        overflow()
                    } else {
                        original::overflow()
                    });
                    elapsed += start.elapsed();
                }
                if sample > 0 {
                    samples[variant].push(elapsed.as_nanos() as f64 / 200.0);
                }
            }
        }
        for sample in &mut samples {
            sample.sort_by(f64::total_cmp);
        }
        let (old, new) = (samples[0][4], samples[1][4]);
        println!(
            "banner-evict-{done}: original {old:.2} ns/op, current {new:.2} ns/op, {:.2}x throughput",
            old / new
        );
    }
}
