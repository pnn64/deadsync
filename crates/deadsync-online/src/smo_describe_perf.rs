use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

#[path = "smo_describe_original.rs"]
mod original;

static RUNTIME_TEST: Mutex<()> = Mutex::new(());

struct RuntimeFixture {
    saved: Option<RuntimeState>,
    saved_original: Option<original::RuntimeState>,
    _serial: MutexGuard<'static, ()>,
}

impl RuntimeFixture {
    fn new() -> Self {
        let serial = RUNTIME_TEST.lock().unwrap();
        Self {
            saved: Some(std::mem::take(&mut *lock_runtime())),
            saved_original: Some(std::mem::take(&mut *original::lock_runtime())),
            _serial: serial,
        }
    }

    fn install(&self, count: usize, phase: ViewPhase, retrying: bool) {
        let (current, old) = states(count, phase, retrying);
        *lock_runtime() = current;
        *original::lock_runtime() = old;
    }
}

impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        *lock_runtime() = self.saved.take().unwrap();
        *original::lock_runtime() = self.saved_original.take().unwrap();
    }
}

fn details(id: usize) -> PackDetails {
    PackDetails {
        banner_url: Some(format!(
            "https://stepmaniaonline.net/media/images/packs/{id}.png"
        )),
        date_added: Some("2026-10-09".to_owned()),
        chart_types: vec!["dance".to_owned(), "pump".to_owned()],
    }
}

fn states(
    count: usize,
    phase: ViewPhase,
    retrying: bool,
) -> (RuntimeState, original::RuntimeState) {
    let rows: HashMap<_, _> = (0..count).map(|id| (id as u64, details(id))).collect();
    let mut old = original::RuntimeState {
        generation: u64::MAX,
        view_rows: rows.clone(),
        by_id: HashMap::from([(10_000, details(10_000))]),
        missing: HashSet::from([20_000]),
        queue: VecDeque::from([
            (30_000, "queued".to_owned()),
            (30_000, "duplicate".to_owned()),
        ]),
        in_flight: HashSet::from([40_000]),
        workers: WORKERS,
        retry_at: HashMap::from([(50_000, Instant::now() + RETRY_AFTER)]),
        stamina: phase,
        all_around: phase,
        stamina_retrying: retrying,
        all_around_retrying: retrying,
        ..original::RuntimeState::default()
    };
    old.snapshot = Arc::new(DescribeSnapshot {
        by_id: Arc::new(old.by_id.clone()),
        view_rows: Arc::new(rows.clone()),
        missing: Arc::new(old.missing.clone()),
        pending: Arc::new(HashSet::from([30_000, 40_000])),
        stamina: shown_phase(phase, retrying),
        all_around: shown_phase(phase, retrying),
        revision: u64::MAX,
        views_revision: u64::MAX,
    });
    let current = RuntimeState {
        snapshot: Arc::new(DescribeSnapshot {
            view_rows: Arc::new(rows),
            ..(*old.snapshot).clone()
        }),
        generation: old.generation,
        by_id: old.by_id.clone(),
        missing: old.missing.clone(),
        queue: old.queue.clone(),
        in_flight: old.in_flight.clone(),
        workers: old.workers,
        retry_at: old.retry_at.clone(),
        stamina: phase,
        all_around: phase,
        stamina_retrying: retrying,
        all_around_retrying: retrying,
        ..RuntimeState::default()
    };
    (current, old)
}

fn same_snapshot(actual: &DescribeSnapshot, expected: &DescribeSnapshot) {
    assert_eq!(actual.by_id, expected.by_id);
    assert_eq!(actual.view_rows, expected.view_rows);
    assert_eq!(actual.missing, expected.missing);
    assert_eq!(actual.pending, expected.pending);
    assert_eq!(actual.stamina, expected.stamina);
    assert_eq!(actual.all_around, expected.all_around);
    assert_eq!(actual.revision, expected.revision);
    assert_eq!(actual.views_revision, expected.views_revision);
}

fn same_runtime() {
    let current = lock_runtime();
    let old = original::lock_runtime();
    same_snapshot(&current.snapshot, &old.snapshot);
    assert_eq!(*current.snapshot.view_rows, old.view_rows);
    assert_eq!(current.by_id, old.by_id);
    assert_eq!(current.missing, old.missing);
    assert_eq!(current.queue, old.queue);
    assert_eq!(current.in_flight, old.in_flight);
    assert_eq!(current.workers, old.workers);
    assert_eq!(current.retry_at, old.retry_at);
    assert_eq!(current.generation, old.generation);
    assert_eq!(current.stamina, old.stamina);
    assert_eq!(current.all_around, old.all_around);
    assert_eq!(current.stamina_retrying, old.stamina_retrying);
    assert_eq!(current.all_around_retrying, old.all_around_retrying);
    assert_eq!(
        current.stamina_failed_at.is_some(),
        old.stamina_failed_at.is_some()
    );
    assert_eq!(
        current.all_around_failed_at.is_some(),
        old.all_around_failed_at.is_some()
    );
}

#[test]
fn view_responses_preserve_snapshots_phases_and_generations() {
    let fixture = RuntimeFixture::new();
    for phase in [
        ViewPhase::Idle,
        ViewPhase::Loading,
        ViewPhase::Ready,
        ViewPhase::Error,
    ] {
        for view in [View::Stamina, View::AllAround] {
            for retrying in [false, true] {
                for result in [
                    Ok(Vec::new()),
                    Ok(vec![(1, details(101)), (9, details(9)), (1, details(102))]),
                    Err("offline".to_owned()),
                ] {
                    fixture.install(7, phase, retrying);
                    let held = runtime_snapshot();
                    let old_held = Arc::clone(&original::lock_runtime().snapshot);
                    let before = (*held).clone();
                    let view_rows = Arc::clone(&held.view_rows);
                    finish_view(u64::MAX - 1, view, result.clone());
                    original::finish_view(u64::MAX - 1, view, result.clone());
                    assert!(
                        Arc::ptr_eq(&held, &runtime_snapshot()),
                        "stale answer republished"
                    );
                    same_runtime();
                    let changes_rows = result.as_ref().is_ok_and(|rows| !rows.is_empty());
                    finish_view(u64::MAX, view, result.clone());
                    original::finish_view(u64::MAX, view, result);
                    same_runtime();
                    same_snapshot(&held, &before);
                    same_snapshot(&old_held, &before);
                    assert_eq!(
                        Arc::ptr_eq(&view_rows, &runtime_snapshot().view_rows),
                        !changes_rows
                    );
                    if changes_rows {
                        let snapshot = runtime_snapshot();
                        assert_eq!(snapshot.view_rows.len(), 8);
                        assert_eq!(snapshot.view_rows[&1], details(102), "last duplicate wins");
                        assert_eq!(snapshot.view_rows[&0], details(0));
                    }
                    assert_eq!(runtime_snapshot().revision, 0);
                    assert_eq!(runtime_snapshot().views_revision, 0);
                }
            }
        }
    }
}

#[test]
fn unshared_view_updates_reuse_the_map_and_refresh_preserves_old_readers() {
    let fixture = RuntimeFixture::new();
    fixture.install(200, ViewPhase::Loading, false);
    let map = Arc::as_ptr(&lock_runtime().snapshot.view_rows);
    finish_view(u64::MAX, View::Stamina, Ok(vec![(1, details(101))]));
    original::finish_view(u64::MAX, View::Stamina, Ok(vec![(1, details(101))]));
    same_runtime();
    assert_eq!(Arc::as_ptr(&lock_runtime().snapshot.view_rows), map);
    let held = runtime_snapshot();
    let before = (*held).clone();
    runtime_refresh();
    original::runtime_refresh();
    same_runtime();
    same_snapshot(&held, &before);
    assert!(runtime_snapshot().view_rows.is_empty());
    assert_eq!(lock_runtime().generation, 0);
    // Late responses from the discarded generation cannot restore the old map.
    finish_view(u64::MAX, View::AllAround, Ok(vec![(999, details(999))]));
    original::finish_view(u64::MAX, View::AllAround, Ok(vec![(999, details(999))]));
    same_runtime();
}

#[test]
fn a_reader_can_retain_the_view_map_without_its_snapshot() {
    let fixture = RuntimeFixture::new();
    fixture.install(7, ViewPhase::Ready, false);
    let held_map = Arc::clone(&runtime_snapshot().view_rows);
    let before = (*held_map).clone();
    finish_view(u64::MAX, View::AllAround, Ok(vec![(1, details(101))]));
    original::finish_view(u64::MAX, View::AllAround, Ok(vec![(1, details(101))]));
    same_runtime();
    assert_eq!(*held_map, before);
    assert!(!Arc::ptr_eq(&held_map, &runtime_snapshot().view_rows));
    assert_eq!(runtime_snapshot().view_rows[&1], details(101));
}

#[test]
fn view_answers_suppress_lookup_work_without_allocation() {
    let fixture = RuntimeFixture::new();
    fixture.install(7, ViewPhase::Ready, false);
    lock_runtime().queue.clear();
    let before = runtime_snapshot();
    perf::assert_no_churn(|| runtime_want(&[(1, "already described"), (6, "also described")]));
    assert!(Arc::ptr_eq(&before, &runtime_snapshot()));
    assert!(lock_runtime().queue.is_empty());
}

#[test]
fn phase_publication_keeps_the_view_map_and_matches_lookup_updates() {
    for moved in [false, true] {
        let (mut current, mut old) = states(200, ViewPhase::Loading, true);
        let held = Arc::clone(&current.snapshot);
        let old_held = Arc::clone(&old.snapshot);
        current.by_id.insert(10_001, details(10_001));
        old.by_id.insert(10_001, details(10_001));
        current.missing.insert(20_001);
        old.missing.insert(20_001);
        let (_, before) = perf::measure(|| original::publish(&mut old, moved));
        let (_, after) = perf::measure(|| publish(&mut current, moved));
        same_snapshot(&current.snapshot, &old.snapshot);
        same_snapshot(&held, &old_held);
        assert!(Arc::ptr_eq(&held.view_rows, &current.snapshot.view_rows));
        if moved {
            assert!(after.allocs + 1000 < before.allocs);
        } else {
            assert_eq!(after, before);
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_view_phase_publication() {
    for (count, moved) in [
        (0, true),
        (7, true),
        (200, true),
        (1200, true),
        (3000, true),
        (1200, false),
    ] {
        for retained in [false, true] {
            let (mut current, mut old) = states(count, ViewPhase::Loading, true);
            let held = retained.then(|| Arc::clone(&current.snapshot));
            let old_held = retained.then(|| Arc::clone(&old.snapshot));
            let label = format!("view/{count}-moved-{moved}-retained-{retained}");
            let (_, before) = perf::measure(|| original::publish(&mut old, moved));
            let (_, after) = perf::measure(|| publish(&mut current, moved));
            same_snapshot(&current.snapshot, &old.snapshot);
            println!("{label} churn: {before:?} -> {after:?}");
            paired_bench::compare(&label, (20_000 / count.max(1)).max(20), |is_current| {
                if is_current {
                    let reader = retained.then(|| Arc::clone(&current.snapshot));
                    publish(black_box(&mut current), black_box(moved));
                    black_box(reader);
                } else {
                    let reader = retained.then(|| Arc::clone(&old.snapshot));
                    original::publish(black_box(&mut old), black_box(moved));
                    black_box(reader);
                }
            });
            black_box((held, old_held));
        }
    }
}

#[test]
fn view_responses_avoid_unnecessary_description_copies() {
    let fixture = RuntimeFixture::new();
    for count in [0, 200] {
        for retained in [false, true] {
            fixture.install(count, ViewPhase::Loading, false);
            let held = retained.then(runtime_snapshot);
            let old_held = retained.then(|| Arc::clone(&original::lock_runtime().snapshot));
            let rows: Vec<_> = (0..200)
                .map(|id| (id, details(id as usize + 1000)))
                .collect();
            let old_rows = rows.clone();
            let (_, before) =
                perf::measure(|| original::finish_view(u64::MAX, View::Stamina, Ok(old_rows)));
            let (_, after) = perf::measure(|| finish_view(u64::MAX, View::Stamina, Ok(rows)));
            same_runtime();
            assert!(
                after.allocs <= before.allocs,
                "{count}/{retained}: {before:?} -> {after:?}"
            );
            if count == 0 || !retained {
                assert!(after.allocs + 1000 < before.allocs);
            }
            black_box((held, old_held));
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_view_responses() {
    let fixture = RuntimeFixture::new();
    for (count, updated) in [(200, 200), (200, 7), (1200, 7)] {
        for retained in [false, true] {
            fixture.install(count, ViewPhase::Loading, false);
            let rows = || {
                (0..updated)
                    .map(|id| (id, details(id as usize + 1000)))
                    .collect::<Vec<_>>()
            };
            let old_rows = rows();
            let new_rows = rows();
            let held = retained.then(runtime_snapshot);
            let old_held = retained.then(|| Arc::clone(&original::lock_runtime().snapshot));
            let (_, before) =
                perf::measure(|| original::finish_view(u64::MAX, View::Stamina, Ok(old_rows)));
            let (_, after) = perf::measure(|| finish_view(u64::MAX, View::Stamina, Ok(new_rows)));
            same_runtime();
            let label = format!("view-response/{count}-{updated}-retained-{retained}");
            println!("{label} churn: {before:?} -> {after:?}");
            drop((held, old_held));
            // Every response can have a reader retaining its immediate predecessor.
            paired_bench::compare_prepared(
                &label,
                (10_000 / count.max(updated as usize)).max(20),
                rows,
                |rows, current| {
                    if current {
                        let held = retained.then(runtime_snapshot);
                        finish_view(u64::MAX, View::Stamina, Ok(rows));
                        black_box(held);
                    } else {
                        let held = retained.then(|| Arc::clone(&original::lock_runtime().snapshot));
                        original::finish_view(u64::MAX, View::Stamina, Ok(rows));
                        black_box(held);
                    }
                },
            );
        }
    }
}
