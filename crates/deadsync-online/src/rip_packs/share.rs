//! Telling itgcontent.net about rips: its list of recent rips, and the mark
//! a kept one gets there.
//!
//! The ITGmania Content Browser's rips are made by its site, which keeps each
//! one for the list as it makes it. DeadSync makes its rips on the player's
//! machine, so it reports each one instead: when a search deals cards, a
//! worker posts what the rip asked for and the songs it found to
//! `/api/rips`, and the site answers at once with the rip's id. It believes
//! none of it -- every song is checked against stepmaniaonline.net before the
//! rip is listed. KEEP posts that id to `/api/rips/kept`; a KEEP that comes
//! before the answer is posted as soon as the answer does, even when the rip
//! was thrown back or reset by then. A re-rip also sends the keys of the
//! cards it threw back, by which the site marks the earlier rip re-ripped; a
//! re-rip that found nothing, or was stopped, sends only those.
//!
//! A song's folder and simfiles are named as the site's relay names them
//! ([`crate::pack_archive::ZipEntry::site_name`]), so the site finds them in
//! the pack it serves.
//!
//! Only while the machine's Share Rips option is on, which the shell hands
//! over at startup and whenever the machine config changes
//! ([`super::runtime_set_sharing`]). Nothing about the player is sent. Fire
//! and forget: every request runs on a thread of its own with short
//! timeouts, and a failure is logged at debug, never shown, never tried
//! again.
//!
//! The requests go through a [`Wire`], so tests can stand in for the
//! network and run the threads' work by hand.
//!
//! Runtime contract:
//!
//! - Owner: one short-lived thread per request. The rip runtime notes the
//!   rip on the table; the shell only sets the switch.
//! - Thread safety: the switch is an atomic; one mutex guards what is known
//!   of the rip on the table (which it is, its id, whether KEEP is waiting
//!   for that id) and of a kept rip still waiting for its id after the table
//!   moved on, and is never held across a request.
//! - Capacity: one report per rip that dealt cards or re-ripped, one kept
//!   per kept rip.
//! - Worst frame cost: an atomic load and a lock; a KEEP whose id is known,
//!   or a re-rip stopped or reset mid-search, also spawns its request's
//!   thread.

use super::{DeclaredSync, Era, HeldSong, RipOptions, RipSong};
use crate::pack_archive::PackIndex;
use crate::stepmaniaonline::PackInfo;
use deadsync_net as network;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

/// Where rips are reported, and where a kept one is marked: the site the
/// booster lists come from ([`super::lists`]).
pub(super) const RIPS_URL: &str = "https://itgcontent.net/api/rips";
pub(super) const KEPT_URL: &str = "https://itgcontent.net/api/rips/kept";
const JSON: &str = "application/json";
const FORM: &str = "application/x-www-form-urlencoded";

/// Nothing waits on these, so they give up early.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(15);
/// The answer is an id, or nothing.
const MAX_ANSWER_BYTES: usize = 4 * 1024;

/// What the site takes, as the protocol bounds it.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_SONGS: usize = 30;
const MAX_NAMES: usize = 20;
/// A name's length as the site counts it: UTF-16 units.
const MAX_NAME_UNITS: usize = 64;
const MAX_THROWN: usize = 60;
const MAX_SIMS: usize = 8;
const MAX_METER: u32 = 30;
const MAX_SONG_BYTES: u64 = (1 << 31) - 1;
/// A credit's length as the site counts it: code points.
const MAX_CREDIT_CHARS: usize = 255;
/// A folder's or a simfile's name, counted the same way.
const MAX_FILE_CHARS: usize = 255;
/// The earliest year the site takes: DDR's own, which dates the oldest packs.
/// The latest is next year, UTC, as dates are believed up to tomorrow.
const FIRST_YEAR: u16 = 1998;
/// A rip's id: 16 lowercase hex digits.
const ID_LEN: usize = 16;

// ------------------------------------------------------------------ report

/// What a rip asked for, held from the rip until its search answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Asked {
    /// As the search ran with them: the meter range in order and in bounds.
    pub options: RipOptions,
    /// The keys of the cards a re-rip threw back; empty for a first rip.
    pub thrown: Vec<String>,
}

/// A card as the site hears of it.
#[derive(Debug)]
pub(super) struct SharedSong<'a> {
    pub song: &'a RipSong,
    /// Its folder as the site's relay names it ([`site_folder`]), else as
    /// the card has it.
    pub folder: &'a str,
    /// What the catalogue says its pack is synced to.
    pub sync: Option<DeclaredSync>,
    /// The simfiles directly in its folder ([`sims_of`]).
    pub sims: Vec<String>,
}

/// The cards, with what the report says of each beyond the card itself:
/// their pack's declared sync from `catalogue`, and their folder and
/// simfiles as the relay names them, from the index the search found them in
/// (`held`, card for card).
pub(super) fn shared_songs<'a>(
    songs: &'a [RipSong],
    held: &'a [Arc<HeldSong>],
    catalogue: &[PackInfo],
) -> Vec<SharedSong<'a>> {
    songs
        .iter()
        .enumerate()
        .map(|(at, song)| {
            let held = held.get(at);
            SharedSong {
                song,
                folder: held
                    .and_then(|held| site_folder(&held.cache.index, held.folder))
                    .unwrap_or(&song.folder),
                sync: catalogue
                    .iter()
                    .find(|pack| pack.id == song.pack_id)
                    .and_then(super::declared_sync),
                sims: held.map_or_else(Vec::new, |held| sims_of(&held.cache.index, held.folder)),
            }
        })
        .collect()
}

/// A path's song folder and file name as the relay reads them: split at `/`
/// alone, exactly `<pack>/<folder>/<file>`. `None` for anything else, and
/// for an empty folder or file or one with a backslash, which the site takes
/// as neither.
fn site_parts(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.split('/');
    let (_pack, folder, file) = (parts.next()?, parts.next()?, parts.next()?);
    (parts.next().is_none() && site_file_name(folder) && site_file_name(file))
        .then_some((folder, file))
}

/// Whether the site takes this as a song folder's or a simfile's name: one
/// name, not a path -- no `/` or `\`, not `.` or `..` -- of at most
/// [`MAX_FILE_CHARS`] characters and no control characters. It refuses the
/// whole report over one that is not, so such a song or simfile is left out
/// instead.
fn site_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.chars().count() <= MAX_FILE_CHARS
        && !name.contains(['/', '\\'])
        && !name.chars().any(is_control)
}

/// The control characters the site refuses in any name: U+0000-U+001F and
/// U+007F.
fn is_control(ch: char) -> bool {
    ch <= '\u{1f}' || ch == '\u{7f}'
}

/// A song folder's name as the relay spells it: its simfile's folder.
fn site_folder(index: &PackIndex, folder: usize) -> Option<&str> {
    let simfile = index.folders.get(folder)?.simfile?;
    site_parts(index.entries.get(simfile)?.site_name()).map(|(folder, _)| folder)
}

/// The simfiles directly in a song's folder, named as the relay names them
/// -- `.sm` and `.ssc`, case aside, AppleDouble companions left out -- in
/// directory order, at most [`MAX_SIMS`].
pub(super) fn sims_of(index: &PackIndex, folder: usize) -> Vec<String> {
    let Some(folder) = index.folders.get(folder) else {
        return Vec::new();
    };
    folder
        .entries
        .iter()
        .filter_map(|&at| index.entries.get(at))
        .filter_map(|entry| site_parts(entry.site_name()))
        .filter_map(|(_, file)| {
            let lower = file.to_ascii_lowercase();
            let simfile = lower.ends_with(".sm") || lower.ends_with(".ssc");
            (simfile && !file.starts_with("._")).then(|| file.to_owned())
        })
        .take(MAX_SIMS)
        .collect()
}

#[derive(Serialize)]
struct ReportBody<'a> {
    eras: Vec<&'static str>,
    lo: u32,
    hi: u32,
    full: bool,
    authors: Vec<&'a str>,
    avoid: Vec<&'a str>,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    x: &'a [String],
    songs: Vec<SongBody<'a>>,
}

#[derive(Serialize)]
struct SongBody<'a> {
    key: &'a str,
    pack: u64,
    song: u64,
    folder: &'a str,
    credit: &'a str,
    meter: u32,
    year: u16,
    sync: &'static str,
    sims: &'a [String],
    bytes: u64,
}

const fn era_key(era: Era) -> &'static str {
    match era {
        Era::Modern => "modern",
        Era::Mid => "mid",
        Era::Vintage => "vintage",
    }
}

/// `name` cut to `units` UTF-16 units, at a character.
fn clip(name: &str, units: usize) -> &str {
    let mut used = 0;
    for (at, ch) in name.char_indices() {
        used += ch.len_utf16();
        if used > units {
            return &name[..at];
        }
    }
    name
}

/// The names asked for, as the site takes them. One with a control character
/// would have it refuse the whole report, so it is left out.
fn names(names: &[String]) -> Vec<&str> {
    names
        .iter()
        .filter(|name| !name.chars().any(is_control))
        .take(MAX_NAMES)
        .map(|name| clip(name, MAX_NAME_UNITS))
        .collect()
}

/// A credit the site takes, or none: one too long would have it refuse the
/// whole report.
fn site_credit(credit: &str) -> &str {
    if credit.chars().count() <= MAX_CREDIT_CHARS {
        credit
    } else {
        ""
    }
}

/// A year the site takes, or 0 for none: [`FIRST_YEAR`] to next year, when
/// `this_year` is this year, UTC.
fn site_year(year: Option<u16>, this_year: u16) -> u16 {
    year.filter(|year| (FIRST_YEAR..=this_year.saturating_add(1)).contains(year))
        .unwrap_or(0)
}

/// This year, UTC.
fn this_year() -> u16 {
    u16::try_from(super::dates::today() / 10_000).unwrap_or(u16::MAX)
}

/// The report's JSON. The lists are cut to what the site takes, a value it
/// would refuse -- a meter, year or credit out of its bounds -- is sent as
/// none, and a song whose folder it would refuse is left out, so one song
/// cannot cost the whole report. A report left with no song and nothing
/// thrown back is not worth sending.
pub(super) fn report_body(asked: &Asked, songs: &[SharedSong<'_>]) -> Result<Vec<u8>, String> {
    let options = &asked.options;
    let this_year = this_year();
    let songs: Vec<&SharedSong<'_>> = songs
        .iter()
        .filter(|shared| site_file_name(shared.folder))
        .take(MAX_SONGS)
        .collect();
    if songs.is_empty() && asked.thrown.is_empty() {
        return Err("no song the site would take".to_owned());
    }
    let body = ReportBody {
        eras: Era::ALL
            .into_iter()
            .filter(|era| options.eras.contains(era))
            .map(era_key)
            .collect(),
        lo: options.lo,
        hi: options.hi,
        full: options.full,
        authors: names(&options.authors),
        avoid: names(&options.avoid),
        x: &asked.thrown[..asked.thrown.len().min(MAX_THROWN)],
        songs: songs
            .iter()
            .map(|shared| {
                let song = shared.song;
                SongBody {
                    key: &song.key,
                    pack: song.pack_id,
                    song: song.song_id,
                    folder: shared.folder,
                    credit: site_credit(&song.credit),
                    meter: if (1..=MAX_METER).contains(&song.meter) {
                        song.meter
                    } else {
                        0
                    },
                    year: site_year(song.year, this_year),
                    sync: shared.sync.map_or("", DeclaredSync::label),
                    sims: &shared.sims[..shared.sims.len().min(MAX_SIMS)],
                    bytes: song.bytes.min(MAX_SONG_BYTES),
                }
            })
            .collect(),
    };
    let json = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    if json.len() > MAX_BODY_BYTES {
        return Err(format!("the report is {} bytes", json.len()));
    }
    Ok(json)
}

#[derive(Deserialize)]
struct RipAnswer {
    rip: String,
}

/// The id in the site's answer to a report, if it is one.
fn rip_id(body: &[u8]) -> Option<String> {
    let answer: RipAnswer = serde_json::from_slice(body).ok()?;
    let id = answer.rip;
    let hex = |byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
    (id.len() == ID_LEN && id.bytes().all(hex)).then_some(id)
}

// -------------------------------------------------------------------- wire

/// The site's answer to a request.
#[derive(Debug)]
pub(super) struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

/// How requests leave: the network in the game, a recorder in tests.
pub(super) trait Wire: Send + Sync + 'static {
    /// Post `body`, waiting for the answer. Worker threads only.
    fn post(
        &self,
        url: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    ) -> Result<Answer, String>;
    /// Run `job` off the caller's thread.
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), String>;
}

/// The network.
pub(super) struct Net;

static AGENT: LazyLock<network::HttpAgent> = LazyLock::new(|| {
    network::build_agent_with_connect(network::AgentConfig { timeout: TIMEOUT }, CONNECT_TIMEOUT)
});

impl Wire for Net {
    fn post(
        &self,
        url: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    ) -> Result<Answer, String> {
        let response = AGENT
            .post(url)
            .header("Content-Type", content_type)
            .send(body)
            .map_err(|error| network::error_from_ureq(error).to_string())?;
        let status = response.status().as_u16();
        let body = network::read_bytes_body_bounded(response, MAX_ANSWER_BYTES)
            .map_err(|error| error.to_string())?;
        Ok(Answer {
            status,
            body: body.into_vec(),
        })
    }

    fn spawn(&self, job: Box<dyn FnOnce() + Send>) -> Result<(), String> {
        thread::Builder::new()
            .name("rip-share".to_owned())
            .spawn(job)
            .map(drop)
            .map_err(|error| error.to_string())
    }
}

// ------------------------------------------------------------------ sharer

/// What is known of the rip on the table.
#[derive(Debug, Default)]
struct Table {
    /// The rip, once its cards were dealt with sharing on; `None` before,
    /// and after a new rip or a reset.
    set: Option<u64>,
    /// Its id on the site, once the site answered.
    id: Option<String>,
    /// KEEP was pressed: the kept has gone, or goes with the id.
    kept: bool,
    /// An earlier rip KEEP was pressed on before the site named it, carried
    /// past a reset or a new rip (allowed only once its keep has ended), so
    /// its kept still goes when its answer does.
    waiting: Option<u64>,
}

impl Table {
    /// The rip whose kept still waits for its id once this table is
    /// replaced: this one's, if KEEP is waiting on it, else the one already
    /// carried.
    fn carried(&self) -> Option<u64> {
        match (self.set, &self.id, self.kept) {
            (Some(set), None, true) => Some(set),
            _ => self.waiting,
        }
    }
}

pub(super) struct Sharer<W> {
    wire: W,
    on: AtomicBool,
    table: Mutex<Table>,
}

/// The game's.
pub(super) static SHARE: Sharer<Net> = Sharer::new(Net);

impl<W: Wire> Sharer<W> {
    /// Off until told otherwise.
    pub(super) const fn new(wire: W) -> Self {
        Self {
            wire,
            on: AtomicBool::new(false),
            table: Mutex::new(Table {
                set: None,
                id: None,
                kept: false,
                waiting: None,
            }),
        }
    }

    pub(super) fn set_on(&self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
    }

    pub(super) fn on(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    fn lock(&self) -> MutexGuard<'_, Table> {
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new rip, or a reset: whatever was known of the last one goes, but
    /// for a KEEP still waiting for its id, which is posted when the id
    /// arrives. Any other id that still comes lands nowhere.
    pub(super) fn forget(&self) {
        let mut table = self.lock();
        let waiting = table.carried();
        *table = Table {
            waiting,
            ..Table::default()
        };
    }

    /// Rip `set` dealt its cards. Noted before it is reported, so a KEEP that
    /// beats the site's answer waits for it.
    pub(super) fn dealt(&self, set: u64) {
        if self.on() {
            let mut table = self.lock();
            let waiting = table.carried();
            *table = Table {
                set: Some(set),
                waiting,
                ..Table::default()
            };
        }
    }

    /// Report rip `set`: its cards, or none for a re-rip that found nothing.
    /// A first rip that found nothing has nothing to say.
    pub(super) fn report(&'static self, set: u64, asked: &Asked, songs: &[SharedSong<'_>]) {
        if !self.on() || (songs.is_empty() && asked.thrown.is_empty()) {
            return;
        }
        match report_body(asked, songs) {
            Ok(body) => self.post(RIPS_URL, JSON, body, move |sharer, answer| {
                sharer.answered(set, &answer);
            }),
            Err(error) => log::debug!("Rip Packs: rip {set} was not reported: {error}"),
        }
    }

    /// The site's answer to rip `set`'s report.
    fn answered(&'static self, set: u64, answer: &Answer) {
        if answer.status == 204 {
            // A re-rip that found nothing: there is no rip to name.
            return;
        }
        let Some(id) = rip_id(&answer.body) else {
            log::debug!(
                "Rip Packs: itgcontent.net answered rip {set}'s report without an id (status {})",
                answer.status
            );
            return;
        };
        let kept = {
            let mut table = self.lock();
            if table.waiting == Some(set) {
                // Kept, then thrown back or reset before its answer came.
                table.waiting = None;
                true
            } else if table.set == Some(set) {
                table.id = Some(id.clone());
                table.kept
            } else {
                return;
            }
        };
        if kept && self.on() {
            self.post_kept(id);
        }
    }

    /// KEEP on rip `set`: marked kept once its id is known.
    pub(super) fn kept(&'static self, set: u64) {
        if !self.on() {
            return;
        }
        let id = {
            let mut table = self.lock();
            if table.set != Some(set) || table.kept {
                return;
            }
            table.kept = true;
            table.id.clone()
        };
        if let Some(id) = id {
            self.post_kept(id);
        }
    }

    fn post_kept(&'static self, id: String) {
        self.post(KEPT_URL, FORM, format!("rip={id}").into_bytes(), |_, _| {});
    }

    fn post(
        &'static self,
        url: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
        then: impl FnOnce(&'static Self, Answer) + Send + 'static,
    ) {
        let job = Box::new(move || match self.wire.post(url, content_type, body) {
            Ok(answer) => then(self, answer),
            Err(error) => log::debug!("Rip Packs: {url} failed: {error}"),
        });
        if let Err(error) = self.wire.spawn(job) {
            log::debug!("Rip Packs: could not start a request to {url}: {error}");
        }
    }
}

/// A stand-in for the site, for this module's tests and the runtime's.
#[cfg(test)]
pub(super) mod fake {
    use super::{Answer, RIPS_URL, Sharer, Wire};
    use std::sync::Mutex;

    pub(in super::super) const ID: &str = "0123456789abcdef";

    type Job = Box<dyn FnOnce() + Send>;

    /// The site, as far as the tests need it: a report with songs gets an
    /// id, one without gets a 204, a kept gets a 204. Nothing runs until
    /// [`run`] does.
    #[derive(Default)]
    pub(in super::super) struct Fake {
        sent: Mutex<Vec<(&'static str, &'static str, String)>>,
        pub(in super::super) jobs: Mutex<Vec<Job>>,
    }

    impl Wire for Fake {
        fn post(
            &self,
            url: &'static str,
            content_type: &'static str,
            body: Vec<u8>,
        ) -> Result<Answer, String> {
            let text = String::from_utf8(body).expect("utf-8");
            let dealt = url == RIPS_URL
                && serde_json::from_str::<serde_json::Value>(&text).expect("json")["songs"]
                    .as_array()
                    .is_some_and(|songs| !songs.is_empty());
            self.sent.lock().unwrap().push((url, content_type, text));
            Ok(if dealt {
                Answer {
                    status: 200,
                    body: format!("{{\"rip\":\"{ID}\"}}").into_bytes(),
                }
            } else {
                Answer {
                    status: 204,
                    body: Vec::new(),
                }
            })
        }

        fn spawn(&self, job: Job) -> Result<(), String> {
            self.jobs.lock().unwrap().push(job);
            Ok(())
        }
    }

    /// A sharer of its own, sharing on.
    pub(in super::super) fn new_sharer() -> &'static Sharer<Fake> {
        let sharer = Box::leak(Box::new(Sharer::new(Fake::default())));
        sharer.set_on(true);
        sharer
    }

    /// Every request asked for so far, and any they start, as the threads
    /// would.
    pub(in super::super) fn run(sharer: &Sharer<Fake>) {
        loop {
            let jobs = std::mem::take(&mut *sharer.wire.jobs.lock().unwrap());
            if jobs.is_empty() {
                return;
            }
            for job in jobs {
                job();
            }
        }
    }

    /// Every request sent: its URL, content type and body.
    pub(in super::super) fn sent(
        sharer: &Sharer<Fake>,
    ) -> Vec<(&'static str, &'static str, String)> {
        sharer.wire.sent.lock().unwrap().clone()
    }

    pub(in super::super) fn urls(sharer: &Sharer<Fake>) -> Vec<&'static str> {
        sent(sharer).into_iter().map(|(url, ..)| url).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;
    use crate::pack_archive::{SongFolder, ZipEntry};

    fn card(
        pack_id: u64,
        song_id: u64,
        folder: &str,
        credit: &str,
        meter: u32,
        year: Option<u16>,
        bytes: u64,
    ) -> RipSong {
        RipSong {
            key: format!("{pack_id}:{song_id}"),
            pack_id,
            pack_name: "Never sent".to_owned(),
            song_id,
            title: "Never sent".to_owned(),
            artist: "Never sent".to_owned(),
            credit: credit.to_owned(),
            meter,
            year,
            art_url: Some("https://example.invalid/never-sent.png".to_owned()),
            folder: folder.to_owned(),
            bytes,
        }
    }

    fn cards() -> Vec<RipSong> {
        vec![
            card(
                4110,
                528511,
                "Song Folder",
                "Mad Matt",
                14,
                Some(2021),
                12_345_678,
            ),
            card(77, 9001, "Other", "", 9, None, 4096),
        ]
    }

    fn shared(cards: &[RipSong]) -> Vec<SharedSong<'_>> {
        vec![
            SharedSong {
                song: &cards[0],
                folder: &cards[0].folder,
                sync: Some(DeclaredSync::Itg),
                sims: vec!["song.ssc".to_owned()],
            },
            SharedSong {
                song: &cards[1],
                folder: &cards[1].folder,
                sync: None,
                sims: vec!["Other.sm".to_owned(), "Other.ssc".to_owned()],
            },
        ]
    }

    /// Each card as it is, with no simfiles and no sync.
    fn bare(cards: &[RipSong]) -> Vec<SharedSong<'_>> {
        cards
            .iter()
            .map(|song| SharedSong {
                song,
                folder: &song.folder,
                sync: None,
                sims: Vec::new(),
            })
            .collect()
    }

    fn first_rip() -> Asked {
        Asked {
            options: RipOptions {
                eras: vec![Era::Mid, Era::Modern],
                authors: vec!["Zaia".to_owned(), "dimo".to_owned()],
                avoid: vec!["someone".to_owned()],
                lo: 8,
                hi: 14,
                full: false,
            },
            thrown: Vec::new(),
        }
    }

    fn re_rip() -> Asked {
        Asked {
            options: RipOptions {
                eras: Vec::new(),
                full: true,
                ..RipOptions::default()
            },
            thrown: vec!["1:2".to_owned(), "3:4".to_owned()],
        }
    }

    fn text(body: Vec<u8>) -> String {
        String::from_utf8(body).expect("utf-8")
    }

    #[test]
    fn a_rip_is_reported_as_the_protocol_has_it() {
        let cards = cards();
        let body = report_body(&first_rip(), &shared(&cards)).expect("a report");
        assert_eq!(
            text(body),
            concat!(
                r#"{"eras":["modern","mid"],"lo":8,"hi":14,"full":false,"#,
                r#""authors":["Zaia","dimo"],"avoid":["someone"],"songs":["#,
                r#"{"key":"4110:528511","pack":4110,"song":528511,"folder":"Song Folder","#,
                r#""credit":"Mad Matt","meter":14,"year":2021,"sync":"ITG","#,
                r#""sims":["song.ssc"],"bytes":12345678},"#,
                r#"{"key":"77:9001","pack":77,"song":9001,"folder":"Other","#,
                r#""credit":"","meter":9,"year":0,"sync":"","#,
                r#""sims":["Other.sm","Other.ssc"],"bytes":4096}]}"#,
            )
        );
    }

    #[test]
    fn a_re_rip_sends_the_keys_it_threw_back() {
        let cards = cards();
        let mut songs = shared(&cards);
        songs.truncate(1);
        songs[0].sync = Some(DeclaredSync::Null);
        let body = report_body(&re_rip(), &songs).expect("a report");
        assert_eq!(
            text(body),
            concat!(
                r#"{"eras":[],"lo":8,"hi":14,"full":true,"authors":[],"avoid":[],"#,
                r#""x":["1:2","3:4"],"songs":["#,
                r#"{"key":"4110:528511","pack":4110,"song":528511,"folder":"Song Folder","#,
                r#""credit":"Mad Matt","meter":14,"year":2021,"sync":"NULL","#,
                r#""sims":["song.ssc"],"bytes":12345678}]}"#,
            )
        );
    }

    #[test]
    fn a_report_keeps_to_what_the_site_takes() {
        let long = "\u{e9}".repeat(70);
        let mut asked = first_rip();
        asked.options.authors = (0..25).map(|n| format!("author {n}")).collect();
        asked.options.authors[0] = long.clone();
        asked.options.avoid = vec!["\u{1f600}".repeat(40)];
        asked.thrown = (0..70).map(|n| format!("1:{n}")).collect();
        let cards = [card(1, 2, "F", "", 31, None, u64::MAX)];
        let songs = vec![SharedSong {
            song: &cards[0],
            folder: &cards[0].folder,
            sync: None,
            sims: (0..10).map(|n| format!("{n}.sm")).collect(),
        }];
        let body = report_body(&asked, &songs).expect("a report");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let authors = json["authors"].as_array().unwrap();
        assert_eq!(authors.len(), MAX_NAMES);
        assert_eq!(authors[0], long[..2 * MAX_NAME_UNITS].to_owned());
        // A character of two UTF-16 units is cut whole.
        assert_eq!(json["avoid"][0], "\u{1f600}".repeat(32));
        assert_eq!(json["x"].as_array().unwrap().len(), MAX_THROWN);
        let song = &json["songs"][0];
        assert_eq!(song["meter"], 0, "a meter out of range is unknown");
        assert_eq!(song["bytes"], (1u64 << 31) - 1);
        assert_eq!(song["sims"].as_array().unwrap().len(), MAX_SIMS);
    }

    #[test]
    fn a_name_the_site_refuses_is_left_out_not_sent() {
        let mut asked = first_rip();
        asked.options.authors = vec!["Zaia".to_owned(), "bad\u{7}name".to_owned()];
        asked.options.avoid = vec!["tab\there".to_owned()];
        let cards = [
            card(1, 1, "..", "", 10, None, 1),
            card(1, 2, "fine\u{1}", "", 10, None, 1),
            card(1, 3, "Kept", "", 10, None, 1),
        ];
        let body = report_body(&asked, &bare(&cards)).expect("a report");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["authors"], serde_json::json!(["Zaia"]));
        assert_eq!(json["avoid"], serde_json::json!([]));
        let folders: Vec<&str> = json["songs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|song| song["folder"].as_str().unwrap())
            .collect();
        assert_eq!(folders, vec!["Kept"]);
        // Nothing the site would take and nothing thrown back: no report.
        assert!(report_body(&first_rip(), &bare(&cards[..2])).is_err());
        // ...but a re-rip still says what it threw back.
        assert!(report_body(&re_rip(), &bare(&cards[..2])).is_ok());
        // Simfile names keep to the same rule.
        assert_eq!(site_parts("Pack/Song/a\u{1}.sm"), None);
        assert_eq!(site_parts("Pack/../song.sm"), None);
        assert_eq!(site_parts("Pack/Song/song.sm"), Some(("Song", "song.sm")));
    }

    #[test]
    fn a_year_or_credit_the_site_refuses_is_sent_as_none() {
        let this_year = this_year();
        // Code points, not bytes or UTF-16 units: 255 of either kind fit.
        let longest = "\u{1f600}".repeat(MAX_CREDIT_CHARS);
        let too_long = "a".repeat(MAX_CREDIT_CHARS + 1);
        let cards = [
            card(1, 1, "A", &longest, 10, Some(FIRST_YEAR), 1),
            card(1, 2, "B", &too_long, 10, Some(FIRST_YEAR - 1), 1),
            card(1, 3, "C", "Zaia", 10, Some(this_year + 1), 1),
            card(1, 4, "D", "Zaia", 10, Some(this_year + 2), 1),
            card(1, 5, "E", "", 10, Some(2021), 1),
        ];
        let body = report_body(&first_rip(), &bare(&cards)).expect("a report");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        let songs = json["songs"].as_array().unwrap();
        let years: Vec<u64> = songs
            .iter()
            .map(|song| song["year"].as_u64().unwrap())
            .collect();
        assert_eq!(
            years,
            vec![1998, 0, u64::from(this_year) + 1, 0, 2021],
            "1998 through next year, UTC"
        );
        let credits: Vec<&str> = songs
            .iter()
            .map(|song| song["credit"].as_str().unwrap())
            .collect();
        assert_eq!(credits, vec![longest.as_str(), "", "Zaia", "Zaia", ""]);
    }

    #[test]
    fn years_run_from_ddr_to_next_year() {
        assert_eq!(site_year(Some(1998), 2026), 1998);
        assert_eq!(site_year(Some(1997), 2026), 0);
        assert_eq!(site_year(Some(2026), 2026), 2026);
        assert_eq!(site_year(Some(2027), 2026), 2027);
        assert_eq!(site_year(Some(2028), 2026), 0);
        assert_eq!(site_year(None, 2026), 0);
        assert_eq!(site_year(Some(u16::MAX), u16::MAX), u16::MAX);
        assert!(this_year() >= 2026);
    }

    #[test]
    fn a_re_rip_that_found_nothing_sends_only_what_it_threw_back() {
        let sharer = new_sharer();
        sharer.report(3, &re_rip(), &[]);
        run(sharer);
        assert_eq!(
            sent(sharer),
            vec![(
                RIPS_URL,
                JSON,
                concat!(
                    r#"{"eras":[],"lo":8,"hi":14,"full":true,"authors":[],"avoid":[],"#,
                    r#""x":["1:2","3:4"],"songs":[]}"#,
                )
                .to_owned()
            )]
        );
        // The 204 names no rip, so a KEEP has nothing to mark.
        sharer.kept(3);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL]);

        // A first rip that found nothing says nothing at all.
        let sharer = new_sharer();
        sharer.report(4, &first_rip(), &[]);
        run(sharer);
        assert!(sent(sharer).is_empty());
    }

    #[test]
    fn a_keep_before_the_answer_is_posted_when_the_id_arrives() {
        let sharer = new_sharer();
        let cards = cards();
        sharer.dealt(5);
        sharer.report(5, &first_rip(), &shared(&cards));
        sharer.kept(5);
        assert!(
            sent(sharer).is_empty(),
            "nothing goes on the caller's thread"
        );
        run(sharer);
        let sent = sent(sharer);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, RIPS_URL);
        assert_eq!(sent[1], (KEPT_URL, FORM, format!("rip={ID}")));

        // Once only.
        sharer.kept(5);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL, KEPT_URL]);
    }

    #[test]
    fn a_keep_after_the_answer_is_posted_at_once() {
        let sharer = new_sharer();
        let cards = cards();
        sharer.dealt(6);
        sharer.report(6, &first_rip(), &shared(&cards));
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL]);
        sharer.kept(6);
        run(sharer);
        assert_eq!(
            sent(sharer)[1],
            (KEPT_URL, FORM, format!("rip={ID}")),
            "the id the site gave"
        );
    }

    #[test]
    fn a_keep_waiting_for_its_id_outlasts_a_reset_or_a_new_rip() {
        let cards = cards();
        let kept = (KEPT_URL, FORM, format!("rip={ID}"));

        // KEEP, then (its keep over) a reset, then the answer.
        let sharer = new_sharer();
        sharer.dealt(7);
        sharer.report(7, &first_rip(), &shared(&cards));
        sharer.kept(7);
        sharer.forget();
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL, KEPT_URL]);
        assert_eq!(sent(sharer)[1], kept);

        // KEEP, then a reset and a new rip dealt, then the old answer.
        let sharer = new_sharer();
        sharer.dealt(8);
        sharer.report(8, &first_rip(), &shared(&cards));
        sharer.kept(8);
        sharer.forget();
        sharer.dealt(9);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL, KEPT_URL]);
        assert_eq!(sent(sharer)[1], kept);
        // ...while the new rip keeps its own table: its KEEP waits for its
        // own id, and the old one's is not posted twice.
        sharer.kept(9);
        sharer.report(9, &re_rip(), &shared(&cards));
        sharer.forget();
        run(sharer);
        assert_eq!(
            urls(sharer),
            vec![RIPS_URL, KEPT_URL, RIPS_URL, KEPT_URL],
            "the new rip's kept, carried past its own reset"
        );
        sharer.forget();
        run(sharer);
        assert_eq!(sent(sharer).len(), 4);
    }

    #[test]
    fn no_kept_for_a_rip_replaced_before_it_was_kept() {
        let cards = cards();

        // A new rip dealt before the answer: the old one's id lands nowhere,
        // and a KEEP of the old one finds nothing to mark.
        let sharer = new_sharer();
        sharer.dealt(8);
        sharer.report(8, &first_rip(), &shared(&cards));
        sharer.forget();
        sharer.dealt(9);
        run(sharer);
        sharer.kept(8);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL]);

        // ...while the new one, reported and kept, is marked.
        sharer.report(9, &re_rip(), &shared(&cards));
        sharer.kept(9);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL, RIPS_URL, KEPT_URL]);
    }

    #[test]
    fn nothing_at_all_while_sharing_is_off() {
        let cards = cards();
        let sharer = new_sharer();
        sharer.set_on(false);
        sharer.dealt(10);
        sharer.report(10, &first_rip(), &shared(&cards));
        sharer.report(10, &re_rip(), &[]);
        sharer.kept(10);
        run(sharer);
        assert!(sent(sharer).is_empty());
        assert!(sharer.wire.jobs.lock().unwrap().is_empty());

        // Turned off while KEEP waits for the answer: the kept stays home.
        let sharer = new_sharer();
        sharer.dealt(11);
        sharer.report(11, &first_rip(), &shared(&cards));
        sharer.kept(11);
        sharer.set_on(false);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL]);

        // ...even when the KEEP was carried past a reset.
        let sharer = new_sharer();
        sharer.dealt(12);
        sharer.report(12, &first_rip(), &shared(&cards));
        sharer.kept(12);
        sharer.forget();
        sharer.set_on(false);
        run(sharer);
        assert_eq!(urls(sharer), vec![RIPS_URL]);
    }

    #[test]
    fn only_an_id_of_sixteen_lowercase_hex_digits_is_taken() {
        assert_eq!(
            rip_id(br#"{"rip":"0123456789abcdef"}"#).as_deref(),
            Some(ID)
        );
        assert_eq!(rip_id(br#"{"rip":"0123456789ABCDEF"}"#), None);
        assert_eq!(rip_id(br#"{"rip":"0123456789abcde"}"#), None);
        assert_eq!(rip_id(br#"{"rip":"0123456789abcdefg"}"#), None);
        assert_eq!(rip_id(br#"{"error":"bad"}"#), None);
        assert_eq!(rip_id(b""), None);
    }

    fn entry(name: &str) -> ZipEntry {
        ZipEntry {
            name: name.to_owned(),
            compressed: 1,
            uncompressed: 1,
            local_header: 0,
            end: 0,
            day: 0,
            crc: 0,
            site_name: None,
        }
    }

    #[test]
    fn sims_are_the_simfiles_directly_in_the_folder() {
        let names = [
            "Pack/Song/",
            "Pack/Song/song.ssc",
            "Pack/Song/song.sm",
            "Pack/Song/EDIT.SM",
            "Pack/Song/._song.ssc",
            "Pack/Song/song.ogg",
            "Pack/Song/song.ssc.oldsync",
            "Pack/Song/old/song.sm",
            // The relay splits at '/' alone: to it, this is no song's.
            "Pack\\Song\\back.ssc",
            "Pack/Song/a\\b.sm",
            "Pack/Other/other.sm",
        ];
        let entries: Vec<ZipEntry> = names.iter().map(|name| entry(name)).collect();
        let index = PackIndex {
            pack_id: 1,
            total: 0,
            etag: None,
            entries,
            folders: vec![
                SongFolder {
                    name: "Song".to_owned(),
                    entries: (0..10).collect(),
                    simfile: Some(1),
                    audio: vec![5],
                },
                SongFolder {
                    name: "Other".to_owned(),
                    entries: vec![10],
                    simfile: Some(10),
                    audio: Vec::new(),
                },
            ],
            tail_start: 0,
            tail: Arc::from(Vec::<u8>::new()),
        };
        assert_eq!(sims_of(&index, 0), vec!["song.ssc", "song.sm", "EDIT.SM"]);
        assert_eq!(sims_of(&index, 1), vec!["other.sm"]);
        assert!(sims_of(&index, 2).is_empty());
        assert_eq!(site_folder(&index, 0), Some("Song"));
        assert_eq!(site_folder(&index, 2), None);
    }

    #[test]
    fn site_parts_are_the_relays_pack_folder_file() {
        assert_eq!(site_parts("Pack/Song/song.ssc"), Some(("Song", "song.ssc")));
        assert_eq!(site_parts("/Song/song.ssc"), Some(("Song", "song.ssc")));
        assert_eq!(site_parts("Pack/Song/"), None);
        assert_eq!(site_parts("Pack//song.ssc"), None);
        assert_eq!(site_parts("Pack/Song/old/song.sm"), None);
        assert_eq!(site_parts("Pack\\Song\\song.sm"), None);
        assert_eq!(site_parts("Pack/So\\ng/song.sm"), None);
        assert_eq!(site_parts("Pack/Song"), None);
    }

    /// A pack zip of these files, stored, whose records lack the UTF-8 flag
    /// as some zippers write them: the crate then reads a non-ASCII name as
    /// CP437, and the relay as UTF-8.
    fn unflagged_zip(files: &[&str]) -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for name in files {
            zip.start_file(*name, options).expect("start entry");
            zip.write_all(b"#TITLE:x;").expect("write entry");
        }
        let mut bytes = zip.finish().expect("finish zip").into_inner();
        // Bit 11 of the flags: at 6 in a local header, at 8 in a central
        // record.
        for at in 0..bytes.len().saturating_sub(10) {
            let flags = match bytes[at..at + 4] {
                [b'P', b'K', 3, 4] => at + 6,
                [b'P', b'K', 1, 2] => at + 8,
                _ => continue,
            };
            bytes[flags + 1] &= !0x08;
        }
        bytes
    }

    #[test]
    fn a_folder_and_its_sims_are_named_as_the_relay_names_them() {
        let zip = unflagged_zip(&[
            "Pack/Se\u{f1}orita/song.ssc",
            "Pack/Se\u{f1}orita/EDIT.sm",
            "Pack/Se\u{f1}orita/song.ogg",
        ]);
        let shelf = PackIndex {
            pack_id: 0x5A1E_F01D,
            total: zip.len() as u64,
            etag: None,
            entries: Vec::new(),
            folders: Vec::new(),
            tail_start: 0,
            tail: Arc::from(zip),
        };
        let bytes = crate::pack_archive::index_to_bytes(&shelf);
        let index = crate::pack_archive::index_from_bytes(&bytes).expect("an index");
        let cp437 = "Se\u{251c}\u{2592}orita";
        assert_eq!(index.folders[0].name, cp437, "the crate reads CP437");

        let mut cards = vec![card(0x5A1E_F01D, 1, cp437, "", 10, None, 1)];
        cards.push(card(77, 2, "Not held", "", 10, None, 1));
        let held = vec![Arc::new(HeldSong {
            cache: crate::smo_songs::adopt_index(index),
            folder: 0,
        })];
        let shared = shared_songs(&cards, &held, &[]);
        assert_eq!(shared[0].folder, "Se\u{f1}orita");
        assert_eq!(shared[0].sims, vec!["song.ssc", "EDIT.sm"]);
        assert_eq!(shared[1].folder, "Not held", "the card's own, unheld");
        assert!(shared[1].sims.is_empty());

        let body = report_body(&first_rip(), &shared).expect("a report");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["songs"][0]["folder"], "Se\u{f1}orita");
        assert_eq!(json["songs"][0]["sims"][0], "song.ssc");
    }

    #[test]
    fn a_cards_sync_is_its_packs_in_the_catalogue() {
        let pack = |id: u64, sync: Option<&str>| {
            PackInfo::new(
                id,
                format!("Pack {id}"),
                1,
                1,
                sync.map(str::to_owned),
                None,
                None,
                None,
            )
        };
        let catalogue = [pack(4110, Some("9ms")), pack(77, Some("n/a"))];
        let mut cards = cards();
        cards.push(card(5, 6, "Not listed", "", 10, None, 1));
        let shared = shared_songs(&cards, &[], &catalogue);
        let syncs: Vec<Option<DeclaredSync>> = shared.iter().map(|song| song.sync).collect();
        assert_eq!(syncs, vec![Some(DeclaredSync::Itg), None, None]);
        assert!(shared.iter().all(|song| song.sims.is_empty()));
        assert!(shared.iter().all(|song| song.folder == song.song.folder));
    }

    #[test]
    fn the_site_is_the_one_the_lists_come_from() {
        let site = super::super::lists::LISTS_BASE
            .strip_suffix("booster/")
            .expect("the lists' folder");
        assert_eq!(RIPS_URL, format!("{site}api/rips"));
        assert_eq!(KEPT_URL, format!("{site}api/rips/kept"));
    }
}
