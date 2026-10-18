//! Finding a rip's songs: the relay's booster search (`ripBooster` and
//! `pickFrom`), run here against stepmaniaonline.net itself, with the free
//! pages asked first.
//!
//! The relay opened a random pack's zip and read a random song's simfile to
//! see whether it would do -- counted downloads of somebody's pack for every
//! song looked at, most of them not kept. Here the same questions go first to
//! pages that count for nothing: the pack's page says which songs it has and
//! roughly how hard they are, the song's page which singles charts it has and
//! their meters, and SMO's description of the Challenge chart who made it.
//! Only a song every page says yes to costs anything: its pack's zip index,
//! for its folder, its size and the dates its files carry -- what no page
//! says. An index paid for is kept on disk, so the same pack costs nothing
//! the next time.
//!
//! Otherwise these are the relay's rules, as it runs them:
//!
//! * **Pool.** Every catalogue pack but the keyboard ones -- or, with authors
//!   asked for, the packs SMO's exact credit search finds them in, dance
//!   packs only, their own packs (every song theirs) first. Kept to eras,
//!   only the packs the site's `pack-dates.json` and their names leave able
//!   to be in one ([`dates::may_be_in`]). Shuffled.
//! * **Per pack.** Its songs in random order, an author's matched songs
//!   first; six tried, forty with authors. With all five difficulties asked
//!   for, five songs in a row short of one and the pack is passed over.
//! * **Per song.** Expert is the singles Challenge chart, its meter in range.
//!   Authors and avoided names are whole credits, case aside. One of each
//!   song, by key and by title and artist.
//! * **Eras.** Three dates, each held to the eras in turn ([`dates`]): the
//!   pack's, the song's (older than its pack when its own files say so), and
//!   its Expert chart's first appearance (older still when the same chart is
//!   in an older pack, by the site's `chart-firsts.json`). A pack whose date
//!   is not in the eras is passed over; the card shows the chart's year.
//!   The pack's date is tested as early as it is free: before any page when
//!   its listing or the game it rips dates it, or its index is in hand.
//! * **Authors' second pass.** An author's songs sit in few packs, so the
//!   packs that gave one are asked for another, round robin, until the rip is
//!   full or none has more.
//!
//! The two date indexes come from [`lists`], which a search waits a few
//! seconds for at most; one that has not arrived dates songs by less, it
//! never holds a rip up.
//!
//! **Budgets.** Eight packs at once, six hundred opened, a minute, and a hard
//! cap on counted requests. A place in the rip is held before a song is paid
//! for, so a full rip never pays for a song it then throws away. Whichever
//! runs out first, the rip is what was found by then.

use super::dates::{self, Appearances, PackDate, ZipFacts};
use super::lists::{self, PackDates};
use super::pages::{self, Singles};
use super::{BOOSTER_SONGS, HeldSong, RipSong};
use crate::pack_archive::{self, ArchiveError, PackIndex};
use crate::pack_page::{self, PackPage, SongRow};
use crate::smo_search::{self, CreditPack};
use crate::smo_songs::{self, Failure, PackCache};
use crate::stepmaniaonline::{self, PackInfo};
use deadsync_net as network;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// Packs looked into at once, as the relay looks into them. Most of what a
/// worker waits on is a free page, and the counted requests have a budget of
/// their own whatever the pace.
const PARALLEL: usize = 8;
/// Packs one rip may open, as the relay allowed. Most candidates are turned
/// away by their pages for free, so this bounds a crawl, not a cost.
const MAX_PACKS: usize = 600;
/// What one rip may spend looking: the relay's forty seconds. The free pages
/// a song is held to first come back fast enough that a rip which can fill
/// does so well inside it; one that cannot stops here rather than later.
const TIME_BUDGET: Duration = Duration::from_secs(40);
/// Counted requests one rip may make: pack indexes and title scans, every one
/// of which counts as a download of somebody's pack on the site. A rip of
/// fifteen songs costs about sixteen from nothing cached; this allows for an
/// unlucky one, and stops a rip kept to a rare era from paying for packs
/// until it fills.
const COUNTED_BUDGET: u32 = 45;
/// Songs in one pack tried before moving on, as the relay tried them.
const TRIES_PER_PACK: usize = 6;
/// ...and with authors asked for, whose songs sit in few packs.
const AUTHOR_TRIES: usize = 40;
/// ...of which only this many past the songs SMO matched to the author: their
/// Expert chart is nearly always one SMO matched, and a tournament pack can
/// hold a hundred songs and two of theirs.
const AUTHOR_EXTRA: usize = 2;
/// Songs in a row without every difficulty before a pack is passed over: a
/// pack charts the same difficulties for most of its songs.
const FULL_GIVE_UP: usize = 5;
/// The longest a search waits for the site's date indexes when none is on
/// disk yet -- the first launch, or a cache cleared. Past it the rip goes
/// ahead with what there is, and picks either up as it arrives.
const LISTS_WAIT: Duration = Duration::from_secs(5);
/// A `Pack.ini` is a handful of lines; one claiming more is not read, as the
/// relay does not read it.
const MAX_INI_BYTES: u64 = 64 * 1024;
/// Pack pages kept during a rip besides those of packs that gave a song,
/// which the authors' second pass reads again.
const MAX_PAGES_KEPT: usize = 32;
/// Pack indexes kept in memory between rips, with what their title scans
/// read. An author's rips keep returning to the same few packs. Each is a few
/// hundred kilobytes, and the simfiles a scan read with it.
const MAX_INDEXES_KEPT: usize = 16;
/// Under the module's own cache folder.
const INDEX_DIR: &str = "indexes";
const INDEX_EXTENSION: &str = "idx";
/// Indexes kept on disk. Each cost a counted request, so they are kept long;
/// the oldest-used go first past either bound.
const MAX_INDEX_FILES: usize = 256;
const MAX_INDEX_DIR_BYTES: u64 = 128 * 1024 * 1024;
/// A kept index read back: the largest directory the archive reader takes,
/// and its tail.
const MAX_INDEX_FILE_BYTES: u64 = 40 * 1024 * 1024;
/// Ranges one counted request carries: `pack_archive`'s own limit, which
/// is how many requests a title scan of a big pack takes.
const RANGES_PER_REQUEST: usize = 200;
/// What `pack_archive` asks for first when reading an index: 1.5 KB of
/// directory a song and 4 KB of end records, at most 2 MB. A pack whose
/// directory may not fit that takes a second request.
const INDEX_BYTES_PER_SONG: u64 = 1536;
const INDEX_SLACK_BYTES: u64 = 4096;
const INDEX_FIRST_READ_BYTES: u64 = 2 * 1024 * 1024;
/// Counted requests get an agent of their own: an index or a title scan runs
/// to megabytes, which the shared ten-second agent could cut off half way --
/// and a request cut off still counted.
const COUNTED_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a worker waiting for a place in a full-but-unsettled rip sleeps
/// before looking again, if nobody wakes it.
const PLACE_WAIT: Duration = Duration::from_millis(100);

pub(super) struct SearchJob {
    pub generation: u64,
    pub options: super::RipOptions,
    /// Keys of songs to leave out: a re-rip's thrown-back cards.
    pub exclude: HashSet<String>,
    /// The app cache dir; this module keeps its own files under
    /// `rip-packs/` in it.
    pub cache_dir: PathBuf,
}

/// Runs on the thread `runtime_rip` started, and always ends by reporting
/// through [`super::search_finished`].
pub(super) fn run(job: SearchJob) {
    let found = search(&job);
    super::search_finished(job.generation, found);
}

static COUNTED_AGENT: LazyLock<network::HttpAgent> = LazyLock::new(|| {
    network::build_agent(network::AgentConfig {
        timeout: COUNTED_TIMEOUT,
    })
});

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn search(job: &SearchJob) -> Result<Vec<(RipSong, HeldSong)>, String> {
    let started = Instant::now();
    let deadline = started + TIME_BUDGET;
    let alive = || super::search_wanted(job.generation) && Instant::now() < deadline;
    let catalogue = stepmaniaonline::runtime_snapshot();
    if catalogue.catalog.is_empty() {
        return Err("the pack list hasn't loaded yet - try again in a moment".to_owned());
    }
    // Usually loading since the tab opened; if not, from now, while the
    // credit searches run.
    lists::warm(&job.cache_dir);
    let agent = network::get_agent();
    let options = &job.options;

    let mut made = HashMap::new();
    let mut pool = if options.authors.is_empty() {
        catalogue_pool(&catalogue.catalog)
    } else {
        if !alive() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        let mut answered = false;
        for (author, answer) in credit_searches(&agent, &options.authors) {
            match answer {
                Ok(packs) => {
                    answered = true;
                    made.insert(credit_key(author), appearances(&packs));
                    found.extend(packs);
                }
                Err(error) => {
                    log::warn!("Rip Packs: the credit search for {author:?} failed: {error}")
                }
            }
        }
        if !answered {
            return Err("the search didn't work: stepmaniaonline.net didn't answer".to_owned());
        }
        author_pool(&catalogue.catalog, found)
    };

    let today = dates::today();
    let eras: Vec<(u32, u32)> = options
        .limiting_eras()
        .into_iter()
        .map(super::Era::days)
        .collect();
    // Only a rip kept to eras is worth holding up for the date indexes: they
    // decide which packs it may open. Any other rip starts at once and dates
    // its cards with whatever has arrived by then.
    let listed = if eras.is_empty() {
        lists::current()
    } else {
        lists::ready(&job.cache_dir, LISTS_WAIT)
    };
    keep_to_eras(&mut pool, listed.dates.as_deref(), &eras, today);
    pool.shuffle(&mut rand::rng());
    if !options.authors.is_empty() {
        pool = own_first(pool);
    }

    let search = Search {
        job,
        eras,
        today,
        names: catalogue
            .catalog
            .iter()
            .map(|pack| (pack.id, pack.name.as_str()))
            .collect(),
        made,
        deadline,
        pool,
        next: AtomicUsize::new(0),
        opened: AtomicUsize::new(0),
        budget: Budget::new(COUNTED_BUDGET),
        table: Mutex::new(Table::default()),
        room: Condvar::new(),
        pages: Mutex::new(Pages::default()),
        rejected: Mutex::new(HashSet::new()),
        passed: Mutex::new(HashSet::new()),
        gave: Mutex::new(HashMap::new()),
        shelf: Shelf::open(&job.cache_dir),
        pages_read: AtomicUsize::new(0),
        pages_failed: AtomicUsize::new(0),
        agent,
    };
    search.first_pass();
    if !options.authors.is_empty() {
        search.second_pass();
    }
    search.finish(started)
}

// --------------------------------------------------------------------- pool

/// A pack a rip may draw from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    id: u64,
    /// As the catalogue names it.
    name: String,
    /// The catalogue's song count: it sizes the index request, and tells an
    /// author's own pack.
    songs: u32,
    /// The catalogue's size, which says which version of the pack an index
    /// kept on disk is of.
    size: u64,
    /// With authors asked for: the songs SMO credits to them here, by
    /// [`bare_name`].
    hint: Option<HashSet<String>>,
}

impl Candidate {
    fn from_catalogue(pack: &PackInfo) -> Self {
        Self {
            id: pack.id,
            name: pack.name.clone(),
            songs: pack.song_count,
            size: pack.size_bytes,
            hint: None,
        }
    }

    /// An author's own pack: SMO credits every song in it to them.
    fn own(&self) -> bool {
        self.songs > 0
            && self
                .hint
                .as_ref()
                .is_some_and(|hint| hint.len() >= self.songs as usize)
    }
}

/// Every pack on the site but the keyboard ones. Untyped packs stay in: most
/// of the catalogue is untyped, and a song with no singles chart fails the
/// Expert test on its own anyway.
fn catalogue_pool(catalog: &[PackInfo]) -> Vec<Candidate> {
    catalog
        .iter()
        .filter(|pack| {
            !pack
                .pack_type
                .as_deref()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("keyboard"))
        })
        .map(Candidate::from_catalogue)
        .collect()
}

/// Every author's exact credit search, asked for together: SMO takes a second
/// or so over each, and an author rip waits on all of them before it opens a
/// pack. In the authors' order.
fn credit_searches<'a>(
    agent: &network::HttpAgent,
    authors: &'a [String],
) -> Vec<(&'a str, Result<Vec<CreditPack>, String>)> {
    thread::scope(|scope| {
        let asked: Vec<_> = authors
            .iter()
            .map(|author| {
                let search = move || smo_search::exact_credit_packs(agent, author);
                let spawned = thread::Builder::new()
                    .name("rip-credit".to_owned())
                    .spawn_scoped(scope, search);
                (author.as_str(), spawned)
            })
            .collect();
        asked
            .into_iter()
            .map(|(author, spawned)| {
                let answer = match spawned {
                    Ok(handle) => handle
                        .join()
                        .unwrap_or_else(|_| Err("the credit search stopped".to_owned())),
                    // No thread to be had: asked here instead.
                    Err(_) => smo_search::exact_credit_packs(agent, author),
                };
                (author, answer)
            })
            .collect()
    })
}

/// The packs the authors' credit searches found, each once, dance packs only,
/// carrying every song matched in them. A pack the catalogue does not list is
/// left out: nothing else in the browser knows it either, and without its
/// size no index kept for it could be trusted.
fn author_pool(catalog: &[PackInfo], found: Vec<CreditPack>) -> Vec<Candidate> {
    let by_id: HashMap<u64, &PackInfo> = catalog.iter().map(|pack| (pack.id, pack)).collect();
    let mut pool: Vec<Candidate> = Vec::new();
    let mut at: HashMap<u64, usize> = HashMap::new();
    for pack in found {
        if !pack.is_dance() {
            continue;
        }
        let Some(info) = by_id.get(&pack.id) else {
            continue;
        };
        let slot = *at.entry(pack.id).or_insert_with(|| {
            let mut candidate = Candidate::from_catalogue(info);
            candidate.hint = Some(HashSet::new());
            pool.push(candidate);
            pool.len() - 1
        });
        if let Some(hint) = pool[slot].hint.as_mut() {
            hint.extend(
                pack.matching_songs
                    .iter()
                    .map(|song| bare_name(song))
                    .filter(|name| !name.is_empty()),
            );
        }
    }
    pool
}

/// An author's own packs first, otherwise in the order given. Events and
/// collaborations, where they charted a song or two, come after.
fn own_first(pool: Vec<Candidate>) -> Vec<Candidate> {
    let (mut own, rest): (Vec<Candidate>, Vec<Candidate>) =
        pool.into_iter().partition(Candidate::own);
    own.extend(rest);
    own
}

/// Kept to eras, only the packs that can be in one before they are opened
/// (relay `mayBeIn`): by the site's listing and the pack's name. A pack the
/// listing does not know yet, or with no listing at all, stays in and is
/// judged when opened.
fn keep_to_eras(
    pool: &mut Vec<Candidate>,
    dates: Option<&PackDates>,
    eras: &[(u32, u32)],
    today: u32,
) {
    if !eras.is_empty() {
        pool.retain(|candidate| {
            dates::may_be_in(dates, candidate.id, &candidate.name, eras, today)
        });
    }
}

/// What a credit search was made for, as the relay files it: the whole
/// credit, trimmed, case aside -- so a chart credited to an asked-for author
/// finds that author's search.
fn credit_key(credit: &str) -> String {
    credit.trim().to_lowercase()
}

/// What a credit search says of where else its charter's songs are, for
/// dating a chart by its first appearance: the dance packs it found, each
/// with the songs it matched there.
fn appearances(found: &[CreditPack]) -> Vec<(u64, Vec<String>)> {
    found
        .iter()
        .filter(|pack| pack.is_dance())
        .map(|pack| (pack.id, pack.matching_songs.clone()))
        .collect()
}

/// The pack's `Pack.ini` Year, when reading it costs nothing: the file lies
/// wholly inside the tail the index was read from, as it does whenever it is
/// stored near the end of the zip. Anywhere else, reading it would be a
/// counted request of its own, so the pack is dated without it.
fn free_ini_year(index: &PackIndex) -> Option<u32> {
    let at = pack_archive::pack_ini_entry(index)?;
    let entry = &index.entries[at];
    let (start, end) = entry.span();
    let tail_end = index.tail_start.saturating_add(index.tail.len() as u64);
    if entry.uncompressed > MAX_INI_BYTES || start < index.tail_start || end > tail_end {
        return None;
    }
    match pack_archive::read_entry(index, &[], at, MAX_INI_BYTES) {
        Ok(bytes) => dates::ini_year(&bytes),
        Err(error) => {
            log::debug!(
                "Rip Packs: pack {}'s Pack.ini did not read: {error}",
                index.pack_id
            );
            None
        }
    }
}

// ------------------------------------------------------------------ songs

/// The relay's `norm`: lowercase, then only `a`-`z` and `0`-`9` kept, so
/// case, punctuation and spacing never tell two names apart.
fn norm(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// A name with its `[tags]` and `(asides)` off, folded as [`norm`] folds it
/// (the relay's `bareName`): what SMO's matched titles are compared with, and
/// what one song goes by whichever pack dresses its title.
fn bare_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['[', '(']) {
        out.push_str(&rest[..at]);
        let close = if rest[at..].starts_with('[') {
            ']'
        } else {
            ')'
        };
        match rest[at + 1..].find(close) {
            Some(end) => {
                out.push(' ');
                rest = &rest[at + 1 + end + 1..];
            }
            None => {
                // An opening with no close is kept, as the relay's pattern
                // only matches whole pairs.
                out.push_str(&rest[at..=at]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    norm(&out)
}

/// Whether a page title is one of the songs SMO matched to an author (the
/// relay's `isHinted`): the same bare name, or -- for names of four or more --
/// one holding the other: "09 - Catch Me If You Can" holds "Catch Me If You
/// Can". Exact names find little more than half of an author's songs; this
/// finds nearly all of them.
fn is_hinted(hint: &HashSet<String>, title: &str) -> bool {
    let name = bare_name(title);
    if name.is_empty() {
        return false;
    }
    if hint.contains(&name) {
        return true;
    }
    if name.len() < 4 {
        return false;
    }
    hint.iter()
        .any(|held| held.len() >= 4 && (name.contains(held.as_str()) || held.contains(&name)))
}

/// The names a song goes by, for one of each in a rip (the relay's
/// `sameSong`): its title and its transliterated title with the `[tags]` and
/// `(asides)` off -- a tournament numbers its titles, a second version of a
/// chart says so -- and, once its folder is known, its audio file. Two songs
/// sharing any name are one song.
fn song_names(title: &str, translit: Option<&str>, audio: Option<String>) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(3);
    for text in std::iter::once(title).chain(translit) {
        let name = bare_name(text);
        if !name.is_empty() {
            let name = format!("title:{name}");
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names.extend(audio);
    names
}

/// The song's own audio as the relay knows it -- the biggest `.mp3` or `.ogg`
/// directly in the folder -- named by its CRC and size, which say it is the
/// very same file whatever folder or pack it sits in.
fn audio_name(index: &PackIndex, folder: usize) -> Option<String> {
    let mut best: Option<&pack_archive::ZipEntry> = None;
    for &at in &index.folders.get(folder)?.entries {
        let entry = index.entries.get(at)?;
        let parts: Vec<&str> = entry.name.split('/').collect();
        let audio = parts.len() == 3
            && !parts[2].is_empty()
            && entry.compressed > 0
            && parts[2].rsplit_once('.').is_some_and(|(_, ext)| {
                ext.eq_ignore_ascii_case("mp3") || ext.eq_ignore_ascii_case("ogg")
            });
        if audio && best.is_none_or(|have| entry.uncompressed > have.uncompressed) {
            best = Some(entry);
        }
    }
    best.map(|entry| format!("audio:{}:{}", entry.crc, entry.uncompressed))
}

/// `<pack id>:<song id>`.
fn song_key(pack_id: u64, song_id: u64) -> String {
    format!("{pack_id}:{song_id}")
}

/// The page's rows a rip can ask about -- those with a song id -- shuffled,
/// then the ones SMO matched to an author ahead of the rest, and how many of
/// those there are. The credit check is what decides; the hint saves tries.
fn row_order(rows: &[SongRow], hint: Option<&HashSet<String>>) -> (Vec<usize>, usize) {
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|&at| rows[at].song_id.is_some())
        .collect();
    order.shuffle(&mut rand::rng());
    hinted_first(order, rows, hint)
}

fn hinted_first(
    order: Vec<usize>,
    rows: &[SongRow],
    hint: Option<&HashSet<String>>,
) -> (Vec<usize>, usize) {
    let Some(hint) = hint else {
        return (order, 0);
    };
    let (mut hinted, rest): (Vec<usize>, Vec<usize>) = order
        .into_iter()
        .partition(|&at| is_hinted(hint, &rows[at].title));
    let count = hinted.len();
    hinted.extend(rest);
    (hinted, count)
}

/// What the pack page's row already rules out, without spending a try: a song
/// whose styles the page lists with no singles among them, or whose meters --
/// the page's range over every chart of every style -- cannot hold an Expert
/// meter in `[lo, hi]`.
fn row_may_fit(row: &SongRow, lo: u32, hi: u32) -> bool {
    if !row.styles.is_empty()
        && !row
            .styles
            .iter()
            .any(|style| style.eq_ignore_ascii_case("dance-single"))
    {
        return false;
    }
    let printed = row.meters.bytes().any(|byte| byte.is_ascii_digit());
    if printed && row.top_meter() < lo {
        return false;
    }
    !row.low_meter().is_some_and(|low| low > hi)
}

/// The artist, for a title the pack's page lists more than once -- so the
/// folder lookup can tell the two apart -- and nothing otherwise, as the
/// song browser asks.
fn request_artist<'a>(rows: &[SongRow], row: &'a SongRow) -> &'a str {
    let title = row.title.trim();
    let repeated = rows
        .iter()
        .filter(|other| other.title.trim().eq_ignore_ascii_case(title))
        .nth(1)
        .is_some();
    if repeated { &row.artist } else { "" }
}

/// Whether the Expert chart's credit passes: one of the authors when any are
/// asked for, and none of the avoided. Whole credits, case aside. An empty
/// credit is nobody's, so it never matches an author and is never avoided.
fn credit_fits(credit: &str, authors: &[String], avoid: &[String]) -> bool {
    let credit = credit.trim();
    if !authors.is_empty()
        && (credit.is_empty() || !authors.iter().any(|name| super::same_name(name, credit)))
    {
        return false;
    }
    credit.is_empty() || !avoid.iter().any(|name| super::same_name(name, credit))
}

/// Whether a date is in the eras asked for (relay `inEras`). With none asked
/// for, anything is; with some, a song nothing dates is in none.
fn in_eras(eras: &[(u32, u32)], date: Option<PackDate>) -> bool {
    eras.is_empty() || date.is_some_and(|date| dates::in_eras(date, eras))
}

/// The year a card shows: the one its Expert chart came out in, or the best
/// guess at it. None when nothing dates it.
fn card_year(date: Option<PackDate>) -> Option<u16> {
    date.and_then(|date| u16::try_from(date.year).ok())
        .filter(|&year| year != 0)
}

// ------------------------------------------------------------------ budget

/// The counted requests a rip has left. Taken before a request goes out,
/// never after, so the cap holds however many workers ask at once.
struct Budget {
    limit: u32,
    left: AtomicU32,
}

impl Budget {
    fn new(limit: u32) -> Self {
        Self {
            limit,
            left: AtomicU32::new(limit),
        }
    }

    /// Take `cost` requests' worth, or nothing when that much is not left.
    fn reserve(&self, cost: u32) -> bool {
        self.left
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(cost)
            })
            .is_ok()
    }

    /// Give back what a request that never went out had taken.
    fn refund(&self, cost: u32) {
        self.left.fetch_add(cost, Ordering::AcqRel);
    }

    fn affords(&self, cost: u32) -> bool {
        self.left.load(Ordering::Acquire) >= cost
    }

    fn spent(&self) -> u32 {
        self.limit.saturating_sub(self.left.load(Ordering::Acquire))
    }
}

/// The counted requests reading a pack's index takes: one, or two for a pack
/// so big its directory may not fit the first read.
fn index_cost(songs: u32) -> u32 {
    let first = u64::from(songs) * INDEX_BYTES_PER_SONG + INDEX_SLACK_BYTES;
    if first > INDEX_FIRST_READ_BYTES { 2 } else { 1 }
}

/// The counted requests a title scan of `simfiles` simfiles takes: none for
/// none, else one per request's worth of ranges.
fn scan_cost(simfiles: usize) -> u32 {
    u32::try_from(simfiles.div_ceil(RANGES_PER_REQUEST)).unwrap_or(u32::MAX)
}

// ------------------------------------------------------------------- table

/// The rip as it fills: the songs found, and the places held by songs being
/// paid for. Generic over what a song is found in so it can be tested
/// without a pack.
struct Table<H> {
    found: Vec<(RipSong, H)>,
    keys: HashSet<String>,
    /// Every name the songs found go by ([`song_names`]).
    names: HashSet<String>,
    /// Songs past every free check, paying for their index or title scan now,
    /// by key and the names known so far. Each holds a place.
    pending: Vec<(String, Vec<String>)>,
}

impl<H> Default for Table<H> {
    fn default() -> Self {
        Self {
            found: Vec::new(),
            keys: HashSet::new(),
            names: HashSet::new(),
            pending: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    /// Held: pay for the song.
    Go,
    /// Every place is found or held; one may free up.
    Wait,
    /// The rip is full, or no longer wanted.
    Full,
    /// This song, or one going by one of its names, is found or being paid
    /// for.
    Taken,
}

impl<H> Table<H> {
    fn holds(&self, key: &str, names: &[String]) -> bool {
        self.keys.contains(key)
            || names.iter().any(|name| self.names.contains(name))
            || self.pending.iter().any(|(held_key, held_names)| {
                held_key == key || held_names.iter().any(|name| names.contains(name))
            })
    }

    fn try_place(&mut self, key: &str, names: &[String]) -> Place {
        if self.found.len() >= BOOSTER_SONGS {
            return Place::Full;
        }
        if self.holds(key, names) {
            return Place::Taken;
        }
        if self.found.len() + self.pending.len() >= BOOSTER_SONGS {
            return Place::Wait;
        }
        self.pending.push((key.to_owned(), names.to_vec()));
        Place::Go
    }

    /// Give up `key`'s place, and take its song if it was found: at most
    /// [`BOOSTER_SONGS`], one of each by key and by every name it goes by --
    /// its audio file's among them, known only now. Whether it was taken.
    fn settle(&mut self, key: &str, song: Option<(RipSong, H)>, names: &[String]) -> bool {
        self.pending.retain(|(held_key, _)| held_key != key);
        let Some((song, held)) = song else {
            return false;
        };
        if self.found.len() >= BOOSTER_SONGS
            || self.keys.contains(&song.key)
            || names.iter().any(|name| self.names.contains(name))
        {
            return false;
        }
        self.keys.insert(song.key.clone());
        self.names.extend(names.iter().cloned());
        self.found.push((song, held));
        true
    }
}

// ------------------------------------------------------------------- pages

/// Pack pages read during a rip.
#[derive(Default)]
struct Pages {
    /// Most recently used last.
    recent: VecDeque<(u64, Arc<PackPage>)>,
    /// Packs that gave a song, kept for the second pass whatever else is read
    /// meanwhile.
    gave: HashMap<u64, Arc<PackPage>>,
}

impl Pages {
    fn get(&mut self, pack_id: u64) -> Option<Arc<PackPage>> {
        if let Some(page) = self.gave.get(&pack_id) {
            return Some(Arc::clone(page));
        }
        let at = self.recent.iter().position(|(id, _)| *id == pack_id)?;
        let entry = self.recent.remove(at)?;
        let page = Arc::clone(&entry.1);
        self.recent.push_back(entry);
        Some(page)
    }

    fn put(&mut self, pack_id: u64, page: Arc<PackPage>) {
        self.recent.retain(|(id, _)| *id != pack_id);
        self.recent.push_back((pack_id, page));
        while self.recent.len() > MAX_PAGES_KEPT {
            self.recent.pop_front();
        }
    }

    fn pin(&mut self, pack_id: u64, page: Arc<PackPage>) {
        self.recent.retain(|(id, _)| *id != pack_id);
        self.gave.insert(pack_id, page);
    }
}

// ----------------------------------------------------------------- indexes

/// A pack index a rip paid for or read back, with what is known of it.
struct HeldIndex {
    /// The catalogue's size for the pack when it was read: its version.
    size: u64,
    cache: Arc<PackCache>,
    /// What the zip's dates say of the pack, worked out once.
    facts: ZipFacts,
    /// The `Pack.ini` Year, read the first time a date needs it and only
    /// when that is free ([`free_ini_year`]).
    ini_year: OnceLock<Option<u32>>,
    /// Folders whose simfile titles `cache` already holds, so a title scan
    /// that would read nothing new is not counted.
    tagged: Mutex<HashSet<usize>>,
}

impl HeldIndex {
    /// The pack's date (relay `packDate`) by the site's listing as it stands.
    /// The `Pack.ini` is looked at only when the listing and the pack's name
    /// leave the date to it, as the relay looks at it.
    fn pack_date(&self, candidate: &Candidate, dates: Option<&PackDates>, today: u32) -> PackDate {
        let ini_year = if dates::wants_ini_year(dates, candidate.id, &candidate.name, today) {
            *self
                .ini_year
                .get_or_init(|| free_ini_year(&self.cache.index))
        } else {
            None
        };
        dates::pack_date(
            dates,
            candidate.id,
            &candidate.name,
            &self.facts,
            ini_year,
            today,
        )
    }
}

/// Most recently used last.
static INDEXES: LazyLock<Mutex<VecDeque<Arc<HeldIndex>>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

fn held_index(pack_id: u64, size: u64) -> Option<Arc<HeldIndex>> {
    let mut indexes = lock(&INDEXES);
    let at = indexes
        .iter()
        .position(|held| held.cache.index.pack_id == pack_id && held.size == size)?;
    let held = indexes.remove(at)?;
    indexes.push_back(Arc::clone(&held));
    Some(held)
}

fn hold_index(size: u64, cache: Arc<PackCache>, today: u32) -> Arc<HeldIndex> {
    let held = Arc::new(HeldIndex {
        size,
        facts: dates::zip_facts(&cache.index, today),
        ini_year: OnceLock::new(),
        cache,
        tagged: Mutex::new(HashSet::new()),
    });
    let pack_id = held.cache.index.pack_id;
    let mut indexes = lock(&INDEXES);
    indexes.retain(|known| known.cache.index.pack_id != pack_id);
    indexes.push_back(Arc::clone(&held));
    while indexes.len() > MAX_INDEXES_KEPT {
        indexes.pop_front();
    }
    held
}

fn drop_index(pack_id: u64) {
    lock(&INDEXES).retain(|known| known.cache.index.pack_id != pack_id);
}

/// The indexes a rip paid for, on disk: `<pack id>-<catalogue size>.idx`, so
/// one is used only while the catalogue still lists the pack at that size.
struct Shelf {
    dir: PathBuf,
    /// `(pack id, size)` of every index on disk.
    kept: Mutex<HashSet<(u64, u64)>>,
    /// Held while the folder is written to, so two workers saving at once do
    /// not evict each other's files.
    writing: Mutex<()>,
}

impl Shelf {
    fn open(cache_dir: &Path) -> Self {
        let dir = cache_dir.join("rip-packs").join(INDEX_DIR);
        let mut kept = HashSet::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if let Some(version) = parse_index_file_name(name) {
                    kept.insert(version);
                } else if name.ends_with(".tmp") {
                    // A save cut off before its rename.
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Self {
            dir,
            kept: Mutex::new(kept),
            writing: Mutex::new(()),
        }
    }

    fn has(&self, pack_id: u64, size: u64) -> bool {
        lock(&self.kept).contains(&(pack_id, size))
    }

    /// The pack's index at this size, when it is on disk and reads back
    /// whole. One that does not is deleted, and fetched again when wanted.
    fn load(&self, pack_id: u64, size: u64) -> Option<PackIndex> {
        if !self.has(pack_id, size) {
            return None;
        }
        let path = self.dir.join(index_file_name(pack_id, size));
        let index = fs::metadata(&path)
            .ok()
            .filter(|meta| meta.len() <= MAX_INDEX_FILE_BYTES)
            .and_then(|_| fs::read(&path).ok())
            .and_then(|bytes| pack_archive::index_from_bytes(&bytes).ok())
            .filter(|index| index.pack_id == pack_id);
        if index.is_some() {
            // Used now: the last to be evicted.
            let touched = fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|file| file.set_modified(SystemTime::now()));
            if let Err(error) = touched {
                log::debug!("Rip Packs: could not touch {}: {error}", path.display());
            }
        } else {
            log::debug!("Rip Packs: the kept index of pack {pack_id} did not read back");
            let _writing = lock(&self.writing);
            let _ = fs::remove_file(&path);
            lock(&self.kept).remove(&(pack_id, size));
        }
        index
    }

    /// Keep a fresh index, in place of any other version of the pack's, and
    /// make room.
    fn save(&self, index: &PackIndex, size: u64) {
        let bytes = pack_archive::index_to_bytes(index);
        if bytes.is_empty() {
            return;
        }
        let pack_id = index.pack_id;
        let _writing = lock(&self.writing);
        let path = self.dir.join(index_file_name(pack_id, size));
        let temp = self
            .dir
            .join(format!("{}.tmp", index_file_name(pack_id, size)));
        let written = fs::create_dir_all(&self.dir)
            .and_then(|()| fs::write(&temp, &bytes))
            .and_then(|()| fs::rename(&temp, &path));
        if let Err(error) = written {
            log::warn!("Rip Packs: could not keep the index of pack {pack_id}: {error}");
            let _ = fs::remove_file(&temp);
            return;
        }
        lock(&self.kept).insert((pack_id, size));
        let files = self.files();
        for file in &files {
            if file.pack_id == pack_id && file.size != size {
                self.remove(file);
            }
        }
        let files: Vec<IndexFile> = files
            .into_iter()
            .filter(|file| file.pack_id != pack_id || file.size == size)
            .collect();
        let ages: Vec<(u64, SystemTime)> = files
            .iter()
            .map(|file| (file.bytes, file.modified))
            .collect();
        for at in evictions(&ages, MAX_INDEX_FILES, MAX_INDEX_DIR_BYTES) {
            self.remove(&files[at]);
        }
    }

    /// Every version of the pack's index: the site has a different file now.
    fn forget(&self, pack_id: u64) {
        let _writing = lock(&self.writing);
        for file in self.files() {
            if file.pack_id == pack_id {
                self.remove(&file);
            }
        }
    }

    fn files(&self) -> Vec<IndexFile> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let (pack_id, size) = parse_index_file_name(entry.file_name().to_str()?)?;
                let meta = entry.metadata().ok()?;
                Some(IndexFile {
                    path: entry.path(),
                    pack_id,
                    size,
                    bytes: meta.len(),
                    modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                })
            })
            .collect()
    }

    fn remove(&self, file: &IndexFile) {
        if let Err(error) = fs::remove_file(&file.path) {
            log::debug!(
                "Rip Packs: could not remove {}: {error}",
                file.path.display()
            );
        }
        lock(&self.kept).remove(&(file.pack_id, file.size));
    }
}

struct IndexFile {
    path: PathBuf,
    pack_id: u64,
    size: u64,
    bytes: u64,
    modified: SystemTime,
}

fn index_file_name(pack_id: u64, size: u64) -> String {
    format!("{pack_id}-{size}.{INDEX_EXTENSION}")
}

fn parse_index_file_name(name: &str) -> Option<(u64, u64)> {
    let stem = name.strip_suffix(INDEX_EXTENSION)?.strip_suffix('.')?;
    let (pack_id, size) = stem.split_once('-')?;
    Some((pack_id.parse().ok()?, size.parse().ok()?))
}

/// Which files go, oldest-used first, until no more than `max_files` and
/// `max_bytes` are left. `files` is `(bytes, last used)`.
fn evictions(files: &[(u64, SystemTime)], max_files: usize, max_bytes: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by_key(|&at| files[at].1);
    let mut count = files.len();
    let mut bytes: u64 = files.iter().map(|&(len, _)| len).sum();
    let mut doomed = Vec::new();
    for at in order {
        if count <= max_files && bytes <= max_bytes {
            break;
        }
        doomed.push(at);
        count -= 1;
        bytes = bytes.saturating_sub(files[at].0);
    }
    doomed
}

// ------------------------------------------------------------------ search

struct Search<'a> {
    job: &'a SearchJob,
    /// The eras that limit anything, from and until; empty is every era.
    eras: Vec<(u32, u32)>,
    /// `YYYYMMDD`, UTC, when the rip began: the latest date believed.
    today: u32,
    /// Every catalogue pack's name, for dating a chart by the other packs it
    /// is in.
    names: HashMap<u64, &'a str>,
    /// The credit searches this rip made, by [`credit_key`]: where else each
    /// asked-for author's songs are.
    made: HashMap<String, Vec<(u64, Vec<String>)>>,
    deadline: Instant,
    pool: Vec<Candidate>,
    /// The next pool entry the first pass opens.
    next: AtomicUsize,
    opened: AtomicUsize,
    budget: Budget,
    table: Mutex<Table<HeldSong>>,
    /// Woken whenever a held place is given up or filled.
    room: Condvar,
    pages: Mutex<Pages>,
    /// Songs a check turned away whose answer will not change during the rip,
    /// so the second pass does not ask about them again.
    rejected: Mutex<HashSet<String>>,
    /// Packs passed over whole: dated outside the eras asked for, or no index
    /// to be had.
    passed: Mutex<HashSet<u64>>,
    /// The indexes of packs that gave a song, kept for the second pass so
    /// none is read twice -- and no title scan run twice -- however many
    /// other packs pass through the shared cache meanwhile.
    gave: Mutex<HashMap<u64, Arc<HeldIndex>>>,
    shelf: Shelf,
    pages_read: AtomicUsize,
    pages_failed: AtomicUsize,
    /// For the free pages.
    agent: network::HttpAgent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Gave,
    Nothing,
    /// Out of time or no longer wanted, or the rip is full: stop opening packs.
    Stop,
}

enum PageRead {
    Read(Arc<PackPage>),
    Missing,
    Stop,
}

/// How the counted step for one song came out.
enum Paid {
    /// The song, the index it was found in, and every name it goes by.
    Song(Box<(RipSong, HeldSong)>, Arc<HeldIndex>, Vec<String>),
    /// This song is out for the rest of the rip.
    Reject,
    /// Out for now: no counted request left for its title scan, or a request
    /// that failed in passing.
    Skip,
    /// Its pack can give nothing more this time: no counted request left for
    /// its index.
    Done,
    /// Its pack is out for the rest of the rip.
    PassPack,
    Stop,
}

impl Search<'_> {
    fn alive(&self) -> bool {
        super::search_wanted(self.job.generation) && Instant::now() < self.deadline
    }

    fn found(&self) -> usize {
        lock(&self.table).found.len()
    }

    fn wants_more(&self) -> bool {
        self.alive() && self.found() < BOOSTER_SONGS
    }

    /// Whether the pack's index costs nothing: kept for the second pass, in
    /// memory, or on disk. Nothing is read to say so.
    fn index_is_free(&self, candidate: &Candidate) -> bool {
        let gave = lock(&self.gave).contains_key(&candidate.id);
        gave || held_index(candidate.id, candidate.size).is_some()
            || self.shelf.has(candidate.id, candidate.size)
    }

    /// The pack's index when it costs nothing: kept for the second pass, in
    /// memory, or read back from disk.
    fn index_in_hand(&self, candidate: &Candidate) -> Option<Arc<HeldIndex>> {
        let gave = lock(&self.gave).get(&candidate.id).map(Arc::clone);
        if let Some(held) = gave {
            return Some(held);
        }
        if let Some(held) = held_index(candidate.id, candidate.size) {
            return Some(held);
        }
        let index = self.shelf.load(candidate.id, candidate.size)?;
        Some(hold_index(
            candidate.size,
            smo_songs::adopt_index(index),
            self.today,
        ))
    }

    /// The pack's date is outside the eras asked for, so far as can be told
    /// before reading a page for it. Free either way: the site's listing and
    /// the pack's name ([`dates::may_be_in`], which is the relay's whole test
    /// for a pack its listing or the game it rips dates) -- and when its index
    /// is already in hand, the date the relay would give it on opening it.
    ///
    /// The listing is read as it stands now, so one that arrived after the
    /// pool was drawn still keeps a pack from being paid for. The index in
    /// hand, if any, is handed on so it is not looked up again.
    fn out_of_eras(&self, candidate: &Candidate) -> (bool, Option<Arc<HeldIndex>>) {
        if self.eras.is_empty() {
            return (false, None);
        }
        let listed = lists::current();
        let dates = listed.dates.as_deref();
        if !dates::may_be_in(dates, candidate.id, &candidate.name, &self.eras, self.today) {
            return (true, None);
        }
        let Some(held) = self.index_in_hand(candidate) else {
            return (false, None);
        };
        let date = held.pack_date(candidate, dates, self.today);
        (!in_eras(&self.eras, Some(date)), Some(held))
    }

    /// What this rip's credit searches say of where else `credit`'s songs
    /// are; nothing for a credit it made no search for.
    fn made_by(&self, credit: &str) -> &[(u64, Vec<String>)] {
        self.made
            .get(&credit_key(credit))
            .map_or(&[], Vec::as_slice)
    }

    /// Run `work` on up to [`PARALLEL`] threads -- no more than `jobs` -- and
    /// wait for them all. On this thread when none will start.
    fn in_parallel(&self, jobs: usize, work: &(dyn Fn() + Sync)) {
        thread::scope(|scope| {
            let mut started = 0;
            for n in 0..PARALLEL.min(jobs) {
                let spawned = thread::Builder::new()
                    .name(format!("rip-search-{n}"))
                    .spawn_scoped(scope, work);
                match spawned {
                    Ok(_) => started += 1,
                    Err(error) => log::warn!("Rip Packs: could not start a search worker: {error}"),
                }
            }
            if started == 0 && jobs > 0 {
                work();
            }
        });
    }

    /// Open packs from the pool, one song from each at most, until the rip is
    /// full or a budget or the pool runs out.
    fn first_pass(&self) {
        let tries = if self.job.options.authors.is_empty() {
            TRIES_PER_PACK
        } else {
            AUTHOR_TRIES
        };
        self.in_parallel(self.pool.len(), &|| {
            while self.wants_more() {
                let at = self.next.fetch_add(1, Ordering::Relaxed);
                let Some(candidate) = self.pool.get(at) else {
                    break;
                };
                // With nothing counted left, only a pack whose index is in hand
                // can still give a song.
                if !self.budget.affords(index_cost(candidate.songs))
                    && !self.index_is_free(candidate)
                {
                    continue;
                }
                if self.opened.fetch_add(1, Ordering::Relaxed) >= MAX_PACKS {
                    break;
                }
                if self.pick_from(candidate, tries) == Pick::Stop {
                    break;
                }
            }
        });
    }

    /// The relay's second pass for authors: the packs that gave a song are
    /// asked for another, a song from each in turn -- their own packs first,
    /// as the pool is ordered -- until the rip is full or a round finds
    /// nothing.
    fn second_pass(&self) {
        let gave: HashSet<u64> = lock(&self.table)
            .found
            .iter()
            .map(|(song, _)| song.pack_id)
            .collect();
        let again: Vec<&Candidate> = self
            .pool
            .iter()
            .filter(|candidate| gave.contains(&candidate.id))
            .collect();
        while self.wants_more() && !again.is_empty() {
            let before = self.found();
            let next = AtomicUsize::new(0);
            self.in_parallel(again.len(), &|| {
                while self.wants_more() {
                    let Some(candidate) = again.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    if self.pick_from(candidate, AUTHOR_TRIES) == Pick::Stop {
                        break;
                    }
                }
            });
            if self.found() == before {
                break;
            }
        }
    }

    fn finish(self, started: Instant) -> Result<Vec<(RipSong, HeldSong)>, String> {
        let found = std::mem::take(&mut lock(&self.table).found);
        log::info!(
            "Rip Packs: {} of {BOOSTER_SONGS} songs from {} packs in {:.1}s, {} of {} counted requests",
            found.len(),
            self.opened.load(Ordering::Relaxed).min(MAX_PACKS),
            started.elapsed().as_secs_f32(),
            self.budget.spent(),
            self.budget.limit,
        );
        if found.is_empty()
            && self.pages_read.load(Ordering::Relaxed) == 0
            && self.pages_failed.load(Ordering::Relaxed) > 0
        {
            return Err("the search didn't work: stepmaniaonline.net didn't answer".to_owned());
        }
        Ok(found)
    }

    fn page(&self, pack_id: u64) -> PageRead {
        if let Some(page) = lock(&self.pages).get(pack_id) {
            return PageRead::Read(page);
        }
        if !self.alive() {
            return PageRead::Stop;
        }
        match pack_page::fetch_blocking(&self.agent, pack_id) {
            Ok(page) => {
                self.pages_read.fetch_add(1, Ordering::Relaxed);
                let page = Arc::new(page);
                lock(&self.pages).put(pack_id, Arc::clone(&page));
                PageRead::Read(page)
            }
            Err(error) => {
                self.pages_failed.fetch_add(1, Ordering::Relaxed);
                log::debug!("Rip Packs: pack {pack_id}'s page did not arrive: {error}");
                PageRead::Missing
            }
        }
    }

    fn reject(&self, key: String) {
        lock(&self.rejected).insert(key);
    }

    /// Hold a place for a song about to be paid for, waiting while every
    /// place is held by songs not yet settled.
    fn place(&self, key: &str, names: &[String]) -> Place {
        let mut table = lock(&self.table);
        loop {
            let place = table.try_place(key, names);
            if place != Place::Wait {
                return place;
            }
            if !self.alive() {
                return Place::Full;
            }
            table = self
                .room
                .wait_timeout(table, PLACE_WAIT)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn settle(&self, key: &str, song: Option<(RipSong, HeldSong)>, names: &[String]) -> bool {
        let took = lock(&self.table).settle(key, song, names);
        self.room.notify_all();
        took
    }

    /// At most one song from the pack: its rows in turn, each held to the free
    /// pages, and only a song they all pass paid for.
    fn pick_from(&self, candidate: &Candidate, tries_allowed: usize) -> Pick {
        if lock(&self.passed).contains(&candidate.id) {
            return Pick::Nothing;
        }
        let (out, in_hand) = self.out_of_eras(candidate);
        if out {
            lock(&self.passed).insert(candidate.id);
            return Pick::Nothing;
        }
        let page = match self.page(candidate.id) {
            PageRead::Read(page) => page,
            PageRead::Missing => return Pick::Nothing,
            PageRead::Stop => return Pick::Stop,
        };
        let options = &self.job.options;
        let mut tries = 0;
        let mut short_in_a_row = 0;
        let (order, hinted) = row_order(&page.songs, candidate.hint.as_ref());
        // Past the songs SMO matched to an author only a couple more are
        // tried, as the relay tries them.
        let tries_allowed = if candidate.hint.is_some() {
            tries_allowed.min(hinted + AUTHOR_EXTRA)
        } else {
            tries_allowed
        };
        for at in order {
            let row = &page.songs[at];
            let Some(song_id) = row.song_id else {
                continue;
            };
            let key = song_key(candidate.id, song_id);
            let names = song_names(&row.title, None, None);
            if self.job.exclude.contains(&key)
                || lock(&self.rejected).contains(&key)
                || lock(&self.table).holds(&key, &names)
                || !row_may_fit(row, options.lo, options.hi)
            {
                continue;
            }
            tries += 1;
            if tries > tries_allowed {
                break;
            }

            if !self.alive() {
                return Pick::Stop;
            }
            let charts = match pages::fetch_song_charts(&self.agent, song_id) {
                Ok(charts) => charts,
                Err(error) => {
                    log::debug!("Rip Packs: song {song_id}'s page did not arrive: {error}");
                    continue;
                }
            };
            let singles = Singles::from_charts(&charts);
            if options.full && !singles.full() {
                self.reject(key);
                short_in_a_row += 1;
                if short_in_a_row >= FULL_GIVE_UP {
                    break;
                }
                continue;
            }
            short_in_a_row = 0;
            let expert = singles
                .expert()
                .filter(|&(meter, _)| (options.lo..=options.hi).contains(&meter));
            let Some((meter, chart_id)) = expert else {
                self.reject(key);
                continue;
            };

            if !self.alive() {
                return Pick::Stop;
            }
            let chart = match pages::fetch_chart(&self.agent, chart_id) {
                Ok(chart) => chart,
                Err(error) => {
                    log::debug!("Rip Packs: chart {chart_id} did not arrive: {error}");
                    continue;
                }
            };
            let singles_chart =
                chart.charttype.is_empty() || chart.charttype.eq_ignore_ascii_case("dance-single");
            if !singles_chart || !credit_fits(&chart.author, &options.authors, &options.avoid) {
                self.reject(key);
                continue;
            }

            // Every free page says yes. Hold its place, then pay.
            match self.place(&key, &names) {
                Place::Go => {}
                Place::Taken | Place::Wait => continue,
                Place::Full => return Pick::Stop,
            }
            match self.pay(
                candidate,
                in_hand.as_ref(),
                &page.songs,
                at,
                &chart.author,
                meter,
            ) {
                Paid::Song(found, index, names) => {
                    if !self.settle(&key, Some(*found), &names) {
                        return Pick::Nothing;
                    }
                    lock(&self.pages).pin(candidate.id, Arc::clone(&page));
                    lock(&self.gave).insert(candidate.id, index);
                    return Pick::Gave;
                }
                Paid::Reject => {
                    self.settle(&key, None, &[]);
                    self.reject(key);
                }
                Paid::Skip => {
                    self.settle(&key, None, &[]);
                }
                Paid::Done => {
                    self.settle(&key, None, &[]);
                    return Pick::Nothing;
                }
                Paid::PassPack => {
                    self.settle(&key, None, &[]);
                    lock(&self.passed).insert(candidate.id);
                    return Pick::Nothing;
                }
                Paid::Stop => {
                    self.settle(&key, None, &[]);
                    return Pick::Stop;
                }
            }
        }
        Pick::Nothing
    }

    /// The counted step, for a song every free page passed: its pack's index,
    /// its folder, and from those its dates, year and size. `in_hand` is the
    /// index when [`Self::out_of_eras`] already had it.
    fn pay(
        &self,
        candidate: &Candidate,
        in_hand: Option<&Arc<HeldIndex>>,
        rows: &[SongRow],
        at: usize,
        credit: &str,
        meter: u32,
    ) -> Paid {
        let held = match in_hand {
            Some(held) => Arc::clone(held),
            None => match self.index_of(candidate) {
                Ok(held) => held,
                Err(paid) => return paid,
            },
        };
        // The listing as it stands now: one that arrived during the rip is
        // used from then on. Neither file waits here.
        let listed = lists::current();
        let dates = listed.dates.as_deref();
        let date = held.pack_date(candidate, dates, self.today);
        if !in_eras(&self.eras, Some(date)) {
            return Paid::PassPack;
        }
        let row = &rows[at];
        let Some(song_id) = row.song_id else {
            return Paid::Reject;
        };
        let artist = request_artist(rows, row);
        let index = &held.cache.index;

        // The folder: free when one is named for the title, else a title scan
        // -- counted, unless what it would read is read already.
        let needed: Vec<usize> = {
            let tagged = lock(&held.tagged);
            pack_archive::folders_needing_tags(index, &row.title, artist)
                .into_iter()
                .filter(|folder| !tagged.contains(folder))
                .collect()
        };
        let cost = scan_cost(needed.len());
        if cost > 0 && !self.budget.reserve(cost) {
            return Paid::Skip;
        }
        let wanted = || self.alive();
        let resolved =
            smo_songs::resolve_song(&COUNTED_AGENT, &held.cache, &row.title, artist, &wanted);
        let (folder, simfile) = match resolved {
            Ok(found) => {
                lock(&held.tagged).extend(needed);
                found
            }
            Err(Failure::Stale) => {
                self.budget.refund(cost);
                return Paid::Stop;
            }
            Err(Failure::Reader(why)) => {
                lock(&held.tagged).extend(needed);
                log::debug!("Rip Packs: pack {}: {why}", candidate.id);
                return Paid::Reject;
            }
            Err(Failure::Archive(ArchiveError::Changed)) => {
                // Replaced on the site since its index was read.
                drop_index(candidate.id);
                self.shelf.forget(candidate.id);
                return Paid::PassPack;
            }
            Err(Failure::Archive(error)) => {
                log::debug!(
                    "Rip Packs: pack {}'s titles did not arrive: {error}",
                    candidate.id
                );
                return Paid::Skip;
            }
        };

        // The song's date, then its Expert chart's first appearance, each held
        // to the eras as the relay holds them.
        let when = dates::song_date(Some(date), &held.facts, index, folder, self.today);
        if !in_eras(&self.eras, when) {
            return Paid::Reject;
        }
        let folder_name = index.folders[folder].name.as_str();
        let chart_title = if row.title.trim().is_empty() {
            folder_name
        } else {
            row.title.as_str()
        };
        let seen = Appearances {
            firsts: listed.firsts.as_deref(),
            made: self.made_by(credit),
        };
        let name_of = |id: u64| {
            self.names
                .get(&id)
                .map_or_else(String::new, |&name| name.to_owned())
        };
        let first = dates::chart_date(
            when,
            candidate.id,
            chart_title,
            folder_name,
            credit.trim(),
            seen,
            dates,
            &name_of,
            self.today,
        );
        if !in_eras(&self.eras, first) {
            return Paid::Reject;
        }
        let bytes = match pack_archive::folder_fetch_bytes(
            index,
            folder,
            &pack_archive::is_resync_backup,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::debug!("Rip Packs: pack {} song {song_id}: {error}", candidate.id);
                return Paid::Reject;
            }
        };
        let song = RipSong {
            key: song_key(candidate.id, song_id),
            pack_id: candidate.id,
            pack_name: candidate.name.clone(),
            song_id,
            title: row.title.clone(),
            artist: row.artist.clone(),
            credit: credit.trim().to_owned(),
            meter,
            year: card_year(first),
            art_url: row.image_url.clone(),
            folder: folder_name.to_owned(),
            bytes,
        };
        // Every name it goes by, now its folder is known: its transliterated
        // title when finding the folder meant reading its simfile, and its
        // audio file.
        let translit = simfile
            .map(|bytes| pack_archive::simfile_tags(&bytes).translit)
            .filter(|translit| !translit.trim().is_empty());
        let names = song_names(
            &song.title,
            translit.as_deref(),
            audio_name(&held.cache.index, folder),
        );
        let found = HeldSong {
            cache: Arc::clone(&held.cache),
            folder,
        };
        Paid::Song(Box::new((song, found)), held, names)
    }

    /// The pack's index: in memory, on disk, or -- counted -- from the site.
    fn index_of(&self, candidate: &Candidate) -> Result<Arc<HeldIndex>, Paid> {
        if let Some(held) = self.index_in_hand(candidate) {
            return Ok(held);
        }
        let cost = index_cost(candidate.songs);
        if !self.budget.reserve(cost) {
            return Err(Paid::Done);
        }
        let wanted = || self.alive();
        match smo_songs::pack(&COUNTED_AGENT, candidate.id, &wanted) {
            Ok(cache) => {
                self.shelf.save(&cache.index, candidate.size);
                Ok(hold_index(candidate.size, cache, self.today))
            }
            Err(Failure::Stale) => {
                self.budget.refund(cost);
                Err(Paid::Stop)
            }
            Err(failure) => {
                log::debug!(
                    "Rip Packs: pack {}'s index did not arrive: {}",
                    candidate.id,
                    failure.into_message()
                );
                Err(Paid::PassPack)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(id: u64, name: &str, songs: u32, pack_type: Option<&str>) -> PackInfo {
        PackInfo::new(
            id,
            name.to_owned(),
            songs,
            u64::from(songs) * 1_000_000,
            None,
            pack_type.map(str::to_owned),
            None,
            None,
        )
    }

    fn credit(id: u64, types: Option<&[&str]>, songs: &[&str]) -> CreditPack {
        CreditPack {
            id,
            name: format!("search's name for {id}"),
            types: types.map(|types| types.iter().map(|&kind| kind.to_owned()).collect()),
            songs_count: 0,
            matching_songs: songs.iter().map(|&song| song.to_owned()).collect(),
        }
    }

    fn row(title: &str, artist: &str, song_id: Option<u64>) -> SongRow {
        SongRow {
            title: title.to_owned(),
            artist: artist.to_owned(),
            song_id,
            ..SongRow::default()
        }
    }

    fn song(key: &str, title: &str) -> RipSong {
        RipSong {
            key: key.to_owned(),
            pack_id: 1,
            pack_name: "Pack".to_owned(),
            song_id: 1,
            title: title.to_owned(),
            artist: "Artist".to_owned(),
            credit: String::new(),
            meter: 10,
            year: None,
            art_url: None,
            folder: title.to_owned(),
            bytes: 0,
        }
    }

    #[test]
    fn the_pool_is_every_pack_but_the_keyboard_ones() {
        let catalog = [
            pack(1, "Pad", 10, Some("itg")),
            pack(2, "Keys", 10, Some(" Keyboard ")),
            pack(3, "Untyped", 10, None),
        ];
        let ids: Vec<u64> = catalogue_pool(&catalog).iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn an_authors_pool_is_their_dance_packs_once_each_with_every_match_as_a_hint() {
        let catalog = [
            pack(1, "Their Pack", 2, None),
            pack(2, "Event", 40, None),
            pack(3, "Pump Pack", 5, None),
        ];
        let found = vec![
            credit(2, Some(&["dance", "pump"]), &["[11] RED Zone."]),
            credit(1, Some(&["dance"]), &["Song A"]),
            credit(3, Some(&["pump"]), &["Pumped"]),
            credit(99, None, &["Not in the catalogue"]),
            credit(1, None, &["Song B", "song a"]),
        ];
        let pool = author_pool(&catalog, found);
        let ids: Vec<u64> = pool.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![2, 1], "pump-only and unknown packs are left out");
        assert_eq!(
            pool[1].name, "Their Pack",
            "named as the catalogue names it"
        );
        let hint = pool[1].hint.as_ref().unwrap();
        assert_eq!(hint.len(), 2, "two authors' matches, each song once");
        assert!(hint.contains("songa") && hint.contains("songb"));
        assert!(
            pool[0].hint.as_ref().unwrap().contains("redzone"),
            "the site's bracketed index is not part of the title"
        );
    }

    #[test]
    fn an_authors_own_packs_come_first_in_the_order_given() {
        let catalog = [
            pack(1, "Event", 40, None),
            pack(2, "Theirs", 2, None),
            pack(3, "Collab", 10, None),
            pack(4, "Also Theirs", 1, None),
        ];
        let found = vec![
            credit(1, None, &["x"]),
            credit(2, None, &["a", "b"]),
            credit(3, None, &["y", "z"]),
            credit(4, None, &["only"]),
        ];
        let pool = own_first(author_pool(&catalog, found));
        let ids: Vec<u64> = pool.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![2, 4, 1, 3]);
        assert!(
            !Candidate::from_catalogue(&pack(5, "Empty", 0, None)).own(),
            "a pack with no songs is nobody's own"
        );
    }

    #[test]
    fn hinted_rows_come_first_and_rows_without_a_song_never() {
        let rows = [
            row("Other", "", Some(1)),
            row("[07] Long Time", "", Some(2)),
            row("No Link", "", None),
            row("Third", "", Some(3)),
        ];
        let hint: HashSet<String> = [bare_name("Long Time")].into_iter().collect();
        assert_eq!(
            hinted_first(vec![3, 0, 1], &rows, Some(&hint)),
            (vec![1, 3, 0], 1)
        );
        assert_eq!(hinted_first(vec![3, 0, 1], &rows, None), (vec![3, 0, 1], 0));
        let (mut order, hinted) = row_order(&rows, Some(&hint));
        assert_eq!((order[0], hinted), (1, 1));
        order.sort_unstable();
        assert_eq!(order, vec![0, 1, 3]);
    }

    #[test]
    fn a_matched_title_is_found_however_the_page_dresses_it() {
        let hint: HashSet<String> = ["Catch Me If You Can", "ABC", "Vertex (Long ver.)"]
            .into_iter()
            .map(bare_name)
            .collect();
        assert!(
            is_hinted(&hint, "09 - Catch Me If You Can"),
            "one holds the other"
        );
        assert!(is_hinted(&hint, "[12] Vertex"), "tags and asides off");
        assert!(is_hinted(&hint, "abc"), "a short name only exactly");
        assert!(!is_hinted(&hint, "ABCD"), "...never by holding");
        assert!(!is_hinted(&hint, "[07]"), "nothing left of it");
        assert!(!is_hinted(&hint, "Something Else"));
    }

    #[test]
    fn a_row_is_turned_away_only_by_what_its_page_rules_out() {
        let with = |meters: &str, styles: &[&str]| SongRow {
            meters: meters.to_owned(),
            styles: styles.iter().map(|&style| style.to_owned()).collect(),
            ..SongRow::default()
        };
        assert!(row_may_fit(&with("12-23", &["dance-single"]), 8, 14));
        assert!(
            !row_may_fit(&with("12-23", &["dance-single"]), 1, 11),
            "too hard"
        );
        assert!(
            !row_may_fit(&with("3-7", &["dance-single"]), 8, 14),
            "too easy"
        );
        assert!(row_may_fit(&with("3, 6, 9, 12", &[]), 8, 8), "a meter list");
        assert!(
            !row_may_fit(&with("12-23", &["pump-single", "pump-double"]), 8, 14),
            "no singles chart"
        );
        assert!(
            row_may_fit(&with("", &[]), 8, 14),
            "a row that says nothing"
        );
        assert!(row_may_fit(&with("14", &["DANCE-SINGLE"]), 14, 14));
    }

    #[test]
    fn credits_are_whole_names_case_aside() {
        let authors = vec!["Mad Matt".to_owned()];
        let avoid = vec!["Zaia".to_owned()];
        assert!(credit_fits("mad matt", &authors, &[]));
        assert!(
            !credit_fits("Mad Matt + Zeph", &authors, &[]),
            "the whole credit"
        );
        assert!(
            !credit_fits("", &authors, &[]),
            "nobody's chart is not theirs"
        );
        assert!(
            credit_fits("", &[], &avoid),
            "nobody's chart is not avoided"
        );
        assert!(!credit_fits(" ZAIA ", &[], &avoid));
        assert!(credit_fits("Zaia + Zeph", &[], &avoid));
    }

    const MODERN: (u32, u32) = (20_200_101, 99_999_999);
    const MID: (u32, u32) = (20_150_101, 20_200_101);
    const VINTAGE: (u32, u32) = (0, 20_150_101);

    fn dated(day: u32, year: u32, ceiling: bool) -> PackDate {
        PackDate { day, year, ceiling }
    }

    #[test]
    fn eras_and_years_by_the_relays_dates() {
        assert!(in_eras(&[], None), "no era asked for");
        assert!(!in_eras(&[MODERN], None), "a song nothing dates is in none");
        assert!(in_eras(&[MID, MODERN], Some(dated(20210304, 2021, false))));
        assert!(!in_eras(&[MID], Some(dated(20210304, 2021, false))));
        // A ceiling counts only when everything before it is asked for.
        assert!(!in_eras(&[MID, MODERN], Some(dated(20190304, 2016, true))));
        assert!(in_eras(&[VINTAGE, MID], Some(dated(20190304, 2016, true))));

        assert_eq!(card_year(Some(dated(20210304, 2021, false))), Some(2021));
        assert_eq!(
            card_year(Some(dated(20210304, 2016, true))),
            Some(2016),
            "a ceiling's best guess"
        );
        assert_eq!(card_year(Some(dated(0, 0, true))), None);
        assert_eq!(card_year(None), None);
    }

    #[test]
    fn the_pool_keeps_to_packs_that_can_be_in_the_eras() {
        let listing = PackDates {
            // id -> (uploaded, released)
            packs: HashMap::from([
                (1, (20_230_101, 20_100_301)),
                (2, (20_120_428, 0)),
                (3, (20_230_101, 0)),
            ]),
            bulk: HashSet::new(),
        };
        let catalog = [
            pack(1, "Old Pack", 10, None),
            pack(2, "Uploaded Early", 10, None),
            pack(3, "Uploaded Late", 10, None),
            pack(4, "Brand New", 10, None),
            pack(5, "Dance Dance Revolution EXTREME (AC) (JP)", 10, None),
        ];
        let kept = |dates: Option<&PackDates>, eras: &[(u32, u32)]| {
            let mut pool = catalogue_pool(&catalog);
            keep_to_eras(&mut pool, dates, eras, 20_261_017);
            pool.iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<u64>>()
        };
        // Released in 2010, however late it was uploaded; uploaded in 2012
        // and dated no better, so no later than that; the game's rip as old as
        // the game. The rest could be from any time before their upload, or
        // are not listed yet and are judged when opened.
        assert_eq!(kept(Some(&listing), &[MODERN]), vec![3, 4]);
        assert_eq!(kept(Some(&listing), &[MID]), vec![3, 4]);
        // With no listing, only the names say anything.
        assert_eq!(kept(None, &[MODERN]), vec![1, 2, 3, 4]);
        assert_eq!(kept(None, &[]), vec![1, 2, 3, 4, 5], "every era");
    }

    #[test]
    fn credit_searches_are_kept_by_credit_with_their_dance_packs() {
        assert_eq!(credit_key("  Mad Matt "), credit_key("mad matt"));
        let found = vec![
            credit(1, Some(&["dance"]), &["Song A"]),
            credit(2, Some(&["pump"]), &["Pumped"]),
            credit(3, None, &["[03] Song B"]),
        ];
        assert_eq!(
            appearances(&found),
            vec![
                (1, vec!["Song A".to_owned()]),
                (3, vec!["[03] Song B".to_owned()])
            ],
            "pump packs say nothing of a dance chart"
        );
    }

    /// A zip of these stored files, in this order, and its index as a read of
    /// its last `tail` bytes has it.
    fn zip_index(files: &[(&str, &[u8])], tail: usize) -> PackIndex {
        use std::io::{Cursor, Write};
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            zip.start_file(*name, stored).expect("a file");
            zip.write_all(bytes).expect("written");
        }
        let bytes = zip.finish().expect("a zip").into_inner();
        let tail_start = bytes.len().saturating_sub(tail);
        let read = PackIndex {
            pack_id: 7,
            total: bytes.len() as u64,
            etag: None,
            entries: Vec::new(),
            folders: Vec::new(),
            tail_start: tail_start as u64,
            tail: Arc::from(&bytes[tail_start..]),
        };
        pack_archive::index_from_bytes(&pack_archive::index_to_bytes(&read)).expect("an index")
    }

    #[test]
    fn a_pack_ini_is_read_only_from_the_tail_already_held() {
        let ini: &[u8] = b"[Group]\r\nName=Old Pack\r\nYear=2009\r\nSyncOffset=ITG\r\n";
        let audio = vec![7u8; 200_000];
        // Stored last, inside the tail the index was read from: free.
        let last = zip_index(
            &[("Pack/Song/song.ogg", &audio), ("Pack/Pack.ini", ini)],
            4096,
        );
        assert_eq!(free_ini_year(&last), Some(2009));
        // Stored first, it would take a counted request of its own: not read.
        let first = zip_index(
            &[("Pack/Pack.ini", ini), ("Pack/Song/song.ogg", &audio)],
            4096,
        );
        assert!(pack_archive::pack_ini_entry(&first).is_some());
        assert_eq!(free_ini_year(&first), None);
        // The whole zip held, it is free wherever it is.
        let whole = zip_index(
            &[("Pack/Pack.ini", ini), ("Pack/Song/song.ogg", &audio)],
            usize::MAX,
        );
        assert_eq!(free_ini_year(&whole), Some(2009));
        // No Pack.ini, or one too big to be one.
        let none = zip_index(&[("Pack/Song/song.ogg", &audio)], usize::MAX);
        assert_eq!(free_ini_year(&none), None);
        let mut huge = b"Year=2009\r\n".to_vec();
        huge.resize(MAX_INI_BYTES as usize + 1, b' ');
        let huge = zip_index(&[("Pack/Pack.ini", &huge)], usize::MAX);
        assert_eq!(free_ini_year(&huge), None);
    }

    #[test]
    fn the_table_holds_fifteen_songs_one_of_each() {
        let mut table: Table<()> = Table::default();
        let title = |name: &str| song_names(name, None, None);
        assert_eq!(table.try_place("1:1", &title("Song")), Place::Go);
        assert_eq!(
            table.try_place("1:1", &title("Song")),
            Place::Taken,
            "being paid for"
        );
        assert_eq!(
            table.try_place("2:9", &title("SONG!")),
            Place::Taken,
            "same title"
        );
        assert!(table.settle("1:1", Some((song("1:1", "Song"), ())), &title("Song")));
        assert!(table.pending.is_empty());
        assert!(table.holds("3:3", &title("song")), "found, by title");
        assert!(
            !table.settle("2:2", Some((song("2:2", "Song"), ())), &title("Song")),
            "a copy is not taken"
        );

        for n in 2..=BOOSTER_SONGS {
            let key = format!("{n}:{n}");
            assert_eq!(table.try_place(&key, &title(&key)), Place::Go);
        }
        assert_eq!(
            table.try_place("99:99", &title("Late")),
            Place::Wait,
            "every place is held, and one may come free"
        );
        assert!(
            !table.settle("2:2", None, &[]),
            "a song that failed frees its place"
        );
        assert_eq!(table.try_place("99:99", &title("Late")), Place::Go);
        for n in (3..=BOOSTER_SONGS)
            .map(|n| format!("{n}:{n}"))
            .chain(["99:99".to_owned()])
        {
            assert!(table.settle(&n, Some((song(&n, &n), ())), &title(&n)));
        }
        assert_eq!(table.found.len(), BOOSTER_SONGS);
        assert_eq!(table.try_place("100:1", &title("More")), Place::Full);
        assert!(
            !table.settle("100:1", Some((song("100:1", "More"), ())), &title("More")),
            "never past fifteen"
        );
    }

    #[test]
    fn one_song_whatever_its_pack_calls_it() {
        let mut table: Table<()> = Table::default();
        let laser = song_names("Laser Cruster", None, Some("audio:7:4000".to_owned()));
        assert!(table.settle("1:1", Some((song("1:1", "Laser Cruster"), ())), &laser));
        // A tournament numbers it; a second version says so.
        assert!(table.holds("2:2", &song_names("(13) Laser Cruster", None, None)));
        assert!(table.holds(
            "2:2",
            &song_names("[12] laser cruster (vers. 2)", None, None)
        ));
        // The same audio file under another name altogether.
        let renamed = song_names("Something Else", None, Some("audio:7:4000".to_owned()));
        assert!(!table.holds("3:3", &song_names("Something Else", None, None)));
        assert!(
            !table.settle("3:3", Some((song("3:3", "Something Else"), ())), &renamed),
            "known only once its folder is, and still one song"
        );
        // By its transliterated title.
        let kanji = song_names("\u{6771}\u{65b9}", Some("Toho"), None);
        assert!(table.settle("4:4", Some((song("4:4", "\u{6771}\u{65b9}"), ())), &kanji));
        assert!(table.holds("5:5", &song_names("Toho", None, None)));
    }

    #[test]
    fn a_song_folders_audio_is_its_biggest_mp3_or_ogg_directly_in_it() {
        let audio = vec![7u8; 4000];
        let clip = vec![1u8; 100];
        let index = zip_index(
            &[
                ("Pack/A/song.ogg", &audio),
                ("Pack/A/preview.ogg", &clip),
                ("Pack/A/song.ssc", b"#TITLE:A;"),
                ("Pack/B/other.ogg", &audio),
                ("Pack/B/song.sm", b"#TITLE:B;"),
                ("Pack/C/sub/deep.ogg", &audio),
                ("Pack/C/song.sm", b"#TITLE:C;"),
            ],
            usize::MAX,
        );
        let folder = |name: &str| {
            index
                .folders
                .iter()
                .position(|folder| folder.name == name)
                .expect("a folder")
        };
        let a = audio_name(&index, folder("A")).expect("its audio");
        assert_eq!(a, format!("audio:{}:4000", crc32(&audio)));
        assert_eq!(
            audio_name(&index, folder("B")).as_deref(),
            Some(a.as_str()),
            "the same file in another folder"
        );
        assert_eq!(
            audio_name(&index, folder("C")),
            None,
            "nothing directly in it"
        );
    }

    /// The zip's own CRC-32, for checking what the directory records.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn the_budget_is_reserved_before_and_never_overdrawn() {
        let budget = Budget::new(3);
        assert!(budget.reserve(2));
        assert!(!budget.reserve(2), "only one left");
        assert!(budget.affords(1));
        assert!(budget.reserve(1));
        assert!(!budget.affords(1));
        assert_eq!(budget.spent(), 3);
        budget.refund(1);
        assert_eq!(
            budget.spent(),
            2,
            "a request that never went out is given back"
        );

        assert_eq!(scan_cost(0), 0, "a folder named for the title is free");
        assert_eq!(scan_cost(1), 1);
        assert_eq!(scan_cost(200), 1);
        assert_eq!(scan_cost(201), 2);
        assert_eq!(index_cost(150), 1);
        assert_eq!(index_cost(1362), 1);
        assert_eq!(index_cost(3000), 2, "a directory past the first read");
    }

    #[test]
    fn names_fold_as_the_relay_folds_them() {
        assert_eq!(norm("Caf\u{e9} Del Mar!"), "cafdelmar");
        assert_eq!(norm("V.L.S.I."), "vlsi");
        assert_eq!(bare_name("(13) Laser Cruster"), "lasercruster");
        assert_eq!(bare_name("vanessa (vers. 2)"), "vanessa");
        assert_eq!(bare_name("[07] Long Time [Feraligatr]"), "longtime");
        assert_eq!(
            bare_name("Open [bracket"),
            "openbracket",
            "only whole pairs go"
        );
        assert_eq!(song_key(3799, 172_168), "3799:172168");
    }

    #[test]
    fn the_artist_rides_along_only_for_a_repeated_title() {
        let rows = [
            row("Butterfly", "SMiLE.dk", Some(1)),
            row("butterfly ", "Other", Some(2)),
            row("Unique", "Somebody", Some(3)),
        ];
        assert_eq!(request_artist(&rows, &rows[0]), "SMiLE.dk");
        assert_eq!(request_artist(&rows, &rows[2]), "");
    }

    #[test]
    fn kept_indexes_are_named_for_their_version() {
        assert_eq!(index_file_name(3799, 98_598_990), "3799-98598990.idx");
        assert_eq!(
            parse_index_file_name("3799-98598990.idx"),
            Some((3799, 98_598_990))
        );
        assert_eq!(parse_index_file_name("3799-98598990.idx.tmp"), None);
        assert_eq!(parse_index_file_name("3799.idx"), None);
        assert_eq!(parse_index_file_name("notes.txt"), None);
    }

    #[test]
    fn eviction_takes_the_least_recently_used_until_within_bounds() {
        let at = |seconds: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
        let files = [(40, at(3)), (40, at(1)), (40, at(2)), (40, at(4))];
        assert!(evictions(&files, 4, 160).is_empty(), "within both bounds");
        assert_eq!(evictions(&files, 3, 1000), vec![1], "too many");
        assert_eq!(evictions(&files, 10, 100), vec![1, 2], "too heavy");
    }

    #[test]
    fn the_shelf_knows_its_files_and_sweeps_cut_off_saves() {
        let root = std::env::temp_dir().join(format!(
            "deadsync-rip-shelf-{}-{}",
            std::process::id(),
            line!()
        ));
        let dir = root.join("rip-packs").join(INDEX_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("12-345.idx"), b"x").unwrap();
        fs::write(dir.join("12-345.idx.tmp"), b"x").unwrap();
        fs::write(dir.join("readme.txt"), b"x").unwrap();
        let shelf = Shelf::open(&root);
        assert!(shelf.has(12, 345));
        assert!(!shelf.has(12, 346), "another version of the pack");
        assert!(!dir.join("12-345.idx.tmp").exists());
        assert!(shelf.load(12, 345).is_none(), "bytes that do not read back");
        assert!(!dir.join("12-345.idx").exists(), "are deleted");
        assert!(!shelf.has(12, 345));
        assert!(
            dir.join("readme.txt").exists(),
            "what it did not make, it leaves"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
