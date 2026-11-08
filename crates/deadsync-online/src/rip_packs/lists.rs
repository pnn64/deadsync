//! The indexes the ITGmania Content Browser's site publishes for booster
//! packs, read rather than rebuilt.
//!
//! Three files, built on a schedule on the site from pages nobody wants every
//! player's machine to crawl:
//!
//! * `authors.json` -- the charters the author picker offers, best first;
//! * `pack-dates.json` -- each SMO pack's upload date and itgdb's release
//!   date;
//! * `chart-firsts.json` -- for every song a charter has in more than one
//!   SMO pack, which packs those are.
//!
//! Each is kept on disk as `<cache>/rip-packs/<file>`, beside a sidecar
//! `<file stem>.meta.json` that remembers when it was fetched and the
//! validators the site sent with it. A copy younger than its refresh age is
//! used as it is; an older one is asked for again conditionally, so an
//! unchanged file costs a few hundred bytes and a 304. When the site cannot be
//! reached the copy on disk stands, however old; with no copy at all, rips
//! still work, only with less to date songs by.
//!
//! A file is read through before it is kept: one that does not parse, or
//! lists nothing, is turned away and the copy already on disk stays. A copy on
//! disk that does not parse counts as no copy, so a spoiled one is fetched
//! again rather than trusted.
//!
//! Runtime contract:
//!
//! - Owner: the warm thread loads the two date files; whichever worker asks
//!   [`fetch_file`] does that file's request on its own thread. Searches only
//!   take snapshots.
//! - Thread safety: one mutex guards the loaded `Arc` snapshots and is never
//!   held across a request, a parse or a disk access. A condvar wakes a
//!   search waiting in [`ready`] when a copy arrives or is given up on.
//! - Capacity: one warm at a time, at most one request per date file each; a
//!   file still missing is asked for again at most every five minutes (at
//!   once when the author list is retried), and loaded ones once a day; a file
//!   is believed up to [`MAX_BYTES`].
//! - Worst frame cost: [`warm`] is a lock and a time check, and when a warm
//!   is due a thread spawn. [`current`] is a lock and two `Arc` clones.

use super::dates::{YEAR_DAYS, day_number};
use deadsync_net as network;
use serde::de::{self, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Where the site publishes them.
pub(super) const LISTS_BASE: &str = "https://itgcontent.net/booster/";
pub(super) const AUTHORS_FILE: &str = "authors.json";
pub(super) const PACK_DATES_FILE: &str = "pack-dates.json";
pub(super) const CHART_FIRSTS_FILE: &str = "chart-firsts.json";

/// The folder under the cache directory the copies live in, beside the
/// search's paid-for indexes.
const DIR: &str = "rip-packs";
/// How old the date files may get before the site is asked again. It
/// rebuilds them weekly or so; asking daily costs a 304 and picks a rebuild
/// up within a day.
const DATES_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// The biggest file believed. `chart-firsts.json`, the big one, is a little
/// over a megabyte.
const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Nothing on the frame thread waits on these requests, and a megabyte on a
/// slow line takes a while.
const TIMEOUT: Duration = Duration::from_secs(30);

static AGENT: LazyLock<network::HttpAgent> =
    LazyLock::new(|| network::build_agent(network::AgentConfig { timeout: TIMEOUT }));

/// `pack-dates.json`, as a rip reads it: each pack's upload and release
/// (`YYYYMMDD`, or for a release known only to the year `YYYY`; 0 for none),
/// and SMO's bulk days -- upload days on which at least two of the packs taken
/// in were ones itgdb dates, and half or more of those were over a year old by
/// then (relay `lib/packdates.ts` `weekly()`).
#[derive(Debug, Default)]
pub(super) struct PackDates {
    pub packs: HashMap<u64, (u32, u32)>,
    pub bulk: HashSet<u32>,
}

/// `chart-firsts.json`: the packs each song-by-a-charter is in, keyed as
/// relay `chartKey` folds them (see `dates::chart_key`).
#[derive(Debug, Default)]
pub(super) struct ChartFirsts {
    pub charts: HashMap<String, Vec<u64>>,
}

impl ChartFirsts {
    /// The packs this key is in; empty when it is in one or none.
    pub(super) fn packs(&self, key: &str) -> &[u64] {
        self.charts.get(key).map_or(&[], Vec::as_slice)
    }
}

/// What is loaded now. Either may be missing.
#[derive(Clone, Debug, Default)]
pub(super) struct Lists {
    pub dates: Option<Arc<PackDates>>,
    pub firsts: Option<Arc<ChartFirsts>>,
}

// ------------------------------------------------------------- the shelf

/// What the warm has loaded, and which files it is done with.
struct Shelf {
    lists: Lists,
    /// The first warm is done with the file: it is loaded, or there was none
    /// to be had. A search waits only on a file that is neither, so a later
    /// retry never holds a rip up.
    dates_settled: bool,
    firsts_settled: bool,
    /// A warm is running.
    warming: bool,
    /// When the last warm started; `None` before the first.
    last_warm: Option<Instant>,
}

impl Shelf {
    /// Whether a search still has something worth waiting for.
    fn waiting(&self) -> bool {
        (self.lists.dates.is_none() && !self.dates_settled)
            || (self.lists.firsts.is_none() && !self.firsts_settled)
    }
}

static SHELF: Mutex<Shelf> = Mutex::new(Shelf {
    lists: Lists {
        dates: None,
        firsts: None,
    },
    dates_settled: false,
    firsts_settled: false,
    warming: false,
    last_warm: None,
});
/// Rung whenever the shelf changes.
static ARRIVED: Condvar = Condvar::new();
/// How long a file that could not be had waits before it is asked for again:
/// long enough that a site that is down is not asked at every rip, short
/// enough that a machine that was offline at launch catches up.
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

fn lock_shelf() -> MutexGuard<'static, Shelf> {
    SHELF.lock().unwrap_or_else(|error| error.into_inner())
}

fn shelve(change: impl FnOnce(&mut Shelf)) {
    change(&mut lock_shelf());
    ARRIVED.notify_all();
}

/// Whatever is loaded right now; never blocks.
pub(super) fn current() -> Lists {
    lock_shelf().lists.clone()
}

/// Load the date indexes from disk and refresh stale ones, on a thread of
/// its own. Cheap to call often -- a lock and a time check -- and it only
/// starts work when some is due: the first call in a session; a file still
/// missing, [`RETRY_AFTER`] after the last try; or both loaded, once a day
/// has passed, so a machine left running picks up the site's rebuild.
pub(super) fn warm(cache_dir: &Path) {
    start_warm(cache_dir, false);
}

/// [`warm`], without waiting out [`RETRY_AFTER`] for a file still missing:
/// the player asked to try again.
pub(super) fn retry(cache_dir: &Path) {
    start_warm(cache_dir, true);
}

fn start_warm(cache_dir: &Path, force: bool) {
    let first = {
        let mut shelf = lock_shelf();
        if shelf.warming {
            return;
        }
        let missing = shelf.lists.dates.is_none() || shelf.lists.firsts.is_none();
        let since = shelf.last_warm.map(|at| at.elapsed());
        if !warm_due(since, missing, force) {
            return;
        }
        let first = shelf.last_warm.is_none();
        shelf.warming = true;
        shelf.last_warm = Some(Instant::now());
        first
    };
    let dir = cache_dir.join(DIR);
    let spawned = thread::Builder::new()
        .name("rip-lists".to_owned())
        .spawn(move || warm_up(&dir, first));
    if let Err(error) = spawned {
        log::warn!("Rip Packs: could not start loading the date indexes: {error}");
        // Nothing is coming, so nothing should wait for it.
        shelve(|shelf| {
            shelf.warming = false;
            shelf.dates_settled = true;
            shelf.firsts_settled = true;
        });
    }
}

/// For a search about to start: [`warm`], then wait up to `wait` for a first
/// copy of each when none is loaded yet. Worker threads only.
pub(super) fn ready(cache_dir: &Path, wait: Duration) -> Lists {
    warm(cache_dir);
    let started = Instant::now();
    let mut shelf = lock_shelf();
    while shelf.waiting() {
        let Some(left) = wait
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
        else {
            break;
        };
        shelf = ARRIVED
            .wait_timeout(shelf, left)
            .map_or_else(|error| error.into_inner().0, |(shelf, _)| shelf);
    }
    shelf.lists.clone()
}

/// Whether a warm should start, `since` the last one began (`None`: there
/// has been none): always the first time; for a file still missing, after
/// [`RETRY_AFTER`], or at once when `force`d; and with both loaded, once
/// they could be a day old.
fn warm_due(since: Option<Duration>, missing: bool, force: bool) -> bool {
    since.is_none_or(|age| (missing && (force || age >= RETRY_AFTER)) || age >= DATES_MAX_AGE)
}

/// The warm thread: both copies on disk first, offered at once, then a
/// refresh of whichever is stale or missing, offered again when it brings
/// something new. A refresh that fails leaves the copy already offered; a
/// copy loaded earlier is never taken away. `first` is the session's first
/// warm, the one that sweeps up after an earlier run.
fn warm_up(dir: &Path, first: bool) {
    if first {
        sweep(dir);
    }
    let dates_parse = |bytes: &[u8]| parse_pack_dates(bytes).map(Arc::new);
    let firsts_parse = |bytes: &[u8]| parse_chart_firsts(bytes).map(Arc::new);
    let dates = read_copy(dir, PACK_DATES_FILE, &dates_parse);
    let firsts = read_copy(dir, CHART_FIRSTS_FILE, &firsts_parse);
    let on_disk = (
        dates.as_ref().map(|copy| Arc::clone(&copy.value)),
        firsts.as_ref().map(|copy| Arc::clone(&copy.value)),
    );
    if on_disk.0.is_some() || on_disk.1.is_some() {
        shelve(|shelf| {
            if let Some(dates) = on_disk.0 {
                shelf.lists.dates = Some(dates);
            }
            if let Some(firsts) = on_disk.1 {
                shelf.lists.firsts = Some(firsts);
            }
        });
    }

    let now = unix_now();
    let always = || true;
    let dates = refresh(
        dir,
        PACK_DATES_FILE,
        dates,
        DATES_MAX_AGE,
        now,
        &always,
        &|have| get(PACK_DATES_FILE, have),
        &dates_parse,
    );
    shelve(|shelf| {
        if let Ok(Got::New(dates)) = dates {
            shelf.lists.dates = Some(dates);
        }
        shelf.dates_settled = true;
    });
    let firsts = refresh(
        dir,
        CHART_FIRSTS_FILE,
        firsts,
        DATES_MAX_AGE,
        now,
        &always,
        &|have| get(CHART_FIRSTS_FILE, have),
        &firsts_parse,
    );
    shelve(|shelf| {
        if let Ok(Got::New(firsts)) = firsts {
            shelf.lists.firsts = Some(firsts);
        }
        shelf.firsts_settled = true;
        shelf.warming = false;
    });
}

// ------------------------------------------------------------ the fetch

/// One published file's bytes: the copy on disk while younger than
/// `max_age`, else the site's (asked conditionally, and saved), else -- the
/// site not answering -- the copy on disk however old. `Err` only when there
/// is neither. `wanted` is asked before the request. Worker threads only.
pub(super) fn fetch_file(
    name: &str,
    max_age: Duration,
    cache_dir: &Path,
    wanted: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let dir = cache_dir.join(DIR);
    let parse = |bytes: &[u8]| check(name, bytes).map(|()| bytes.to_vec());
    let copy = read_copy(&dir, name, &parse);
    refresh(
        &dir,
        name,
        copy,
        max_age,
        unix_now(),
        wanted,
        &|have| get(name, have),
        &parse,
    )
    .map(Got::into_inner)
}

/// The copy on disk of `name`, when there is one that reads and it is older
/// than `max_age` -- what [`fetch_file`] would fall back to if the site did
/// not answer, offered before the site is asked so nobody waits on a slow
/// network for a list already in hand. Worker threads only.
pub(super) fn stale_copy(name: &str, max_age: Duration, cache_dir: &Path) -> Option<Vec<u8>> {
    let dir = cache_dir.join(DIR);
    let parse = |bytes: &[u8]| check(name, bytes).map(|()| bytes.to_vec());
    read_copy(&dir, name, &parse)
        .filter(|copy| !copy.meta.is_fresh(unix_now(), max_age))
        .map(|copy| copy.value)
}

/// Whether `bytes` are a file worth keeping under `name`: the same reading a
/// user of the file will give it, so what is saved is what can be used.
fn check(name: &str, bytes: &[u8]) -> Result<(), String> {
    match name {
        AUTHORS_FILE => super::authors::parse(bytes).map(|_| ()),
        PACK_DATES_FILE => parse_pack_dates(bytes).map(|_| ()),
        CHART_FIRSTS_FILE => parse_chart_firsts(bytes).map(|_| ()),
        _ => serde_json::from_slice::<IgnoredAny>(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
    }
}

/// The sidecar beside each copy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    /// The validators the site sent with the copy, sent back to ask whether
    /// it changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
    /// When the site last gave or confirmed the copy, Unix seconds.
    #[serde(default)]
    fetched_at: u64,
}

impl Meta {
    /// A stamp from the future means the clock moved back, and is not
    /// trusted.
    fn is_fresh(&self, now: u64, max_age: Duration) -> bool {
        self.fetched_at <= now && now - self.fetched_at < max_age.as_secs()
    }
}

/// A copy on disk that parsed, and its sidecar.
struct OnDisk<T> {
    value: T,
    meta: Meta,
}

/// What the site answered a conditional GET: the seam between the cache and
/// the network.
#[derive(Debug)]
enum Reply {
    /// 200: a new body, and the validators to ask about it with next time.
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    /// 304: the copy on disk is still the site's.
    NotModified,
}

/// What a refresh settled on.
#[derive(Debug, PartialEq, Eq)]
enum Got<T> {
    /// The copy on disk: fresh, confirmed, or all there is.
    Kept(T),
    /// A new body from the site, now on disk.
    New(T),
}

impl<T> Got<T> {
    fn into_inner(self) -> T {
        match self {
            Self::Kept(value) | Self::New(value) => value,
        }
    }
}

/// The cache's rules, with the request behind `ask` (which gets the copy's
/// sidecar when there is a copy, so it can ask conditionally).
#[allow(clippy::too_many_arguments)]
fn refresh<T>(
    dir: &Path,
    name: &str,
    copy: Option<OnDisk<T>>,
    max_age: Duration,
    now: u64,
    wanted: &dyn Fn() -> bool,
    ask: &dyn Fn(Option<&Meta>) -> Result<Reply, String>,
    parse: &dyn Fn(&[u8]) -> Result<T, String>,
) -> Result<Got<T>, String> {
    let fresh = copy
        .as_ref()
        .is_some_and(|copy| copy.meta.is_fresh(now, max_age));
    if fresh || !wanted() {
        return copy
            .map(|copy| Got::Kept(copy.value))
            .ok_or_else(|| format!("{name} is no longer wanted"));
    }
    let failed = |copy: Option<OnDisk<T>>, error: String| {
        copy.map(|copy| Got::Kept(copy.value)).ok_or(error)
    };
    match ask(copy.as_ref().map(|copy| &copy.meta)) {
        Ok(Reply::NotModified) => {
            let Some(mut copy) = copy else {
                // Nothing was sent to compare with, so this is no answer.
                return Err(format!(
                    "itgcontent.net answered 304 for {name}, which is not on disk"
                ));
            };
            copy.meta.fetched_at = now;
            if let Err(error) = write_meta(dir, name, &copy.meta) {
                log::warn!("Rip Packs: could not note that {name} is unchanged: {error}");
            }
            Ok(Got::Kept(copy.value))
        }
        Ok(Reply::Body {
            bytes,
            etag,
            last_modified,
        }) => match parse(&bytes) {
            Ok(value) => {
                let meta = Meta {
                    etag,
                    last_modified,
                    fetched_at: now,
                };
                if let Err(error) = save(dir, name, &bytes, &meta) {
                    // Still good for this launch; the next one asks again.
                    log::warn!("Rip Packs: could not save {name}: {error}");
                }
                Ok(Got::New(value))
            }
            Err(error) => {
                log::warn!(
                    "Rip Packs: turned away a malformed {name} from itgcontent.net: {error}"
                );
                failed(
                    copy,
                    format!("{name} from itgcontent.net is malformed: {error}"),
                )
            }
        },
        Err(error) => {
            log::warn!("Rip Packs: could not fetch {name} from itgcontent.net: {error}");
            failed(
                copy,
                format!("could not fetch {name} from itgcontent.net: {error}"),
            )
        }
    }
}

/// The real request.
fn get(name: &str, have: Option<&Meta>) -> Result<Reply, String> {
    let url = format!("{LISTS_BASE}{name}");
    let mut request = AGENT.get(url.as_str());
    if let Some(meta) = have {
        if let Some(etag) = meta.etag.as_deref() {
            request = request.header("If-None-Match", etag);
        }
        if let Some(when) = meta.last_modified.as_deref() {
            request = request.header("If-Modified-Since", when);
        }
    }
    let response = request
        .call()
        .map_err(|error| network::error_from_ureq(error).to_string())?;
    let status = response.status().as_u16();
    if status == 304 {
        return Ok(Reply::NotModified);
    }
    if status != 200 {
        return Err(format!("http status {status}"));
    }
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let etag = header("etag");
    let last_modified = header("last-modified");
    let bytes =
        network::read_bytes_body_bounded(response, MAX_BYTES).map_err(|error| error.to_string())?;
    Ok(Reply::Body {
        bytes: bytes.into_vec(),
        etag,
        last_modified,
    })
}

// ---------------------------------------------------------------- the disk

fn meta_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name).with_extension("meta.json")
}

/// The copy on disk, when there is one that parses. Its sidecar missing or
/// spoiled, it is a copy of unknown age, asked for again without validators.
fn read_copy<T>(
    dir: &Path,
    name: &str,
    parse: &dyn Fn(&[u8]) -> Result<T, String>,
) -> Option<OnDisk<T>> {
    let bytes = match std::fs::read(dir.join(name)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!("Rip Packs: could not read the copy of {name}: {error}");
            return None;
        }
    };
    let value = match parse(&bytes) {
        Ok(value) => value,
        Err(error) => {
            log::warn!(
                "Rip Packs: the copy of {name} on disk is unreadable, so it is fetched again: {error}"
            );
            return None;
        }
    };
    let meta = std::fs::read(meta_path(dir, name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    Some(OnDisk { value, meta })
}

/// The body, then its sidecar. Interrupted between the two, the new body
/// sits beside the old validators, which the site no longer matches -- the
/// next ask costs a full answer, never a wrong one.
fn save(dir: &Path, name: &str, bytes: &[u8], meta: &Meta) -> std::io::Result<()> {
    write_atomic(&dir.join(name), bytes)?;
    write_meta(dir, name, meta)
}

fn write_meta(dir: &Path, name: &str, meta: &Meta) -> std::io::Result<()> {
    let text = serde_json::to_vec(meta).map_err(std::io::Error::other)?;
    write_atomic(&meta_path(dir, name), &text)
}

/// Written beside and renamed over, so a reader never sees half a file. The
/// temporary name is the process's and the write's own: two threads saving
/// the same file each rename a whole one.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// Clear away what an earlier run's saves left when cut off before their
/// rename. This run's own temporaries may be in flight on another thread, and
/// are left be.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let ours = format!(".{}-", std::process::id());
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let mine = [AUTHORS_FILE, PACK_DATES_FILE, CHART_FIRSTS_FILE]
            .iter()
            .filter_map(|file| file.strip_suffix(".json"))
            .any(|stem| {
                name.strip_prefix(stem)
                    .is_some_and(|rest| rest.starts_with('.'))
            });
        if mine && name.ends_with(".tmp") && !name.contains(ours.as_str()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

// ------------------------------------------------------------- the parsing

#[derive(Deserialize)]
struct PackDatesFile {
    packs: Vec<PackRow>,
}

/// `[id, uploaded, released]`; anything after is a later addition, passed
/// over.
struct PackRow(u64, u32, u32);

impl<'de> Deserialize<'de> for PackRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Row;
        impl<'de> Visitor<'de> for Row {
            type Value = PackRow;
            fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("[pack id, uploaded, released]")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<PackRow, A::Error> {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let uploaded = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let released = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(PackRow(id, uploaded, released))
            }
        }
        deserializer.deserialize_seq(Row)
    }
}

/// `pack-dates.json`: `{"packs": [[id, uploaded, released], ...]}`, and the
/// bulk days counted from it as relay `weekly()` counts them -- every row,
/// its dates as written.
fn parse_pack_dates(bytes: &[u8]) -> Result<PackDates, String> {
    let file: PackDatesFile = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if file.packs.is_empty() {
        return Err("it lists no packs".to_owned());
    }
    let mut packs = HashMap::with_capacity(file.packs.len());
    // Upload day -> (packs itgdb dates, of them over a year old by then).
    let mut tally: HashMap<u32, (u32, u32)> = HashMap::new();
    for PackRow(id, uploaded, released) in file.packs {
        packs.insert(id, (uploaded, released));
        if uploaded == 0 || released == 0 {
            continue;
        }
        let day = tally.entry(uploaded).or_default();
        day.0 += 1;
        if day_number(uploaded) - day_number(released) > YEAR_DAYS {
            day.1 += 1;
        }
    }
    let bulk = tally
        .into_iter()
        .filter(|&(_, (dated, old))| dated >= 2 && old * 2 >= dated)
        .map(|(day, _)| day)
        .collect();
    Ok(PackDates { packs, bulk })
}

#[derive(Deserialize)]
struct ChartFirstsFile {
    charts: Vec<ChartRow>,
}

/// `["key", pack id, pack id, ...]`.
struct ChartRow(String, Vec<u64>);

impl<'de> Deserialize<'de> for ChartRow {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Row;
        impl<'de> Visitor<'de> for Row {
            type Value = ChartRow;
            fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("[key, pack id, ...]")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ChartRow, A::Error> {
                let key = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let mut packs = Vec::new();
                while let Some(id) = seq.next_element()? {
                    packs.push(id);
                }
                Ok(ChartRow(key, packs))
            }
        }
        deserializer.deserialize_seq(Row)
    }
}

/// `chart-firsts.json`: `{"charts": [["key", id, ...], ...]}`. A key listed
/// twice keeps its last row, as the relay's map does.
fn parse_chart_firsts(bytes: &[u8]) -> Result<ChartFirsts, String> {
    let file: ChartFirstsFile = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if file.charts.is_empty() {
        return Err("it lists no charts".to_owned());
    }
    let charts = file
        .charts
        .into_iter()
        .map(|ChartRow(key, packs)| (key, packs))
        .collect();
    Ok(ChartFirsts { charts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Rows of the live `pack-dates.json`, and one made up (pack 40) so that
    /// 2016-09-13 took in three packs itgdb dates, two of them years old: a
    /// bulk day.
    const PACK_DATES: &str = r#"{"generated":"2026-10-17","matched":1619,"packs":[
[1,20120428,0],
[2,20260125,20240203],
[4,20260410,20231028],
[5,20160816,2016],
[8,20160913,2014],
[15,20160913,2014],
[16,20181220,20181223],
[40,20160913,20160901],
[10782,20261016,0]
]}
"#;

    const CHART_FIRSTS: &str = r#"{"generated":"2026-10-17","packs":9831,"charts":[
["00heaven|naoki",3628,8887],
["09chatterbox|benpai",3224,9598,9696],
["09dogjam|mimmons",9598,9696]
]}
"#;

    const AUTHORS: &str = r#"{
 "built": "2026-10-17",
 "authors": [
  {"name": "Zaia", "packs": 30, "popular": 14, "score": 500.33, "last": "2026-10-03"},
  {"name": "dimo", "packs": 10, "popular": 8, "score": 342.17, "last": "2026-10-03"}
 ]
}"#;

    const NEWER_AUTHORS: &str = r#"{"authors": [{"name": "freyja", "packs": 13, "popular": 9}]}"#;

    const DAY: u64 = 24 * 60 * 60;
    const NOW: u64 = 1_792_268_721;

    /// A folder of its own for each test, gone afterwards.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("deadsync-rip-lists-{test}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn put(&self, name: &str, body: &str, meta: Option<&Meta>) {
            std::fs::create_dir_all(&self.0).expect("a folder");
            std::fs::write(self.0.join(name), body).expect("written");
            if let Some(meta) = meta {
                write_meta(&self.0, name, meta).expect("written");
            }
        }

        fn body(&self, name: &str) -> Option<String> {
            std::fs::read_to_string(self.0.join(name)).ok()
        }

        fn meta(&self, name: &str) -> Option<Meta> {
            let bytes = std::fs::read(meta_path(&self.0, name)).ok()?;
            serde_json::from_slice(&bytes).ok()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fetched(at: u64) -> Meta {
        Meta {
            etag: Some("\"5668012c\"".to_owned()),
            last_modified: Some("Sat, 17 Oct 2026 20:16:26 GMT".to_owned()),
            fetched_at: at,
        }
    }

    fn body(text: &str) -> Reply {
        Reply::Body {
            bytes: text.as_bytes().to_vec(),
            etag: Some("\"new\"".to_owned()),
            last_modified: None,
        }
    }

    fn author_names(bytes: &[u8]) -> Vec<String> {
        super::super::authors::parse(bytes)
            .expect("an author list")
            .into_iter()
            .map(|author| author.name)
            .collect()
    }

    /// What a fetch of `authors.json` settled on: the names.
    type Fetched = Result<Got<Vec<String>>, String>;

    /// The fetch the way [`fetch_file`] runs it for `authors.json`, with the
    /// site's answer scripted. Returns the outcome and the sidecar each
    /// request was asked with.
    fn fetch(
        scratch: &Scratch,
        answer: impl Fn() -> Result<Reply, String>,
    ) -> (Fetched, Vec<Option<Meta>>) {
        let parse = |bytes: &[u8]| -> Result<Vec<String>, String> {
            check(AUTHORS_FILE, bytes)?;
            Ok(author_names(bytes))
        };
        let asked = RefCell::new(Vec::new());
        let copy = read_copy(&scratch.0, AUTHORS_FILE, &parse);
        let got = refresh(
            &scratch.0,
            AUTHORS_FILE,
            copy,
            DATES_MAX_AGE,
            NOW,
            &|| true,
            &|have| {
                asked.borrow_mut().push(have.cloned());
                answer()
            },
            &parse,
        );
        (got, asked.into_inner())
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn a_missing_file_is_asked_for_again_and_a_loaded_one_daily() {
        let minute = Duration::from_secs(60);
        // The first warm always runs.
        assert!(warm_due(None, false, false));
        assert!(warm_due(None, true, false));
        // A file still missing waits out the retry delay, unless asked.
        assert!(!warm_due(Some(minute), true, false));
        assert!(warm_due(Some(minute), true, true));
        assert!(warm_due(Some(RETRY_AFTER), true, false));
        // Both loaded: nothing until a day has passed, asked or not.
        assert!(!warm_due(Some(RETRY_AFTER), false, false));
        assert!(!warm_due(Some(minute), false, true));
        assert!(warm_due(Some(DATES_MAX_AGE), false, false));
    }

    #[test]
    fn pack_dates_read_rows_and_count_bulk_days() {
        let dates = parse_pack_dates(PACK_DATES.as_bytes()).expect("pack dates");
        assert_eq!(dates.packs.len(), 9);
        assert_eq!(dates.packs[&2], (20260125, 20240203));
        assert_eq!(
            dates.packs[&5],
            (20160816, 2016),
            "a year alone stays a year"
        );
        assert_eq!(dates.packs[&1], (20120428, 0));
        // 2016-09-13: three dated, two over a year old -- bulk. 2026-01-25
        // and 2026-04-10 took in one dated pack each, too few to say.
        assert_eq!(dates.bulk, HashSet::from([20160913]));
    }

    #[test]
    fn a_bulk_day_needs_two_dated_packs_and_half_of_them_old() {
        let file = |rows: &str| parse_pack_dates(format!(r#"{{"packs":[{rows}]}}"#).as_bytes());
        // Half old is enough.
        let half = file("[1,20200101,20100101],[2,20200101,20191201]").expect("dates");
        assert_eq!(half.bulk, HashSet::from([20200101]));
        // One old of three is not.
        let third = file("[1,20200101,20100101],[2,20200101,20191201],[3,20200101,20191215]")
            .expect("dates");
        assert!(third.bulk.is_empty());
        // Packs itgdb does not date do not count either way.
        let undated = file("[1,20200101,20100101],[2,20200101,0],[3,20200101,0]").expect("dates");
        assert!(undated.bulk.is_empty());
        // A year alone is its first day: 2018 to 2019-01-02 is 366 days,
        // not over a year; to 2019-01-03 it is.
        let edge = file("[1,20190102,2018],[2,20190102,2018],[3,20190103,2018],[4,20190103,2018]")
            .expect("dates");
        assert_eq!(edge.bulk, HashSet::from([20190103]));
    }

    #[test]
    fn date_files_tolerate_additions_and_turn_away_damage() {
        let extra = r#"{"packs":[[7,20200101,2019,"itgdb"]],"source":"itgdb.net"}"#;
        let dates = parse_pack_dates(extra.as_bytes()).expect("extra fields are passed over");
        assert_eq!(dates.packs[&7], (20200101, 2019));
        for spoiled in [
            "",
            "{\"packs\":[[1,20200101,0],[2,2020",
            "{\"packs\":[]}",
            "{\"generated\":\"2026-10-17\"}",
            "{\"packs\":[[1,20200101]]}",
            "{\"packs\":[[1,\"20200101\",0]]}",
        ] {
            assert!(parse_pack_dates(spoiled.as_bytes()).is_err(), "{spoiled:?}");
        }

        let firsts = parse_chart_firsts(CHART_FIRSTS.as_bytes()).expect("chart firsts");
        assert_eq!(firsts.charts.len(), 3);
        assert_eq!(firsts.packs("09chatterbox|benpai"), &[3224, 9598, 9696]);
        assert!(firsts.packs("nothing|nobody").is_empty());
        let twice = r#"{"charts":[["a|b",1,2],["a|b",3,4]]}"#;
        let twice = parse_chart_firsts(twice.as_bytes()).expect("chart firsts");
        assert_eq!(
            twice.packs("a|b"),
            &[3, 4],
            "the last row stands, as in a JS map"
        );
        for spoiled in [
            "{\"charts\":[]}",
            "{\"charts\":[[12,1,2]]}",
            "{\"charts\":[[\"a|b\",\"one\"]]}",
            "{\"charts\":[[\"a|b\",1,2]",
        ] {
            assert!(
                parse_chart_firsts(spoiled.as_bytes()).is_err(),
                "{spoiled:?}"
            );
        }
    }

    #[test]
    fn check_reads_each_file_as_its_user_will() {
        assert!(check(AUTHORS_FILE, AUTHORS.as_bytes()).is_ok());
        assert!(check(AUTHORS_FILE, PACK_DATES.as_bytes()).is_err());
        assert!(check(PACK_DATES_FILE, PACK_DATES.as_bytes()).is_ok());
        assert!(check(PACK_DATES_FILE, CHART_FIRSTS.as_bytes()).is_err());
        assert!(check(CHART_FIRSTS_FILE, CHART_FIRSTS.as_bytes()).is_ok());
        assert!(check("other.json", b"{\"any\": 1}").is_ok());
        assert!(check("other.json", b"<html>").is_err());
    }

    #[test]
    fn a_fresh_copy_is_used_without_asking() {
        let scratch = Scratch::new("fresh");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - DAY + 60)));
        let (got, asked) = fetch(&scratch, || panic!("asked"));
        assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))));
        assert!(asked.is_empty());
    }

    #[test]
    fn a_stale_copy_is_asked_about_and_a_304_keeps_it_another_day() {
        let scratch = Scratch::new("stale-304");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - DAY)));
        let (got, asked) = fetch(&scratch, || Ok(Reply::NotModified));
        assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))));
        assert_eq!(
            asked,
            vec![Some(fetched(NOW - DAY))],
            "asked with its validators"
        );
        assert_eq!(scratch.meta(AUTHORS_FILE), Some(fetched(NOW)));
        assert_eq!(scratch.body(AUTHORS_FILE).as_deref(), Some(AUTHORS));
        // Now fresh again.
        let (_, asked) = fetch(&scratch, || panic!("asked"));
        assert!(asked.is_empty());
    }

    #[test]
    fn a_stale_copy_gives_way_to_a_new_body() {
        let scratch = Scratch::new("stale-new");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - 3 * DAY)));
        let (got, _) = fetch(&scratch, || Ok(body(NEWER_AUTHORS)));
        assert_eq!(got, Ok(Got::New(names(&["freyja"]))));
        assert_eq!(scratch.body(AUTHORS_FILE).as_deref(), Some(NEWER_AUTHORS));
        assert_eq!(
            scratch.meta(AUTHORS_FILE),
            Some(Meta {
                etag: Some("\"new\"".to_owned()),
                last_modified: None,
                fetched_at: NOW,
            })
        );
        let leftovers: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("the folder")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temporary files left behind");
    }

    #[test]
    fn a_missing_copy_is_fetched_without_validators() {
        let scratch = Scratch::new("missing");
        let (got, asked) = fetch(&scratch, || Ok(body(AUTHORS)));
        assert_eq!(got, Ok(Got::New(names(&["Zaia", "dimo"]))));
        assert_eq!(asked, vec![None]);
        assert_eq!(scratch.body(AUTHORS_FILE).as_deref(), Some(AUTHORS));
    }

    #[test]
    fn a_failure_falls_back_to_the_copy_however_old() {
        let scratch = Scratch::new("failure-copy");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - 400 * DAY)));
        let (got, _) = fetch(&scratch, || Err("timed out".to_owned()));
        assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))));
        assert_eq!(
            scratch.meta(AUTHORS_FILE),
            Some(fetched(NOW - 400 * DAY)),
            "still stale, so the next try asks again"
        );
    }

    #[test]
    fn a_failure_with_no_copy_is_an_error() {
        let scratch = Scratch::new("failure-none");
        let (got, _) = fetch(&scratch, || Err("http status 503".to_owned()));
        assert!(got.is_err());
        assert_eq!(scratch.body(AUTHORS_FILE), None);
        // A 304 to a request that sent no validators is no answer either.
        let (got, _) = fetch(&scratch, || Ok(Reply::NotModified));
        assert!(got.is_err());
    }

    #[test]
    fn a_malformed_new_body_keeps_the_old_copy() {
        let scratch = Scratch::new("malformed");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - 2 * DAY)));
        for spoiled in [
            "<html>502</html>",
            "{\"authors\": []}",
            "{\"authors\": [{\"na",
        ] {
            let (got, _) = fetch(&scratch, || Ok(body(spoiled)));
            assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))), "{spoiled:?}");
            assert_eq!(scratch.body(AUTHORS_FILE).as_deref(), Some(AUTHORS));
            assert_eq!(scratch.meta(AUTHORS_FILE), Some(fetched(NOW - 2 * DAY)));
        }
        // With no copy to fall back on, it is an error and nothing is saved.
        let empty = Scratch::new("malformed-none");
        let (got, _) = fetch(&empty, || Ok(body("<html>")));
        assert!(got.is_err());
        assert_eq!(empty.body(AUTHORS_FILE), None);
    }

    #[test]
    fn a_spoiled_copy_on_disk_counts_as_none() {
        let scratch = Scratch::new("spoiled");
        // What the old client-side author crawler kept under the same name.
        scratch.put(
            AUTHORS_FILE,
            r#"{"version":1,"walked":1700000000,"packs":{"3799":{"date":"2025-06-01"}}}"#,
            Some(&fetched(NOW)),
        );
        let (got, asked) = fetch(&scratch, || Ok(body(AUTHORS)));
        assert_eq!(got, Ok(Got::New(names(&["Zaia", "dimo"]))));
        assert_eq!(
            asked,
            vec![None],
            "no validators for a copy that is not one"
        );
    }

    #[test]
    fn a_copy_without_its_sidecar_or_from_the_future_is_stale() {
        let scratch = Scratch::new("no-sidecar");
        scratch.put(AUTHORS_FILE, AUTHORS, None);
        let (got, asked) = fetch(&scratch, || Ok(Reply::NotModified));
        assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))));
        assert_eq!(asked, vec![Some(Meta::default())]);

        let future = Scratch::new("future");
        future.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW + DAY)));
        let (_, asked) = fetch(&future, || Ok(Reply::NotModified));
        assert_eq!(
            asked.len(),
            1,
            "a clock moved back does not keep a copy fresh"
        );
    }

    #[test]
    fn an_unwanted_fetch_asks_nothing() {
        let scratch = Scratch::new("unwanted");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW - 9 * DAY)));
        let parse = |bytes: &[u8]| -> Result<Vec<String>, String> { Ok(author_names(bytes)) };
        let asks = Cell::new(0);
        let ask = |_: Option<&Meta>| -> Result<Reply, String> {
            asks.set(asks.get() + 1);
            Ok(Reply::NotModified)
        };
        let copy = read_copy(&scratch.0, AUTHORS_FILE, &parse);
        let got = refresh(
            &scratch.0,
            AUTHORS_FILE,
            copy,
            DATES_MAX_AGE,
            NOW,
            &|| false,
            &ask,
            &parse,
        );
        assert_eq!(got, Ok(Got::Kept(names(&["Zaia", "dimo"]))));
        let got = refresh(
            &scratch.0,
            "gone.json",
            None,
            DATES_MAX_AGE,
            NOW,
            &|| false,
            &ask,
            &parse,
        );
        assert!(got.is_err());
        assert_eq!(asks.get(), 0);
    }

    #[test]
    fn cut_off_saves_of_an_earlier_run_are_swept() {
        let scratch = Scratch::new("sweep");
        scratch.put(AUTHORS_FILE, AUTHORS, Some(&fetched(NOW)));
        let earlier = [
            "authors.1-0.tmp",
            "pack-dates.meta.1-3.tmp",
            "chart-firsts.1-7.tmp",
        ];
        let ours = format!("authors.{}-0.tmp", std::process::id());
        let others = ["12-345.idx.tmp", "notes.tmp"];
        for name in earlier.iter().chain(&others).chain([&ours.as_str()]) {
            std::fs::write(scratch.0.join(name), b"x").expect("written");
        }
        sweep(&scratch.0);
        for name in earlier {
            assert!(!scratch.0.join(name).exists(), "{name} swept");
        }
        for name in others.iter().chain([&ours.as_str()]) {
            assert!(scratch.0.join(name).exists(), "{name} left be");
        }
        assert_eq!(scratch.body(AUTHORS_FILE).as_deref(), Some(AUTHORS));
        assert_eq!(scratch.meta(AUTHORS_FILE), Some(fetched(NOW)));
    }

    #[test]
    fn a_search_waits_only_for_files_still_coming() {
        let mut shelf = Shelf {
            lists: Lists::default(),
            dates_settled: false,
            firsts_settled: false,
            warming: false,
            last_warm: None,
        };
        assert!(shelf.waiting());
        shelf.lists.dates = Some(Arc::new(PackDates::default()));
        assert!(shelf.waiting(), "chart firsts are still coming");
        shelf.firsts_settled = true;
        assert!(!shelf.waiting(), "given up on for this launch");
        shelf.lists.dates = None;
        assert!(shelf.waiting());
        shelf.dates_settled = true;
        assert!(!shelf.waiting());
    }
}
