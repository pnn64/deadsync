// Frozen from 735a994c75240ae043dc153204572d6952ed14fc; only item/field visibility is changed.
use super::*;

#[derive(Default)]
pub(super) struct RuntimeState {
    pub(super) snapshot: Arc<DescribeSnapshot>,
    pub(super) generation: u64,
    pub(super) by_id: HashMap<u64, PackDetails>,
    pub(super) missing: HashSet<u64>,
    /// Waiting to be looked up, most wanted first.
    pub(super) queue: VecDeque<(u64, String)>,
    pub(super) in_flight: HashSet<u64>,
    pub(super) workers: usize,
    /// Lookups that failed, and when they may be asked again.
    pub(super) retry_at: HashMap<u64, Instant>,
    pub(super) view_rows: HashMap<u64, PackDetails>,
    pub(super) stamina: ViewPhase,
    pub(super) all_around: ViewPhase,
    pub(super) stamina_failed_at: Option<Instant>,
    pub(super) all_around_failed_at: Option<Instant>,
    /// A retry after a failure. Published as still failed until it lands, so
    /// the list keeps showing the whole set it fell back to rather than
    /// blanking to skeletons every thirty seconds.
    pub(super) stamina_retrying: bool,
    pub(super) all_around_retrying: bool,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

pub(super) fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

pub(super) fn publish(runtime: &mut RuntimeState, views_moved: bool) {
    let pending: HashSet<u64> = runtime
        .queue
        .iter()
        .map(|(id, _)| *id)
        .chain(runtime.in_flight.iter().copied())
        .collect();
    let mut snapshot = (*runtime.snapshot).clone();
    if views_moved {
        snapshot.view_rows = Arc::new(runtime.view_rows.clone());
        snapshot.stamina = shown_phase(runtime.stamina, runtime.stamina_retrying);
        snapshot.all_around = shown_phase(runtime.all_around, runtime.all_around_retrying);
        snapshot.views_revision = snapshot.views_revision.wrapping_add(1);
    }
    if snapshot.by_id.len() != runtime.by_id.len() {
        snapshot.by_id = Arc::new(runtime.by_id.clone());
    }
    if snapshot.missing.len() != runtime.missing.len() {
        snapshot.missing = Arc::new(runtime.missing.clone());
    }
    snapshot.pending = Arc::new(pending);
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

pub(super) fn runtime_refresh() {
    let mut runtime = lock_runtime();
    runtime.generation = runtime.generation.wrapping_add(1);
    runtime.by_id.clear();
    runtime.missing.clear();
    runtime.queue.clear();
    runtime.retry_at.clear();
    runtime.view_rows.clear();
    runtime.stamina = ViewPhase::Idle;
    runtime.all_around = ViewPhase::Idle;
    runtime.stamina_failed_at = None;
    runtime.all_around_failed_at = None;
    runtime.stamina_retrying = false;
    runtime.all_around_retrying = false;
    publish(&mut runtime, true);
}

pub(super) fn set_view_phase(runtime: &mut RuntimeState, view: View, phase: ViewPhase) {
    let failed_at = (phase == ViewPhase::Error).then(Instant::now);
    let settled = matches!(phase, ViewPhase::Ready | ViewPhase::Error);
    match view {
        View::Stamina => {
            runtime.stamina = phase;
            runtime.stamina_failed_at = failed_at;
            runtime.stamina_retrying &= !settled;
        }
        View::AllAround => {
            runtime.all_around = phase;
            runtime.all_around_failed_at = failed_at;
            runtime.all_around_retrying &= !settled;
        }
    }
}

pub(super) fn finish_view(
    generation: u64,
    view: View,
    result: Result<Vec<(u64, PackDetails)>, String>,
) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        return;
    }
    match result {
        Ok(rows) => {
            for (id, details) in rows {
                runtime.view_rows.insert(id, details);
            }
            set_view_phase(&mut runtime, view, ViewPhase::Ready);
        }
        Err(error) => {
            log::warn!("Could not load the {view:?} pack set: {error}");
            set_view_phase(&mut runtime, view, ViewPhase::Error);
        }
    }
    publish(&mut runtime, true);
}
