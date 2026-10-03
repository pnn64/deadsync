use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled_with_setup};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/install_queue/baseline.rs"
    ));
}

fn pack(id: u64) -> PackInfo {
    PackInfo::new(
        id,
        "Fixture Pack".into(),
        100,
        12345,
        None,
        None,
        None,
        None,
    )
}

fn runtime(count: usize, capacity: usize, phase: InstallPhase) -> RuntimeState {
    let mut installs = Vec::with_capacity(capacity.max(count));
    installs.extend((0..count).map(|i| InstallSnapshot {
        pack_id: i as u64,
        phase,
        downloaded_bytes: i as u64 * 17,
        total_bytes: 999,
        message: Some(format!("History {i} {}", "x".repeat(128))),
    }));
    RuntimeState {
        generation: 7,
        snapshot: Arc::new(Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from([pack(0)]),
            revision: 9,
            message: Some("Catalog status unchanged".into()),
            installs,
        }),
        ready_song_dirs: vec![PathBuf::from("songs/already-ready")],
    }
}

#[test]
fn install_queue_preserves_rejections_retries_evictions_and_snapshot_readers() {
    for count in [0, 1, 8, MAX_INSTALLS, MAX_INSTALLS + 1] {
        for capacity in [count, count * 2, 4096] {
            for phase in [
                InstallPhase::Queued,
                InstallPhase::Downloading,
                InstallPhase::Extracting,
                InstallPhase::Installed,
                InstallPhase::Error,
            ] {
                for target in [0, count.saturating_sub(1) as u64, 1000] {
                    for shared in [false, true] {
                        let mut old = runtime(count, capacity, phase);
                        let mut new = runtime(count, capacity, phase);
                        let old_observer = shared.then(|| Arc::clone(&old.snapshot));
                        let new_observer = shared.then(|| Arc::clone(&new.snapshot));
                        let old_weak = Arc::downgrade(&old.snapshot);
                        let new_weak = Arc::downgrade(&new.snapshot);
                        let old_catalog = Arc::clone(&old.snapshot.catalog);
                        let new_catalog = Arc::clone(&new.snapshot.catalog);
                        let pack = pack(target);
                        let a = baseline::queue_install_snapshot(&mut old, &pack);
                        let b = queue_install_snapshot(&mut new, &pack);
                        assert_eq!(
                            a, b,
                            "count={count}, capacity={capacity}, phase={phase:?}, target={target}, shared={shared}"
                        );
                        assert_eq!(old.snapshot, new.snapshot);
                        assert_eq!(old.generation, new.generation);
                        assert_eq!(old.ready_song_dirs, new.ready_song_dirs);
                        assert!(Arc::ptr_eq(&old_catalog, &old.snapshot.catalog));
                        assert!(Arc::ptr_eq(&new_catalog, &new.snapshot.catalog));
                        assert_eq!(old_observer, new_observer);
                        assert_eq!(old_weak.upgrade().is_some(), new_weak.upgrade().is_some());
                        assert!(
                            new.snapshot.installs.capacity() <= old.snapshot.installs.capacity(),
                            "capacity count={count}, initial={capacity}, phase={phase:?}, target={target}, shared={shared}; old={}, new={}",
                            old.snapshot.installs.capacity(),
                            new.snapshot.installs.capacity()
                        );
                        if a.is_err() {
                            assert!(old_weak.upgrade().is_some());
                            assert!(new_weak.upgrade().is_some());
                        }
                    }
                }
            }
        }
    }
    // Eviction chooses the first terminal record; retries choose the first ID.
    let mut old = runtime(MAX_INSTALLS, MAX_INSTALLS, InstallPhase::Downloading);
    Arc::get_mut(&mut old.snapshot).unwrap().installs[3].phase = InstallPhase::Error;
    Arc::get_mut(&mut old.snapshot).unwrap().installs[7].phase = InstallPhase::Installed;
    let mut new = RuntimeState {
        generation: old.generation,
        snapshot: Arc::new((*old.snapshot).clone()),
        ready_song_dirs: old.ready_song_dirs.clone(),
    };
    let p = pack(1000);
    assert_eq!(
        baseline::queue_install_snapshot(&mut old, &p),
        queue_install_snapshot(&mut new, &p)
    );
    assert_eq!(old.snapshot, new.snapshot);
    assert!(
        !new.snapshot
            .installs
            .iter()
            .any(|install| install.pack_id == 3)
    );
    assert!(
        new.snapshot
            .installs
            .iter()
            .any(|install| install.pack_id == 7)
    );
}

#[test]
fn install_queue_avoids_history_copies_and_limits_retained_capacity() {
    for (count, capacity, phase, target) in [
        (8, 8, InstallPhase::Downloading, 0),
        (8, 8, InstallPhase::Installed, 0),
        (8, 8, InstallPhase::Error, 0),
        (8, 8, InstallPhase::Downloading, 1000),
        (MAX_INSTALLS, MAX_INSTALLS, InstallPhase::Error, 1000),
        (MAX_INSTALLS, MAX_INSTALLS, InstallPhase::Downloading, 1000),
    ] {
        let p = pack(target);
        let mut old = runtime(count, capacity, phase);
        let mut new = runtime(count, capacity, phase);
        assert_reduced_churn(
            || {
                drop(black_box(baseline::queue_install_snapshot(&mut old, &p)));
                drop(old);
            },
            || {
                drop(black_box(queue_install_snapshot(&mut new, &p)));
                drop(new);
            },
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn install_queue_benchmark() {
    let original = black_box(
        baseline::queue_install_snapshot as fn(&mut RuntimeState, &PackInfo) -> Result<(), String>,
    );
    let current =
        black_box(queue_install_snapshot as fn(&mut RuntimeState, &PackInfo) -> Result<(), String>);
    for (name, count, capacity, phase, target, shared) in [
        ("empty-unique", 0, 0, InstallPhase::Queued, 1000, false),
        (
            "append-unique",
            8,
            8,
            InstallPhase::Downloading,
            1000,
            false,
        ),
        ("append-shared", 8, 8, InstallPhase::Downloading, 1000, true),
        ("retry-unique", 8, 8, InstallPhase::Error, 0, false),
        ("retry-shared", 8, 8, InstallPhase::Error, 0, true),
        (
            "reject-queued",
            MAX_INSTALLS,
            MAX_INSTALLS,
            InstallPhase::Queued,
            0,
            false,
        ),
        (
            "reject-installed",
            MAX_INSTALLS,
            MAX_INSTALLS,
            InstallPhase::Installed,
            0,
            true,
        ),
        (
            "reject-full",
            MAX_INSTALLS,
            MAX_INSTALLS,
            InstallPhase::Downloading,
            1000,
            false,
        ),
        (
            "evict-unique",
            MAX_INSTALLS,
            MAX_INSTALLS,
            InstallPhase::Installed,
            1000,
            false,
        ),
        (
            "evict-shared",
            MAX_INSTALLS,
            MAX_INSTALLS,
            InstallPhase::Installed,
            1000,
            true,
        ),
        (
            "oversized-append",
            8,
            4096,
            InstallPhase::Downloading,
            1000,
            false,
        ),
        ("oversized-retry", 8, 4096, InstallPhase::Error, 0, false),
    ] {
        let pack = pack(target);
        let run = |variant, f: fn(&mut RuntimeState, &PackInfo) -> Result<(), String>| {
            measure_sampled_with_setup(
                &format!("install-queue/{name}/{variant}"),
                4096,
                1,
                || {
                    let state = runtime(count, capacity, phase);
                    let observer = shared.then(|| Arc::clone(&state.snapshot));
                    (state, observer)
                },
                |(state, _)| {
                    drop(black_box(f(black_box(state), black_box(&pack))));
                },
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
}
