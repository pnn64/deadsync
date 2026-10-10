//! Descriptions for packs no list has described.
//!
//! The details walk describes the newest couple of hundred packs and the
//! doubles pass. Every other row the browser shows -- a keyboard pack, a search
//! hit, an itgdb doubles pack, most of the stamina and all-around sets --
//! comes from the CSV, which knows nothing of banners, dates or chart types.
//!
//! The original fills those rows in by reading each visible row's own page,
//! about a hundred kilobytes apiece. This asks the site's pack table for that
//! one pack by exact name instead: the same banner, date and types in about a
//! kilobyte. Two shapes of request:
//!
//! * **Lookups** -- one pack each, only for rows on screen, a few at a time.
//!   The queue is replaced with whatever is visible each time it is asked, so
//!   scrolling past a page abandons that page's lookups instead of queueing
//!   them behind the next.
//! * **Views** -- a whole substyle set, fetched once when its tab is opened.
//!   Those tabs leave out packs with no banner, so they need to know about
//!   every pack in the set before they can be drawn at all, not just the ones
//!   on screen.

use crate::smo_details::{self, PackDetails};
use crate::smo_search::percent_encode;
use deadsync_net as network;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

/// Lookups running at once. Small GETs against one volunteer host.
const WORKERS: usize = 3;
/// How long a lookup that failed waits before it is worth asking again. The
/// original waits this long before re-reading a pack page.
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// One lookup's answer is a single row of about a kilobyte.
const LOOKUP_MAX_BYTES: usize = 256 * 1024;
/// A view's pages. Big enough that the larger set arrives in two requests.
const VIEW_PAGE_LEN: usize = 500;
/// And a bound on how many it may take, should the site ever stop answering
/// short.
const VIEW_MAX_PAGES: usize = 6;
const VIEW_MAX_BYTES: usize = 8 * 1024 * 1024;

/// A substyle set a tab lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Stamina,
    /// The site's `technical` and `all around` together, as the tab lists.
    AllAround,
}

impl View {
    const fn filter(self) -> &'static str {
        match self {
            Self::Stamina => "stamina",
            Self::AllAround => "technical,all%20around",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct DescribeSnapshot {
    /// Packs looked up one at a time.
    pub by_id: Arc<HashMap<u64, PackDetails>>,
    /// Packs the views described. Kept apart from `by_id` because it is ten
    /// times the size and changes twice a session: a lookup landing must not
    /// copy it.
    pub view_rows: Arc<HashMap<u64, PackDetails>>,
    /// Packs looked up and not in the table at all. Nothing is coming.
    pub missing: Arc<HashSet<u64>>,
    /// Packs queued or being looked up now.
    pub pending: Arc<HashSet<u64>>,
    pub stamina: ViewPhase,
    pub all_around: ViewPhase,
    /// Moves on every change.
    pub revision: u64,
    /// Moves only when a view's phase does -- the one change here that can
    /// alter which packs a list holds, rather than how a row is drawn.
    pub views_revision: u64,
}

impl DescribeSnapshot {
    #[must_use]
    pub fn view_phase(&self, view: View) -> ViewPhase {
        match view {
            View::Stamina => self.stamina,
            View::AllAround => self.all_around,
        }
    }

    /// What is known about a pack, from either kind of request.
    #[must_use]
    pub fn get(&self, pack_id: u64) -> Option<&PackDetails> {
        self.view_rows
            .get(&pack_id)
            .or_else(|| self.by_id.get(&pack_id))
    }

    /// Whether this pack has had its answer, found or not.
    #[must_use]
    pub fn answered(&self, pack_id: u64) -> bool {
        self.get(pack_id).is_some() || self.missing.contains(&pack_id)
    }
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<DescribeSnapshot>,
    generation: u64,
    by_id: HashMap<u64, PackDetails>,
    missing: HashSet<u64>,
    /// Waiting to be looked up, most wanted first.
    queue: VecDeque<(u64, String)>,
    in_flight: HashSet<u64>,
    workers: usize,
    /// Lookups that failed, and when they may be asked again.
    retry_at: HashMap<u64, Instant>,
    view_rows: HashMap<u64, PackDetails>,
    stamina: ViewPhase,
    all_around: ViewPhase,
    stamina_failed_at: Option<Instant>,
    all_around_failed_at: Option<Instant>,
    /// A retry after a failure. Published as still failed until it lands, so
    /// the list keeps showing the whole set it fell back to rather than
    /// blanking to skeletons every thirty seconds.
    stamina_retrying: bool,
    all_around_retrying: bool,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

#[must_use]
pub fn runtime_snapshot() -> Arc<DescribeSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Look these packs up: the rows on screen that nothing has described.
///
/// Safe to call every frame. What is already answered, in flight, or cooling
/// off after a failure is skipped, and asking for the same set as last time
/// changes nothing and allocates nothing. Asking for a different set replaces
/// what was waiting: the reader has moved on, and so does the queue.
pub fn runtime_want(wanted: &[(u64, &str)]) {
    let mut runtime = lock_runtime();
    let now = Instant::now();
    let worth = |runtime: &RuntimeState, id: u64| {
        !runtime.by_id.contains_key(&id)
            && !runtime.view_rows.contains_key(&id)
            && !runtime.missing.contains(&id)
            && !runtime.in_flight.contains(&id)
            && runtime.retry_at.get(&id).is_none_or(|at| now >= *at)
    };
    let same = {
        let mut queued = runtime.queue.iter().map(|(id, _)| *id);
        wanted
            .iter()
            .filter(|(id, _)| worth(&runtime, *id))
            .all(|(id, _)| queued.next() == Some(*id))
            && queued.next().is_none()
    };
    if !same {
        let queue: VecDeque<(u64, String)> = wanted
            .iter()
            .filter(|(id, _)| worth(&runtime, *id))
            .map(|(id, name)| (*id, (*name).to_owned()))
            .collect();
        runtime.queue = queue;
        publish(&mut runtime, false);
    }
    // Every call, not only when the queue changed: work can be waiting with
    // too few workers, and this is two comparisons when it is not.
    while runtime.workers < WORKERS && runtime.workers < runtime.queue.len() {
        let spawned = thread::Builder::new()
            .name("smo-describe".to_owned())
            .spawn(run_lookups);
        if spawned.is_err() {
            break;
        }
        runtime.workers += 1;
    }
}

/// Fetch a whole substyle set, once, for the tab that lists it. A failure is
/// tried again after a pause rather than every frame.
pub fn runtime_want_view(view: View) {
    let generation = {
        let mut runtime = lock_runtime();
        let (phase, failed_at) = match view {
            View::Stamina => (runtime.stamina, runtime.stamina_failed_at),
            View::AllAround => (runtime.all_around, runtime.all_around_failed_at),
        };
        let due = match phase {
            ViewPhase::Idle => true,
            ViewPhase::Error => failed_at.is_none_or(|at| at.elapsed() >= RETRY_AFTER),
            ViewPhase::Loading | ViewPhase::Ready => false,
        };
        if !due {
            return;
        }
        let retrying = phase == ViewPhase::Error;
        set_view_phase(&mut runtime, view, ViewPhase::Loading);
        match view {
            View::Stamina => runtime.stamina_retrying = retrying,
            View::AllAround => runtime.all_around_retrying = retrying,
        }
        // A first load says so, and the tab waits for it. A retry does not:
        // the tab already shows the set it fell back to, and the answer is
        // published when it lands.
        if !retrying {
            publish(&mut runtime, true);
        }
        runtime.generation
    };
    let spawned = thread::Builder::new()
        .name("smo-describe-view".to_owned())
        .spawn(move || {
            let result = fetch_view(view);
            finish_view(generation, view, result);
        });
    if spawned.is_err() {
        let mut runtime = lock_runtime();
        set_view_phase(&mut runtime, view, ViewPhase::Error);
        publish(&mut runtime, true);
    }
}

/// Forget everything, as a catalogue refresh does. Work in flight lands on a
/// generation nobody is listening to.
pub fn runtime_refresh() {
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

fn set_view_phase(runtime: &mut RuntimeState, view: View, phase: ViewPhase) {
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

/// The phase a view is published as: a retry in flight still reads as the
/// failure it is retrying, whichever view's publish carries it out.
const fn shown_phase(phase: ViewPhase, retrying: bool) -> ViewPhase {
    if retrying && matches!(phase, ViewPhase::Loading) {
        ViewPhase::Error
    } else {
        phase
    }
}

/// Publish what is known. The view rows are only copied when they changed.
fn publish(runtime: &mut RuntimeState, views_moved: bool) {
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

/// One worker: take the next lookup, answer it, until there are none.
///
/// Not tied to a generation. A refresh clears the queue, so whatever a worker
/// pops is current work; only an answer that lands after a refresh is thrown
/// away, and the worker carries on with the new queue rather than leaving it
/// unserved while it still counts as running.
fn run_lookups() {
    let agent = network::get_agent();
    loop {
        let (pack_id, name, generation) = {
            let mut runtime = lock_runtime();
            let Some((pack_id, name)) = runtime.queue.pop_front() else {
                runtime.workers = runtime.workers.saturating_sub(1);
                return;
            };
            runtime.in_flight.insert(pack_id);
            (pack_id, name, runtime.generation)
        };
        let result = lookup(&agent, pack_id, name.as_str());
        let mut runtime = lock_runtime();
        runtime.in_flight.remove(&pack_id);
        if runtime.generation == generation {
            match result {
                Ok(Some(details)) => {
                    runtime.by_id.insert(pack_id, details);
                }
                Ok(None) => {
                    runtime.missing.insert(pack_id);
                }
                Err(error) => {
                    log::debug!("Could not describe pack {pack_id}: {error}");
                    runtime
                        .retry_at
                        .insert(pack_id, Instant::now() + RETRY_AFTER);
                }
            }
        }
        publish(&mut runtime, false);
    }
}

fn lookup_url(name: &str) -> String {
    format!(
        "{}?start=0&length=10&search%5Bvalue%5D={}&pack_name_exact=1",
        smo_details::DETAILS_URL,
        percent_encode(name)
    )
}

/// One pack's row, found by exact name and confirmed by id. A name the table
/// answers with some other pack is not an answer about this one.
fn lookup(
    agent: &network::HttpAgent,
    pack_id: u64,
    name: &str,
) -> Result<Option<PackDetails>, String> {
    let response = agent
        .get(lookup_url(name))
        .call()
        .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
    let body = network::read_text_body_bounded(response, LOOKUP_MAX_BYTES)
        .map_err(|error| format!("{error:?}"))?;
    Ok(pick(smo_details::parse_page(body.as_str())?, pack_id))
}

fn pick(rows: Vec<(u64, PackDetails)>, pack_id: u64) -> Option<PackDetails> {
    rows.into_iter()
        .find(|(id, _)| *id == pack_id)
        .map(|(_, details)| details)
}

fn view_url(view: View, start: usize) -> String {
    format!(
        "{}?start={start}&length={VIEW_PAGE_LEN}&pack_substyle_filters={}",
        smo_details::DETAILS_URL,
        view.filter()
    )
}

fn fetch_view(view: View) -> Result<Vec<(u64, PackDetails)>, String> {
    let agent = network::get_agent();
    let mut rows = Vec::new();
    for page in 0..VIEW_MAX_PAGES {
        let response = agent
            .get(view_url(view, page * VIEW_PAGE_LEN))
            .call()
            .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
        let body = network::read_text_body_bounded(response, VIEW_MAX_BYTES)
            .map_err(|error| format!("{error:?}"))?;
        let got = smo_details::parse_page(body.as_str())?;
        let count = got.len();
        rows.extend(got);
        if count < VIEW_PAGE_LEN {
            break;
        }
    }
    Ok(rows)
}

fn finish_view(generation: u64, view: View, result: Result<Vec<(u64, PackDetails)>, String>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The name is the one thing a reader never controls, and it goes into a
    /// query string: every character that means something in a URL is escaped.
    #[test]
    fn a_lookup_asks_for_exactly_one_name() {
        let url = lookup_url("Tom & Jerry's #1 Pack?");
        assert!(url.contains("pack_name_exact=1"));
        assert!(
            url.contains("search%5Bvalue%5D=Tom%20%26%20Jerry%27s%20%231%20Pack%3F"),
            "{url}"
        );
        assert!(!url.contains(' '));
    }

    /// A name can be shared, and the table answers with whatever matches it.
    /// Only the row for this pack's id is an answer about this pack.
    #[test]
    fn only_the_row_for_this_pack_is_its_answer() {
        let other = PackDetails {
            banner_url: Some("other".to_owned()),
            ..PackDetails::default()
        };
        let mine = PackDetails {
            banner_url: Some("mine".to_owned()),
            ..PackDetails::default()
        };
        let rows = vec![(1, other.clone()), (2, mine.clone())];
        assert_eq!(pick(rows.clone(), 2), Some(mine));
        assert_eq!(pick(rows, 3), None, "no row for it is no answer");
    }

    /// The all-around tab is two of the site's substyles together, and the
    /// comma and space in that filter survive the trip.
    #[test]
    fn a_view_asks_for_its_whole_set() {
        assert!(view_url(View::Stamina, 0).ends_with("pack_substyle_filters=stamina"));
        let url = view_url(View::AllAround, 500);
        assert!(url.contains("start=500"));
        assert!(url.ends_with("pack_substyle_filters=technical,all%20around"));
        assert!(!url.contains(' '));
    }

    /// A retry after a failure is published as the failure until it lands,
    /// so the list that fell back to the whole set does not blank to
    /// skeletons every time it is tried again. A first load is not.
    #[test]
    fn a_retry_reads_as_the_failure_until_it_lands() {
        assert_eq!(shown_phase(ViewPhase::Loading, true), ViewPhase::Error);
        assert_eq!(shown_phase(ViewPhase::Loading, false), ViewPhase::Loading);
        assert_eq!(shown_phase(ViewPhase::Ready, true), ViewPhase::Ready);
    }

    /// Something described or given up on is answered; a pack only in flight
    /// is not.
    #[test]
    fn an_answer_is_a_description_or_a_miss() {
        let snapshot = DescribeSnapshot {
            by_id: Arc::new(HashMap::from([(1, PackDetails::default())])),
            view_rows: Arc::new(HashMap::from([(2, PackDetails::default())])),
            missing: Arc::new(HashSet::from([3])),
            pending: Arc::new(HashSet::from([4])),
            ..DescribeSnapshot::default()
        };
        assert!(snapshot.answered(1));
        assert!(snapshot.answered(2));
        assert!(snapshot.answered(3));
        assert!(!snapshot.answered(4));
        assert!(snapshot.get(3).is_none());
    }
}
