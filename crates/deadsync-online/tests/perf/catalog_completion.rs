use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled_with_setup};
use std::hint::black_box;
use std::sync::Weak;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/catalog_completion/baseline.rs"
    ));
}

static TEST_RUNTIME: Mutex<()> = Mutex::new(());
type Finish = fn(u64, Result<Vec<PackInfo>, StepManiaOnlineError>);

fn pack(id: u64) -> PackInfo {
    PackInfo::new(id, format!("Pack {id}"), 17, 12345, None, None, None, None)
}

struct Fixture {
    previous: Option<RuntimeState>,
    observer: Option<Arc<Snapshot>>,
    weak: Option<Weak<Snapshot>>,
    catalog: Arc<[PackInfo]>,
    request: Option<(u64, Result<Vec<PackInfo>, StepManiaOnlineError>)>,
}

impl Fixture {
    fn new(
        count: usize,
        capacity: usize,
        shared: bool,
        weak: bool,
        success: bool,
        stale: bool,
    ) -> Self {
        let mut installs = Vec::with_capacity(capacity.max(count));
        installs.extend((0..count).map(|i| InstallSnapshot {
            pack_id: i as u64,
            phase: InstallPhase::Downloading,
            downloaded_bytes: 17,
            total_bytes: 999,
            message: Some(format!("History {i} {}", "x".repeat(128))),
        }));
        let state = RuntimeState {
            generation: 7,
            snapshot: Arc::new(Snapshot {
                phase: CatalogPhase::Loading,
                catalog: Arc::from([pack(0)]),
                revision: u64::MAX,
                message: Some("old status ".repeat(64)),
                installs,
            }),
            ready_song_dirs: vec![PathBuf::from("songs/ready")],
        };
        let observer = shared.then(|| Arc::clone(&state.snapshot));
        let weak = weak.then(|| Arc::downgrade(&state.snapshot));
        let catalog = Arc::clone(&state.snapshot.catalog);
        let previous = Some(std::mem::replace(&mut *lock_runtime(), state));
        let result = if success {
            Ok(vec![pack(17), pack(23)])
        } else {
            Err(StepManiaOnlineError::Catalog("fixture failure".into()))
        };
        Self {
            previous,
            observer,
            weak,
            catalog,
            request: Some((if stale { 6 } else { 7 }, result)),
        }
    }
    fn call(&mut self, finish: Finish) {
        let (generation, result) = self.request.take().unwrap();
        finish(generation, result);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let state = std::mem::replace(&mut *lock_runtime(), self.previous.take().unwrap());
        drop(state);
    }
}

#[test]
fn catalog_completion_preserves_generations_readers_errors_revisions_and_capacity() {
    let _guard = TEST_RUNTIME.lock().unwrap();
    for count in [0, 1, 8, 64] {
        for capacity in [count, count * 2, 4096] {
            for shared in [false, true] {
                for success in [false, true] {
                    for stale in [false, true] {
                        let run = |finish: Finish| {
                            let mut fixture =
                                Fixture::new(count, capacity, shared, true, success, stale);
                            fixture.call(finish);
                            let state = lock_runtime();
                            let snapshot = Arc::clone(&state.snapshot);
                            let capacity = state.snapshot.installs.capacity();
                            let generation = state.generation;
                            let ready = state.ready_song_dirs.clone();
                            let catalog_same =
                                Arc::ptr_eq(&fixture.catalog, &state.snapshot.catalog);
                            let weak_live = fixture.weak.as_ref().unwrap().upgrade().is_some();
                            let observer = fixture.observer.take();
                            drop(state);
                            drop(fixture);
                            (
                                snapshot,
                                capacity,
                                generation,
                                ready,
                                catalog_same,
                                weak_live,
                                observer,
                            )
                        };
                        let old = run(baseline::finish_catalog_request);
                        let new = run(finish_catalog_request);
                        assert_eq!(old.0, new.0);
                        assert!(new.1 <= old.1);
                        assert_eq!(
                            (old.2, &old.3, old.4, old.5, &old.6),
                            (new.2, &new.3, new.4, new.5, &new.6)
                        );
                        assert_eq!(new.0.revision, if success && !stale { 0 } else { u64::MAX });
                        assert_eq!(new.4, !success || stale);
                        assert_eq!(new.5, shared || stale);
                    }
                }
            }
        }
    }
}

#[test]
fn catalog_completion_avoids_unique_history_copies() {
    let _guard = TEST_RUNTIME.lock().unwrap();
    for success in [false, true] {
        // Allocate inputs outside the tracked operation, including the saved state.
        let mut old = Fixture::new(64, 64, false, false, success, false);
        let mut new_input = Some(Fixture::new(64, 64, false, false, success, false));
        // The second fixture currently occupies the global slot; restore the old one.
        std::mem::swap(
            &mut *lock_runtime(),
            new_input.as_mut().unwrap().previous.as_mut().unwrap(),
        );
        assert_reduced_churn(
            || old.call(baseline::finish_catalog_request),
            || {
                std::mem::swap(
                    &mut *lock_runtime(),
                    new_input.as_mut().unwrap().previous.as_mut().unwrap(),
                );
                new_input.as_mut().unwrap().call(finish_catalog_request);
            },
        );
        // Restore nested fixtures in reverse construction order.
        drop(new_input);
        drop(old);
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn catalog_completion_benchmark() {
    let _guard = TEST_RUNTIME.lock().unwrap();
    let old = black_box(baseline::finish_catalog_request as Finish);
    let new = black_box(finish_catalog_request as Finish);
    for (name, count, capacity, shared, success, stale) in [
        ("empty-success", 0, 0, false, true, false),
        ("success-unique", 8, 8, false, true, false),
        ("success-shared", 8, 8, true, true, false),
        ("failure-unique", 8, 8, false, false, false),
        ("failure-shared", 8, 8, true, false, false),
        ("many-success", 64, 64, false, true, false),
        ("many-failure", 64, 64, false, false, false),
        ("oversized-success", 8, 4096, false, true, false),
        ("oversized-failure", 8, 4096, false, false, false),
        ("stale", 64, 64, false, true, true),
    ] {
        let run = |variant, finish: Finish| {
            measure_sampled_with_setup(
                &format!("catalog-completion/{name}/{variant}"),
                2048,
                1,
                || Fixture::new(count, capacity, shared, false, success, stale),
                |fixture| fixture.call(finish),
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", new);
            run("original", old);
        } else {
            run("original", old);
            run("current", new);
        }
    }
}
