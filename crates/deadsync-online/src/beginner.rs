//! Packs worth handing to a beginner.
//!
//! This is the one view the catalogue cannot answer. stepmaniaonline.net
//! publishes a pack's substyle and its game types, but nothing about how hard
//! its charts are -- so "does this pack reach down to the easy blocks" can only
//! be settled by reading the pack's own page and looking at the meters.
//!
//! That costs one request per candidate, and about three in four fail, so
//! filling a screen of seven reads around thirty pages. Two things make that
//! bearable, and both are the original's:
//!
//! * **It stops.** Seven rows, then it waits to be asked for more. Nobody
//!   reads past the first screen before deciding whether the list is any good.
//! * **It remembers.** A verdict is about a pack's charts, and those do not
//!   change -- a pack that reached down to the easy blocks last month still
//!   does. The answers are kept on disk with no expiry, so the thirty requests
//!   happen once on this machine rather than once per launch.

use crate::pack_page;
use deadsync_net as network;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

/// Rows to find before pausing: one page, then on demand.
pub const FIRST_HELPING: usize = 7;
/// A stop on one walk, so a run of failures cannot become an unbounded crawl
/// of the whole ranking in a single go.
const MAX_READS_PER_WALK: usize = 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BeginnerPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct BeginnerSnapshot {
    pub phase: BeginnerPhase,
    /// Packs that passed, in the order they were considered -- which is
    /// popularity order, so the most played beginner packs lead.
    pub packs: Arc<[u64]>,
    /// How many candidates have been decided, and how many there are. The
    /// screen reports this rather than a spinner alone: a walk that reads
    /// thirty pages to find seven rows needs to say it is getting somewhere.
    pub checked: usize,
    pub candidates: usize,
    /// Every candidate has been looked at; there is no more to find.
    pub exhausted: bool,
    pub revision: u64,
    pub message: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<BeginnerSnapshot>,
    generation: u64,
    /// What has been decided, ever. Loaded from disk on first use.
    verdicts: HashMap<u64, bool>,
    loaded: bool,
    /// How many rows the reader has asked for.
    want: usize,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

#[must_use]
pub fn runtime_snapshot() -> Arc<BeginnerSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Look for beginner packs among these candidates, in this order.
///
/// The caller owns which packs are worth a request -- that needs the catalogue
/// -- so it hands over an ordered list and this decides them one at a time.
/// Safe to call every frame: a walk already running is left alone, and one
/// that has found what was asked for does nothing.
///
/// `cache` is where verdicts live between launches. The caller owns the
/// app's directories, so it says where.
pub fn runtime_walk(candidates: &[u64], cache: &Path) {
    start(candidates, cache, false);
}

/// Ask for another helping.
pub fn runtime_find_more(candidates: &[u64], cache: &Path) {
    start(candidates, cache, true);
}

fn start(candidates: &[u64], cache: &Path, more: bool) {
    let (generation, want, known) = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == BeginnerPhase::Loading {
            return;
        }
        if !runtime.loaded {
            runtime.verdicts = read_verdicts(cache);
            runtime.loaded = true;
        }
        if more {
            runtime.want += FIRST_HELPING;
        } else if runtime.want == 0 {
            runtime.want = FIRST_HELPING;
        } else {
            // Only the first walk starts unasked. One that stopped short --
            // on its read budget, or because every page failed while offline
            // -- waits for the reader to ask for more rather than starting
            // another sixty reads on the next frame.
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = BeginnerPhase::Loading;
        snapshot.candidates = candidates.len();
        snapshot.revision = snapshot.revision.wrapping_add(1);
        runtime.snapshot = Arc::new(snapshot);
        (runtime.generation, runtime.want, runtime.verdicts.clone())
    };

    let candidates = candidates.to_vec();
    let cache = cache.to_path_buf();
    let spawn = thread::Builder::new()
        .name("smo-beginner".to_owned())
        .spawn(move || walk(generation, candidates, want, known, &cache));
    if spawn.is_err() {
        let mut runtime = lock_runtime();
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = BeginnerPhase::Error;
        snapshot.message = Some("could not start the beginner walk".to_owned());
        snapshot.revision = snapshot.revision.wrapping_add(1);
        runtime.snapshot = Arc::new(snapshot);
    }
}

fn walk(
    generation: u64,
    candidates: Vec<u64>,
    want: usize,
    mut known: HashMap<u64, bool>,
    cache: &Path,
) {
    let agent = network::get_agent();
    let mut found: Vec<u64> = Vec::with_capacity(want);
    let mut fresh: Vec<(u64, bool)> = Vec::new();
    let mut reads = 0usize;
    let mut checked = 0usize;
    let mut exhausted = true;

    for pack_id in candidates {
        if found.len() >= want {
            // Stopped early, so there may well be more.
            exhausted = false;
            break;
        }
        // A verdict already on disk costs nothing, so cached candidates are
        // decided without touching the budget.
        if let Some(verdict) = known.get(&pack_id).copied() {
            checked += 1;
            if verdict {
                found.push(pack_id);
            }
            continue;
        }
        if reads >= MAX_READS_PER_WALK {
            exhausted = false;
            break;
        }
        reads += 1;
        checked += 1;

        // A page that will not load is not a verdict. It is left undecided so
        // a later walk tries again, rather than being remembered as a failure.
        let Ok(page) = pack_page::fetch_blocking(&agent, pack_id) else {
            continue;
        };
        let verdict = page.is_beginner_friendly();
        known.insert(pack_id, verdict);
        fresh.push((pack_id, verdict));
        if verdict {
            found.push(pack_id);
        }
        publish(generation, &found, checked, BeginnerPhase::Loading, false);
    }

    // Remembered before the walk reports itself done: a reader who asks for
    // more the moment the list lands would otherwise start a walk that reads
    // the same pages again.
    let verdicts = (!fresh.is_empty()).then(|| {
        let mut runtime = lock_runtime();
        for (pack_id, verdict) in fresh {
            runtime.verdicts.insert(pack_id, verdict);
        }
        runtime.verdicts.clone()
    });

    publish(generation, &found, checked, BeginnerPhase::Ready, exhausted);

    // Written once at the end rather than per verdict: the walk is dozens of
    // requests and the file is a few kilobytes, so there is nothing to gain
    // from rewriting it thirty times.
    if let Some(verdicts) = verdicts
        && let Err(error) = write_verdicts(cache, &verdicts)
    {
        log::warn!("Could not save the beginner verdicts: {error}");
    }
}

fn publish(generation: u64, found: &[u64], checked: usize, phase: BeginnerPhase, exhausted: bool) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        return;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    snapshot.phase = phase;
    snapshot.packs = Arc::from(found.to_vec());
    snapshot.checked = checked;
    snapshot.exhausted = exhausted;
    snapshot.message = None;
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

// --- the verdict file ---------------------------------------------------------

/// Read the verdicts, treating anything unreadable as none.
///
/// A missing or corrupt cache costs requests, not correctness, so there is
/// nothing here worth failing over.
fn read_verdicts(path: &Path) -> HashMap<u64, bool> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str::<HashMap<String, bool>>(text.as_str())
        .map(|raw| {
            raw.into_iter()
                .filter_map(|(id, verdict)| id.parse().ok().map(|id| (id, verdict)))
                .collect()
        })
        .unwrap_or_default()
}

fn write_verdicts(path: &Path, verdicts: &HashMap<u64, bool>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string(verdicts).map_err(|error| error.to_string())?;
    // Written beside and renamed, so an interrupted write cannot leave a
    // half-file that reads as "no verdicts" next launch.
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids are the key and the file is JSON, so they round-trip as strings --
    /// a detail worth pinning because a silent parse failure here reads as
    /// "nothing was ever decided" and costs thirty requests.
    #[test]
    fn verdicts_round_trip_through_the_file() {
        let path = std::env::temp_dir().join(format!(
            "deadsync-beginner-test-{}.json",
            std::process::id()
        ));
        let mut verdicts = HashMap::new();
        verdicts.insert(10_391u64, true);
        verdicts.insert(9_950u64, false);

        write_verdicts(&path, &verdicts).expect("write");
        let read = read_verdicts(&path);
        assert_eq!(read.get(&10_391), Some(&true));
        assert_eq!(read.get(&9_950), Some(&false));
        assert_eq!(read.len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    /// A cache that is not there, or is not JSON, costs requests rather than
    /// correctness -- so it answers "nothing decided" instead of failing.
    #[test]
    fn an_unreadable_cache_is_simply_empty() {
        let missing = std::env::temp_dir().join("deadsync-beginner-not-here.json");
        let _ = std::fs::remove_file(&missing);
        assert!(read_verdicts(&missing).is_empty());

        let junk = std::env::temp_dir().join(format!(
            "deadsync-beginner-junk-{}.json",
            std::process::id()
        ));
        std::fs::write(&junk, "not json at all").expect("write");
        assert!(read_verdicts(&junk).is_empty());
        let _ = std::fs::remove_file(&junk);
    }

    /// One page, then on demand. Reading the whole ranking to fill a screen
    /// nobody scrolls is the thing the helping exists to avoid.
    #[test]
    fn a_helping_is_one_screen_and_the_walk_is_bounded() {
        const { assert!(FIRST_HELPING == 7, "one page of the list") };
        const { assert!(MAX_READS_PER_WALK >= 4 * FIRST_HELPING) };
        const { assert!(MAX_READS_PER_WALK <= 120, "and not the whole ranking") };
    }
}

#[cfg(test)]
#[path = "beginner_verdict_perf.rs"]
mod verdict_perf_tests;
