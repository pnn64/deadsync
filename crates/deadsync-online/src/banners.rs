//! Pack banner artwork, fetched on demand.
//!
//! `smo_details` learns each pack's banner URL; this fetches the image behind
//! it. The split matters: the details walk is one burst of a dozen requests at
//! startup, while banners are driven by wherever the player has scrolled, so
//! they need a request queue with a most-recent-wins policy rather than a
//! single job.
//!
//! Bytes only. Decoding belongs to whoever owns a GPU, so this hands raw bytes
//! to the shell and the shell turns them into a texture -- which also keeps
//! `image` out of this crate.

use deadsync_net as network;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

/// The ceiling on one banner.
///
/// Measured rather than guessed: across the twenty-four packs the featured
/// strip actually shows, the largest banner served is 2,270,307 bytes of PNG.
/// A two-megabyte bound rejected that one and the next largest, which is why
/// they never drew. Four megabytes clears the real maximum with room, and an
/// untrusted URL still cannot grow past it.
///
/// The site does serve one pack banner of 9,277,141 bytes. It is left out on
/// purpose: banners are decoded on the game thread, and a picture that size
/// is a visible hitch for one row's worth of art.
const MAX_BANNER_BYTES: usize = 4 * 1024 * 1024;

/// How many requests may be waiting.
///
/// A screen wants roughly thirty banners at once -- twelve featured cards plus
/// seven rows plus the look-ahead either side -- so a queue shorter than that
/// drops requests it will only be asked for again next frame. It is bounded
/// all the same: the queue exists to smooth a scroll, not to hold a backlog.
const QUEUE_DEPTH: usize = 64;

/// Banner fetches running at once.
///
/// One worker made every banner wait for the one before it: a full TLS
/// handshake and round trip each, so a screen of art took the better part of a
/// minute to fill in. These are small independent GETs against one host, which
/// is exactly the shape that parallelises, and six is polite to a volunteer
/// server while still filling a screen in about one round trip.
const WORKERS: usize = 6;

/// Fetched banners kept in memory at once.
///
/// This has to be comfortably larger than everything one screen can ask for at
/// once, or the cache evicts artwork that is still visible: the row re-asks,
/// which evicts something else that is also visible, and the screen sits there
/// half drawn with wheels spinning on packs it already fetched. One screen
/// wants about eighty -- twenty-four featured cards, a list window with its
/// look-ahead either side, two doubles columns, and a page of song jackets --
/// so this is more than twice that.
const MAX_CACHED: usize = 192;

/// How many times a banner that failed to arrive is asked for again.
///
/// A 404 or a picture that will not decode is settled the first time, but a
/// timeout is not evidence of anything except a busy moment. Retrying twice
/// costs two requests and recovers the common case; retrying forever would
/// turn a dead URL into a request every frame.
const MAX_ATTEMPTS: u8 = 3;
/// How long a failed banner waits before it is worth asking again.
const RETRY_AFTER: Duration = Duration::from_secs(10);
/// How long one that has used up its attempts waits before another round.
///
/// Not forever: a network that dropped for a minute would otherwise leave a
/// hole in the screen for the rest of the session. The original waits ten
/// minutes between tries, and so does this.
const LONG_RETRY: Duration = Duration::from_secs(600);

/// A banner that arrived, ready for whoever can upload it.
#[derive(Clone, Debug)]
pub struct FetchedBanner {
    pub pack_id: u64,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// Asked for, not yet answered. Carries the failures before this try, so
    /// a retry that fails again counts towards the bound instead of starting
    /// the count over.
    Pending { attempts: u8 },
    /// Arrived and been handed over.
    Done,
    /// It did not arrive. Remembered so it is not asked for again every frame,
    /// with what it would take to try once more.
    Failed {
        attempts: u8,
        /// `None` once the answer is settled -- the URL 404s, or the bytes are
        /// not a picture. Retrying those is just noise.
        retry_at: Option<Instant>,
    },
}

#[derive(Default)]
struct RuntimeState {
    /// What we know about each pack's banner, so a request is made once.
    slots: HashMap<u64, Slot>,
    /// When each banner was last asked for.
    ///
    /// Eviction used to go by arrival order, which is exactly backwards for
    /// this screen: the featured cards are fetched first and wanted for as
    /// long as the browser is open, so they were the first things thrown away
    /// once paging the list had filled the cache -- and then re-fetched, and
    /// thrown away again. Going by last-wanted instead means the things on
    /// screen are the last to go.
    used: HashMap<u64, u64>,
    /// Ticks on every request, so "last wanted" is an order rather than a
    /// clock reading.
    clock: u64,
    /// Packs with no artwork to show, published so a row can draw a
    /// placeholder rather than a wheel that never stops turning.
    failed: Arc<HashSet<u64>>,
    /// The URL each pack was last asked for by. A pack whose picture failed
    /// from one source gets a fresh try when another source turns up.
    urls: HashMap<u64, String>,
    sender: Option<SyncSender<Job>>,
    ready: Option<Receiver<FetchedBanner>>,
}

struct Job {
    pack_id: u64,
    url: String,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

/// Ask for a pack's banner. Cheap and idempotent: a pack already fetched,
/// already queued, or already known to have failed costs nothing.
///
/// Returns whether the request was newly queued, which is only interesting to
/// a caller that wants to rate-limit itself.
pub fn request(pack_id: u64, url: &str) -> bool {
    let mut runtime = lock_runtime();
    // Touched whether or not it is fetched. Asking is what "wanted" means, and
    // a banner already in hand is the most wanted thing there is.
    runtime.clock = runtime.clock.wrapping_add(1);
    let now = runtime.clock;
    runtime.used.insert(pack_id, now);

    // A failure is about the URL it came from. A different one is a
    // different question, so it is asked fresh.
    if matches!(runtime.slots.get(&pack_id), Some(Slot::Failed { .. }))
        && runtime.urls.get(&pack_id).is_some_and(|asked| asked != url)
    {
        runtime.slots.remove(&pack_id);
        forget_failure(&mut runtime, pack_id);
    }

    if !worth_asking(runtime.slots.get(&pack_id).copied(), Instant::now()) {
        return false;
    }
    ensure_worker(&mut runtime);
    let Some(sender) = runtime.sender.clone() else {
        return false;
    };
    let job = Job {
        pack_id,
        url: url.to_owned(),
    };
    match sender.try_send(job) {
        Ok(()) => {
            let attempts = match runtime.slots.get(&pack_id) {
                Some(Slot::Failed { attempts, .. }) => *attempts,
                _ => 0,
            };
            runtime.slots.insert(pack_id, Slot::Pending { attempts });
            if runtime.urls.get(&pack_id).is_none_or(|asked| asked != url) {
                runtime.urls.insert(pack_id, url.to_owned());
            }
            // It is being asked for again, so it is no longer an answer.
            forget_failure(&mut runtime, pack_id);
            true
        }
        // A full queue means the player is scrolling faster than the network.
        // Dropping is right: the row will ask again next frame, and by then it
        // may not even be on screen.
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Disconnected(_)) => {
            runtime.sender = None;
            runtime.ready = None;
            false
        }
    }
}

/// Whether a pack in this state should be asked for.
///
/// Never asked for is obvious. Pending and Done are answered already. A
/// failure is the interesting case: one that is settled stays settled, and one
/// that is not waits out its gap -- short between attempts, long once they are
/// used up -- and then gets another go. Kept as a function of its inputs so
/// the rule can be tested without the process-wide fetch state.
fn worth_asking(slot: Option<Slot>, now: Instant) -> bool {
    match slot {
        None => true,
        Some(Slot::Failed {
            retry_at: Some(at), ..
        }) => now >= at,
        Some(_) => false,
    }
}

/// Take whatever has arrived since the last call, newest work first.
///
/// Bounded per call so a burst cannot stall a frame; the rest waits.
pub fn take_ready(max: usize) -> Vec<FetchedBanner> {
    let mut runtime = lock_runtime();
    let Some(ready) = runtime.ready.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while out.len() < max {
        match ready.try_recv() {
            Ok(banner) => out.push(banner),
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                runtime.sender = None;
                runtime.ready = None;
                break;
            }
        }
    }
    for banner in &out {
        runtime.slots.insert(banner.pack_id, Slot::Done);
        runtime.clock = runtime.clock.wrapping_add(1);
        let now = runtime.clock;
        runtime.used.insert(banner.pack_id, now);
        forget_failure(&mut runtime, banner.pack_id);
    }
    out
}

/// Packs whose artwork is not coming.
///
/// A cheap clone: the set only changes when a fetch settles, so a screen may
/// read this every frame. Without it a row cannot tell "still arriving" from
/// "never arriving", and draws the same spinner for both -- forever, in the
/// second case.
#[must_use]
pub fn failed_ids() -> Arc<HashSet<u64>> {
    Arc::clone(&lock_runtime().failed)
}

fn forget_failure(runtime: &mut RuntimeState, pack_id: u64) {
    if !runtime.failed.contains(&pack_id) {
        return;
    }
    let mut next = (*runtime.failed).clone();
    next.remove(&pack_id);
    runtime.failed = Arc::new(next);
}

/// Pack ids to stop holding, once the cache is over its bound. The caller owns
/// the textures, so it decides when they actually go.
pub fn overflow() -> Vec<u64> {
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

/// Mark a pack's banner as not having arrived.
///
/// `settled` says whether the answer can change: a 404 or bytes that are not a
/// picture will say the same thing next time, where a timeout will not.
///
/// Only a failure that is final for now is published. One with a retry a few
/// seconds off is still coming, and a row that swapped to its placeholder in
/// the gap would be saying otherwise.
pub fn mark_failed(pack_id: u64, settled: bool) {
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

fn ensure_worker(runtime: &mut RuntimeState) {
    if runtime.sender.is_some() {
        return;
    }
    let (jobs_tx, jobs_rx) = sync_channel::<Job>(QUEUE_DEPTH);
    let (done_tx, done_rx) = sync_channel::<FetchedBanner>(QUEUE_DEPTH);

    // One receiver behind a mutex rather than a queue each: a worker that is
    // still waiting on a slow banner must not hold a share of the queue that
    // an idle worker could be draining.
    let jobs = Arc::new(Mutex::new(jobs_rx));
    let mut started = 0usize;
    for worker in 0..WORKERS {
        let jobs = Arc::clone(&jobs);
        let done_tx = done_tx.clone();
        let spawn = thread::Builder::new()
            .name(format!("smo-banners-{worker}"))
            .spawn(move || run_worker(&jobs, &done_tx));
        if spawn.is_ok() {
            started += 1;
        }
    }

    if started == 0 {
        log::warn!("Could not start the banner workers; packs will show no artwork.");
        return;
    }
    runtime.sender = Some(jobs_tx);
    runtime.ready = Some(done_rx);
}

/// One worker's whole life: take the next job, fetch it, hand it back.
fn run_worker(jobs: &Mutex<Receiver<Job>>, done: &SyncSender<FetchedBanner>) {
    // The agent pools connections, so every worker sharing this host reuses
    // sockets instead of handshaking afresh.
    let agent = network::get_agent();
    loop {
        let job = {
            let Ok(rx) = jobs.lock() else {
                return;
            };
            match rx.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        };
        match fetch(&agent, job.url.as_str()) {
            Ok(bytes) => {
                let banner = FetchedBanner {
                    pack_id: job.pack_id,
                    bytes: Arc::from(bytes),
                };
                if done.send(banner).is_err() {
                    return;
                }
            }
            Err(error) => {
                // A missing picture stays missing; a transport failure may
                // not repeat, so that one is allowed another go later.
                log::debug!("Banner for pack {} failed: {error:?}", job.pack_id);
                mark_failed(job.pack_id, is_settled(&error));
            }
        }
    }
}

fn fetch(agent: &network::HttpAgent, url: &str) -> Result<Box<[u8]>, network::NetworkError> {
    let response = agent.get(url).call().map_err(network::error_from_ureq)?;
    network::read_bytes_body_bounded(response, MAX_BANNER_BYTES)
}

/// Whether a failure will say the same thing next time.
///
/// Gone, forbidden or not there: asking again is noise. So is a body over the
/// size bound -- the only way this read can fail to decode -- since it will be
/// just as big next time. Anything else -- a timeout, a reset, a 5xx -- is a
/// moment, and worth another go.
fn is_settled(error: &network::NetworkError) -> bool {
    matches!(
        error,
        network::NetworkError::HttpStatus(400 | 401 | 403 | 404 | 410)
            | network::NetworkError::Decode(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fetch state is process-wide, so the tests that reach into it have
    /// to take turns. One of them clears the whole slot map, which without
    /// this would wipe another test's setup out from under it -- a flake that
    /// looks like the retry rule misbehaving.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn exclusively() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The bound has to be large enough for a real banner and small enough to
    /// matter. SMO's are around 800x250 JPEGs, tens of kilobytes.
    #[test]
    fn the_size_bound_is_generous_for_a_banner_and_still_a_bound() {
        // The largest banner actually served to the featured strip.
        const OBSERVED_LARGEST: usize = 2_270_307;
        const { assert!(MAX_BANNER_BYTES > OBSERVED_LARGEST) };
        const { assert!(MAX_BANNER_BYTES <= 8 * 1024 * 1024) };
    }

    /// The cache has to hold more than one screen can ask for at once, or it
    /// evicts artwork that is still visible and the row asks for it again --
    /// which evicts something else that is also visible.
    #[test]
    fn the_cache_outruns_what_one_screen_asks_for() {
        // twenty-four featured cards, a list window with look-ahead either
        // side, two doubles columns, and a page of song jackets
        const ONE_SCREEN: usize = 24 + (7 + 14) + 2 * (7 + 4) + 10;
        const { assert!(MAX_CACHED >= 2 * ONE_SCREEN) };
    }

    /// A 404 stays a 404, but a timeout is not evidence of anything. The
    /// difference is what stops a dead URL becoming a request every frame
    /// while still letting a busy moment recover.
    #[test]
    fn only_an_unsettled_failure_is_ever_asked_for_again() {
        let now = Instant::now();
        let later = now + RETRY_AFTER * 2;

        assert!(worth_asking(None, now), "never asked for");
        assert!(!worth_asking(Some(Slot::Pending { attempts: 0 }), now));
        assert!(!worth_asking(Some(Slot::Done), now));

        // bytes that are not a picture: settled, and never asked again
        let settled = Slot::Failed {
            attempts: 1,
            retry_at: None,
        };
        assert!(!worth_asking(Some(settled), later));

        // a timeout: not yet, then yes
        let waiting = Slot::Failed {
            attempts: 1,
            retry_at: Some(later),
        };
        assert!(!worth_asking(Some(waiting), now), "not before the gap");
        assert!(worth_asking(Some(waiting), later), "and after it");

        // and one that has used up its attempts waits the long gap, but is
        // not given up on for the rest of the session
        let spent = Slot::Failed {
            attempts: MAX_ATTEMPTS,
            retry_at: Some(now + LONG_RETRY),
        };
        assert!(!worth_asking(Some(spent), later));
        assert!(worth_asking(Some(spent), now + LONG_RETRY));
    }

    /// Gone and forbidden are answers; a timeout or a server error is not.
    #[test]
    fn only_a_missing_picture_is_settled() {
        use network::NetworkError;
        assert!(is_settled(&NetworkError::HttpStatus(404)));
        assert!(is_settled(&NetworkError::HttpStatus(403)));
        assert!(!is_settled(&NetworkError::HttpStatus(503)));
        assert!(
            is_settled(&NetworkError::Decode("too large".to_owned())),
            "a body over the bound stays over it"
        );
        assert!(!is_settled(&NetworkError::Timeout));
        assert!(!is_settled(&NetworkError::Request("reset".to_owned())));
    }

    /// A failure with a retry a few seconds off is still coming, so it is not
    /// published: the row keeps its spinner rather than flashing "no picture"
    /// in the gap.
    #[test]
    fn a_failure_with_a_retry_due_is_not_published() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.slots.remove(&7003);
            forget_failure(&mut runtime, 7003);
        }
        mark_failed(7003, false);
        assert!(
            !failed_ids().contains(&7003),
            "one timeout is not an answer"
        );
        for _ in 1..MAX_ATTEMPTS {
            mark_failed(7003, false);
        }
        assert!(failed_ids().contains(&7003), "but running out of tries is");
    }

    /// The count survives the retry itself. Each retry puts the pack back to
    /// Pending, and a count that restarted there would never run out: the
    /// banner would spin for the whole session and never earn its long pause.
    #[test]
    fn a_retry_that_fails_again_counts_towards_the_bound() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.slots.remove(&7005);
            runtime.urls.remove(&7005);
            forget_failure(&mut runtime, 7005);
        }
        for round in 1..=MAX_ATTEMPTS {
            // what request() does when a retry is due: Pending, carrying the
            // failures so far
            {
                let mut runtime = lock_runtime();
                let attempts = match runtime.slots.get(&7005) {
                    Some(Slot::Failed { attempts, .. }) => *attempts,
                    _ => 0,
                };
                runtime.slots.insert(7005, Slot::Pending { attempts });
            }
            mark_failed(7005, false);
            assert_eq!(
                failed_ids().contains(&7005),
                round == MAX_ATTEMPTS,
                "published only once the tries are spent (round {round})"
            );
        }
        let mut runtime = lock_runtime();
        let Some(Slot::Failed {
            retry_at: Some(at), ..
        }) = runtime.slots.get(&7005).copied()
        else {
            panic!("spent, with a long retry");
        };
        assert!(at > Instant::now() + RETRY_AFTER);
        runtime.slots.remove(&7005);
        forget_failure(&mut runtime, 7005);
    }

    /// A pack whose picture failed from one source is asked again when a
    /// different source turns up.
    #[test]
    fn a_new_url_is_a_new_question() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.slots.insert(
                7004,
                Slot::Failed {
                    attempts: 1,
                    retry_at: None,
                },
            );
            runtime.urls.insert(7004, "https://a.test/x.png".to_owned());
        }
        assert!(!request(7004, "https://a.test/x.png"), "same URL, settled");
        assert!(request(7004, "https://b.test/x.png"), "a new source");
        let mut runtime = lock_runtime();
        runtime.slots.remove(&7004);
        runtime.urls.remove(&7004);
    }

    /// A row cannot tell "still arriving" from "never arriving" unless the
    /// service says so, and draws the same spinner for both -- forever, in the
    /// second case.
    #[test]
    fn a_failure_is_published_so_a_row_can_stop_spinning() {
        let _serial = exclusively();
        mark_failed(7001, true);
        assert!(failed_ids().contains(&7001));

        // and asking for it again clears the report, so a retry that works
        // does not leave a placeholder behind
        {
            let mut runtime = lock_runtime();
            forget_failure(&mut runtime, 7001);
        }
        assert!(!failed_ids().contains(&7001));
    }

    /// Attempts accumulate rather than resetting, or a URL that fails every
    /// time would be retried every time.
    #[test]
    fn repeated_failures_use_up_their_attempts() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.slots.remove(&7002);
        }
        for _ in 0..MAX_ATTEMPTS {
            mark_failed(7002, false);
        }
        let runtime = lock_runtime();
        let Some(Slot::Failed {
            attempts,
            retry_at: Some(at),
        }) = runtime.slots.get(&7002).copied()
        else {
            panic!("a spent failure still has a long retry");
        };
        assert_eq!(attempts, MAX_ATTEMPTS);
        assert!(at > Instant::now() + RETRY_AFTER, "and it is the long one");
    }

    /// Six workers is what turns a screenful of artwork from a minute of
    /// sequential round trips into about one, and the queue has to be able to
    /// hold everything one screen asks for or it drops requests it will only
    /// be asked for again next frame.
    #[test]
    fn the_pool_can_fill_a_screen_in_one_round_trip() {
        const { assert!(WORKERS >= 4) };
        // twelve featured cards, seven rows, and the look-ahead either side
        const { assert!(QUEUE_DEPTH >= 12 + 7 * 3) };
    }

    /// A pack asked about twice must only ever be fetched once, or scrolling
    /// past a row would queue it on every frame.
    #[test]
    fn a_known_pack_is_not_requested_again() {
        let _serial = exclusively();
        let mut runtime = lock_runtime();
        runtime.slots.insert(4242, Slot::Done);
        drop(runtime);
        assert!(!request(4242, "https://example.test/x.jpg"));

        let mut runtime = lock_runtime();
        runtime.slots.insert(
            4243,
            Slot::Failed {
                attempts: MAX_ATTEMPTS,
                retry_at: None,
            },
        );
        drop(runtime);
        // a settled failure is remembered too: no retry storm against a 404
        assert!(!request(4243, "https://example.test/y.jpg"));

        let mut runtime = lock_runtime();
        runtime.slots.remove(&4242);
        runtime.slots.remove(&4243);
    }

    /// What is on screen is what was asked for most recently, so it is the
    /// last thing evicted. Getting this backwards is what made the featured
    /// cards spin: fetched first, so evicted first, so fetched again.
    #[test]
    fn eviction_takes_what_was_wanted_longest_ago() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.slots.clear();
            runtime.used.clear();
            for id in 0..(MAX_CACHED as u64 + 3) {
                runtime.slots.insert(id, Slot::Done);
                // ids arrive in order, so id 0 is the oldest arrival
                runtime.used.insert(id, id);
            }
            // The clock has to start past the seeded ticks, or a later request
            // would score lower than an earlier arrival.
            runtime.clock = MAX_CACHED as u64 + 3;
        }
        // ...but the three oldest arrivals are the ones still on screen
        for id in 0..3u64 {
            request(id, "https://example.test/x.jpg");
        }

        let evicted = overflow();
        assert_eq!(evicted.len(), 3);
        for id in 0..3u64 {
            assert!(!evicted.contains(&id), "id {id} is on screen");
        }
        let runtime = lock_runtime();
        for id in 0..3u64 {
            assert!(runtime.slots.contains_key(&id), "and is still cached");
        }
    }

    #[test]
    fn eviction_only_happens_past_the_bound_and_forgets_the_slot() {
        let _serial = exclusively();
        {
            let mut runtime = lock_runtime();
            runtime.used.clear();
            runtime.slots.clear();
            runtime.clock = 0;
            for id in 0..(MAX_CACHED as u64 + 3) {
                runtime.slots.insert(id, Slot::Done);
                runtime.used.insert(id, id);
            }
        }

        let mut evicted = overflow();
        evicted.sort_unstable();
        assert_eq!(evicted, vec![0, 1, 2], "the three wanted longest ago");

        {
            let runtime = lock_runtime();
            assert_eq!(runtime.slots.len(), MAX_CACHED);
            // forgotten, so coming back re-fetches rather than showing nothing
            assert!(!runtime.slots.contains_key(&0));
            assert!(runtime.slots.contains_key(&3));
        }

        assert!(overflow().is_empty(), "at the bound, nothing is evicted");

        let mut runtime = lock_runtime();
        runtime.used.clear();
        runtime.slots.clear();
    }
}
