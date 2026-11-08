use super::banners_failure_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;
use std::sync::Weak;

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

enum Reader {
    None,
    Strong(Arc<HashSet<u64>>),
    Weak(Weak<HashSet<u64>>),
}

fn fixture(count: usize) -> RuntimeState {
    let mut state = RuntimeState {
        failed: Arc::new((0..count as u64).collect()),
        clock: 87,
        ..RuntimeState::default()
    };
    state.slots.reserve(count + 64);
    for id in 0..count as u64 {
        state.slots.insert(
            id,
            Slot::Failed {
                attempts: MAX_ATTEMPTS,
                retry_at: None,
            },
        );
    }
    state.urls.insert(0, "https://example.test/zero.png".into());
    state.used.insert(0, 17);
    state
}

fn copy_state(state: &RuntimeState, mode: &str) -> (RuntimeState, Reader) {
    let failed = Arc::new((*state.failed).clone());
    let reader = match mode {
        "shared" => Reader::Strong(Arc::clone(&failed)),
        "weak" => Reader::Weak(Arc::downgrade(&failed)),
        _ => Reader::None,
    };
    (
        RuntimeState {
            slots: state.slots.clone(),
            used: state.used.clone(),
            urls: state.urls.clone(),
            clock: state.clock,
            failed,
            sender: None,
            ready: None,
        },
        reader,
    )
}

fn assert_reader(reader: &Reader, expected: &HashSet<u64>, changed: bool) {
    match reader {
        Reader::None => {}
        Reader::Strong(value) => assert_eq!(value.as_ref(), expected),
        Reader::Weak(value) if changed => assert!(value.upgrade().is_none()),
        Reader::Weak(value) => assert_eq!(value.upgrade().unwrap().as_ref(), expected),
    }
}

fn assert_same(left: &RuntimeState, right: &RuntimeState) {
    assert_eq!(left.slots, right.slots);
    assert_eq!(left.used, right.used);
    assert_eq!(left.urls, right.urls);
    assert_eq!(left.clock, right.clock);
    assert_eq!(left.failed, right.failed);
    assert_eq!(
        left.failed.iter().collect::<Vec<_>>(),
        right.failed.iter().collect::<Vec<_>>()
    );
}

#[test]
fn failure_updates_preserve_owned_shared_and_weak_snapshot_contents() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for count in [0, 1, 24, 512] {
        let base = fixture(count);
        for mode in ["unique", "shared", "weak"] {
            let (mut old, old_reader) = copy_state(&base, mode);
            let (mut new, new_reader) = copy_state(&base, mode);
            original::forget_failure(&mut old, 0);
            forget_failure(&mut new, 0);
            assert_same(&new, &old);
            assert_reader(&old_reader, &base.failed, count != 0);
            assert_reader(&new_reader, &base.failed, count != 0);

            let (old, old_reader) = copy_state(&base, mode);
            *lock_runtime() = old;
            original::mark_failed(9000, true);
            let old = std::mem::take(&mut *lock_runtime());
            let (new, new_reader) = copy_state(&base, mode);
            *lock_runtime() = new;
            mark_failed(9000, true);
            let new = std::mem::take(&mut *lock_runtime());
            assert_same(&new, &old);
            assert_reader(&old_reader, &base.failed, true);
            assert_reader(&new_reader, &base.failed, true);
        }
    }
}

#[test]
fn absent_removals_and_duplicate_failures_keep_snapshot_identity() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for current in [false, true] {
        let mut state = fixture(24);
        let held = Arc::clone(&state.failed);
        let (_, churn) = measure(|| {
            if current {
                forget_failure(&mut state, 9000);
            } else {
                original::forget_failure(&mut state, 9000);
            }
        });
        assert_eq!(churn, crate::perf::Churn::default());
        assert!(Arc::ptr_eq(&state.failed, &held));
        *lock_runtime() = state;
        let (_, churn) = measure(|| {
            if current {
                mark_failed(0, true);
            } else {
                original::mark_failed(0, true);
            }
        });
        assert_eq!(churn, crate::perf::Churn::default());
        assert!(Arc::ptr_eq(&lock_runtime().failed, &held));
    }
}

#[test]
fn banner_retry_counts_deadlines_and_publication_rules_are_unchanged() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for prior in [
        None,
        Some(Slot::Done),
        Some(Slot::Pending { attempts: 1 }),
        Some(Slot::Pending {
            attempts: MAX_ATTEMPTS - 1,
        }),
        Some(Slot::Failed {
            attempts: u8::MAX,
            retry_at: None,
        }),
    ] {
        for settled in [false, true] {
            let mut observations = Vec::new();
            for current in [false, true] {
                let mut state = fixture(24);
                if let Some(prior) = prior {
                    state.slots.insert(9000, prior);
                }
                *lock_runtime() = state;
                let start = Instant::now();
                if current {
                    mark_failed(9000, settled);
                } else {
                    original::mark_failed(9000, settled);
                }
                let end = Instant::now();
                let state = lock_runtime();
                let Slot::Failed { attempts, retry_at } = state.slots[&9000] else {
                    panic!("failed slot")
                };
                if let Some(deadline) = retry_at {
                    let gap = if attempts >= MAX_ATTEMPTS {
                        LONG_RETRY
                    } else {
                        RETRY_AFTER
                    };
                    assert!((start + gap..=end + gap).contains(&deadline));
                }
                observations.push((attempts, retry_at.is_some(), state.failed.contains(&9000)));
                assert_eq!(state.clock, 87);
                assert_eq!(state.used.get(&0), Some(&17));
                assert_eq!(
                    state.urls.get(&0).map(String::as_str),
                    Some("https://example.test/zero.png")
                );
            }
            assert_eq!(observations[0], observations[1]);
        }
    }
}

fn updates(current: bool, operation: &str, first: u64, count: usize) {
    if operation == "remove" {
        let mut state = lock_runtime();
        for id in first..first + count as u64 {
            if current {
                forget_failure(&mut state, id);
            } else {
                original::forget_failure(&mut state, id);
            }
        }
    } else {
        for id in first..first + count as u64 {
            if current {
                mark_failed(id, true);
            } else {
                original::mark_failed(id, true);
            }
        }
    }
}

fn install(base: &RuntimeState, mode: &str) -> Reader {
    let (state, reader) = copy_state(base, mode);
    *lock_runtime() = state;
    reader
}

#[test]
fn unique_failure_batches_remove_table_and_arc_allocation_churn() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    let base = fixture(512);
    for mode in ["unique", "shared", "weak"] {
        for (operation, first, count) in [("insert", 9000, 32), ("remove", 0, 32)] {
            let old_reader = install(&base, mode);
            let (_, before) = measure(|| updates(false, operation, first, count));
            let old = std::mem::take(&mut *lock_runtime());
            let new_reader = install(&base, mode);
            let (_, after) = measure(|| updates(true, operation, first, count));
            assert_same(&lock_runtime(), &old);
            assert!(after.allocs < before.allocs);
            assert!(after.allocated_bytes < before.allocated_bytes);
            if mode == "unique" {
                assert_eq!(after, crate::perf::Churn::default());
            }
            assert_reader(&old_reader, &base.failed, true);
            assert_reader(&new_reader, &base.failed, true);
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_banner_failure_snapshots() {
    let _serial = super::tests::exclusively();
    let _restore = Restore::save();
    for (size, mode, operation, first, count) in [
        (0, "unique", "insert", 9000, 32),
        (0, "shared", "insert", 9000, 1),
        (24, "shared", "insert", 9000, 1),
        (512, "unique", "insert", 9000, 1),
        (512, "shared", "insert", 9000, 1),
        (512, "weak", "insert", 9000, 1),
        (512, "unique", "insert", 9000, 32),
        (512, "shared", "insert", 9000, 32),
        (512, "shared", "duplicate", 0, 1),
        (512, "unique", "remove", 0, 32),
        (512, "shared", "remove", 0, 1),
        (4096, "shared", "remove", 0, 32),
    ] {
        let base = fixture(size);
        let label = format!("failure-{size}-{mode}-{operation}-{count}");
        let old_reader = install(&base, mode);
        let (_, before) = measure(|| updates(false, operation, first, count));
        let old = std::mem::take(&mut *lock_runtime());
        let new_reader = install(&base, mode);
        let (_, after) = measure(|| updates(true, operation, first, count));
        assert_same(&lock_runtime(), &old);
        println!("{label} churn: original {before:?}, current {after:?}");
        drop((old_reader, new_reader, old));

        // Reset the singleton and its reader outside each measured call.
        // Include identical timer/lock overhead on both sides, never fixture copying.
        let iterations = if count == 1 { 500 } else { 100 };
        let mut samples = [Vec::with_capacity(9), Vec::with_capacity(9)];
        for sample in 0..10 {
            for variant in [sample % 2, 1 - sample % 2] {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iterations {
                    let reader = install(&base, mode);
                    let start = Instant::now();
                    updates(
                        black_box(variant == 1),
                        black_box(operation),
                        black_box(first),
                        count,
                    );
                    elapsed += start.elapsed();
                    drop(reader);
                }
                if sample != 0 {
                    samples[variant].push(elapsed.as_nanos() as f64 / iterations as f64);
                }
            }
        }
        for values in &mut samples {
            values.sort_by(f64::total_cmp);
        }
        let before = samples[0][4];
        let after = samples[1][4];
        println!(
            "{label}: original {before:.2} ns/op, current {after:.2} ns/op, {:.2}x throughput",
            before / after
        );
    }
}
