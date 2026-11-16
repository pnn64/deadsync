use super::*;
use crate::perf::assert_no_churn;
use std::hint::black_box;


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
