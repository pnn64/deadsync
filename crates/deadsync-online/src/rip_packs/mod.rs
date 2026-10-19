//! Rip Packs: booster packs, the trading-card way to download songs.
//!
//! A rip is fifteen songs, each from a different pack on stepmaniaonline.net,
//! picked at random within a few limits -- the era its Expert chart came out
//! in, who charted it, how hard it is, and whether the song has every singles
//! difficulty. Nothing is downloaded until the player keeps the rip; then the
//! songs come, a few at a time, into a pack folder of their own, `Content
//! Booster Pack <n> - <date>`, all null-synced.
//!
//! Three services live here, sharing one runtime:
//!
//! * **Search** ([`search`]) -- finds the songs. Free pages decide first: a
//!   pack's page, a song's page and SMO's own description of its Expert
//!   chart. Only a song that passes all of them costs a counted request (the
//!   pack's zip index, for the song's folder, size and dates). Songs are
//!   dated as the ITGmania version dates them ([`dates`]). A new rip, a
//!   reset or leaving the screen makes a running search stale: it sends
//!   nothing more and its answer lands nowhere.
//! * **Keep** ([`keep`]) -- numbers the pack, downloads each song's folder
//!   (one counted request each, with the pack's `Pack.ini` riding along),
//!   moves ITG-synced songs to NULL, asks itgcontent.net for the pack's own
//!   banner, and files the pack.
//! * **Authors** ([`authors`]) -- the charters the author picker offers.
//! * **Share** ([`share`]) -- tells itgcontent.net about each rip, for its
//!   list of recent rips, and marks the kept ones kept there; only while the
//!   machine's Share Rips option is on.
//!
//! What would take a crawl of the site -- the author list, and the two
//! indexes songs are dated by -- the ITGmania Content Browser's site builds
//! on a schedule and publishes; [`lists`] reads those files and keeps them on
//! disk.
//!
//! **Etiquette.** Every request to `/download/pack/<id>/` counts as a download
//! of that pack on stepmaniaonline.net, `HEAD` included. A search spends at
//! most [`search`]'s counted budget, and only on songs that already passed
//! every check the free pages can answer; indexes it paid for are kept on disk
//! so the same pack is never paid for twice.
//!
//! Runtime contract:
//!
//! - Owner: the search, keep and author threads; the UI only snapshots.
//! - Thread safety: one mutex guards immutable `Arc` snapshots, never held
//!   across a request or a disk access. The search generation is an atomic
//!   too, so a worker can ask "am I still wanted" before every request
//!   without the lock.
//! - Capacity: one search, one keep and one author build at a time -- a keep
//!   fetching a few songs at once, each on a thread of its own ([`keep`]); a
//!   short thread per request to itgcontent.net about a rip ([`share`]).
//! - Worst frame cost: one mutex acquisition plus an `Arc` clone. Nothing here
//!   touches the network or the disk on the caller's thread.

mod authors;
mod bpm;
mod dates;
mod keep;
mod lists;
mod pages;
mod search;
mod share;

pub use bpm::{CardBpm, Pace};

use crate::smo_songs::PackCache;
use crate::stepmaniaonline::PackInfo;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

/// Songs in a booster.
pub const BOOSTER_SONGS: usize = 15;
/// The Expert meter range a rip may ask for, and where it starts.
pub const METER_FLOOR: u32 = 1;
pub const METER_CEILING: u32 = 30;
pub const DEFAULT_LO: u32 = 8;
pub const DEFAULT_HI: u32 = 14;
/// Every booster folder's name starts with this, then its number.
pub const BOOSTER_PREFIX: &str = "Content Booster Pack ";

// ------------------------------------------------------------------ options

/// The eras a rip can keep to, by when a song's Expert chart came out, as
/// its pack, its own files and its other packs date it. Any of them can be
/// ticked together; none is every era.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Era {
    Modern,
    Mid,
    Vintage,
}

impl Era {
    /// The picker's order.
    pub const ALL: [Self; 3] = [Self::Modern, Self::Mid, Self::Vintage];

    /// The picker's row.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Modern => "Modern (2020-present)",
            Self::Mid => "Mid-era (2015-2020)",
            Self::Vintage => "Vintage (before 2015)",
        }
    }

    /// The option row's value and the pack's recipe line.
    #[must_use]
    pub const fn short(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::Mid => "Mid-era",
            Self::Vintage => "Vintage",
        }
    }

    /// From, and until (not included), as `YYYYMMDD`.
    #[must_use]
    pub const fn days(self) -> (u32, u32) {
        match self {
            Self::Vintage => (0, 20150101),
            Self::Mid => (20150101, 20200101),
            Self::Modern => (20200101, 99999999),
        }
    }

    /// Whether `day` (`YYYYMMDD`) is in this era.
    #[must_use]
    pub const fn holds(self, day: u32) -> bool {
        let (from, until) = self.days();
        day >= from && day < until
    }
}

/// What the player asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RipOptions {
    /// None, or all three, is every era.
    pub eras: Vec<Era>,
    /// Whole chart credits, any one of which the Expert chart must be by.
    /// Empty is anyone. Chosen and typed names together, each once.
    pub authors: Vec<String>,
    /// Whole chart credits whose Expert charts are left out.
    pub avoid: Vec<String>,
    /// The Expert (singles Challenge) meter range, both ends included.
    pub lo: u32,
    pub hi: u32,
    /// Only songs with all five singles charts, Beginner through Challenge.
    pub full: bool,
}

impl Default for RipOptions {
    fn default() -> Self {
        Self {
            eras: Vec::new(),
            authors: Vec::new(),
            avoid: Vec::new(),
            lo: DEFAULT_LO,
            hi: DEFAULT_HI,
            full: false,
        }
    }
}

impl RipOptions {
    /// The eras that actually limit anything: none when none or all are
    /// ticked.
    #[must_use]
    pub fn limiting_eras(&self) -> Vec<Era> {
        let mut eras: Vec<Era> = Era::ALL
            .into_iter()
            .filter(|era| self.eras.contains(era))
            .collect();
        if eras.len() == Era::ALL.len() {
            eras.clear();
        }
        eras
    }
}

/// Whether two credits are the same name: the whole credit, case aside.
#[must_use]
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Typed names: split on commas, trimmed, blanks dropped, each once (case
/// aside), in the order typed.
#[must_use]
pub fn split_names(typed: &str) -> Vec<String> {
    merge_names(&[], typed)
}

/// Chosen names, then typed ones, each once (case aside).
#[must_use]
pub fn merge_names(chosen: &[String], typed: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let typed = typed.split(',');
    for name in chosen.iter().map(String::as_str).chain(typed) {
        let name = name.trim();
        if name.is_empty() || out.iter().any(|have| same_name(have, name)) {
            continue;
        }
        out.push(name.to_owned());
    }
    out
}

/// Whether a folder in the Songs directory is a booster this module made.
#[must_use]
pub fn is_booster_folder(name: &str) -> bool {
    booster_number(name).is_some()
}

/// The number in `Content Booster Pack <n>...`, case aside.
#[must_use]
pub fn booster_number(name: &str) -> Option<u32> {
    let head = name.get(..BOOSTER_PREFIX.len())?;
    if !head.eq_ignore_ascii_case(BOOSTER_PREFIX) {
        return None;
    }
    let rest = &name[BOOSTER_PREFIX.len()..];
    let end = rest
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// The sync a pack declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredSync {
    Itg,
    Null,
}

impl DeclaredSync {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Itg => "ITG",
            Self::Null => "NULL",
        }
    }
}

/// What the catalogue's sync column says: `null` and `0` are NULL, `9ms` and
/// `itg` are ITG, and anything else -- `n/a` above all -- says nothing.
#[must_use]
pub fn declared_sync(pack: &PackInfo) -> Option<DeclaredSync> {
    let value = pack.sync.as_deref()?.trim();
    if value.eq_ignore_ascii_case("null") || value == "0" {
        Some(DeclaredSync::Null)
    } else if value.eq_ignore_ascii_case("9ms") || value.eq_ignore_ascii_case("itg") {
        Some(DeclaredSync::Itg)
    } else {
        None
    }
}

// ------------------------------------------------------------------- songs

/// One card: a song the search found, as the stage shows it and KEEP fetches
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RipSong {
    /// The song's identity, which a re-rip sends back to leave it out:
    /// `<pack id>:<song id>`.
    pub key: String,
    pub pack_id: u64,
    /// The pack it came from, as the catalogue names it.
    pub pack_name: String,
    pub song_id: u64,
    pub title: String,
    pub artist: String,
    /// Who charted its Expert chart, as SMO credits it. May be empty.
    pub credit: String,
    /// That chart's meter.
    pub meter: u32,
    /// The year its Expert chart came out, or the best guess at it; `None`
    /// when nothing dates it.
    pub year: Option<u16>,
    /// The song's tempo, as SMO prints it and as it plays.
    pub bpm: CardBpm,
    /// The song's picture on SMO, for the card.
    pub art_url: Option<String>,
    /// The song's folder in the pack zip.
    pub folder: String,
    /// What downloading the folder weighs, `.oldsync` backups left out.
    pub bytes: u64,
}

/// What a rip found the song in, held from the search to KEEP so the pack's
/// index is never paid for twice.
pub(crate) struct HeldSong {
    pub(crate) cache: Arc<PackCache>,
    /// The folder's index in `cache.index.folders`.
    pub(crate) folder: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RipPhase {
    /// No rip on the table.
    #[default]
    Idle,
    Searching,
    /// `songs` is the rip on the table.
    Ready,
    /// The search found nothing; `message` says why.
    Failed,
}

/// One card's song, once the rip is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongState {
    Waiting,
    Downloading,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepPhase {
    Filling,
    /// At least one song got in.
    Done,
    /// None did, or the pack could not be made.
    Failed,
}

/// A kept rip on its way into the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepState {
    /// The rip it kept: [`RipSnapshot::set`] at the time.
    pub set: u64,
    /// `Content Booster Pack <n> - <date>`, once numbered; empty before.
    pub folder: String,
    pub phase: KeepPhase,
    /// One per card, in card order.
    pub songs: Arc<[SongState]>,
    /// Bytes downloaded so far, over every song.
    pub done_bytes: u64,
    /// What every song weighs together.
    pub total_bytes: u64,
    pub got: usize,
    pub failed: usize,
    /// The finished pack folder, once it is in the Songs folder.
    pub group_dir: Option<PathBuf>,
    pub message: Option<String>,
}

impl KeepState {
    #[must_use]
    pub fn filling(&self) -> bool {
        self.phase == KeepPhase::Filling
    }
}

#[derive(Clone, Debug, Default)]
pub struct RipSnapshot {
    pub phase: RipPhase,
    /// Which rip this is. Bumped by every new search, so a card's state is
    /// always read against the rip it belongs to.
    pub set: u64,
    /// The cards, in the order they were found.
    pub songs: Arc<[RipSong]>,
    /// Fewer than [`BOOSTER_SONGS`] matched.
    pub short: bool,
    pub message: Option<String>,
    /// The kept rip, if any: this set's, or an earlier one still filling or
    /// finished.
    pub keep: Option<Arc<KeepState>>,
    /// Boosters this machine has kept, once the history has been read.
    pub kept_count: Option<u32>,
    pub revision: u64,
}

impl RipSnapshot {
    /// Whether a kept rip is still downloading.
    #[must_use]
    pub fn keeping(&self) -> bool {
        self.keep.as_ref().is_some_and(|keep| keep.filling())
    }

    /// The keep of the rip on the table, if it was kept.
    #[must_use]
    pub fn keep_of_set(&self) -> Option<&KeepState> {
        self.keep
            .as_deref()
            .filter(|keep| keep.set == self.set && self.phase == RipPhase::Ready)
    }
}

// ----------------------------------------------------------------- authors

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthorsPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

/// A charter the author picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RipAuthor {
    pub name: String,
    /// How many of the packs they released in the window are on arrowcloud's
    /// most-played list now.
    pub popular: u32,
}

#[derive(Clone, Debug, Default)]
pub struct AuthorsSnapshot {
    pub phase: AuthorsPhase,
    /// Best first.
    pub authors: Arc<[RipAuthor]>,
    pub revision: u64,
    pub message: Option<String>,
}

// ------------------------------------------------------------------ runtime

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<RipSnapshot>,
    /// What the rip on the table was found in, card for card.
    held: Vec<Arc<HeldSong>>,
    /// What the running search was asked for, for the report its answer
    /// makes to itgcontent.net.
    asked: Option<share::Asked>,
    /// What the rip on the table was asked for, for the banner its pack gets
    /// when it is kept: the options as the search ran with them.
    ripped_with: Option<RipOptions>,
    authors: Arc<AuthorsSnapshot>,
    authors_generation: u64,
    history_asked: bool,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));
/// The search that is wanted. Bumped only under `RUNTIME`'s lock.
static SEARCH_GENERATION: AtomicU64 = AtomicU64::new(0);

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

fn publish(runtime: &mut RuntimeState, change: impl FnOnce(&mut RipSnapshot)) {
    let mut snapshot = (*runtime.snapshot).clone();
    change(&mut snapshot);
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

#[must_use]
pub fn runtime_snapshot() -> Arc<RipSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Whether rips are reported to itgcontent.net: the machine's Share Rips
/// option, which the shell hands over at startup and whenever the machine
/// config changes. An atomic store.
pub fn runtime_set_sharing(enabled: bool) {
    share::SHARE.set_on(enabled);
}

/// Rip a pack: start looking for songs within `options`, leaving out the
/// songs whose keys are in `exclude` -- a re-rip's thrown-back cards.
///
/// Refused while a search is running or a kept rip is still downloading. The
/// cards on the table, if any, are thrown back at once.
pub fn runtime_rip(
    options: RipOptions,
    exclude: &[String],
    cache_dir: &Path,
) -> Result<(), String> {
    let mut options = options;
    options.lo = options.lo.clamp(METER_FLOOR, METER_CEILING);
    options.hi = options.hi.clamp(METER_FLOOR, METER_CEILING);
    if options.lo > options.hi {
        std::mem::swap(&mut options.lo, &mut options.hi);
    }
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == RipPhase::Searching || runtime.snapshot.keeping() {
            return Err("a pack is still being ripped".to_owned());
        }
        let generation = SEARCH_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        runtime.held.clear();
        runtime.asked = Some(share::Asked {
            options: options.clone(),
            thrown: exclude.to_vec(),
        });
        runtime.ripped_with = Some(options.clone());
        share::SHARE.forget();
        publish(&mut runtime, |snapshot| {
            snapshot.phase = RipPhase::Searching;
            snapshot.set = snapshot.set.wrapping_add(1);
            snapshot.songs = Arc::from(Vec::new());
            snapshot.short = false;
            snapshot.message = None;
        });
        generation
    };
    let job = search::SearchJob {
        generation,
        options,
        exclude: exclude.iter().cloned().collect::<HashSet<String>>(),
        cache_dir: cache_dir.to_path_buf(),
    };
    let spawned = thread::Builder::new()
        .name("rip-search".to_owned())
        .spawn(move || search::run(job));
    if let Err(error) = spawned {
        search_finished(generation, Err(format!("could not start the rip: {error}")));
    }
    Ok(())
}

/// Stop a running search -- the player left, or reset. It sends nothing more,
/// and whatever it finds lands nowhere. The table goes back to unopened. A
/// re-rip still tells itgcontent.net which cards it threw back.
pub fn runtime_stop_search() {
    let owed = stop_search(&mut lock_runtime());
    send_owed(&share::SHARE, owed);
}

/// [`runtime_stop_search`] under the lock: the report the stopped search
/// still owes, sent once the lock is let go.
fn stop_search(runtime: &mut RuntimeState) -> Option<Owed> {
    if runtime.snapshot.phase != RipPhase::Searching {
        return None;
    }
    SEARCH_GENERATION.fetch_add(1, Ordering::AcqRel);
    let owed = owed_report(runtime);
    publish(runtime, |snapshot| {
        snapshot.phase = RipPhase::Idle;
        snapshot.message = None;
    });
    owed
}

/// Throw the rip on the table away, and any search with it. Refused while a
/// kept rip is still downloading. A re-rip still searching still tells
/// itgcontent.net which cards it threw back.
pub fn runtime_reset() -> Result<(), String> {
    let owed = reset(&mut lock_runtime(), &share::SHARE)?;
    send_owed(&share::SHARE, owed);
    Ok(())
}

/// [`runtime_reset`] under the lock: the report a search it stopped still
/// owes, sent once the lock is let go.
fn reset<W: share::Wire>(
    runtime: &mut RuntimeState,
    sharer: &share::Sharer<W>,
) -> Result<Option<Owed>, String> {
    if runtime.snapshot.keeping() {
        return Err("a pack is still downloading".to_owned());
    }
    SEARCH_GENERATION.fetch_add(1, Ordering::AcqRel);
    runtime.held.clear();
    runtime.ripped_with = None;
    let owed = owed_report(runtime);
    sharer.forget();
    publish(runtime, |snapshot| {
        snapshot.phase = RipPhase::Idle;
        snapshot.set = snapshot.set.wrapping_add(1);
        snapshot.songs = Arc::from(Vec::new());
        snapshot.short = false;
        snapshot.message = None;
        snapshot.keep = None;
    });
    Ok(owed)
}

/// A report a search that will never answer still owes: the rip it was,
/// and what it was asked.
type Owed = (u64, share::Asked);

/// What a search that will never answer still owes itgcontent.net, taking
/// what it was asked: for a re-rip, the cards it threw back, which mark the
/// rip they came from re-ripped though nothing was dealt -- as `/api/booster`
/// marks it the moment a re-rip is asked for. A first rip owes nothing.
fn owed_report(runtime: &mut RuntimeState) -> Option<Owed> {
    let asked = runtime.asked.take()?;
    (!asked.thrown.is_empty()).then(|| (runtime.snapshot.set, asked))
}

/// Send what a stopped search owed, alone, when sharing is on.
fn send_owed<W: share::Wire>(sharer: &'static share::Sharer<W>, owed: Option<Owed>) {
    if let Some((set, asked)) = owed {
        sharer.report(set, &asked, &[]);
    }
}

/// Keep the rip on the table: number it, make it a pack, and download its
/// songs, a few at a time. `history` is where the count of kept boosters
/// lives.
pub fn runtime_keep(songs_root: &Path, cache_dir: &Path, history: &Path) -> Result<(), String> {
    if songs_root.as_os_str().is_empty() {
        return Err("there is no Songs folder to put it in".to_owned());
    }
    let job = {
        let mut runtime = lock_runtime();
        let snapshot = Arc::clone(&runtime.snapshot);
        if snapshot.phase != RipPhase::Ready || snapshot.songs.is_empty() {
            return Err("there is no pack to keep".to_owned());
        }
        if snapshot.keeping() {
            return Err("a pack is still downloading".to_owned());
        }
        if snapshot
            .keep
            .as_ref()
            .is_some_and(|keep| keep.set == snapshot.set)
        {
            return Err("this pack was already kept".to_owned());
        }
        let total_bytes = snapshot.songs.iter().map(|song| song.bytes).sum();
        let keep = KeepState {
            set: snapshot.set,
            folder: String::new(),
            phase: KeepPhase::Filling,
            songs: Arc::from(vec![SongState::Waiting; snapshot.songs.len()]),
            done_bytes: 0,
            total_bytes,
            got: 0,
            failed: 0,
            group_dir: None,
            message: None,
        };
        publish(&mut runtime, |snapshot| {
            snapshot.keep = Some(Arc::new(keep))
        });
        keep::KeepJob {
            set: snapshot.set,
            songs: snapshot.songs.iter().cloned().collect(),
            held: runtime.held.clone(),
            options: runtime.ripped_with.clone().unwrap_or_default(),
            songs_root: songs_root.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            history: history.to_path_buf(),
        }
    };
    let set = job.set;
    let spawned = thread::Builder::new()
        .name("rip-keep".to_owned())
        .spawn(move || keep::run(job));
    if let Err(error) = spawned {
        keep_finished(
            set,
            None,
            Some(format!("could not start the download: {error}")),
        );
    }
    // Marked kept on itgcontent.net as it is kept, as ITGmania's are: now if
    // the site has named the rip, else when it does.
    share::SHARE.kept(set);
    Ok(())
}

/// Read how many boosters this machine has kept, once. Safe every frame.
pub fn runtime_load_history(history: &Path) {
    {
        let mut runtime = lock_runtime();
        if runtime.history_asked {
            return;
        }
        runtime.history_asked = true;
    }
    let history = history.to_path_buf();
    let spawned = thread::Builder::new()
        .name("rip-history".to_owned())
        .spawn(move || history_loaded(keep::read_history(&history)));
    if spawned.is_err() {
        history_loaded(0);
    }
}

#[must_use]
pub fn runtime_authors() -> Arc<AuthorsSnapshot> {
    Arc::clone(&lock_runtime().authors)
}

/// Build the author list, once a session. Safe every frame: anything but Idle
/// is left alone, so a failed build is not retried until asked.
pub fn runtime_ensure_authors(cache_dir: &Path) {
    // The date indexes are wanted by the same tab, so they start loading
    // with the author list rather than with the first rip.
    lists::warm(cache_dir);
    start_authors(cache_dir, false);
}

/// Try the author list again after it failed.
pub fn runtime_retry_authors(cache_dir: &Path) {
    // The date indexes come from the same site, so whatever kept the list
    // away may have kept them away too.
    lists::retry(cache_dir);
    start_authors(cache_dir, true);
}

fn start_authors(cache_dir: &Path, retry: bool) {
    let generation = {
        let mut runtime = lock_runtime();
        let phase = runtime.authors.phase;
        let go = phase == AuthorsPhase::Idle || (retry && phase == AuthorsPhase::Error);
        if !go {
            return;
        }
        runtime.authors_generation = runtime.authors_generation.wrapping_add(1);
        let mut authors = (*runtime.authors).clone();
        authors.phase = AuthorsPhase::Loading;
        authors.message = None;
        authors.revision = authors.revision.wrapping_add(1);
        runtime.authors = Arc::new(authors);
        runtime.authors_generation
    };
    let cache_dir = cache_dir.to_path_buf();
    let spawned = thread::Builder::new()
        .name("rip-authors".to_owned())
        .spawn(move || authors::run(generation, cache_dir));
    if let Err(error) = spawned {
        authors_publish(
            generation,
            AuthorsPhase::Error,
            None,
            Some(format!("could not start the author list: {error}")),
        );
    }
}

// ------------------------------------------------- what the workers report

/// Whether the search `generation` is still the one wanted. Asked before
/// every request a search makes.
pub(super) fn search_wanted(generation: u64) -> bool {
    SEARCH_GENERATION.load(Ordering::Acquire) == generation
}

/// A search's answer: the cards, or why there are none.
pub(super) fn search_finished(generation: u64, result: Result<Vec<(RipSong, HeldSong)>, String>) {
    let report = {
        let mut runtime = lock_runtime();
        if !search_wanted(generation) || runtime.snapshot.phase != RipPhase::Searching {
            return;
        }
        let asked = runtime.asked.take();
        match result {
            Ok(found) if !found.is_empty() => {
                let (songs, held): (Vec<RipSong>, Vec<Arc<HeldSong>>) = found
                    .into_iter()
                    .map(|(song, held)| (song, Arc::new(held)))
                    .unzip();
                runtime.held = held;
                let short = songs.len() < BOOSTER_SONGS;
                publish(&mut runtime, |snapshot| {
                    snapshot.phase = RipPhase::Ready;
                    snapshot.songs = Arc::from(songs);
                    snapshot.short = short;
                    snapshot.message = None;
                });
                // Before KEEP can be pressed on it, so a KEEP that beats the
                // site's answer waits for it.
                share::SHARE.dealt(runtime.snapshot.set);
            }
            Ok(_) => publish(&mut runtime, |snapshot| {
                snapshot.phase = RipPhase::Failed;
                snapshot.message =
                    Some("no songs on stepmaniaonline.net match these settings".to_owned());
            }),
            Err(message) => publish(&mut runtime, |snapshot| {
                snapshot.phase = RipPhase::Failed;
                snapshot.message = Some(message);
            }),
        }
        let (songs, held) = if runtime.snapshot.phase == RipPhase::Ready {
            (Arc::clone(&runtime.snapshot.songs), runtime.held.clone())
        } else {
            (Arc::from(Vec::new()), Vec::new())
        };
        asked.map(|asked| (runtime.snapshot.set, asked, songs, held))
    };
    if let Some((set, asked, songs, held)) = report {
        share_rip(set, &asked, &songs, &held);
    }
}

/// Report a finished search's rip to itgcontent.net, when sharing is on: its
/// cards, or for a re-rip that found none the cards it threw back. Built on
/// the search's thread; sent from a thread of its own.
fn share_rip(set: u64, asked: &share::Asked, songs: &[RipSong], held: &[Arc<HeldSong>]) {
    if !share::SHARE.on() {
        return;
    }
    let catalogue = crate::stepmaniaonline::runtime_snapshot();
    let shared = share::shared_songs(songs, held, &catalogue.catalog);
    share::SHARE.report(set, asked, &shared);
}

fn update_keep(set: u64, change: impl FnOnce(&mut KeepState)) {
    let mut runtime = lock_runtime();
    let Some(keep) = runtime
        .snapshot
        .keep
        .as_ref()
        .filter(|keep| keep.set == set)
    else {
        return;
    };
    let mut keep = (**keep).clone();
    change(&mut keep);
    publish(&mut runtime, |snapshot| {
        snapshot.keep = Some(Arc::new(keep))
    });
}

/// The kept rip has its number and folder name; `kept_count` is the history
/// as it now stands.
pub(super) fn keep_numbered(set: u64, folder: String, kept_count: u32) {
    let mut runtime = lock_runtime();
    if let Some(keep) = runtime
        .snapshot
        .keep
        .as_ref()
        .filter(|keep| keep.set == set)
    {
        let mut keep = (**keep).clone();
        keep.folder = folder;
        publish(&mut runtime, |snapshot| {
            snapshot.keep = Some(Arc::new(keep));
            snapshot.kept_count = Some(kept_count);
        });
    }
}

/// Card `at`'s song moved on. `Done` and `Failed` also count it.
pub(super) fn keep_song_state(set: u64, at: usize, state: SongState) {
    update_keep(set, |keep| {
        let mut songs = keep.songs.to_vec();
        let Some(slot) = songs.get_mut(at) else {
            return;
        };
        let was = *slot;
        *slot = state;
        keep.songs = Arc::from(songs);
        if was != state {
            match state {
                SongState::Done => keep.got += 1,
                SongState::Failed => keep.failed += 1,
                SongState::Waiting | SongState::Downloading => {}
            }
        }
    });
}

/// Bytes downloaded so far, over every song.
pub(super) fn keep_bytes(set: u64, done_bytes: u64) {
    update_keep(set, |keep| keep.done_bytes = done_bytes);
}

/// The kept rip is finished: `group_dir` is the pack folder when any song got
/// in. Done when one did, Failed when none did.
pub(super) fn keep_finished(set: u64, group_dir: Option<PathBuf>, message: Option<String>) {
    update_keep(set, |keep| {
        let mut songs = keep.songs.to_vec();
        for state in &mut songs {
            if matches!(state, SongState::Waiting | SongState::Downloading) {
                *state = SongState::Failed;
                keep.failed += 1;
            }
        }
        keep.songs = Arc::from(songs);
        keep.phase = if keep.got > 0 && group_dir.is_some() {
            KeepPhase::Done
        } else {
            KeepPhase::Failed
        };
        keep.group_dir = group_dir;
        keep.message = message;
    });
}

fn history_loaded(count: u32) {
    let mut runtime = lock_runtime();
    if runtime.snapshot.kept_count.is_some() {
        return;
    }
    publish(&mut runtime, |snapshot| snapshot.kept_count = Some(count));
}

/// Whether the author build `generation` is still the one wanted.
pub(super) fn authors_wanted(generation: u64) -> bool {
    lock_runtime().authors_generation == generation
}

/// The author list as it stands. `authors` of `None` keeps the list already
/// published (an error after a partial build still offers what it found).
pub(super) fn authors_publish(
    generation: u64,
    phase: AuthorsPhase,
    authors: Option<Vec<RipAuthor>>,
    message: Option<String>,
) {
    let mut runtime = lock_runtime();
    if runtime.authors_generation != generation {
        return;
    }
    let mut snapshot = (*runtime.authors).clone();
    snapshot.phase = phase;
    if let Some(authors) = authors {
        snapshot.authors = Arc::from(authors);
    }
    snapshot.message = message;
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.authors = Arc::new(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_merge_once_each_case_aside() {
        let chosen = vec!["Zaia".to_owned(), "dimo".to_owned()];
        assert_eq!(
            merge_names(&chosen, " zaia ,  Andeh + Exschwasion,, DIMO,freyja"),
            vec!["Zaia", "dimo", "Andeh + Exschwasion", "freyja"]
        );
        assert!(split_names(" , ,").is_empty());
    }

    #[test]
    fn a_name_matches_the_whole_credit_only() {
        assert!(same_name("Andeh + Exschwasion", "andeh + exschwasion "));
        assert!(!same_name("Andeh", "Andeh + Exschwasion"));
    }

    #[test]
    fn eras_split_by_day() {
        assert!(Era::Vintage.holds(20141231));
        assert!(Era::Mid.holds(20150101));
        assert!(Era::Mid.holds(20191231));
        assert!(Era::Modern.holds(20200101));
        assert!(!Era::Modern.holds(20191231));
        let all = RipOptions {
            eras: vec![Era::Vintage, Era::Modern, Era::Mid],
            ..RipOptions::default()
        };
        assert!(all.limiting_eras().is_empty());
        let two = RipOptions {
            eras: vec![Era::Vintage, Era::Modern],
            ..RipOptions::default()
        };
        assert_eq!(two.limiting_eras(), vec![Era::Modern, Era::Vintage]);
    }

    /// A search under way for rip 4, asked to leave out `thrown`.
    fn searching(thrown: &[&str]) -> RuntimeState {
        let mut runtime = RuntimeState::default();
        publish(&mut runtime, |snapshot| {
            snapshot.phase = RipPhase::Searching;
            snapshot.set = 4;
        });
        runtime.asked = Some(share::Asked {
            options: RipOptions::default(),
            thrown: thrown.iter().map(|&key| key.to_owned()).collect(),
        });
        runtime
    }

    #[test]
    fn a_re_rip_stopped_or_reset_still_sends_what_it_threw_back() {
        let x_only = concat!(
            r#"{"eras":[],"lo":8,"hi":14,"full":false,"authors":[],"avoid":[],"#,
            r#""x":["1:2","3:4"],"songs":[]}"#,
        );
        let expected = vec![(share::RIPS_URL, "application/json", x_only.to_owned())];

        // The player left mid-search.
        let sharer = share::fake::new_sharer();
        let mut runtime = searching(&["1:2", "3:4"]);
        let owed = stop_search(&mut runtime);
        assert_eq!(runtime.snapshot.phase, RipPhase::Idle);
        assert!(runtime.asked.is_none());
        assert!(
            share::fake::sent(sharer).is_empty(),
            "nothing goes under the lock"
        );
        send_owed(sharer, owed);
        share::fake::run(sharer);
        assert_eq!(share::fake::sent(sharer), expected);

        // The player reset mid-search.
        let sharer = share::fake::new_sharer();
        let mut runtime = searching(&["1:2", "3:4"]);
        let owed = reset(&mut runtime, sharer).expect("a reset");
        assert_eq!(runtime.snapshot.phase, RipPhase::Idle);
        assert!(runtime.asked.is_none());
        send_owed(sharer, owed);
        share::fake::run(sharer);
        assert_eq!(share::fake::sent(sharer), expected);
    }

    #[test]
    fn a_first_rip_stopped_or_reset_sends_nothing() {
        let sharer = share::fake::new_sharer();
        let mut runtime = searching(&[]);
        send_owed(sharer, stop_search(&mut runtime));
        let mut runtime = searching(&[]);
        send_owed(sharer, reset(&mut runtime, sharer).expect("a reset"));
        // Nor does a stop with no search running, or a reset of a rip
        // already dealt (its report went when it was).
        let mut runtime = RuntimeState::default();
        assert!(stop_search(&mut runtime).is_none());
        assert!(reset(&mut runtime, sharer).expect("a reset").is_none());
        share::fake::run(sharer);
        assert!(share::fake::sent(sharer).is_empty());
    }

    #[test]
    fn booster_folders_are_known_by_their_number() {
        assert_eq!(
            booster_number("Content Booster Pack 12 - 2026-10-17"),
            Some(12)
        );
        assert_eq!(booster_number("content booster pack 3"), Some(3));
        assert_eq!(booster_number("Content Booster Pack - x"), None);
        assert!(!is_booster_folder("Content Browser Singles - NULL Sync"));
    }
}
