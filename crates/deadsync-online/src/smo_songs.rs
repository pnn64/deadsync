//! Single songs out of stepmaniaonline.net pack zips: a song's charts and
//! audio for the chart preview, and one song's folder for a single-song
//! install. Both are read straight out of the pack's zip with HTTP ranges, so
//! neither costs the whole pack, and nothing stands between the game and the
//! site.
//!
//! The pack's index is the zip's own central directory, read from the end of
//! the file in one request and kept for the session. The song's folder is
//! found by name or, failing that, by the titles its simfiles declare -- read
//! for the whole pack in one batched request, and remembered with their text,
//! so the next song from the same pack costs nothing to find. Only then are
//! the song's own bytes asked for. `pack_archive` does the reading and
//! `song_preview` turns a simfile into the window's notes.
//!
//! Every request to a pack's download address counts as a download on the
//! site, HEAD included. So nothing here is fetched except for an explicit
//! press, the index is fetched once per pack, ranges are batched, two threads
//! after the same pack wait for each other rather than both asking, and a
//! preview that has been stopped or replaced sends nothing more.
//!
//! Two services live here, sharing one runtime:
//!
//! * **Preview** -- find the song, publish its charts, then bring its audio
//!   into the cache. The charts are published first, so the note window can be
//!   drawn while the audio is still arriving. Starting another preview
//!   replaces this one outright: a generation token makes whatever the old
//!   thread finds out afterwards land nowhere, and the old thread notices
//!   between chunks and stops downloading rather than finishing audio nobody
//!   will play.
//! * **Single-song install** -- one worker with its own short queue, separate
//!   from the pack downloads so a song is never stuck behind a gigabyte. The
//!   song's folder is held to exactly the rules a pack archive is, then filed
//!   into a singles group named for the sync it was authored with, so a group's
//!   `Pack.ini` offset is right for every song in it.
//!
//! Runtime/cache contract:
//!
//! - Owner: the preview threads and the song worker; the UI only snapshots.
//! - Thread safety: one mutex guards immutable `Arc` snapshots. The preview's
//!   generation is an atomic so a download can check it per chunk without the
//!   lock, and is only ever bumped while the lock is held, so a check made
//!   under the lock cannot race a restart. The pack indexes have their own
//!   lock, never held across a request. Each pack has two more, held across
//!   a request on purpose -- one while its index is fetched, and its read
//!   simfiles' while simfiles are fetched -- and only worker threads take
//!   them.
//! - Capacity: one preview at a time and one preview file on disk; four pack
//!   indexes; charts for 256 songs remembered; 64 song install records; 16
//!   queued songs.
//! - Worst frame cost: one mutex acquisition plus an `Arc` clone. Nothing here
//!   touches the network or the disk on the caller's thread.

use crate::pack_archive::{self, ArchiveError, PackIndex, SimfileTags, SongMatch};
use crate::song_preview;
use crate::stepmaniaonline::{self, StepManiaOnlineError};
use deadsync_chart::song::ITG_SYNC_OFFSET_SECONDS;
use deadsync_net as network;
use deadsync_simfile::sync_offset;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Weak};
use std::thread;

/// One simfile's compressed text. A bound, not a size: most are a few
/// kilobytes.
const MAX_SIMFILE_BYTES: u64 = 4 * 1024 * 1024;
/// Every simfile in a pack, read to find a song by its title. A big pack's
/// are a few hundred kilobytes all told.
const MAX_TITLE_SCAN_BYTES: u64 = 32 * 1024 * 1024;
/// The whole song's audio. A long song at a high bitrate is a few tens of
/// megabytes; past this it is not a preview any more.
const MAX_AUDIO_BYTES: u64 = 64 * 1024 * 1024;
/// One song's folder, video and all.
const MAX_SONG_BYTES: u64 = 512 * 1024 * 1024;
/// How far a download gets between progress publishes. Each publish clones a
/// snapshot under the lock, so per-chunk would be churn the bar cannot show.
const PROGRESS_STEP_BYTES: u64 = 256 * 1024;
/// Pack indexes kept. One is a few hundred kilobytes for a big pack.
const MAX_PACK_INDEXES: usize = 4;
/// Simfile text kept per pack once it has been read, so a second song from
/// the same pack needs no request for its chart. Past this only titles stay.
const MAX_CACHED_SIMFILE_BYTES: usize = 16 * 1024 * 1024;
/// Under the app cache dir. Holds one preview's audio at a time.
const PREVIEW_DIR: &str = "content-preview";
/// Every file this module writes into `PREVIEW_DIR` starts with this, so the
/// sweep deletes only what it made.
const PREVIEW_FILE_PREFIX: &str = "preview-";
/// Songs whose charts are remembered for the popup. A song's charts are a few
/// kilobytes; this keeps a long browse from growing without limit.
const MAX_KNOWN_SONGS: usize = 256;
const MAX_SONG_INSTALLS: usize = 64;
const SONG_QUEUE_CAPACITY: usize = 16;
/// A song job's download, under the cache's downloads folder.
const SONG_DOWNLOAD_PREFIX: &str = ".deadsync-song-";
/// A song job's staging folder, under Songs. "._" because the song scanner
/// skips that prefix: a half-extracted song is never found by a rescan that
/// happens to run meanwhile.
const SONG_STAGING_PREFIX: &str = "._deadsync-song-";
/// What the game can play, by extension.
const AUDIO_EXTENSIONS: [&str; 6] = ["ogg", "mp3", "wav", "flac", "opus", "oga"];

/// Where every single song lands. All of it null-synced: a song from a pack
/// taken to be ITG-synced has its offsets moved to NULL as it installs, so it
/// plays right whatever the machine's Pack.ini settings, and one group is right
/// for every song in it.
pub const SINGLES_GROUP: &str = "Content Browser Singles - NULL Sync";

/// What the singles group says about itself. Written once, when the group is
/// first made, and never rewritten: a player who edits it keeps their edit.
/// It declares NULL so a machine that defaults to ITG leaves these songs be.
const SINGLES_PACK_INI: &str = concat!(
    "[Group]\n",
    "# Written by DeadSync's Find Content.\n",
    "# Songs downloaded one at a time land here, all null-synced: one from\n",
    "# an ITG-synced pack has its offsets moved 9 ms as it installs.\n",
    "Version=1\n",
    "SyncOffset=NULL\n",
);

// --------------------------------------------------------------- public types

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

/// One row of a chart's note window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewNote {
    /// Seconds from the start of the sample window.
    pub time: f32,
    /// Columns with something in them, lowest bit leftmost: taps, hold and
    /// roll heads, lifts and mines alike.
    pub cols: u8,
    /// Which of `cols` are lifts.
    pub lifts: u8,
    /// Which of `cols` are mines.
    pub mines: u8,
    /// 0 = 4th, 1 = 8th, 2 = 12th, 3 = 16th, 4 = 24th, 5 = 32nd, 6 = 48th,
    /// 7 = 64th, 8 = anything finer.
    pub quant: u8,
}

/// One difficulty of the song, as the window draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewChart {
    pub doubles: bool,
    /// As the simfile names it: `Challenge`, `Hard`, `Edit`...
    pub difficulty: String,
    pub meter: u32,
    /// 4 for singles, 8 for doubles.
    pub lanes: u8,
    /// Time-ordered.
    pub notes: Arc<[PreviewNote]>,
}

#[derive(Clone, Debug, Default)]
pub struct PreviewSnapshot {
    pub phase: PreviewPhase,
    pub pack_id: u64,
    pub title: String,
    /// As the preview was asked for with.
    pub artist: String,
    /// 0..=1 while the audio arrives, once its size is known.
    pub progress: Option<f32>,
    /// The sample window, seconds into the audio file; clip = the file IS a
    /// #PREVIEW clip, play it from 0.
    ///
    /// The window is still the one the simfile declared when `clip` is set:
    /// it is the stretch such a clip is normally cut from, so it is where the
    /// notes come from.
    pub start: f32,
    pub length: f32,
    pub bpm: f32,
    pub clip: bool,
    /// Easiest first, singles before doubles.
    pub charts: Arc<[PreviewChart]>,
    /// Set once the audio is on disk (phase Ready).
    pub audio_path: Option<PathBuf>,
    pub message: Option<String>,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongInstallPhase {
    Queued,
    Downloading,
    Extracting,
    Installed,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongInstall {
    pub pack_id: u64,
    pub title: String,
    /// As it was asked for: empty unless the pack's page lists the title more
    /// than once. Part of the song's identity, with the pack and the title.
    pub artist: String,
    /// The singles group it lands in, which is where to look for it after.
    pub group: String,
    pub phase: SongInstallPhase,
    pub downloaded_bytes: u64,
    /// The song's size in the pack, once known.
    pub total_bytes: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SongInstallsSnapshot {
    /// Oldest first.
    pub installs: Arc<[SongInstall]>,
    pub revision: u64,
}

// ------------------------------------------------------------------ runtime

#[derive(Default)]
struct RuntimeState {
    preview: Arc<PreviewSnapshot>,
    known: KnownCharts,
    /// The working copy; `installs_snapshot` is what was last published.
    installs: Vec<SongInstall>,
    installs_snapshot: Arc<SongInstallsSnapshot>,
    /// Names each song job's temporary files. Starts again every launch,
    /// which is why a job clears its paths before it uses them.
    next_song_seq: u64,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));
/// The preview attempt that is wanted. Bumped only under `RUNTIME`'s lock.
static PREVIEW_GENERATION: AtomicU64 = AtomicU64::new(0);
static SONG_QUEUE: LazyLock<Result<SyncSender<SongJob>, String>> = LazyLock::new(start_song_worker);

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

/// A song, by title and the artist it was asked for with, and every chart a
/// preview of it turned up.
struct KnownSong {
    title: Arc<str>,
    artist: Box<str>,
    charts: Arc<[PreviewChart]>,
}

impl KnownSong {
    fn is(&self, title: &str, artist: &str) -> bool {
        self.title.as_ref() == title && self.artist.as_ref() == artist
    }
}

/// The original's `Snd.known`: every difficulty a preview has turned up,
/// filed under the pack as well as the title. Pack names are unique and song
/// titles are not -- "Vertex" is in a dozen packs, and they are not the same
/// song. The artist is part of the key too, for the pack that has two.
///
/// Keyed by pack, then searched by title, so a lookup borrows the caller's
/// title rather than building a key for it.
#[derive(Default)]
struct KnownCharts {
    by_pack: HashMap<u64, Vec<KnownSong>>,
    /// Insertion order, for evicting the oldest song once full.
    order: VecDeque<(u64, Arc<str>)>,
}

impl KnownCharts {
    fn get(&self, pack_id: u64, title: &str, artist: &str) -> Option<Arc<[PreviewChart]>> {
        self.by_pack
            .get(&pack_id)?
            .iter()
            .find(|song| song.is(title, artist))
            .map(|song| Arc::clone(&song.charts))
    }

    fn insert(&mut self, pack_id: u64, title: &str, artist: &str, charts: Arc<[PreviewChart]>) {
        let songs = self.by_pack.entry(pack_id).or_default();
        if let Some(song) = songs.iter_mut().find(|song| song.is(title, artist)) {
            song.charts = charts;
            return;
        }
        let title: Arc<str> = Arc::from(title);
        songs.push(KnownSong {
            title: Arc::clone(&title),
            artist: Box::from(artist),
            charts,
        });
        self.order.push_back((pack_id, title));
        while self.order.len() > MAX_KNOWN_SONGS {
            let Some((pack_id, title)) = self.order.pop_front() else {
                break;
            };
            if let Some(songs) = self.by_pack.get_mut(&pack_id) {
                songs.retain(|song| !Arc::ptr_eq(&song.title, &title));
                if songs.is_empty() {
                    self.by_pack.remove(&pack_id);
                }
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.order.len()
    }
}

// ------------------------------------------------------------------- packs

/// A pack's index, and what has been read out of it so far.
struct PackCache {
    index: PackIndex,
    songs: Mutex<PackSongs>,
}

/// Per song folder: the titles its simfile declares, and the simfile's text
/// while it fits the budget.
#[derive(Default)]
struct PackSongs {
    tags: HashMap<usize, SimfileTags>,
    simfiles: HashMap<usize, Arc<[u8]>>,
    simfile_bytes: usize,
}

impl PackSongs {
    fn keep(&mut self, folder: usize, bytes: Arc<[u8]>) {
        if self.simfiles.contains_key(&folder)
            || self.simfile_bytes + bytes.len() > MAX_CACHED_SIMFILE_BYTES
        {
            return;
        }
        self.simfile_bytes += bytes.len();
        self.simfiles.insert(folder, bytes);
    }
}

/// Most recently used last.
static PACKS: LazyLock<Mutex<VecDeque<Arc<PackCache>>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

fn lock_packs() -> MutexGuard<'static, VecDeque<Arc<PackCache>>> {
    PACKS.lock().unwrap_or_else(|error| error.into_inner())
}

fn lock_songs(cache: &PackCache) -> MutexGuard<'_, PackSongs> {
    cache
        .songs
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Why a song could not be had, before it is put in the reader's words.
#[derive(Debug)]
enum Failure {
    Archive(ArchiveError),
    /// Already in the reader's words.
    Reader(String),
    /// The preview it belonged to has been replaced; say nothing.
    Stale,
}

impl From<ArchiveError> for Failure {
    fn from(error: ArchiveError) -> Self {
        Self::Archive(error)
    }
}

impl Failure {
    fn into_message(self) -> String {
        match self {
            Self::Archive(error) => error.to_string(),
            Self::Reader(message) => message,
            Self::Stale => String::new(),
        }
    }
}

/// One lock per pack whose index is being fetched, so a second thread after
/// the same pack waits for the first one's answer instead of asking the site
/// again. Held across the request; nothing else waits on it.
static INDEX_FETCHES: LazyLock<Mutex<HashMap<u64, Weak<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn index_fetch_lock(pack_id: u64) -> Arc<Mutex<()>> {
    let mut fetches = INDEX_FETCHES
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    fetches.retain(|_, fetch| fetch.strong_count() > 0);
    if let Some(fetch) = fetches.get(&pack_id).and_then(Weak::upgrade) {
        return fetch;
    }
    let fetch = Arc::new(Mutex::new(()));
    fetches.insert(pack_id, Arc::downgrade(&fetch));
    fetch
}

fn cached_pack(pack_id: u64) -> Option<Arc<PackCache>> {
    let mut packs = lock_packs();
    let at = packs
        .iter()
        .position(|cache| cache.index.pack_id == pack_id)?;
    let hit = packs.remove(at)?;
    packs.push_back(Arc::clone(&hit));
    Some(hit)
}

/// The pack's index, from the cache or the site. Asked for only while
/// `wanted` says the answer still matters.
fn pack(
    agent: &network::HttpAgent,
    pack_id: u64,
    wanted: &dyn Fn() -> bool,
) -> Result<Arc<PackCache>, Failure> {
    if let Some(hit) = cached_pack(pack_id) {
        return Ok(hit);
    }
    let fetch = index_fetch_lock(pack_id);
    let _fetching = fetch.lock().unwrap_or_else(|error| error.into_inner());
    // Fetched by whoever held the lock first.
    if let Some(hit) = cached_pack(pack_id) {
        return Ok(hit);
    }
    if !wanted() {
        return Err(Failure::Stale);
    }
    // The catalogue's song count sizes the first read so the whole index
    // usually arrives in one request.
    let song_count = stepmaniaonline::runtime_snapshot()
        .catalog
        .iter()
        .find(|pack| pack.id == pack_id)
        .map_or(0, |pack| pack.song_count);
    let index = pack_archive::fetch_index(agent, pack_id, song_count)?;
    let cache = Arc::new(PackCache {
        index,
        songs: Mutex::new(PackSongs::default()),
    });
    let mut packs = lock_packs();
    packs.retain(|known| known.index.pack_id != pack_id);
    packs.push_back(Arc::clone(&cache));
    while packs.len() > MAX_PACK_INDEXES {
        packs.pop_front();
    }
    Ok(cache)
}

/// Run `work` against the pack's index -- and once more against a fresh one
/// when the pack turns out to have been replaced on the site meanwhile.
/// `wanted` is asked before every request this makes or `work` hands it to;
/// once it says no, nothing more goes out and the answer is
/// [`Failure::Stale`].
fn with_pack<T>(
    pack_id: u64,
    wanted: &dyn Fn() -> bool,
    mut work: impl FnMut(&network::HttpAgent, &PackCache) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let agent = network::get_streaming_agent();
    let mut fresh = false;
    loop {
        let cache = pack(&agent, pack_id, wanted)?;
        match work(&agent, &cache) {
            Err(Failure::Archive(ArchiveError::Changed)) if !fresh => {
                // Only the index that failed: another thread may already
                // have fetched the new one.
                lock_packs().retain(|known| !Arc::ptr_eq(known, &cache));
                fresh = true;
            }
            result => return result,
        }
    }
}

/// A song found in its pack: its folder, and its simfile when finding it meant
/// reading it.
type Resolved = (usize, Option<Arc<[u8]>>);

/// The folder of the song with this title, and its simfile when finding it
/// meant reading it.
///
/// A folder named for the title settles it for free. Otherwise the simfiles
/// that could decide it -- less those already read -- are fetched in one
/// batched request (a very large pack's in a few), and the titles they declare
/// decide it: ITL's folders are named nothing like its page titles. The
/// simfiles are kept while they fit, so the pack's next song costs nothing to
/// find. Never a guess between equals.
///
/// `artist` is only for a title the pack's page lists more than once; the
/// matching then reads every simfile so the artist can tell them apart.
///
/// The pack's read simfiles stay locked through the request, so a second
/// thread after the same pack waits and then finds them read.
fn resolve_song(
    agent: &network::HttpAgent,
    cache: &PackCache,
    title: &str,
    artist: &str,
    wanted: &dyn Fn() -> bool,
) -> Result<Resolved, Failure> {
    let index = &cache.index;
    let mut songs = lock_songs(cache);
    let unread: Vec<(usize, usize)> = pack_archive::folders_needing_tags(index, title, artist)
        .into_iter()
        .filter(|folder| !songs.tags.contains_key(folder))
        .filter_map(|folder| Some((folder, index.folders[folder].simfile?)))
        .collect();
    let mut segments = Vec::new();
    if !unread.is_empty() {
        if !wanted() {
            return Err(Failure::Stale);
        }
        let entries: Vec<usize> = unread.iter().map(|&(_, entry)| entry).collect();
        let ranges: Vec<(u64, u64)> = entries
            .iter()
            .map(|&entry| index.entries[entry].span())
            .collect();
        segments = pack_archive::fetch_ranges(agent, index, &ranges, MAX_TITLE_SCAN_BYTES)?;
        let mut position = 0;
        pack_archive::read_entries(
            index,
            &segments,
            &entries,
            MAX_SIMFILE_BYTES,
            |entry, bytes| {
                let folder = unread[position].0;
                position += 1;
                match bytes {
                    Ok(bytes) => {
                        songs
                            .tags
                            .insert(folder, pack_archive::simfile_tags(&bytes));
                        songs.keep(folder, Arc::from(bytes));
                    }
                    Err(error) => {
                        // Damaged, not missing: remembered as titled nothing,
                        // so it is not fetched again.
                        log::debug!(
                            "Could not read {} in pack {}: {error}",
                            index.entries[entry].name,
                            index.pack_id
                        );
                        songs.tags.insert(folder, SimfileTags::default());
                    }
                }
            },
        )?;
    }

    let found = pack_archive::match_song(index, title, artist, |folder| {
        songs.tags.get(&folder).cloned()
    });
    let SongMatch::Folder(folder) = found else {
        return Err(no_match(&found, title));
    };
    // Read just now but past the cache's budget: inflated again from the
    // bytes still in hand rather than asked for a second time.
    let simfile = match songs.simfiles.get(&folder) {
        Some(bytes) => Some(Arc::clone(bytes)),
        None => unread
            .iter()
            .find(|&&(read, _)| read == folder)
            .and_then(|&(_, entry)| {
                pack_archive::read_entry(index, &segments, entry, MAX_SIMFILE_BYTES).ok()
            })
            .map(Arc::from),
    };
    Ok((folder, simfile))
}

fn no_match(found: &SongMatch, title: &str) -> Failure {
    Failure::Reader(match found {
        SongMatch::Ambiguous => format!("more than one song in this pack is called \"{title}\""),
        _ => format!("no song called \"{title}\" in this pack"),
    })
}

/// The song's simfile text and its extension: `read` when finding the song
/// already read it, else from the cache or the site.
fn simfile_for(
    agent: &network::HttpAgent,
    cache: &PackCache,
    folder: usize,
    read: Option<Arc<[u8]>>,
    wanted: &dyn Fn() -> bool,
) -> Result<(Arc<[u8]>, String), Failure> {
    let index = &cache.index;
    let Some(entry) = index.folders[folder].simfile else {
        return Err(Failure::Reader("this song has no simfile".to_owned()));
    };
    let extension = extension_of(&index.entries[entry].name).to_ascii_lowercase();
    if let Some(bytes) = read {
        return Ok((bytes, extension));
    }
    // Locked through the request, as the title scan is.
    let mut songs = lock_songs(cache);
    if let Some(bytes) = songs.simfiles.get(&folder) {
        return Ok((Arc::clone(bytes), extension));
    }
    if !wanted() {
        return Err(Failure::Stale);
    }
    let segments = pack_archive::fetch_ranges(
        agent,
        index,
        &[index.entries[entry].span()],
        MAX_SIMFILE_BYTES,
    )?;
    let bytes: Arc<[u8]> = Arc::from(pack_archive::read_entry(
        index,
        &segments,
        entry,
        MAX_SIMFILE_BYTES,
    )?);
    songs
        .tags
        .entry(folder)
        .or_insert_with(|| pack_archive::simfile_tags(&bytes));
    songs.keep(folder, Arc::clone(&bytes));
    Ok((bytes, extension))
}

/// The extension without its dot, preserving case.
fn extension_of(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, extension)| extension)
}

fn is_audio(name: &str) -> bool {
    let extension = extension_of(name);
    AUDIO_EXTENSIONS
        .iter()
        .any(|audio| extension.eq_ignore_ascii_case(audio))
}

/// An entry's path inside its song folder: past the pack's root and the
/// song's own folder.
fn within_song(name: &str) -> &str {
    name.splitn(3, '/').nth(2).unwrap_or_default()
}

/// The audio to play, and whether it is a `#PREVIEW` clip. The clip when the
/// simfile names one that is in the folder; else `#MUSIC`; else the folder's
/// audio, for a simfile whose `#MUSIC` names a file that is not there.
fn audio_entry(
    index: &PackIndex,
    folder: usize,
    data: &song_preview::SongPreviewData,
) -> Option<(usize, bool)> {
    let song = &index.folders[folder];
    let named = |wanted: &str| -> Option<usize> {
        let wanted = wanted.trim().replace('\\', "/");
        if wanted.is_empty() {
            return None;
        }
        let wanted_file = wanted.rsplit('/').next().unwrap_or_default().to_lowercase();
        song.entries.iter().copied().find(|&entry| {
            let name = &index.entries[entry].name;
            if !is_audio(name) {
                return false;
            }
            let inside = within_song(name);
            inside.eq_ignore_ascii_case(&wanted)
                || inside
                    .rsplit('/')
                    .next()
                    .is_some_and(|file| file.to_lowercase() == wanted_file)
        })
    };
    if let Some(entry) = named(&data.preview_clip) {
        return Some((entry, true));
    }
    if let Some(entry) = named(&data.music) {
        return Some((entry, false));
    }
    song.audio.first().map(|&entry| (entry, false))
}

/// The simfile's charts, in the window's own types.
fn preview_charts(data: &song_preview::SongPreviewData) -> Arc<[PreviewChart]> {
    data.charts
        .iter()
        .map(|chart| PreviewChart {
            doubles: chart.doubles,
            difficulty: chart.difficulty.clone(),
            meter: chart.meter,
            lanes: chart.lanes,
            notes: chart
                .notes
                .iter()
                .map(|note| PreviewNote {
                    time: note.time,
                    cols: note.cols,
                    lifts: note.lifts,
                    mines: note.mines,
                    quant: note.quant,
                })
                .collect(),
        })
        .collect()
}

// ------------------------------------------------------------------ preview

#[must_use]
pub fn runtime_preview_snapshot() -> Arc<PreviewSnapshot> {
    Arc::clone(&lock_runtime().preview)
}

/// Start (or restart) a preview, cancelling any preview in flight: find the
/// song in its pack, publish its charts, then bring its audio into
/// cache_dir/content-preview/ -- any older preview files there are deleted
/// first, so at most one is kept. The phase stays Loading until the audio is on
/// disk. `artist` breaks a tie between songs of the same title.
///
/// Every call is a new attempt, so this is for a keypress rather than a
/// frame. The old file is swept by the new attempt's thread, never by the
/// caller's; a file the audio device still holds open on Windows survives the
/// sweep and goes with the next one.
pub fn runtime_preview_start(pack_id: u64, title: &str, artist: &str, cache_dir: &Path) {
    let generation = {
        let mut runtime = lock_runtime();
        let generation = PREVIEW_GENERATION
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let empty = title.is_empty();
        publish_preview(
            &mut runtime,
            PreviewSnapshot {
                phase: if empty {
                    PreviewPhase::Error
                } else {
                    PreviewPhase::Loading
                },
                pack_id,
                title: title.to_owned(),
                artist: artist.to_owned(),
                message: empty.then(|| "no song selected".to_owned()),
                ..PreviewSnapshot::default()
            },
        );
        if empty {
            return;
        }
        generation
    };

    let job = PreviewJob {
        generation,
        pack_id,
        title: title.to_owned(),
        artist: artist.to_owned(),
        dir: cache_dir.join(PREVIEW_DIR),
    };
    let spawn = thread::Builder::new()
        .name("content-preview".to_owned())
        .spawn(move || run_preview(&job));
    if let Err(error) = spawn {
        log::warn!("Could not start the content preview worker: {error}");
        fail_preview(generation, "could not start the preview".to_owned());
    }
}

/// Cancel or forget the preview; phase Idle. Whatever its thread finds out
/// afterwards is dropped, and its partial download deletes itself.
pub fn runtime_preview_stop() {
    let mut runtime = lock_runtime();
    if runtime.preview.phase == PreviewPhase::Idle {
        return;
    }
    PREVIEW_GENERATION.fetch_add(1, Ordering::AcqRel);
    publish_preview(&mut runtime, PreviewSnapshot::default());
}

/// Charts already seen this session for (pack, title, artist) -- the
/// original's Snd.known -- so a popup can label the difficulty before
/// playing. `artist` as the preview was asked for with.
///
/// Nothing before the first preview of the song: its simfile lives inside the
/// pack, and reading it is the fetch itself.
#[must_use]
pub fn runtime_known_charts(
    pack_id: u64,
    title: &str,
    artist: &str,
) -> Option<Arc<[PreviewChart]>> {
    lock_runtime().known.get(pack_id, title, artist)
}

fn publish_preview(runtime: &mut RuntimeState, mut snapshot: PreviewSnapshot) {
    snapshot.revision = runtime.preview.revision.wrapping_add(1);
    runtime.preview = Arc::new(snapshot);
}

#[inline]
fn preview_is_current(generation: u64) -> bool {
    PREVIEW_GENERATION.load(Ordering::Acquire) == generation
}

/// Change the published preview, if it still belongs to `generation`.
fn update_preview(generation: u64, update: impl FnOnce(&mut PreviewSnapshot)) -> bool {
    let mut runtime = lock_runtime();
    if !preview_is_current(generation) {
        return false;
    }
    let mut snapshot = (*runtime.preview).clone();
    update(&mut snapshot);
    publish_preview(&mut runtime, snapshot);
    true
}

fn fail_preview(generation: u64, message: String) {
    let published = update_preview(generation, |snapshot| {
        snapshot.phase = PreviewPhase::Error;
        snapshot.progress = None;
        snapshot.message = Some(message.clone());
    });
    if published {
        log::warn!("Content preview failed: {message}");
    } else {
        log::debug!("Discarding stale content preview generation {generation}: {message}");
    }
}

struct PreviewJob {
    generation: u64,
    pack_id: u64,
    title: String,
    artist: String,
    dir: PathBuf,
}

fn run_preview(job: &PreviewJob) {
    if !preview_is_current(job.generation) {
        return;
    }
    sweep_preview_dir(&job.dir);

    let wanted = || preview_is_current(job.generation);
    let result = with_pack(job.pack_id, &wanted, |agent, cache| {
        let (folder, read) = resolve_song(agent, cache, &job.title, &job.artist, &wanted)?;
        let (simfile, extension) = simfile_for(agent, cache, folder, read, &wanted)?;
        let data = song_preview::preview_from_simfile(&simfile, &extension).map_err(|error| {
            Failure::Reader(format!("could not read this song's chart: {error}"))
        })?;
        let charts = preview_charts(&data);
        let (audio, clip) = audio_entry(&cache.index, folder, &data)
            .ok_or_else(|| Failure::Reader("this song has no audio".to_owned()))?;

        // The charts are a fact about the song, remembered even when the
        // preview has been replaced -- the reading has already been paid for.
        // Published before the audio, so the window can be drawn meanwhile.
        let current = {
            let mut runtime = lock_runtime();
            if !charts.is_empty() {
                runtime.known.insert(
                    job.pack_id,
                    job.title.as_str(),
                    job.artist.as_str(),
                    Arc::clone(&charts),
                );
            }
            let current = preview_is_current(job.generation);
            if current {
                let mut snapshot = (*runtime.preview).clone();
                snapshot.start = data.sample_start;
                snapshot.length = data.sample_length;
                snapshot.bpm = data.bpm;
                snapshot.clip = clip;
                snapshot.charts = Arc::clone(&charts);
                publish_preview(&mut runtime, snapshot);
            }
            current
        };
        if !current {
            return Err(Failure::Stale);
        }
        fetch_audio(agent, &cache.index, audio, job)
    });

    match result {
        Ok(path) => {
            let published = update_preview(job.generation, |snapshot| {
                snapshot.phase = PreviewPhase::Ready;
                snapshot.progress = None;
                snapshot.audio_path = Some(path.clone());
            });
            if published {
                log::info!(
                    "Content preview ready for '{}' (pack {}).",
                    job.title,
                    job.pack_id
                );
            } else if let Err(error) = stepmaniaonline::remove_temp_path(&path) {
                // Replaced while the last bytes landed. Nothing will play it.
                log::debug!("Could not remove a replaced preview file: {error}");
            }
        }
        Err(Failure::Stale) => log::debug!(
            "Discarding stale content preview generation {}.",
            job.generation
        ),
        Err(failure) => fail_preview(job.generation, failure.into_message()),
    }
}

/// Bring one audio entry out of the pack and into the preview folder, under a
/// name never reused in this run: it cannot collide with a file the audio
/// device still holds open from an earlier preview. No `.part` is left behind.
fn fetch_audio(
    agent: &network::HttpAgent,
    index: &PackIndex,
    entry: usize,
    job: &PreviewJob,
) -> Result<PathBuf, Failure> {
    let extension = extension_of(&index.entries[entry].name).to_ascii_lowercase();
    if !AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Err(Failure::Reader(
            "the sample is in a format this game cannot play".to_owned(),
        ));
    }
    fs::create_dir_all(&job.dir)
        .map_err(|error| Failure::Reader(format!("could not make the preview folder: {error}")))?;
    let name = format!("{PREVIEW_FILE_PREFIX}{}.{extension}", job.generation);
    let final_path = job.dir.join(name.as_str());
    let part_path = job.dir.join(format!("{name}.part"));
    if let Err(error) = stepmaniaonline::remove_temp_path(&part_path) {
        log::debug!("Could not clear a leftover preview download: {error}");
    }

    let generation = job.generation;
    let mut next_report = 0u64;
    let fetched = pack_archive::fetch_entry_to_file(
        agent,
        index,
        entry,
        &part_path,
        MAX_AUDIO_BYTES,
        |done, total| {
            if !preview_is_current(generation) {
                return false;
            }
            if done >= next_report {
                next_report = done.saturating_add(PROGRESS_STEP_BYTES);
                if total > 0 {
                    let fraction = (done as f64 / total as f64).clamp(0.0, 1.0) as f32;
                    update_preview(generation, |snapshot| {
                        snapshot.progress = Some(fraction);
                    });
                }
            }
            true
        },
    );
    let result = fetched.map_err(Failure::from).and_then(|_| {
        fs::rename(&part_path, &final_path)
            .map_err(|error| Failure::Reader(format!("could not keep the preview audio: {error}")))
    });
    match result {
        Ok(()) => Ok(final_path),
        Err(failure) => {
            if let Err(error) = stepmaniaonline::remove_temp_path(&part_path) {
                log::warn!("Could not remove a partial preview download: {error}");
            }
            // Stopped by the callback because the preview was replaced.
            let cancelled = matches!(&failure, Failure::Archive(error) if error.is_cancelled());
            if cancelled || !preview_is_current(generation) {
                Err(Failure::Stale)
            } else {
                Err(failure)
            }
        }
    }
}

/// Delete the preview files earlier attempts left, so at most one is kept.
///
/// Best effort: a file still open elsewhere (the audio device, on Windows)
/// cannot be deleted yet, and is caught by the next sweep instead.
fn sweep_preview_dir(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_ours = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(PREVIEW_FILE_PREFIX));
        if !is_ours || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        if let Err(error) = fs::remove_file(entry.path()) {
            log::debug!(
                "Could not remove old preview file {:?}: {error}",
                entry.path()
            );
        }
    }
}

// -------------------------------------------------------- single-song install

#[must_use]
pub fn runtime_song_installs() -> Arc<SongInstallsSnapshot> {
    Arc::clone(&lock_runtime().installs_snapshot)
}

/// Queue one song, into [`SINGLES_GROUP`]. `artist` is empty unless the pack's
/// page lists the title more than once, and then tells those songs apart.
/// `itg_sync` says the pack is taken to be ITG-synced, and then the song's
/// offsets are moved to NULL as it installs.
/// Refuses (Err with a short reader-facing reason) a song already
/// queued/running/installed this session (an Error may be retried).
///
/// Cheap on the caller's thread: a lock, a record, and a channel send. The
/// first call also starts the worker.
pub fn runtime_queue_song(
    pack_id: u64,
    title: &str,
    artist: &str,
    itg_sync: bool,
    songs_root: &Path,
    cache_dir: &Path,
) -> Result<(), String> {
    if title.is_empty() {
        return Err("no song selected".to_owned());
    }
    if songs_root.as_os_str().is_empty() {
        return Err("there is no Songs folder to put it in".to_owned());
    }
    let sender = SONG_QUEUE.as_ref().map_err(Clone::clone)?.clone();
    let group = SINGLES_GROUP;
    let seq = {
        let mut runtime = lock_runtime();
        queue_song_record(&mut runtime, pack_id, title, artist, group)?;
        runtime.next_song_seq = runtime.next_song_seq.wrapping_add(1);
        runtime.next_song_seq
    };
    let job = SongJob {
        seq,
        pack_id,
        title: title.to_owned(),
        artist: artist.to_owned(),
        group,
        itg_sync,
        songs_root: songs_root.to_path_buf(),
        cache_dir: cache_dir.to_path_buf(),
    };
    let (job, message) = match sender.try_send(job) {
        Ok(()) => return Ok(()),
        Err(TrySendError::Full(job)) => (job, "the song queue is full"),
        Err(TrySendError::Disconnected(job)) => (job, "the song download worker stopped"),
    };
    set_song_error(&job, message.to_owned());
    Err(message.to_owned())
}

/// Add a queued record for a song, or say why not.
fn queue_song_record(
    runtime: &mut RuntimeState,
    pack_id: u64,
    title: &str,
    artist: &str,
    group: &str,
) -> Result<(), String> {
    let queued = SongInstall {
        pack_id,
        title: title.to_owned(),
        artist: artist.to_owned(),
        group: group.to_owned(),
        phase: SongInstallPhase::Queued,
        downloaded_bytes: 0,
        total_bytes: None,
        message: Some("waiting for its turn".to_owned()),
    };
    if let Some(existing) = runtime
        .installs
        .iter_mut()
        .find(|install| install.is(pack_id, title, artist))
    {
        match existing.phase {
            SongInstallPhase::Queued
            | SongInstallPhase::Downloading
            | SongInstallPhase::Extracting => {
                return Err("that song is already on its way".to_owned());
            }
            SongInstallPhase::Installed => {
                return Err("that song was already added this session".to_owned());
            }
            SongInstallPhase::Error => *existing = queued,
        }
    } else {
        if runtime.installs.len() >= MAX_SONG_INSTALLS {
            let terminal = runtime.installs.iter().position(|install| {
                matches!(
                    install.phase,
                    SongInstallPhase::Installed | SongInstallPhase::Error
                )
            });
            let Some(index) = terminal else {
                return Err("too many songs are on their way".to_owned());
            };
            let evicted = runtime.installs.remove(index);
            log::debug!(
                "Evicted content song install history for '{}' (pack {}).",
                evicted.title,
                evicted.pack_id
            );
        }
        runtime.installs.push(queued);
    }
    publish_installs(runtime);
    Ok(())
}

fn publish_installs(runtime: &mut RuntimeState) {
    runtime.installs_snapshot = Arc::new(SongInstallsSnapshot {
        installs: Arc::from(runtime.installs.as_slice()),
        revision: runtime.installs_snapshot.revision.wrapping_add(1),
    });
}

fn update_song(job: &SongJob, update: impl FnOnce(&mut SongInstall)) {
    let mut runtime = lock_runtime();
    let Some(install) = runtime
        .installs
        .iter_mut()
        .find(|install| install.is(job.pack_id, &job.title, &job.artist))
    else {
        return;
    };
    update(install);
    publish_installs(&mut runtime);
}

fn set_song_phase(job: &SongJob, phase: SongInstallPhase, message: &str) {
    update_song(job, |install| {
        install.phase = phase;
        install.message = Some(message.to_owned());
    });
}

fn set_song_error(job: &SongJob, message: String) {
    update_song(job, |install| {
        install.phase = SongInstallPhase::Error;
        install.message = Some(message);
    });
}

impl SongInstall {
    /// Whether this is the record of that song.
    #[must_use]
    pub fn is(&self, pack_id: u64, title: &str, artist: &str) -> bool {
        self.pack_id == pack_id && self.title == title && self.artist == artist
    }
}

struct SongJob {
    seq: u64,
    pack_id: u64,
    title: String,
    /// Empty unless the page lists the title more than once.
    artist: String,
    group: &'static str,
    itg_sync: bool,
    songs_root: PathBuf,
    cache_dir: PathBuf,
}

fn start_song_worker() -> Result<SyncSender<SongJob>, String> {
    let (sender, receiver) = sync_channel::<SongJob>(SONG_QUEUE_CAPACITY);
    thread::Builder::new()
        .name("content-songs".to_owned())
        .spawn(move || {
            let mut swept = false;
            while let Ok(job) = receiver.recv() {
                if !swept {
                    swept = true;
                    sweep_song_leftovers(&job);
                }
                run_song_job(&job);
            }
        })
        .map_err(|error| format!("could not start the song download worker: {error}"))?;
    Ok(sender)
}

/// What an earlier run was doing when it was closed: its download and its
/// half-extracted song, named by a sequence that starts again every launch, so
/// nothing else would ever find them. Swept on the worker's thread, before its
/// first job, since a staging folder can be large.
fn sweep_song_leftovers(job: &SongJob) {
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
                log::warn!("Could not remove an old song download: {error}");
            }
        }
    };
    sweep(&job.cache_dir.join("downloads"), SONG_DOWNLOAD_PREFIX);
    sweep(&job.songs_root, SONG_STAGING_PREFIX);
}

fn run_song_job(job: &SongJob) {
    set_song_phase(
        job,
        SongInstallPhase::Downloading,
        "finding the song in its pack",
    );
    match install_song(job) {
        Ok(group_dir) => {
            // The whole group, not the one song: that is the shape the
            // reload queue already speaks, and one rescan of the group covers
            // every song added to it since.
            stepmaniaonline::queue_ready_song_dir(group_dir, &job.songs_root);
            let message = if job.itg_sync {
                format!("added to {}, moved from ITG to NULL sync", job.group)
            } else {
                format!("added to {}", job.group)
            };
            update_song(job, |install| {
                install.phase = SongInstallPhase::Installed;
                install.message = Some(message);
            });
            log::info!(
                "Installed '{}' from pack {} into '{}'.",
                job.title,
                job.pack_id,
                job.group
            );
        }
        Err(message) => {
            log::warn!(
                "Could not install '{}' from pack {}: {message}",
                job.title,
                job.pack_id
            );
            set_song_error(job, message);
        }
    }
}

/// Find, fetch, check, extract and file one song. Returns its group's folder.
///
/// Only the song's own stretch of the pack is downloaded, and it is extracted
/// under the pack installer's rules. The download and the staging folder are
/// removed on every path out; on success the staged song has become the
/// song, so there is nothing of it left to remove.
fn install_song(job: &SongJob) -> Result<PathBuf, String> {
    fs::create_dir_all(&job.songs_root)
        .map_err(|error| describe_song_error(&io("create the Songs folder", error)))?;
    let downloads = job.cache_dir.join("downloads");
    fs::create_dir_all(&downloads)
        .map_err(|error| describe_song_error(&io("create the Downloads folder", error)))?;
    let download = downloads.join(format!("{SONG_DOWNLOAD_PREFIX}{}.part", job.seq));
    let staging = job
        .songs_root
        .join(format!("{SONG_STAGING_PREFIX}{}.part", job.seq));
    stepmaniaonline::remove_temp_path(&download).map_err(|error| describe_song_error(&error))?;
    stepmaniaonline::remove_temp_path(&staging).map_err(|error| describe_song_error(&error))?;

    let group_dir = job.songs_root.join(job.group);
    let result = with_pack(job.pack_id, &|| true, |agent, cache| {
        let (folder, _) = resolve_song(agent, cache, &job.title, &job.artist, &|| true)?;
        // Already in the library: said before the song is downloaded, not
        // after. `commit_song` checks again for one that lands meanwhile.
        let name = song_folder_name(&pack_archive::written_folder_name(
            &cache.index.folders[folder].name,
        )?);
        if matches!(
            stepmaniaonline::child_exists_case_insensitive(&group_dir, OsStr::new(name.as_str())),
            Ok(true)
        ) {
            return Err(Failure::Reader(describe_song_error(
                &StepManiaOnlineError::AlreadyInstalled(name),
            )));
        }
        // A retry after the pack changed starts its staging afresh.
        stepmaniaonline::remove_temp_path(&staging)
            .map_err(|error| Failure::Reader(describe_song_error(&error)))?;
        let mut next_report = 0u64;
        let name = pack_archive::fetch_folder(
            agent,
            &cache.index,
            folder,
            &download,
            &staging,
            MAX_SONG_BYTES,
            |done, total| {
                let finished = total > 0 && done >= total;
                if done >= next_report || finished {
                    next_report = done.saturating_add(PROGRESS_STEP_BYTES);
                    update_song(job, |install| {
                        install.downloaded_bytes = done;
                        install.total_bytes = (total > 0).then_some(total);
                        let (phase, message) = if finished {
                            (
                                SongInstallPhase::Extracting,
                                "checking and unpacking the song",
                            )
                        } else {
                            (SongInstallPhase::Downloading, "downloading the song")
                        };
                        install.phase = phase;
                        if install.message.as_deref() != Some(message) {
                            install.message = Some(message.to_owned());
                        }
                    });
                }
                true
            },
        )?;
        Ok(name)
    })
    .map_err(Failure::into_message)
    .and_then(|name| {
        let staged = staging.join(name.as_str());
        if job.itg_sync {
            set_song_phase(
                job,
                SongInstallPhase::Extracting,
                "moving it from ITG to NULL sync",
            );
            null_sync_song(&staged)
                .map_err(|error| format!("could not move it from ITG to NULL sync: {error}"))?;
        }
        commit_song(&staged, name.as_str(), &job.songs_root, job.group)
            .map_err(|error| describe_song_error(&error))
    });

    if let Err(error) = stepmaniaonline::remove_temp_path(&download) {
        log::warn!("Could not remove a song download: {error}");
    }
    if let Err(error) = stepmaniaonline::remove_temp_path(&staging) {
        log::warn!("Could not remove a song staging folder: {error}");
    }
    result
}

/// File an extracted song into its singles group. Returns the group's folder.
///
/// The group is made, with its `Pack.ini`, the first time a song lands in it.
/// A song already there -- under any case -- is refused rather than merged or
/// replaced: what is in the library stays exactly as it is.
fn commit_song(
    staged: &Path,
    raw_name: &str,
    songs_root: &Path,
    group: &str,
) -> Result<PathBuf, StepManiaOnlineError> {
    let folder = song_folder_name(raw_name);
    let group_dir = songs_root.join(group);
    fs::create_dir_all(&group_dir).map_err(|error| io("create the singles group", error))?;
    write_pack_ini_if_absent(&group_dir);
    if stepmaniaonline::child_exists_case_insensitive(&group_dir, OsStr::new(folder.as_str()))? {
        return Err(StepManiaOnlineError::AlreadyInstalled(folder));
    }
    fs::rename(staged, group_dir.join(folder.as_str()))
        .map_err(|error| io("move the song into its group", error))?;
    Ok(group_dir)
}

/// The singles group's `Pack.ini`, written only when the group has none.
///
/// A failure here is not the song's failure: the song plays without it, only
/// at the engine's default offset, so it is logged rather than refused. The
/// original did the same.
fn write_pack_ini_if_absent(group_dir: &Path) {
    match stepmaniaonline::child_exists_case_insensitive(group_dir, OsStr::new("Pack.ini")) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            log::warn!("Could not look for the singles Pack.ini: {error}");
            return;
        }
    }
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(group_dir.join("Pack.ini"))
        .and_then(|mut file| file.write_all(SINGLES_PACK_INI.as_bytes()));
    match written {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => log::warn!("Could not write the singles Pack.ini: {error}"),
    }
}

/// Move a staged song from ITG to NULL sync, exactly as far as the engine
/// moves a pack whose `Pack.ini` says `SyncOffset=ITG`: every `#OFFSET` in
/// every simfile directly in the folder, song and chart alike, shifted by
/// [`ITG_SYNC_OFFSET_SECONDS`] with DeadSync's own offset writer. Returns how
/// many simfiles it rewrote.
fn null_sync_song(folder: &Path) -> Result<usize, String> {
    let mut rewritten = 0;
    let entries = fs::read_dir(folder).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if !entry.file_type().is_ok_and(|kind| kind.is_file())
            || !is_chart_simfile(&name.to_string_lossy())
        {
            continue;
        }
        let path = entry.path();
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        let shifted = null_synced_simfile(&bytes)
            .map_err(|error| format!("{}: {error}", name.to_string_lossy()))?;
        fs::write(&path, shifted).map_err(|error| error.to_string())?;
        rewritten += 1;
    }
    Ok(rewritten)
}

/// The engine reads `.ssc` and `.sm`; a `.dwi` beside them is left as it is.
fn is_chart_simfile(name: &str) -> bool {
    let extension = extension_of(name);
    extension.eq_ignore_ascii_case("sm") || extension.eq_ignore_ascii_case("ssc")
}

/// One simfile's text, moved from ITG to NULL sync. A simfile with no
/// `#OFFSET` is at zero, so the writer gives it one.
fn null_synced_simfile(bytes: &[u8]) -> Result<Vec<u8>, String> {
    sync_offset::shift_simfile_offsets(bytes, ITG_SYNC_OFFSET_SECONDS).map(|(shifted, _)| shifted)
}

/// The archive's top-level folder name, made safe to be a folder here.
///
/// The pack installer never needed this -- it names a pack after the
/// catalogue and throws the archive's own root away -- so the archive checks
/// leave the root component alone. A song keeps its root, so it is cleaned
/// the same way a pack name is: Windows' forbidden characters become `_`,
/// trailing dots and spaces go, a reserved device name is prefixed, and a
/// name the scanner would skip (`._`) is not produced.
fn song_folder_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut name = String::with_capacity(trimmed.len());
    let mut chars = 0;
    for ch in trimmed.chars() {
        if chars == stepmaniaonline::DESTINATION_MAX_CHARS {
            break;
        }
        if stepmaniaonline::invalid_path_char(ch) {
            if !name.ends_with('_') {
                name.push('_');
                chars += 1;
            }
        } else {
            name.push(ch);
            chars += 1;
        }
    }
    name.truncate(name.trim_end_matches([' ', '.']).len());
    if name.is_empty() {
        return "Content Browser Song".to_owned();
    }
    let stem = name.split('.').next().unwrap_or_default();
    if name.starts_with("._")
        || stepmaniaonline::WINDOWS_RESERVED_NAMES
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        name.insert(0, '_');
    }
    name
}

fn io(action: &'static str, error: std::io::Error) -> StepManiaOnlineError {
    stepmaniaonline::io_error(action, error)
}

/// A song install's failure, in the reader's words.
fn describe_song_error(error: &StepManiaOnlineError) -> String {
    match error {
        StepManiaOnlineError::Network(network::NetworkError::HttpStatus(status)) => {
            format!("the song did not arrive (HTTP {status})")
        }
        StepManiaOnlineError::Network(error) => error.to_string(),
        StepManiaOnlineError::AlreadyInstalled(_) => "already in your library".to_owned(),
        StepManiaOnlineError::Archive(message) => {
            format!("the song's archive was refused: {message}")
        }
        StepManiaOnlineError::Io { action, message } => format!("could not {action}: {message}"),
        StepManiaOnlineError::Catalog(message) => message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    /// A fresh folder per test, removed when the guard drops, so a failing
    /// assertion does not leave fixtures behind.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "deadsync-smo-songs-{label}-{}-{id}",
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

    fn chart(doubles: bool, difficulty: &str, meter: u32) -> PreviewChart {
        PreviewChart {
            doubles,
            difficulty: difficulty.to_owned(),
            meter,
            lanes: if doubles { 8 } else { 4 },
            notes: Arc::from(Vec::<PreviewNote>::new()),
        }
    }

    fn entry(name: &str) -> pack_archive::ZipEntry {
        pack_archive::ZipEntry {
            name: name.to_owned(),
            compressed: 0,
            uncompressed: 0,
            local_header: 0,
            end: 0,
        }
    }

    /// A pack with one song folder holding these files.
    fn one_song_pack(files: &[&str]) -> PackIndex {
        let entries: Vec<pack_archive::ZipEntry> = files
            .iter()
            .map(|file| entry(&format!("Pack/Song/{file}")))
            .collect();
        let audio = (0..entries.len())
            .filter(|&index| is_audio(&entries[index].name))
            .collect();
        let simfile = (0..entries.len()).find(|&index| entries[index].name.ends_with(".ssc"));
        PackIndex {
            pack_id: 1,
            total: 0,
            etag: None,
            folders: vec![pack_archive::SongFolder {
                name: "Song".to_owned(),
                entries: (0..entries.len()).collect(),
                simfile,
                audio,
            }],
            entries,
            tail_start: 0,
            tail: Arc::from(Vec::<u8>::new()),
        }
    }

    fn song_data(music: &str, clip: &str) -> song_preview::SongPreviewData {
        song_preview::SongPreviewData {
            title: "Song".to_owned(),
            artist: String::new(),
            music: music.to_owned(),
            preview_clip: clip.to_owned(),
            sample_start: 20.0,
            sample_length: 20.0,
            bpm: 120.0,
            charts: Vec::new(),
        }
    }

    /// The clip the simfile names when it is there; else `#MUSIC`, matched
    /// whatever its case or folder; else whatever audio the folder has.
    #[test]
    fn the_audio_is_the_clip_then_the_music_then_whatever_is_there() {
        let index = one_song_pack(&["song.ssc", "Music.OGG", "preview.ogg", "bg.png"]);
        assert_eq!(
            audio_entry(&index, 0, &song_data("music.ogg", "preview.ogg")),
            Some((2, true))
        );
        assert_eq!(
            audio_entry(&index, 0, &song_data("sub\\music.ogg", "missing.ogg")),
            Some((1, false)),
            "a clip that is not in the folder is no clip"
        );
        assert_eq!(
            audio_entry(&index, 0, &song_data("gone.ogg", "")),
            Some((1, false)),
            "a #MUSIC naming nothing falls back to the folder's audio"
        );
        assert_eq!(
            audio_entry(&index, 0, &song_data("bg.png", "")),
            Some((1, false)),
            "an image is never the audio"
        );
        let silent = one_song_pack(&["song.ssc", "bg.png"]);
        assert_eq!(audio_entry(&silent, 0, &song_data("song.ogg", "")), None);
    }

    #[test]
    fn an_entry_is_read_relative_to_its_song_folder() {
        assert_eq!(within_song("Pack/Song/music.ogg"), "music.ogg");
        assert_eq!(within_song("Pack/Song/sub/clip.ogg"), "sub/clip.ogg");
        assert_eq!(within_song("Pack/loose.ogg"), "");
        assert!(is_audio("Pack/Song/a.OGG"));
        assert!(is_audio("Pack/Song/a.mp3"));
        assert!(!is_audio("Pack/Song/a.png"));
        assert!(!is_audio("Pack/Song/ogg"));
        assert_eq!(extension_of("x/y.SSC"), "SSC");
    }

    /// Simfile text is kept while it fits, and a second copy of a song is not
    /// counted twice.
    #[test]
    fn simfile_text_is_kept_within_a_budget() {
        let mut songs = PackSongs::default();
        songs.keep(0, Arc::from(vec![0u8; 10]));
        songs.keep(0, Arc::from(vec![0u8; 10]));
        assert_eq!(songs.simfile_bytes, 10);
        songs.keep(1, Arc::from(vec![0u8; MAX_CACHED_SIMFILE_BYTES]));
        assert!(!songs.simfiles.contains_key(&1), "past the budget");
        assert_eq!(songs.simfile_bytes, 10);
    }

    #[test]
    fn the_extractors_charts_become_the_windows() {
        let mut data = song_data("music.ogg", "");
        data.charts = vec![song_preview::ChartPreview {
            doubles: true,
            difficulty: "Challenge".to_owned(),
            meter: 13,
            lanes: 8,
            notes: vec![song_preview::NoteRow {
                time: 1.5,
                cols: 0b1000_0001,
                lifts: 0,
                mines: 0b1000_0000,
                quant: 3,
            }],
        }];
        let charts = preview_charts(&data);
        assert_eq!(charts.len(), 1);
        assert!(charts[0].doubles);
        assert_eq!(charts[0].lanes, 8);
        assert_eq!(
            charts[0].notes[0],
            PreviewNote {
                time: 1.5,
                cols: 0b1000_0001,
                lifts: 0,
                mines: 0b1000_0000,
                quant: 3
            }
        );
    }

    /// An ITG-synced simfile moves 9 ms to NULL: every offset in it, song and
    /// chart alike, and one with none is given one.
    #[test]
    fn an_itg_simfile_moves_to_null_sync() {
        let ssc = b"#VERSION:0.83;\n#TITLE:Song;\n#OFFSET:-0.123;\n#BPMS:0=120;\n\
            #NOTEDATA:;\n#OFFSET:0.250;\n#NOTES:\n0000\n;\n";
        assert_eq!(
            null_synced_simfile(ssc).unwrap(),
            b"#VERSION:0.83;\n#TITLE:Song;\n#OFFSET:-0.132;\n#BPMS:0=120;\n\
            #NOTEDATA:;\n#OFFSET:0.241;\n#NOTES:\n0000\n;\n"
        );
        // no offset at all is an offset of zero
        assert_eq!(
            null_synced_simfile(b"\xEF\xBB\xBF#TITLE:Song;\r\n#BPMS:0=120;\r\n").unwrap(),
            b"\xEF\xBB\xBF#OFFSET:-0.009;\r\n#TITLE:Song;\r\n#BPMS:0=120;\r\n"
        );
        assert!(null_synced_simfile(b"#OFFSET:-0.1\n#BPMS:0=120;").is_err());
    }

    /// Only the simfiles the engine reads, directly in the song's folder.
    #[test]
    fn a_staged_song_moves_to_null_sync() {
        let root = TempDir::new("nullsync");
        fs::create_dir_all(root.0.join("lights")).unwrap();
        fs::write(root.0.join("song.ssc"), b"#OFFSET:0.000;").unwrap();
        fs::write(root.0.join("song.SM"), b"#OFFSET:0.020;").unwrap();
        fs::write(root.0.join("song.dwi"), b"#GAP:100;").unwrap();
        fs::write(root.0.join("lights/old.sm"), b"#OFFSET:0.000;").unwrap();
        assert_eq!(null_sync_song(&root.0).unwrap(), 2);
        assert_eq!(
            fs::read(root.0.join("song.ssc")).unwrap(),
            b"#OFFSET:-0.009;"
        );
        assert_eq!(fs::read(root.0.join("song.SM")).unwrap(), b"#OFFSET:0.011;");
        assert_eq!(fs::read(root.0.join("song.dwi")).unwrap(), b"#GAP:100;");
        assert_eq!(
            fs::read(root.0.join("lights/old.sm")).unwrap(),
            b"#OFFSET:0.000;"
        );
    }

    #[test]
    fn every_single_song_lands_in_the_null_group() {
        assert_eq!(SINGLES_GROUP, "Content Browser Singles - NULL Sync");
    }

    #[test]
    fn the_singles_pack_ini_declares_null_sync() {
        assert_eq!(
            SINGLES_PACK_INI,
            "[Group]\n# Written by DeadSync's Find Content.\n# Songs downloaded one at a time land here, all null-synced: one from\n# an ITG-synced pack has its offsets moved 9 ms as it installs.\nVersion=1\nSyncOffset=NULL\n"
        );
    }

    #[test]
    fn a_song_is_refused_while_it_is_on_its_way_or_once_it_is_in() {
        let mut runtime = RuntimeState::default();
        let group = SINGLES_GROUP;
        queue_song_record(&mut runtime, 1, "Song", "", group).expect("first");
        assert_eq!(
            queue_song_record(&mut runtime, 1, "Song", "", group).unwrap_err(),
            "that song is already on its way"
        );
        // the same title in another pack is another song
        queue_song_record(&mut runtime, 2, "Song", "", group).expect("other pack");
        // and so is the same title by another artist in the same pack
        queue_song_record(&mut runtime, 1, "Song", "B", group).expect("other artist");
        assert_eq!(runtime.installs_snapshot.installs.len(), 3);

        runtime.installs[0].phase = SongInstallPhase::Installed;
        assert_eq!(
            queue_song_record(&mut runtime, 1, "Song", "", group).unwrap_err(),
            "that song was already added this session"
        );

        // a failure may be tried again, in place
        runtime.installs[1].phase = SongInstallPhase::Error;
        let revision = runtime.installs_snapshot.revision;
        queue_song_record(&mut runtime, 2, "Song", "", group).expect("retry");
        assert_eq!(runtime.installs.len(), 3);
        assert_eq!(runtime.installs[1].phase, SongInstallPhase::Queued);
        assert_eq!(runtime.installs_snapshot.revision, revision + 1);
        assert_eq!(runtime.installs_snapshot.installs[1].group, group);
    }

    /// Leftovers of a run that was closed mid-install go before the next
    /// run's first job; nothing else in those folders is touched.
    #[test]
    fn a_closed_runs_song_leftovers_are_swept() {
        let root = TempDir::new("leftovers");
        let songs = root.0.join("Songs");
        let downloads = root.0.join("cache").join("downloads");
        fs::create_dir_all(songs.join("._deadsync-song-3.part").join("Song")).unwrap();
        fs::create_dir_all(songs.join("Pack")).unwrap();
        fs::create_dir_all(&downloads).unwrap();
        fs::write(downloads.join(".deadsync-song-3.part"), b"half").unwrap();
        fs::write(downloads.join("other.zip"), b"zip").unwrap();
        let job = SongJob {
            seq: 1,
            pack_id: 1,
            title: "Song".to_owned(),
            artist: String::new(),
            group: SINGLES_GROUP,
            itg_sync: true,
            songs_root: songs.clone(),
            cache_dir: root.0.join("cache"),
        };
        sweep_song_leftovers(&job);
        assert!(!songs.join("._deadsync-song-3.part").exists());
        assert!(songs.join("Pack").exists());
        assert!(!downloads.join(".deadsync-song-3.part").exists());
        assert!(downloads.join("other.zip").exists());
    }

    #[test]
    fn install_history_evicts_only_finished_songs() {
        let mut runtime = RuntimeState::default();
        let group = SINGLES_GROUP;
        for id in 0..MAX_SONG_INSTALLS as u64 {
            queue_song_record(&mut runtime, id, "Song", "", group).expect("fill");
        }
        assert_eq!(
            queue_song_record(&mut runtime, 999, "Song", "", group).unwrap_err(),
            "too many songs are on their way"
        );
        runtime.installs[3].phase = SongInstallPhase::Installed;
        queue_song_record(&mut runtime, 999, "Song", "", group).expect("evicts the finished one");
        assert_eq!(runtime.installs.len(), MAX_SONG_INSTALLS);
        assert!(!runtime.installs.iter().any(|install| install.pack_id == 3));
    }

    #[test]
    fn known_charts_are_filed_under_pack_and_title_and_bounded() {
        let charts: Arc<[PreviewChart]> = Arc::from(vec![chart(false, "Hard", 9)]);
        let mut known = KnownCharts::default();
        known.insert(1, "Vertex", "", Arc::clone(&charts));
        assert!(known.get(1, "Vertex", "").is_some());
        assert!(
            known.get(2, "Vertex", "").is_none(),
            "another pack's Vertex"
        );
        assert!(known.get(1, "vertex", "").is_none());
        assert!(known.get(1, "Vertex", "B").is_none(), "another artist's");
        // a second preview of the same song replaces rather than duplicates
        known.insert(1, "Vertex", "", Arc::clone(&charts));
        assert_eq!(known.len(), 1);

        for id in 0..MAX_KNOWN_SONGS as u64 + 10 {
            known.insert(100 + id, "Song", "", Arc::clone(&charts));
        }
        assert_eq!(known.len(), MAX_KNOWN_SONGS);
        assert!(
            known.get(1, "Vertex", "").is_none(),
            "the oldest went first"
        );
        assert!(
            known
                .get(100 + MAX_KNOWN_SONGS as u64 + 9, "Song", "")
                .is_some()
        );
    }

    /// An extracted song lands in its group, which gets its `Pack.ini` the
    /// first time; the same song again, under any case, is refused and what is
    /// in the library is untouched.
    #[test]
    fn an_extracted_song_lands_in_its_group_with_the_groups_pack_ini() {
        let root = TempDir::new("commit");
        let songs = root.0.join("Songs");
        let staged = root.0.join("staging/RED Zone.");
        fs::create_dir_all(staged.join("lights")).unwrap();
        fs::write(staged.join("red zone.ssc"), b"#TITLE:RED Zone;").unwrap();
        fs::write(staged.join("lights/bg.png"), b"png").unwrap();
        let group = SINGLES_GROUP;

        let group_dir = commit_song(&staged, "RED Zone.", &songs, group).expect("commit");
        assert_eq!(group_dir, songs.join(group));
        let song = group_dir.join("RED Zone");
        assert_eq!(
            fs::read(song.join("red zone.ssc")).unwrap(),
            b"#TITLE:RED Zone;"
        );
        assert!(song.join("lights/bg.png").is_file());
        assert!(!staged.exists(), "the staged folder became the song");
        assert_eq!(
            fs::read_to_string(group_dir.join("Pack.ini")).unwrap(),
            SINGLES_PACK_INI
        );

        // An existing Pack.ini is the player's, and is left alone.
        fs::write(group_dir.join("Pack.ini"), "edited").unwrap();
        let again = root.0.join("staging/red zone");
        fs::create_dir_all(&again).unwrap();
        fs::write(again.join("red zone.ssc"), b"#TITLE:Other;").unwrap();
        let error = commit_song(&again, "red zone", &songs, group).unwrap_err();
        assert_eq!(describe_song_error(&error), "already in your library");
        assert_eq!(
            fs::read(song.join("red zone.ssc")).unwrap(),
            b"#TITLE:RED Zone;"
        );
        assert_eq!(
            fs::read_to_string(group_dir.join("Pack.ini")).unwrap(),
            "edited"
        );
    }

    #[test]
    fn a_song_folder_name_is_safe_to_be_a_folder() {
        assert_eq!(song_folder_name("RED Zone."), "RED Zone");
        assert_eq!(song_folder_name("What? Is: This*"), "What_ Is_ This_");
        assert_eq!(song_folder_name("CON"), "_CON");
        assert_eq!(song_folder_name("nul.txt"), "_nul.txt");
        assert_eq!(song_folder_name("._hidden"), "_._hidden");
        assert_eq!(song_folder_name(" .. "), "Content Browser Song");
        assert_eq!(
            song_folder_name(&"x".repeat(400)).chars().count(),
            stepmaniaonline::DESTINATION_MAX_CHARS
        );
    }

    #[test]
    fn a_sweep_removes_only_preview_files() {
        let root = TempDir::new("sweep");
        fs::write(root.0.join("preview-3.ogg"), b"old").unwrap();
        fs::write(root.0.join("preview-4.mp3.part"), b"partial").unwrap();
        fs::write(root.0.join("keep.txt"), b"not ours").unwrap();
        sweep_preview_dir(&root.0);
        assert!(!root.0.join("preview-3.ogg").exists());
        assert!(!root.0.join("preview-4.mp3.part").exists());
        assert!(root.0.join("keep.txt").exists());
        // a folder that is not there yet is nothing to sweep
        sweep_preview_dir(&root.0.join("missing"));
    }

    #[test]
    fn a_failure_reads_as_the_reader_should_see_it() {
        assert_eq!(
            no_match(&SongMatch::Ambiguous, "V.L.S.I").into_message(),
            "more than one song in this pack is called \"V.L.S.I\""
        );
        assert_eq!(
            no_match(&SongMatch::NotFound, "Gone").into_message(),
            "no song called \"Gone\" in this pack"
        );
        assert_eq!(Failure::Stale.into_message(), "");
    }

    #[test]
    fn an_idle_snapshot_claims_nothing() {
        let snapshot = PreviewSnapshot::default();
        assert_eq!(snapshot.phase, PreviewPhase::Idle);
        assert!(snapshot.charts.is_empty());
        assert!(snapshot.audio_path.is_none());
        assert!(snapshot.progress.is_none());
        assert!(SongInstallsSnapshot::default().installs.is_empty());
    }
}

#[cfg(test)]
#[path = "smo_extensions_perf.rs"]
mod extension_perf_tests;
