//! Keeping a rip: the cards on the table become a pack in the library.
//!
//! The pack is numbered first -- one past the history's count and past every
//! booster folder in Songs, whatever its case -- and the number is written to
//! the history before anything is downloaded, so a pack whose songs all fail
//! has still used it and the next pack never takes it again. Then the songs
//! come one at a time, in card order, each its folder's own counted request
//! with the pack's `Pack.ini` riding along -- and one more for a song whose
//! simfile takes its banner from its pack (`../banner.png`), which brings
//! that image into the folder, since on its own the folder would show none.
//! Every song is null-synced as it
//! lands: one whose pack declares ITG, or declares nothing (taken as ITG),
//! has its offsets moved 9 ms, so the booster plays right whatever the
//! machine's `Pack.ini` settings, as the singles group does.
//!
//! Every booster names one series in its `Pack.ini` -- [`SERIES`] -- so the
//! music wheel's series sort keeps them together in a folder of their own;
//! one kept before boosters named it is given it at the next keep.
//!
//! Once the songs are in, the pack asks itgcontent.net for a banner of its
//! own -- its wrapper torn open on a wall of the cards that landed, the
//! hardest in gold, drawn from what the pack is: its number and day, what it
//! was ripped with, and SMO's pictures and Expert meters of its songs -- and
//! its `Pack.ini` names it. It comes last, as ITGmania's does, and a banner
//! that cannot be had costs the pack only its banner.
//!
//! The pack is built where the song scanner cannot see it, in a `._` staging
//! folder under Songs, and appears in one rename once the last song is in: a
//! rescan that happens meanwhile never finds half a pack. Each song is
//! fetched into a scratch folder of its own first and moved in under its own
//! name, one song at a time, so two songs from two packs that share a folder
//! name land side by side as `Name` and `Name (2)` rather than one on top of
//! the other.
//!
//! The rules are the single-song install's (`smo_songs`): the archive
//! checks, staging the scanner skips, a sweep of what a closed run left
//! behind, and temporary files removed on every path out. Nothing outside the
//! Songs folder, the cache's downloads folder and the history file is
//! touched.

use super::share::era_key;
use super::{DeclaredSync, HeldSong, RipOptions, RipSong, SongState};
use crate::pack_archive::{self, ArchiveError, PackIndex};
use crate::smo_songs::{self, PackCache};
use crate::stepmaniaonline::{self, PackInfo, StepManiaOnlineError};
use chrono::{Local, NaiveDate};
use deadsync_chart::song::ITG_SYNC_OFFSET_SECONDS;
use deadsync_net as network;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

/// A song's download, under the cache's downloads folder.
const DOWNLOAD_PREFIX: &str = ".deadsync-booster-";
/// The pack being filled, under Songs. "._" because the song scanner skips
/// that prefix, as it does the single-song install's staging.
const STAGING_PREFIX: &str = "._deadsync-booster-";
/// Where a song is fetched inside the staging folder before it is moved in
/// under its own name: `._song-<card>.part`, one for each card, since songs
/// come a few at a time. Emptied after every song; scanner-skipped too,
/// should one ever be left behind.
const SCRATCH_PREFIX: &str = "._song-";
/// Songs fetched at once. However many run together, each is its folder's
/// own counted request, so this changes only how soon a pack is in: a few
/// keep busy a line that one leaves idle between requests, and get past
/// what one connection is given, while asking far less of the site at once
/// than a browser does.
const SONGS_AT_ONCE: usize = 3;
/// A `Pack.ini` is a handful of lines. The relay left anything larger unread,
/// and so does this.
const MAX_PACK_INI_BYTES: usize = 64 * 1024;
const NONE_KEPT: &str = "none of the songs could be downloaded";

/// Where the pack's banner is drawn: the site the booster lists come from
/// ([`super::lists`]).
const BANNER_URL: &str = "https://itgcontent.net/api/boosterbanner";
/// The banner in the pack, as its `Pack.ini` names it.
const BANNER_FILE: &str = "banner.jpg";
/// The picture is some 60 KB, but one the site has not drawn before takes it
/// a few seconds; the pack waits for it, so it is not waited on for long.
const BANNER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const BANNER_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BANNER_BYTES: usize = 2 * 1024 * 1024;
/// SMO's pictures of songs -- a card's art -- each named for its hash. The
/// site draws the wall from them, told only the name.
const SMO_SONG_ART: &str = "https://stepmaniaonline.net/media/images/songs/";
const ART_HASH_LEN: usize = 40;

/// The series every booster pack names, ITGmania's as well.
const SERIES: &str = "Content Browser Booster Packs";

/// What a booster says about itself: ITGmania's text in DeadSync's words,
/// with its [`SERIES`]. The `#` line and one line per song follow it.
const PACK_INI_HEAD: &str = concat!(
    "[Group]\n",
    "# Written by DeadSync's Find Content.\n",
    "# A booster pack: songs from all over stepmaniaonline.net, every one\n",
    "# null-synced. A song whose own pack declared ITG -- or declared nothing,\n",
    "# and was taken as ITG, as most packs are -- had its offsets moved 9 ms\n",
    "# to NULL, so the pack plays right whatever the machine's settings.\n",
    "Version=1\n",
    "SyncOffset=NULL\n",
    "Series=Content Browser Booster Packs\n",
);

pub(super) struct KeepJob {
    pub set: u64,
    /// The cards, in order.
    pub songs: Vec<RipSong>,
    /// What each card was found in, card for card.
    pub held: Vec<Arc<HeldSong>>,
    /// What the rip was asked for, for its banner.
    pub options: RipOptions,
    pub songs_root: PathBuf,
    pub cache_dir: PathBuf,
    /// The count of kept boosters: `count N`.
    pub history: PathBuf,
}

/// Runs on the thread `runtime_keep` started, and always ends by reporting
/// through [`super::keep_finished`].
pub(super) fn run(job: KeepJob) {
    let mut unreported = Unreported(Some(job.set));
    let outcome = keep(&job);
    unreported.0 = None;
    match outcome {
        Ok(group_dir) => super::keep_finished(job.set, Some(group_dir), None),
        Err(message) => super::keep_finished(job.set, None, Some(message)),
    }
}

/// Says a keep ended when it ended without saying so -- a panic -- so the
/// cards are never left filling forever.
struct Unreported(Option<u64>);

impl Drop for Unreported {
    fn drop(&mut self) {
        if let Some(set) = self.0 {
            super::keep_finished(
                set,
                None,
                Some("the download stopped unexpectedly".to_owned()),
            );
        }
    }
}

/// Number, fill and file the pack. Returns its folder in Songs.
fn keep(job: &KeepJob) -> Result<PathBuf, String> {
    let set = job.set;
    fs::create_dir_all(&job.songs_root).map_err(|error| {
        smo_songs::describe_song_error(&smo_songs::io("create the Songs folder", error))
    })?;
    let downloads = job.cache_dir.join("downloads");
    fs::create_dir_all(&downloads).map_err(|error| {
        smo_songs::describe_song_error(&smo_songs::io("create the Downloads folder", error))
    })?;
    sweep_leftovers(&job.songs_root, &downloads);
    group_boosters(&job.songs_root);

    // The number is taken before anything is downloaded: a pack whose songs
    // all fail has still used it. Unsaved, it is still unique on disk once
    // the pack lands, so a history that cannot be written does not stop it.
    let number = next_number(read_history(&job.history), &job.songs_root);
    let day = Local::now().date_naive();
    let folder = booster_folder_name(number, day);
    if let Err(error) = write_history(&job.history, number) {
        log::warn!("Could not save the booster count: {error}");
    }
    super::keep_numbered(set, folder.clone(), number);

    let staging = job
        .songs_root
        .join(format!("{STAGING_PREFIX}{number}.part"));
    stepmaniaonline::remove_temp_path(&staging)
        .map_err(|error| smo_songs::describe_song_error(&error))?;
    fs::create_dir(&staging).map_err(|error| {
        smo_songs::describe_song_error(&smo_songs::io("create the pack's staging folder", error))
    })?;

    let fill = Fill {
        agent: network::get_streaming_agent(),
        catalogue: Arc::clone(&stepmaniaonline::runtime_snapshot().catalog),
        fresh: Mutex::new(HashMap::new()),
        landing: Mutex::new(()),
        staging,
    };
    // Each card's bytes so far, held to what the card said the song weighs,
    // so the bar never runs past the pack's total; a card that is done counts
    // whole, however it went.
    let fetched: Vec<AtomicU64> = job.songs.iter().map(|_| AtomicU64::new(0)).collect();
    // Summed and told one at a time, so a later sum is never told first.
    let reporting = Mutex::new(());
    let report = || {
        let _reporting = lock(&reporting);
        let done = fetched
            .iter()
            .map(|bytes| bytes.load(Ordering::Relaxed))
            .sum();
        super::keep_bytes(set, done);
    };
    let landed = each_card(job.songs.len(), SONGS_AT_ONCE, |at| {
        let song = &job.songs[at];
        super::keep_song_state(set, at, SongState::Downloading);
        let temp = downloads.join(format!("{DOWNLOAD_PREFIX}{number}-{at}.part"));
        let scratch = scratch_dir(&fill.staging, at);
        let weight = song.bytes;
        let mut next_report = 0u64;
        let progress = |done: u64, total: u64| {
            let finished = total > 0 && done >= total;
            if done >= next_report || finished {
                next_report = done.saturating_add(smo_songs::PROGRESS_STEP_BYTES);
                fetched[at].store(done.min(weight), Ordering::Relaxed);
                report();
            }
            true
        };
        let held = job.held.get(at).map(Arc::as_ref);
        let outcome = fill.song(song, held, &temp, &scratch, progress);
        fill.clear_song(&temp, &scratch);
        fetched[at].store(weight, Ordering::Relaxed);
        report();
        match outcome {
            Ok(landed) => {
                log::info!(
                    "Kept '{}' from pack {} as '{}' in '{folder}'.",
                    song.title,
                    song.pack_id,
                    landed.folder
                );
                super::keep_song_state(set, at, SongState::Done);
                Some(landed)
            }
            Err(message) => {
                log::warn!(
                    "Could not keep '{}' from pack {}: {message}",
                    song.title,
                    song.pack_id
                );
                super::keep_song_state(set, at, SongState::Failed);
                None
            }
        }
    });
    // In card order, whichever came in first: the Pack.ini's lines, and the
    // cards that landed, for the banner.
    let mut kept = Vec::with_capacity(job.songs.len());
    let mut landed_cards = Vec::with_capacity(job.songs.len());
    for (song, landed) in job.songs.iter().zip(landed) {
        if let Some(landed) = landed {
            kept.push(landed);
            landed_cards.push(song);
        }
    }

    let staging = fill.staging;
    if kept.is_empty() {
        remove_staging(&staging);
        log::warn!("Kept nothing for '{folder}': {NONE_KEPT}.");
        return Err(NONE_KEPT.to_owned());
    }
    let banner = write_banner(
        &staging,
        &banner_url(number, day, &job.options, &landed_cards),
    );
    match commit_booster(&staging, &job.songs_root, &folder, &kept, banner) {
        Ok(group_dir) => {
            stepmaniaonline::queue_ready_song_dir(group_dir.clone(), &job.songs_root);
            log::info!(
                "Kept '{}': {} of {} songs.",
                group_dir.display(),
                kept.len(),
                job.songs.len()
            );
            Ok(group_dir)
        }
        Err(error) => {
            remove_staging(&staging);
            let message = smo_songs::describe_song_error(&error);
            log::warn!("Could not file '{folder}': {message}");
            Err(message)
        }
    }
}

fn remove_staging(staging: &Path) {
    if let Err(error) = stepmaniaonline::remove_temp_path(staging) {
        log::warn!("Could not remove a booster staging folder: {error}");
    }
}

/// Card `at`'s scratch folder in the pack's staging folder.
fn scratch_dir(staging: &Path, at: usize) -> PathBuf {
    staging.join(format!("{SCRATCH_PREFIX}{at}.part"))
}

/// `work` for every card from the first to `cards`, on `at_once` threads at
/// most: each takes the next card as it comes free, so the cards still start
/// in order. What each card came to, in card order. Should no thread start,
/// this one does every card itself.
fn each_card<T: Default + Send>(
    cards: usize,
    at_once: usize,
    work: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<T>> = Mutex::new((0..cards).map(|_| T::default()).collect());
    let run = || {
        loop {
            let at = next.fetch_add(1, Ordering::Relaxed);
            if at >= cards {
                break;
            }
            let done = work(at);
            lock(&out)[at] = done;
        }
    };
    thread::scope(|scope| {
        let mut started = 0;
        for n in 0..at_once.min(cards) {
            let spawned = thread::Builder::new()
                .name(format!("rip-keep-{n}"))
                .spawn_scoped(scope, run);
            match spawned {
                Ok(_) => started += 1,
                Err(error) => log::warn!("Could not start a booster download: {error}"),
            }
        }
        if started == 0 {
            run();
        }
    });
    out.into_inner().unwrap_or_else(PoisonError::into_inner)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ------------------------------------------------------------------ history

/// How many boosters the history says were kept; 0 when it says nothing.
pub(super) fn read_history(path: &Path) -> u32 {
    fs::read(path).map_or(0, |bytes| parse_history(&String::from_utf8_lossy(&bytes)))
}

/// `count N`, as ITGmania's `boosters.txt` has it. The largest count wins, and
/// any other line -- the per-song lines it once kept among them -- is passed
/// over.
fn parse_history(text: &str) -> u32 {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches('\u{feff}').trim_start();
            let rest = line.get(.."count".len())?;
            if !rest.eq_ignore_ascii_case("count") {
                return None;
            }
            let digits = line["count".len()..].trim_start();
            let end = digits
                .find(|ch: char| !ch.is_ascii_digit())
                .unwrap_or(digits.len());
            digits[..end].parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0)
}

/// The history, replaced whole: written beside itself, then renamed over, so
/// a crash mid-write leaves the old count rather than none.
fn write_history(path: &Path, count: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    replace_file(path, format!("count {count}\n").as_bytes())
}

/// A file replaced whole: written beside itself, then renamed over.
fn replace_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let written = File::create(&temp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// The next booster's number: one past the history's count and past every
/// booster folder in Songs, any case. A pack deleted in game leaves its
/// folder, so its number is not handed out again either.
fn next_number(history: u32, songs_root: &Path) -> u32 {
    let on_disk = fs::read_dir(songs_root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| super::booster_number(&entry.file_name().to_string_lossy()))
        .max()
        .unwrap_or(0);
    history.max(on_disk).saturating_add(1)
}

fn booster_folder_name(number: u32, day: NaiveDate) -> String {
    format!(
        "{}{number} - {}",
        super::BOOSTER_PREFIX,
        day.format("%Y-%m-%d")
    )
}

/// What an earlier run was doing when it was closed: a song's download and
/// a half-filled pack. Each keep sweeps before it starts, as the song worker
/// does; nothing else in either folder is touched.
fn sweep_leftovers(songs_root: &Path, downloads: &Path) {
    let sweep = |dir: &Path, prefix: &str| {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix)
                && name.ends_with(".part")
                && let Err(error) = stepmaniaonline::remove_temp_path(&entry.path())
            {
                log::warn!("Could not remove an old booster download: {error}");
            }
        }
    };
    sweep(downloads, DOWNLOAD_PREFIX);
    sweep(songs_root, STAGING_PREFIX);
}

// ------------------------------------------------------------------- series

/// Boosters kept before boosters named their series are given it, so every
/// booster pack sits in the one folder: the `Series` line goes in after
/// their `Version`, nothing else changes. Before each keep; the song scanner
/// shows it once it next reads the pack.
fn group_boosters(songs_root: &Path) {
    let Ok(entries) = fs::read_dir(songs_root) else {
        return;
    };
    for entry in entries.flatten() {
        if !super::is_booster_folder(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path().join("Pack.ini");
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if bytes.len() > MAX_PACK_INI_BYTES {
            continue;
        }
        let Some(grouped) = with_series(&String::from_utf8_lossy(&bytes)) else {
            continue;
        };
        if let Err(error) = replace_file(&path, grouped.as_bytes()) {
            log::warn!("Could not add the series to '{}': {error}", path.display());
        }
    }
}

/// `text` with [`SERIES`] added after its `Version` line, in the file's own
/// line endings. `None` when it names a series already, or has no `[Group]`
/// or `Version` -- without which neither game reads it at all.
fn with_series(text: &str) -> Option<String> {
    if !text.contains("[Group]") || text.lines().any(|line| is_key(line, "Series")) {
        return None;
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + SERIES.len() + 16);
    let mut added = false;
    for line in text.split_inclusive('\n') {
        out.push_str(line);
        if !added && is_key(line, "Version") {
            if !line.ends_with('\n') {
                out.push_str(eol);
            }
            let _ = write!(out, "Series={SERIES}{eol}");
            added = true;
        }
    }
    added.then_some(out)
}

/// Whether an ini line sets `key`, case aside, as the song scanner reads it.
fn is_key(line: &str, key: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(key))
}

// --------------------------------------------------------------------- sync

/// What a pack's `Pack.ini` says its sync is, read loosely, as the relay read
/// it: the first `SyncOffset=` anywhere in it -- `Sync Offset` and
/// `sync_offset` too, any case -- NULL when its value is, ITG for any other.
/// `None` when it says nothing, or is too large to be a `Pack.ini`.
fn pack_ini_sync(bytes: &[u8]) -> Option<DeclaredSync> {
    if bytes.len() > MAX_PACK_INI_BYTES {
        return None;
    }
    let value = sync_offset_value(bytes)?;
    Some(if value.eq_ignore_ascii_case(b"null") {
        DeclaredSync::Null
    } else {
        DeclaredSync::Itg
    })
}

/// The relay's `/sync[ _]?offset\s*=\s*([A-Za-z0-9_.+-]+)/i` over the file's
/// bytes (it read them as Latin-1): the leftmost match's value.
fn sync_offset_value(bytes: &[u8]) -> Option<&[u8]> {
    (0..bytes.len()).find_map(|start| sync_offset_at(&bytes[start..]))
}

fn sync_offset_at(bytes: &[u8]) -> Option<&[u8]> {
    let rest = strip_word(bytes, b"sync")?;
    let rest = rest
        .strip_prefix(b" ")
        .or_else(|| rest.strip_prefix(b"_"))
        .unwrap_or(rest);
    let rest = skip_space(strip_word(rest, b"offset")?).strip_prefix(b"=")?;
    let rest = skip_space(rest);
    let len = rest
        .iter()
        .take_while(|&&byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'+' | b'-')
        })
        .count();
    (len > 0).then(|| &rest[..len])
}

fn strip_word<'a>(bytes: &'a [u8], word: &[u8]) -> Option<&'a [u8]> {
    let head = bytes.get(..word.len())?;
    head.eq_ignore_ascii_case(word)
        .then(|| &bytes[word.len()..])
}

/// JavaScript's `\s` as it falls on Latin-1: ASCII whitespace with the
/// vertical tab, and the no-break space.
fn skip_space(bytes: &[u8]) -> &[u8] {
    let len = bytes
        .iter()
        .take_while(|&&byte| matches!(byte, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ' | 0xa0))
        .count();
    &bytes[len..]
}

/// What the catalogue's sync column says of a pack.
fn catalogue_sync(catalogue: &[PackInfo], pack_id: u64) -> Option<DeclaredSync> {
    catalogue
        .iter()
        .find(|pack| pack.id == pack_id)
        .and_then(super::declared_sync)
}

/// The sync a song's pack declared: its own `Pack.ini` first, the catalogue
/// when that says nothing. `None` is neither saying -- taken as ITG.
fn source_sync(pack_ini: Option<&[u8]>, catalogue: Option<DeclaredSync>) -> Option<DeclaredSync> {
    pack_ini.and_then(pack_ini_sync).or(catalogue)
}

// -------------------------------------------------------------------- songs

/// A song in the pack, as its provenance line tells it.
#[derive(Debug, PartialEq)]
struct KeptSong {
    /// Its folder in the booster.
    folder: String,
    /// The pack it came from, as the catalogue names it.
    pack: String,
    /// What that pack declared; `None` is nothing, taken as ITG.
    sync: Option<DeclaredSync>,
    /// How far its offsets were moved, when any simfile was.
    moved: Option<f32>,
}

/// What every song of one keep is fetched with, by every thread fetching.
struct Fill {
    agent: network::HttpAgent,
    catalogue: Arc<[PackInfo]>,
    /// Indexes fetched again during this keep because their pack changed on
    /// the site: another card from the same pack reads the new one rather than
    /// paying for it again. Held while one is fetched, so two cards from one
    /// changed pack never both pay for it.
    fresh: Mutex<HashMap<u64, Arc<PackCache>>>,
    /// Held while a song is moved into the pack, so two never take one name.
    landing: Mutex<()>,
    staging: PathBuf,
}

impl Fill {
    /// One card's song, fetched into `scratch`, null-synced and moved into
    /// the pack.
    fn song(
        &self,
        song: &RipSong,
        held: Option<&HeldSong>,
        temp: &Path,
        scratch: &Path,
        progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<KeptSong, String> {
        let held = held.ok_or("the rip lost track of where the song is")?;
        let (written, pack_ini) = self.fetch(held, temp, scratch, progress)?;
        let sync = source_sync(
            pack_ini.as_deref(),
            catalogue_sync(&self.catalogue, song.pack_id),
        );
        let _landing = lock(&self.landing);
        land_song(
            &scratch.join(&written),
            &written,
            &self.staging,
            &song.pack_name,
            sync,
        )
    }

    /// The song's folder into the scratch folder, `.oldsync` backups left
    /// out and its pack's `Pack.ini` read in the same request, then the
    /// banner it borrows from its pack, if its simfile takes one from there
    /// (`pack_archive::fetch_borrowed_banner`). Returns the folder's written
    /// name and the `Pack.ini`, if it could be read.
    ///
    /// A pack replaced on the site since its index was read has its index
    /// fetched again, once -- a counted request -- and the song looked for in
    /// it by its folder's name.
    fn fetch(
        &self,
        held: &HeldSong,
        temp: &Path,
        scratch: &Path,
        mut progress: impl FnMut(u64, u64) -> bool,
    ) -> Result<(String, Option<Vec<u8>>), String> {
        let pack_id = held.cache.index.pack_id;
        let name = held
            .cache
            .index
            .folders
            .get(held.folder)
            .map(|folder| folder.name.clone())
            .ok_or("the song is not in its pack's index")?;
        let fresh = lock(&self.fresh).get(&pack_id).cloned();
        let (mut cache, mut folder, mut refetched) = match fresh {
            Some(cache) => {
                let folder = find_folder(&cache.index, &name)?;
                (cache, folder, true)
            }
            None => (Arc::clone(&held.cache), held.folder, false),
        };
        loop {
            self.clear_song(temp, scratch);
            let fetched = pack_archive::fetch_folder_skipping(
                &self.agent,
                &cache.index,
                folder,
                temp,
                scratch,
                smo_songs::MAX_SONG_BYTES,
                &pack_archive::is_resync_backup,
                pack_archive::pack_ini_entry(&cache.index),
                &mut progress,
            );
            match fetched {
                Err(ArchiveError::Changed) if !refetched => {
                    refetched = true;
                    cache = self.read_again(pack_id, &cache)?;
                    folder = find_folder(&cache.index, &name)?;
                }
                Ok((written, pack_ini)) => {
                    pack_archive::fetch_borrowed_banner(
                        &self.agent,
                        &cache.index,
                        folder,
                        &scratch.join(&written),
                    );
                    return Ok((written, pack_ini));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    /// The new index of a pack that changed on the site since `stale` was
    /// read: the one another card already fetched, else fetched now -- a
    /// counted request -- with the others waiting on it, so the pack is paid
    /// for once.
    fn read_again(&self, pack_id: u64, stale: &Arc<PackCache>) -> Result<Arc<PackCache>, String> {
        let mut fresh = lock(&self.fresh);
        if let Some(cache) = fresh.get(&pack_id)
            && !Arc::ptr_eq(cache, stale)
        {
            return Ok(Arc::clone(cache));
        }
        log::info!("Pack {pack_id} changed on StepManiaOnline; reading its index again.");
        let song_count = self
            .catalogue
            .iter()
            .find(|pack| pack.id == pack_id)
            .map_or(0, |pack| pack.song_count);
        let index = pack_archive::fetch_index(&self.agent, pack_id, song_count)
            .map_err(|error| error.to_string())?;
        let cache = smo_songs::adopt_index(index);
        fresh.insert(pack_id, Arc::clone(&cache));
        Ok(cache)
    }

    /// A song's leftovers: its download and whatever is in its scratch
    /// folder. Before every fetch, and after every song however it went.
    fn clear_song(&self, temp: &Path, scratch: &Path) {
        for path in [temp, scratch] {
            if let Err(error) = stepmaniaonline::remove_temp_path(path) {
                log::warn!("Could not remove a booster song's temporary files: {error}");
            }
        }
    }
}

/// The song's folder in a pack's new index: by its exact name, else by the
/// one name that differs from it only in case. Never a guess between two.
fn find_folder(index: &PackIndex, name: &str) -> Result<usize, String> {
    let folders = &index.folders;
    if let Some(at) = folders.iter().position(|folder| folder.name == name) {
        return Ok(at);
    }
    let mut alike = folders
        .iter()
        .enumerate()
        .filter(|(_, folder)| folder.name.eq_ignore_ascii_case(name));
    match (alike.next(), alike.next()) {
        (Some((at, _)), None) => Ok(at),
        _ => Err("the song is no longer in its pack".to_owned()),
    }
}

/// A fetched song into the pack: moved to NULL sync unless its pack already
/// was, then moved in under its own name. A song whose offsets cannot be
/// moved is not kept -- left as it is, it would play 9 ms off.
fn land_song(
    staged: &Path,
    written: &str,
    staging: &Path,
    pack: &str,
    sync: Option<DeclaredSync>,
) -> Result<KeptSong, String> {
    let moved = if sync == Some(DeclaredSync::Null) {
        None
    } else {
        let rewritten = smo_songs::shift_song_folder(staged, ITG_SYNC_OFFSET_SECONDS)
            .map_err(|error| format!("could not move it from ITG to NULL sync: {error}"))?;
        (rewritten > 0).then_some(ITG_SYNC_OFFSET_SECONDS)
    };
    let folder = free_name(staging, &smo_songs::song_folder_name(written))
        .map_err(|error| smo_songs::describe_song_error(&error))?;
    fs::rename(staged, staging.join(&folder)).map_err(|error| {
        smo_songs::describe_song_error(&smo_songs::io("move the song into its pack", error))
    })?;
    Ok(KeptSong {
        folder,
        pack: pack.to_owned(),
        sync,
        moved,
    })
}

/// `base`, or the first of `base (2)`, `base (3)`... that nothing in `dir`
/// has, case aside: two packs' `Vertex` are two songs, never one merged
/// folder. `Pack.ini` and the banner count as taken, so no song stands where
/// the pack's own files go.
fn free_name(dir: &Path, base: &str) -> Result<String, StepManiaOnlineError> {
    let mut taken = vec!["Pack.ini".to_owned(), BANNER_FILE.to_owned()];
    let entries =
        fs::read_dir(dir).map_err(|error| smo_songs::io("look inside the pack", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| smo_songs::io("look inside the pack", error))?;
        taken.push(entry.file_name().to_string_lossy().into_owned());
    }
    let free = |name: &str| !taken.iter().any(|have| have.eq_ignore_ascii_case(name));
    if free(base) {
        return Ok(base.to_owned());
    }
    let mut copy = 1u32;
    loop {
        copy += 1;
        let suffix = format!(" ({copy})");
        let keep = stepmaniaonline::DESTINATION_MAX_CHARS.saturating_sub(suffix.len());
        let mut name: String = base.chars().take(keep).collect();
        name.truncate(name.trim_end_matches([' ', '.']).len());
        name.push_str(&suffix);
        if free(&name) {
            return Ok(name);
        }
    }
}

// --------------------------------------------------------------------- pack

/// The booster's `Pack.ini`: NULL, its banner when it has one, and where
/// every song came from, in card order. The songs come from packs that
/// disagree, so the folder alone cannot tell anyone what was done to them.
fn pack_ini_text(kept: &[KeptSong], banner: bool) -> String {
    let mut text = String::from(PACK_INI_HEAD);
    if banner {
        let _ = writeln!(text, "Banner={BANNER_FILE}");
    }
    text.push_str("#\n");
    for song in kept {
        let pack = if song.pack.trim().is_empty() {
            "?"
        } else {
            song.pack.as_str()
        };
        let synced = song.sync.map_or("(not stated)", DeclaredSync::label);
        let what = match song.moved {
            Some(delta) => format!("offsets moved {delta:+.3} s to NULL"),
            None => "offsets as they were".to_owned(),
        };
        let _ = writeln!(
            text,
            "# {} -- from {pack}, synced {synced}; {what}",
            song.folder
        );
    }
    text
}

/// File the filled pack: its `Pack.ini` into the staging folder, naming the
/// banner when `banner` says it is there, then the staging folder into Songs
/// under the pack's name -- `(2)` and on when that is somehow taken -- in one
/// rename. Returns the pack's folder.
///
/// A `Pack.ini` that cannot be written is logged rather than refused, as the
/// singles group's is: the songs are null-synced either way, and only a
/// machine set to apply `Pack.ini` offsets with an ITG default would notice.
fn commit_booster(
    staging: &Path,
    songs_root: &Path,
    folder: &str,
    kept: &[KeptSong],
    banner: bool,
) -> Result<PathBuf, StepManiaOnlineError> {
    if let Err(error) = fs::write(staging.join("Pack.ini"), pack_ini_text(kept, banner)) {
        log::warn!("Could not write the booster's Pack.ini: {error}");
    }
    let name = free_name(songs_root, folder)?;
    let group_dir = songs_root.join(name);
    fs::rename(staging, &group_dir)
        .map_err(|error| smo_songs::io("move the pack into the Songs folder", error))?;
    Ok(group_dir)
}

// ------------------------------------------------------------------- banner

/// The banner's address: everything the picture shows, each part held to
/// what the site takes -- the pack's number and day, its eras and Expert
/// range, and for each card that landed its picture's name (`-` for none)
/// and its meter.
fn banner_url(number: u32, day: NaiveDate, options: &RipOptions, landed: &[&RipSong]) -> String {
    let eras: Vec<&str> = options.limiting_eras().into_iter().map(era_key).collect();
    let art: Vec<&str> = landed
        .iter()
        .map(|song| song.art_url.as_deref().and_then(art_name).unwrap_or("-"))
        .collect();
    let meters: Vec<String> = landed.iter().map(|song| song.meter.to_string()).collect();
    format!(
        "{BANNER_URL}?n={number}&d={}&e={}&lo={}&hi={}&a={}&m={}",
        day.format("%Y-%m-%d"),
        eras.join(","),
        options.lo,
        options.hi,
        art.join(","),
        meters.join(","),
    )
}

/// The name of SMO's picture of a song -- 40 hex digits and `.png` -- from
/// its address; `None` for any other.
fn art_name(url: &str) -> Option<&str> {
    let name = url.strip_prefix(SMO_SONG_ART)?;
    let hash = name.strip_suffix(".png")?;
    let hex = hash.len() == ART_HASH_LEN
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    hex.then_some(name)
}

/// The pack's banner into the staging folder, from `url`. Whether it is
/// there: anything that goes wrong costs the pack its banner and nothing
/// else.
fn write_banner(staging: &Path, url: &str) -> bool {
    let written = fetch_banner(url).and_then(|jpeg| {
        fs::write(staging.join(BANNER_FILE), jpeg).map_err(|error| error.to_string())
    });
    if let Err(error) = &written {
        log::warn!("Could not get the booster's banner: {error}");
    }
    written.is_ok()
}

fn fetch_banner(url: &str) -> Result<Box<[u8]>, String> {
    let agent = network::build_agent_with_connect(
        network::AgentConfig {
            timeout: BANNER_TIMEOUT,
        },
        BANNER_CONNECT_TIMEOUT,
    );
    let response = agent
        .get(url)
        .call()
        .map_err(|error| network::error_from_ureq(error).to_string())?;
    let jpeg = network::read_bytes_body_bounded(response, MAX_BANNER_BYTES)
        .map_err(|error| error.to_string())?;
    if !jpeg.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err("the answer was not a JPEG".to_owned());
    }
    Ok(jpeg)
}

#[cfg(test)]
mod tests {
    use super::super::Era;
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    /// A fresh folder per test, removed when the guard drops, so a failing
    /// assertion does not leave fixtures behind.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "deadsync-rip-keep-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create fixture root");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("a real day")
    }

    /// A fetched song in the scratch folder: one simfile.
    fn fetched(scratch: &Path, name: &str, simfile: &[u8]) -> PathBuf {
        let folder = scratch.join(name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("song.ssc"), simfile).unwrap();
        folder
    }

    fn kept(folder: &str, pack: &str, sync: Option<DeclaredSync>, moved: Option<f32>) -> KeptSong {
        KeptSong {
            folder: folder.to_owned(),
            pack: pack.to_owned(),
            sync,
            moved,
        }
    }

    #[test]
    fn the_history_is_its_largest_count() {
        assert_eq!(parse_history("count 12\n"), 12);
        assert_eq!(
            parse_history("count 3\r\nsong Pack/Song\r\ncount 9\r\ncount 4\r\n"),
            9,
            "the old per-song lines are passed over"
        );
        assert_eq!(parse_history("\u{feff}COUNT  7 kept\n"), 7);
        assert_eq!(parse_history(""), 0);
        assert_eq!(parse_history("count\ncount -2\ncount x\nrecount 5\n"), 0);
        assert_eq!(parse_history("count 99999999999999\n"), 0, "past u32");
    }

    #[test]
    fn the_history_round_trips_and_is_replaced_whole() {
        let root = TempDir::new("history");
        let history = root.0.join("save").join("content-boosters.txt");
        assert_eq!(read_history(&history), 0, "none yet");
        write_history(&history, 7).expect("first write makes the folder");
        assert_eq!(fs::read_to_string(&history).unwrap(), "count 7\n");
        assert_eq!(read_history(&history), 7);
        write_history(&history, 8).expect("replaces");
        assert_eq!(fs::read_to_string(&history).unwrap(), "count 8\n");
        let names: Vec<String> = fs::read_dir(root.0.join("save"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["content-boosters.txt"],
            "no temporary file left"
        );

        fs::write(&history, b"\xff\xfe garbled").unwrap();
        assert_eq!(read_history(&history), 0);
    }

    #[test]
    fn a_pack_is_numbered_past_the_history_and_every_booster_folder() {
        let root = TempDir::new("number");
        for name in [
            "Content Booster Pack 3 - 2026-01-01",
            "content booster pack 11 - 2026-02-02",
            "Content Booster Pack x",
            "Other Pack 40",
            "._deadsync-booster-30.part",
        ] {
            fs::create_dir_all(root.0.join(name)).unwrap();
        }
        assert_eq!(next_number(5, &root.0), 12, "a folder past the history");
        assert_eq!(
            next_number(20, &root.0),
            21,
            "the history past every folder"
        );
        assert_eq!(next_number(0, &root.0.join("missing")), 1);

        let folder = booster_folder_name(12, day(2026, 10, 7));
        assert_eq!(folder, "Content Booster Pack 12 - 2026-10-07");
        assert_eq!(super::super::booster_number(&folder), Some(12));
    }

    #[test]
    fn a_closed_runs_leftovers_are_swept() {
        let root = TempDir::new("sweep");
        let songs = root.0.join("Songs");
        let downloads = root.0.join("cache").join("downloads");
        fs::create_dir_all(songs.join("._deadsync-booster-4.part").join("Song")).unwrap();
        fs::create_dir_all(songs.join("._deadsync-song-2.part")).unwrap();
        fs::create_dir_all(songs.join("Content Booster Pack 3 - 2026-01-01")).unwrap();
        fs::create_dir_all(&downloads).unwrap();
        fs::write(downloads.join(".deadsync-booster-4-0.part"), b"half").unwrap();
        fs::write(downloads.join(".deadsync-song-2.part"), b"not ours").unwrap();
        sweep_leftovers(&songs, &downloads);
        assert!(!songs.join("._deadsync-booster-4.part").exists());
        assert!(!downloads.join(".deadsync-booster-4-0.part").exists());
        assert!(
            songs.join("._deadsync-song-2.part").exists(),
            "the song worker's"
        );
        assert!(songs.join("Content Booster Pack 3 - 2026-01-01").exists());
        assert!(downloads.join(".deadsync-song-2.part").exists());
        // folders that are not there yet are nothing to sweep
        sweep_leftovers(&root.0.join("missing"), &root.0.join("gone"));
    }

    #[test]
    fn a_pack_ini_is_read_as_loosely_as_the_relay_read_it() {
        let null = Some(DeclaredSync::Null);
        let itg = Some(DeclaredSync::Itg);
        assert_eq!(
            pack_ini_sync(b"[Group]\r\nVersion=1\r\nSyncOffset=NULL\r\n"),
            null
        );
        assert_eq!(pack_ini_sync(b"sync_offset = itg"), itg);
        assert_eq!(pack_ini_sync(b"Sync Offset=null;"), null);
        assert_eq!(
            pack_ini_sync(b"SYNCOFFSET\t=\t0.009"),
            itg,
            "any other value is ITG"
        );
        assert_eq!(
            pack_ini_sync(b"\xa0SyncOffset=\xa0NULL"),
            null,
            "Latin-1 spacing"
        );
        assert_eq!(
            pack_ini_sync(b"# SyncOffset=NULL\nSyncOffset=ITG\n"),
            null,
            "the first one, comment or not"
        );
        assert_eq!(
            pack_ini_sync(b"syncsyncOffset=ITG"),
            itg,
            "found past a false start"
        );
        assert_eq!(pack_ini_sync(b"[Group]\nVersion=1\n"), None);
        assert_eq!(pack_ini_sync(b"SyncOffset="), None, "no value");
        assert_eq!(pack_ini_sync(b"Sync-Offset=NULL"), None);
        let mut huge = b"SyncOffset=NULL\n".to_vec();
        huge.resize(MAX_PACK_INI_BYTES + 1, b' ');
        assert_eq!(pack_ini_sync(&huge), None, "too large to be one");
    }

    #[test]
    fn a_songs_sync_is_its_pack_inis_then_the_catalogues() {
        let catalogue = [
            PackInfo::new(
                1,
                "Null Pack".to_owned(),
                1,
                1,
                Some("null".to_owned()),
                None,
                None,
                None,
            ),
            PackInfo::new(
                2,
                "ITG Pack".to_owned(),
                1,
                1,
                Some("9ms".to_owned()),
                None,
                None,
                None,
            ),
            PackInfo::new(
                3,
                "Quiet Pack".to_owned(),
                1,
                1,
                Some("n/a".to_owned()),
                None,
                None,
                None,
            ),
        ];
        assert_eq!(catalogue_sync(&catalogue, 1), Some(DeclaredSync::Null));
        assert_eq!(catalogue_sync(&catalogue, 2), Some(DeclaredSync::Itg));
        assert_eq!(catalogue_sync(&catalogue, 3), None);
        assert_eq!(catalogue_sync(&catalogue, 4), None, "not in the catalogue");

        let catalogue_says = Some(DeclaredSync::Null);
        assert_eq!(
            source_sync(Some(b"SyncOffset=ITG"), catalogue_says),
            Some(DeclaredSync::Itg),
            "the pack's own word first"
        );
        assert_eq!(
            source_sync(Some(b"[Group]\nVersion=1\n"), catalogue_says),
            catalogue_says,
            "a Pack.ini that says nothing"
        );
        assert_eq!(source_sync(None, catalogue_says), catalogue_says);
        assert_eq!(source_sync(None, None), None);
    }

    #[test]
    fn the_pack_ini_says_null_and_where_every_song_came_from() {
        let text = pack_ini_text(
            &[
                kept(
                    "Vertex",
                    "Pack A",
                    Some(DeclaredSync::Itg),
                    Some(ITG_SYNC_OFFSET_SECONDS),
                ),
                kept("Vertex (2)", "Pack B", Some(DeclaredSync::Null), None),
                kept("Quiet", " ", None, Some(ITG_SYNC_OFFSET_SECONDS)),
                kept("Nested", "Pack D", Some(DeclaredSync::Itg), None),
            ],
            false,
        );
        assert_eq!(
            text,
            concat!(
                "[Group]\n",
                "# Written by DeadSync's Find Content.\n",
                "# A booster pack: songs from all over stepmaniaonline.net, every one\n",
                "# null-synced. A song whose own pack declared ITG -- or declared nothing,\n",
                "# and was taken as ITG, as most packs are -- had its offsets moved 9 ms\n",
                "# to NULL, so the pack plays right whatever the machine's settings.\n",
                "Version=1\n",
                "SyncOffset=NULL\n",
                "Series=Content Browser Booster Packs\n",
                "#\n",
                "# Vertex -- from Pack A, synced ITG; offsets moved -0.009 s to NULL\n",
                "# Vertex (2) -- from Pack B, synced NULL; offsets as they were\n",
                "# Quiet -- from ?, synced (not stated); offsets moved -0.009 s to NULL\n",
                "# Nested -- from Pack D, synced ITG; offsets as they were\n",
            )
        );
    }

    #[test]
    fn a_booster_names_the_series_every_booster_shares() {
        assert!(PACK_INI_HEAD.contains(&format!("\nSeries={SERIES}\n")));
        let text = pack_ini_text(&[], false);
        assert_eq!(with_series(&text), None, "named already");
    }

    #[test]
    fn an_older_booster_is_given_the_series_after_its_version() {
        assert_eq!(
            with_series("[Group]\n# ours\nVersion=1\nSyncOffset=ITG\n#\n# Song -- from Pack\n"),
            Some(format!(
                "[Group]\n# ours\nVersion=1\nSeries={SERIES}\nSyncOffset=ITG\n#\n# Song -- from Pack\n"
            ))
        );
        assert_eq!(
            with_series("[Group]\r\nVersion=1\r\nSyncOffset=NULL\r\n"),
            Some(format!(
                "[Group]\r\nVersion=1\r\nSeries={SERIES}\r\nSyncOffset=NULL\r\n"
            )),
            "in the file's own line endings"
        );
        assert_eq!(
            with_series("[Group]\nversion = 1"),
            Some(format!("[Group]\nversion = 1\nSeries={SERIES}\n")),
            "a last line without its newline"
        );
        assert_eq!(with_series("[Group]\nVersion=1\nseries = Mine\n"), None);
        assert_eq!(with_series("[Group]\nSyncOffset=ITG\n"), None, "no Version");
        assert_eq!(with_series("Version=1\n"), None, "no [Group]");
    }

    #[test]
    fn boosters_on_disk_are_grouped_and_nothing_else_is() {
        let root = TempDir::new("series");
        let songs = root.0.join("Songs");
        let older = songs.join("Content Booster Pack 3 - 2026-10-17");
        let named = songs.join("Content Booster Pack 4 - 2026-10-18");
        let other = songs.join("Some Pack");
        for dir in [&older, &named, &other] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(
            older.join("Pack.ini"),
            "[Group]\nVersion=1\nSyncOffset=NULL\n",
        )
        .unwrap();
        let mine = format!("[Group]\nVersion=1\nSeries={SERIES}\nSyncOffset=NULL\n");
        fs::write(named.join("Pack.ini"), &mine).unwrap();
        fs::write(other.join("Pack.ini"), "[Group]\nVersion=1\n").unwrap();

        group_boosters(&songs);
        assert_eq!(
            fs::read_to_string(older.join("Pack.ini")).unwrap(),
            format!("[Group]\nVersion=1\nSeries={SERIES}\nSyncOffset=NULL\n")
        );
        assert_eq!(fs::read_to_string(named.join("Pack.ini")).unwrap(), mine);
        assert_eq!(
            fs::read_to_string(other.join("Pack.ini")).unwrap(),
            "[Group]\nVersion=1\n",
            "not a booster"
        );
        assert!(
            !older.join("Pack.ini.tmp").exists(),
            "no temporary file left"
        );
        group_boosters(&root.0.join("missing"));
    }

    #[test]
    fn a_pack_with_a_banner_names_it() {
        let text = pack_ini_text(&[kept("Song", "Pack", None, None)], true);
        assert_eq!(
            text.strip_prefix(PACK_INI_HEAD),
            Some(concat!(
                "Banner=banner.jpg\n",
                "#\n",
                "# Song -- from Pack, synced (not stated); offsets as they were\n",
            ))
        );
    }

    fn card(meter: u32, art_url: Option<&str>) -> RipSong {
        RipSong {
            key: "1:2".to_owned(),
            pack_id: 1,
            pack_name: "Pack".to_owned(),
            song_id: 2,
            title: "Song".to_owned(),
            artist: String::new(),
            credit: String::new(),
            meter,
            year: None,
            bpm: super::super::CardBpm::default(),
            art_url: art_url.map(str::to_owned),
            folder: "Song".to_owned(),
            bytes: 1,
        }
    }

    #[test]
    fn the_banner_is_asked_for_by_what_the_pack_is() {
        let hash = "f4c1e4e2a8d98179cf74dfc164e09de1655af8a8";
        let art = format!("{SMO_SONG_ART}{hash}.png");
        let cards = [
            card(12, Some(&art)),
            card(9, None),
            card(
                14,
                Some("https://stepmaniaonline.net/media/images/songs/12.png"),
            ),
        ];
        let landed: Vec<&RipSong> = cards.iter().collect();
        let two_eras = RipOptions {
            eras: vec![Era::Vintage, Era::Modern],
            lo: 9,
            hi: 14,
            ..RipOptions::default()
        };
        assert_eq!(
            banner_url(7, day(2026, 10, 19), &two_eras, &landed),
            format!(
                "https://itgcontent.net/api/boosterbanner?n=7&d=2026-10-19&e=modern,vintage\
                 &lo=9&hi=14&a={hash}.png,-,-&m=12,9,14"
            ),
            "the eras in the picker's order; a card without SMO's own picture has none"
        );
        let every_era = RipOptions {
            eras: Era::ALL.to_vec(),
            ..RipOptions::default()
        };
        assert_eq!(
            banner_url(12, day(2026, 1, 2), &every_era, &landed[..1]),
            format!(
                "https://itgcontent.net/api/boosterbanner?n=12&d=2026-01-02&e=\
                 &lo=8&hi=14&a={hash}.png&m=12"
            ),
            "every era is any era"
        );
    }

    #[test]
    fn only_smos_own_song_pictures_are_named() {
        let hash = "369b07d2e5031d92474e30909215890c6302a2da";
        let name = format!("{hash}.png");
        assert_eq!(
            art_name(&format!("{SMO_SONG_ART}{name}")),
            Some(name.as_str())
        );
        for url in [
            format!("{SMO_SONG_ART}{}.png", hash.to_uppercase()),
            format!("{SMO_SONG_ART}{hash}.jpg"),
            format!("{SMO_SONG_ART}{hash}0.png"),
            format!("{SMO_SONG_ART}../{hash}.png"),
            format!("https://example.test/media/images/songs/{name}"),
            format!("{SMO_SONG_ART}{name}?x=1"),
        ] {
            assert_eq!(art_name(&url), None, "{url}");
        }
    }

    /// Every card is done once, three at a time at most, and what each came
    /// to is answered in card order whatever order they finished in.
    #[test]
    fn every_card_is_done_once_a_few_at_a_time_and_answered_in_order() {
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        // the first three wait for one another, so three surely run at once
        let first = std::sync::Barrier::new(3);
        let started = Mutex::new(Vec::new());
        let out = each_card(10, 3, |at| {
            lock(&started).push(at);
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            if at < 3 {
                first.wait();
            }
            // later cards finish first
            thread::sleep(Duration::from_millis(5 * (10 - at as u64)));
            running.fetch_sub(1, Ordering::SeqCst);
            at * 10
        });
        assert_eq!(out, (0..10).map(|at| at * 10).collect::<Vec<_>>());
        let mut started = started.into_inner().unwrap();
        started.sort_unstable();
        assert_eq!(started, (0..10).collect::<Vec<_>>(), "each once");
        assert_eq!(most.load(Ordering::SeqCst), 3, "three at once, never more");

        assert!(each_card(0, 3, |at| at).is_empty());
        assert_eq!(each_card(2, 3, |at| at + 1), vec![1, 2]);
        assert_eq!(each_card(4, 1, |at| at), vec![0, 1, 2, 3], "one at a time");
    }

    /// An ITG song's offsets move as it lands and a NULL one's do not; songs
    /// that share a folder name, case aside, land side by side.
    #[test]
    fn songs_that_share_a_folder_name_land_side_by_side() {
        let root = TempDir::new("land");
        let staging = root.0.join("._deadsync-booster-1.part");
        let scratch = scratch_dir(&staging, 0);

        let staged = fetched(&scratch, "Vertex", b"#OFFSET:0.100;");
        let first = land_song(
            &staged,
            "Vertex",
            &staging,
            "Pack A",
            Some(DeclaredSync::Itg),
        )
        .expect("first");
        assert_eq!(
            first,
            kept(
                "Vertex",
                "Pack A",
                Some(DeclaredSync::Itg),
                Some(ITG_SYNC_OFFSET_SECONDS)
            )
        );
        assert_eq!(
            fs::read(staging.join("Vertex/song.ssc")).unwrap(),
            b"#OFFSET:0.091;"
        );
        assert!(!staged.exists(), "moved, not copied");

        let staged = fetched(&scratch, "VERTEX", b"#OFFSET:0.100;");
        let second = land_song(
            &staged,
            "VERTEX",
            &staging,
            "Pack B",
            Some(DeclaredSync::Null),
        )
        .expect("second");
        assert_eq!(
            second,
            kept("VERTEX (2)", "Pack B", Some(DeclaredSync::Null), None)
        );
        assert_eq!(
            fs::read(staging.join("VERTEX (2)/song.ssc")).unwrap(),
            b"#OFFSET:0.100;",
            "a NULL song is left as it is"
        );

        let staged = fetched(&scratch, "vertex", b"#TITLE:Vertex;");
        let third = land_song(&staged, "vertex", &staging, "Pack C", None).expect("third");
        assert_eq!(third.folder, "vertex (3)");
        assert_eq!(
            third.moved,
            Some(ITG_SYNC_OFFSET_SECONDS),
            "not stated is ITG"
        );

        let staged = fetched(&scratch, "pack.ini", b"#OFFSET:0;");
        let odd = land_song(
            &staged,
            "pack.ini",
            &staging,
            "Pack E",
            Some(DeclaredSync::Null),
        )
        .expect("a song called pack.ini");
        assert_eq!(
            odd.folder, "pack.ini (2)",
            "never where the pack's own file goes"
        );

        let staged = fetched(&scratch, "Banner.jpg", b"#OFFSET:0;");
        let odder = land_song(
            &staged,
            "Banner.jpg",
            &staging,
            "Pack F",
            Some(DeclaredSync::Null),
        )
        .expect("a song called banner.jpg");
        assert_eq!(odder.folder, "Banner.jpg (2)", "nor where its banner goes");
    }

    #[test]
    fn a_song_whose_offsets_cannot_move_is_not_kept() {
        let root = TempDir::new("badshift");
        let staging = root.0.join("._deadsync-booster-1.part");
        let scratch = scratch_dir(&staging, 0);
        let staged = fetched(&scratch, "Broken", b"#OFFSET:-0.1\n#BPMS:0=120;");
        let error =
            land_song(&staged, "Broken", &staging, "Pack", Some(DeclaredSync::Itg)).unwrap_err();
        assert!(
            error.starts_with("could not move it from ITG to NULL sync"),
            "{error}"
        );
        assert!(!staging.join("Broken").exists());
    }

    #[test]
    fn a_long_name_keeps_room_for_its_number() {
        let root = TempDir::new("longname");
        let base = "x".repeat(stepmaniaonline::DESTINATION_MAX_CHARS);
        fs::create_dir_all(root.0.join(&base)).unwrap();
        let name = free_name(&root.0, &base).unwrap();
        assert_eq!(name.chars().count(), stepmaniaonline::DESTINATION_MAX_CHARS);
        assert!(name.ends_with("x (2)"), "{name}");
    }

    /// The filled pack lands in Songs in one rename, with its Pack.ini, and
    /// never on top of a folder already there.
    #[test]
    fn a_filled_pack_lands_in_songs_in_one_rename() {
        let root = TempDir::new("commit");
        let songs = root.0.join("Songs");
        let folder = "Content Booster Pack 2 - 2026-10-17";
        let taken = songs.join("content booster pack 2 - 2026-10-17");
        fs::create_dir_all(&taken).unwrap();
        fs::write(taken.join("keep.txt"), b"the player's").unwrap();

        let staging = songs.join("._deadsync-booster-2.part");
        let scratch = scratch_dir(&staging, 0);
        let staged = fetched(&scratch, "Song", b"#OFFSET:0.000;");
        let song = land_song(&staged, "Song", &staging, "Pack", Some(DeclaredSync::Itg)).unwrap();
        fs::remove_dir_all(&scratch).unwrap();
        let landed = [song];

        let group_dir = commit_booster(&staging, &songs, folder, &landed, true).expect("commit");
        assert_eq!(group_dir, songs.join(format!("{folder} (2)")));
        assert!(!staging.exists(), "the staging folder became the pack");
        assert_eq!(
            fs::read_to_string(group_dir.join("Pack.ini")).unwrap(),
            pack_ini_text(&landed, true)
        );
        assert_eq!(
            fs::read(group_dir.join("Song/song.ssc")).unwrap(),
            b"#OFFSET:-0.009;"
        );
        assert_eq!(fs::read(taken.join("keep.txt")).unwrap(), b"the player's");
        assert_eq!(
            super::super::booster_number(&format!("{folder} (2)")),
            Some(2)
        );
    }
}
