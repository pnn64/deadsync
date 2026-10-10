//! Single songs out of a stepmaniaonline.net pack, without the pack.
//!
//! The Content Browser wants two things a whole-pack download is far too heavy
//! for: a look at one song's charts before committing to anything, and
//! installing one song out of a pack of two hundred. Both are answered by the
//! same observation -- a zip is random access. Its central directory sits at
//! the end of the file and says where every entry starts, and
//! `/download/pack/<id>/` honours HTTP `Range`, so a pack can be read the way
//! an unzip tool reads a file on disk: the index first, then only the bytes of
//! the entries that are wanted. No relay, no mirror, nothing but the site's
//! own download URL.
//!
//! The shape of a session:
//!
//! * [`fetch_index`] -- one suffix request for the tail of the file, sized so
//!   it holds the central directory of any pack of ordinary size. A second
//!   ranged request only when the directory started before the tail did.
//! * [`fetch_ranges`] + [`read_entry`] -- a handful of small entries, such as
//!   the simfiles behind a preview, in one multi-range request.
//! * [`fetch_entry_to_file`] -- one big entry, such as the audio, inflated to
//!   disk as it arrives rather than held in memory.
//! * [`fetch_folder`] -- a whole song folder, staged for install under the
//!   same safety rules as the pack installer.
//!
//! **Etiquette.** Every request to the download URL counts as a download of
//! the pack on stepmaniaonline.net -- `HEAD` included. So this never sends
//! `HEAD`, sizes its first request so a second is rarely needed, and packs
//! every range it can into one multi-range request. A preview that cost ten
//! "downloads" would be a lie told about somebody's pack.
//!
//! **Decoding is not ours.** The `zip` crate does local headers, inflate, CRC
//! and zip64; all it needs is `Read + Seek`. [`SparseReader`] gives it one over
//! the bytes actually fetched, each at its real offset in the file, and fails
//! any read of a byte that was not. What the crate will not do is say where an
//! entry's bytes are without reading them -- and reading them is the thing
//! being planned -- so there is a small central-directory parser here as well.
//! It only ever plans requests: the crate's own parse of the same directory
//! names every entry and has the final word on what it is.
//!
//! Everything here blocks on the network or the disk. Worker threads only.

use crate::stepmaniaonline as smo;
use deadsync_net as network;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::ZipArchive;
use zip::read::{ArchiveOffset, Config, ZipFile};
use zip::result::ZipError;

pub(crate) const PACK_URL_BASE: &str = "https://stepmaniaonline.net/download/pack/";

/// The smallest first request.
///
/// The end records and a few dozen songs' worth of directory fit with room,
/// and asking for less saves nothing that a second request would not cost
/// back several times over -- in bytes and in the site's download counter.
const MIN_TAIL_BYTES: u64 = 64 * 1024;
/// Directory bytes to allow per song in the first request.
///
/// Measured at 0.7 to 1 KB a song across 4,500 entries: a folder is a simfile,
/// its audio, a banner, a background and a few more, each with a directory
/// record carrying its full path. Half again on top of the worst of that keeps
/// a second request for the packs that are genuinely unusual.
const TAIL_BYTES_PER_SONG: u64 = 1536;
/// The end records, a zip64 locator and any archive comment.
const TAIL_SLACK_BYTES: u64 = 4096;
/// The largest first request. A pack big enough to need more than this has its
/// directory fetched by the second request, which is sized exactly.
const MAX_TAIL_BYTES: u64 = 2 * 1024 * 1024;
/// A central directory larger than this is not a song pack.
const MAX_DIRECTORY_BYTES: u64 = 32 * 1024 * 1024;
/// The pack installer's bound on entries, for the same reason.
const MAX_ENTRIES: u64 = 200_000;
/// Ranges closer than this are fetched as one.
///
/// Every part of a multi-range reply costs a part header, and every range a
/// seek on the server. Sixty-four kilobytes of somebody else's banner is
/// cheaper than either at the sizes a song folder comes in.
const MERGE_GAP_BYTES: u64 = 64 * 1024;
/// The longest `Range` header one request carries. Every request is a counted
/// download, so a whole-pack title scan should be one request, and a few
/// hundred ranges fit here; six kilobytes still leaves room under the 8 KB a
/// header line may be on common servers and proxies.
const MAX_RANGE_HEADER_BYTES: usize = 6 * 1024;
/// Ranges in one request, whatever their length. Apache's default
/// `MaxRanges`: past it, Apache answers with the whole file.
const MAX_RANGES_PER_REQUEST: usize = 200;
/// The most an entry can occupy beyond its compressed data: a local header
/// with the longest name and extra field the format allows, then a zip64 data
/// descriptor. An entry's span stops here even if the next entry is further
/// away, so junk between two entries is never fetched.
const MAX_ENTRY_OVERHEAD_BYTES: u64 = 30 + 0xFFFF + 0xFFFF + 24;
/// One header line of a multipart reply. Real ones are under a hundred bytes.
const MAX_PART_LINE_BYTES: usize = 1024;
/// Header lines in one part: Content-Type and Content-Range, and room for a
/// server that adds a few of its own.
const MAX_PART_HEADER_LINES: usize = 16;
/// Lines before the first delimiter. The RFC allows a preamble; servers send an
/// empty line or nothing.
const MAX_PREAMBLE_LINES: usize = 8;
const COPY_CHUNK_BYTES: usize = 64 * 1024;
/// The most memory reserved up front on a size somebody else declared. A
/// Content-Range or a directory record can claim anything up to the caller's
/// bound; the buffer grows to what actually arrives.
const MAX_RESERVE_BYTES: usize = 8 * 1024 * 1024;
const CANCELLED: &str = "cancelled";
const AUDIO_EXTENSIONS: [&str; 6] = ["ogg", "mp3", "wav", "flac", "opus", "oga"];

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const EOCD64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const EOCD64_SIGNATURE: u32 = 0x0606_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const EOCD_LEN: usize = 22;
const EOCD64_LOCATOR_LEN: usize = 20;
const EOCD64_LEN: usize = 56;
const CENTRAL_HEADER_LEN: usize = 46;
const LOCAL_HEADER_LEN: u64 = 30;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const UNICODE_PATH_EXTRA_ID: u16 = 0x7075;

#[derive(Debug)]
pub(crate) enum ArchiveError {
    /// The server answered a range request with the whole file, so this pack
    /// cannot be read in pieces at all.
    RangeUnsupported,
    /// The file on the server is not the one the index was read from: a 200
    /// where a range was asked for under `If-Range`, a different total size,
    /// or a range that no longer fits. Fetch the index again.
    Changed,
    /// The request failed, or its body stopped arriving.
    Network(String),
    /// The archive, or a reply describing it, is not what it claims to be --
    /// including a CRC that does not match.
    Malformed(String),
    /// More than the caller's `max_bytes`, or than this module will hold.
    TooLarge,
    /// A path or entry the pack installer would refuse to write.
    Unsafe(String),
    /// The local side: a file that could not be written, a byte that was not
    /// fetched, or the caller cancelling.
    Io(String),
}

impl ArchiveError {
    /// Whether this is the caller's own progress callback saying stop.
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, Self::Io(message) if message == CANCELLED)
    }
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RangeUnsupported => {
                f.write_str("StepManiaOnline would not send part of the pack")
            }
            Self::Changed => f.write_str("The pack changed on StepManiaOnline; try again"),
            Self::Network(message) => write!(f, "StepManiaOnline request failed: {message}"),
            Self::Malformed(message) => write!(f, "Invalid pack archive: {message}"),
            Self::TooLarge => f.write_str("Too large to fetch"),
            Self::Unsafe(message) => write!(f, "Unsafe pack archive: {message}"),
            Self::Io(message) if message == CANCELLED => f.write_str("Cancelled"),
            Self::Io(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ArchiveError {}

fn malformed(message: impl Into<String>) -> ArchiveError {
    ArchiveError::Malformed(message.into())
}

fn cancelled() -> ArchiveError {
    ArchiveError::Io(CANCELLED.to_owned())
}

fn io_failure(action: &'static str) -> impl Fn(io::Error) -> ArchiveError {
    move |error| ArchiveError::Io(format!("Failed to {action}: {error}"))
}

/// A body that stopped arriving, or arrived short.
fn transfer_error(error: io::Error) -> ArchiveError {
    ArchiveError::Network(error.to_string())
}

fn ended_early() -> ArchiveError {
    ArchiveError::Network("the transfer ended early".to_owned())
}

fn different_range() -> ArchiveError {
    malformed("the server sent a different range than the one asked for")
}

fn smo_error(error: smo::StepManiaOnlineError) -> ArchiveError {
    match error {
        smo::StepManiaOnlineError::Archive(message) => ArchiveError::Unsafe(message),
        smo::StepManiaOnlineError::Io { action, message } => {
            ArchiveError::Io(format!("Failed to {action}: {message}"))
        }
        other => ArchiveError::Io(other.to_string()),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ZipEntry {
    /// The path inside the archive, exactly as the `zip` crate decodes it.
    pub name: String,
    pub compressed: u64,
    pub uncompressed: u64,
    /// Offset of the entry's local header in the file.
    pub local_header: u64,
    /// Where this entry's bytes end in the file: the next entry's local
    /// header, or the start of the central directory after the last one.
    pub end: u64,
}

impl ZipEntry {
    /// The bytes `[start, end)` to fetch for this entry: its local header, its
    /// data, and any data descriptor.
    ///
    /// Almost always exactly `[local_header, end)`. The bound only bites when
    /// something sits between this entry and the next, which is then left
    /// where it is.
    pub(crate) fn span(&self) -> (u64, u64) {
        let most = self
            .local_header
            .saturating_add(MAX_ENTRY_OVERHEAD_BYTES)
            .saturating_add(self.compressed);
        (self.local_header, self.end.min(most))
    }

    fn is_dir(&self) -> bool {
        self.name.ends_with(['/', '\\'])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SongFolder {
    /// The folder's own name: the second component of its entries' paths,
    /// `Pack/<name>/...`.
    pub name: String,
    /// Every entry beneath it, directory records included, in directory order.
    pub entries: Vec<usize>,
    /// The simfile StepMania would load: a `.ssc` directly in the folder, else
    /// a `.sm`. `None` when neither is there -- a DWI-only song, or a folder
    /// that is really a group of songs.
    pub simfile: Option<usize>,
    /// Audio anywhere beneath the folder, in directory order. The simfile's
    /// `#MUSIC` says which one is the song.
    pub audio: Vec<usize>,
}

#[derive(Debug)]
pub(crate) struct PackIndex {
    pub pack_id: u64,
    /// The file's size, as the server reported it. Every later reply is held
    /// to it.
    pub total: u64,
    /// The server's strong validator, sent back as `If-Range` so a pack
    /// replaced between two requests answers with a 200 instead of the wrong
    /// bytes. `None` when the server gave no strong one; the total still
    /// catches most replacements.
    pub etag: Option<String>,
    /// Every entry, in central-directory order -- which is the `zip` crate's
    /// `by_index` order. A name the directory repeats appears once, at its
    /// first position, carrying its last record, because that is what the
    /// crate does with it.
    pub entries: Vec<ZipEntry>,
    pub folders: Vec<SongFolder>,
    /// The fetched tail of the file -- central directory included, and
    /// reaching to the last byte -- and where it starts. Entries inside it are
    /// read from it rather than fetched again.
    pub tail_start: u64,
    pub tail: Arc<[u8]>,
}

#[must_use]
pub(crate) fn pack_url(pack_id: u64) -> String {
    format!("{PACK_URL_BASE}{pack_id}/")
}

/// How much of the end of the file the first request asks for.
fn tail_request_len(song_count: u32) -> u64 {
    (u64::from(song_count) * TAIL_BYTES_PER_SONG + TAIL_SLACK_BYTES)
        .clamp(MIN_TAIL_BYTES, MAX_TAIL_BYTES)
}

/// The pack's central directory, and with it every entry and song folder.
///
/// One `GET` with `Range: bytes=-N`, where N allows 1.5 KB of directory per
/// song plus 4 KB for the end records, at least 64 KB and at most 2 MB. A
/// second ranged `GET`, held to the first reply's validator, only when the
/// directory starts before the tail did. `song_count` is the catalog's figure
/// and only sizes that first request, so a stale one costs at most the second.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn fetch_index(
    agent: &network::HttpAgent,
    pack_id: u64,
    song_count: u32,
) -> Result<PackIndex, ArchiveError> {
    fetch_index_with(&mut Http::new(agent, pack_id), pack_id, song_count)
}

/// These byte ranges `[start, end)` of the pack, as `(offset, bytes)`
/// segments in offset order.
///
/// Ranges within 64 KB of each other are fetched as one, and up to 64 of what
/// is left go in one request -- a plain range when there is only one, a
/// `multipart/byteranges` reply otherwise. Every request carries `If-Range`
/// with the index's validator, so a 200, or any reply about a file of another
/// size, is [`ArchiveError::Changed`] and its body is dropped unread. Bytes the
/// index's tail already holds are copied out of it instead, so ranges at the
/// end of a small pack cost no request at all. `max_bytes` bounds everything
/// returned, gaps included; the segments cover every byte asked for, and more
/// where neighbours were merged.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn fetch_ranges(
    agent: &network::HttpAgent,
    index: &PackIndex,
    ranges: &[(u64, u64)],
    max_bytes: u64,
) -> Result<Vec<(u64, Vec<u8>)>, ArchiveError> {
    fetch_ranges_with(
        &mut Http::new(agent, index.pack_id),
        index,
        ranges,
        max_bytes,
    )
}

/// One entry, inflated and CRC-checked, from segments that cover its
/// [`ZipEntry::span`].
///
/// `segments` need not cover the tail; the index brings that. A byte the
/// decoder needs that neither holds is an error, never a guess.
pub(crate) fn read_entry(
    index: &PackIndex,
    segments: &[(u64, Vec<u8>)],
    entry: usize,
    max_bytes: u64,
) -> Result<Vec<u8>, ArchiveError> {
    let mut archive = open_archive(index, SparseReader::new(index, segments, None))?;
    read_open_entry(&mut archive, index, entry, max_bytes)
}

/// [`read_entry`] for several entries, parsing the central directory once.
///
/// Each entry goes to `each(entry, result)` as soon as it is inflated, so only
/// one is ever held: a title scan reads every simfile in a pack. The outer
/// error means nothing could be read; each result is one entry's, so one
/// damaged simfile does not cost the rest.
pub(crate) fn read_entries(
    index: &PackIndex,
    segments: &[(u64, Vec<u8>)],
    entries: &[usize],
    max_bytes: u64,
    mut each: impl FnMut(usize, Result<Vec<u8>, ArchiveError>),
) -> Result<(), ArchiveError> {
    let mut archive = open_archive(index, SparseReader::new(index, segments, None))?;
    for &entry in entries {
        each(
            entry,
            read_open_entry(&mut archive, index, entry, max_bytes),
        );
    }
    Ok(())
}

/// One entry, inflated into a new file at `dest`; returns the bytes written.
///
/// The compressed range is one ranged request, and its bytes go straight from
/// the socket through inflate to the file -- nothing larger than a copy
/// buffer is held, which matters for the audio. `on_progress(written, total)`
/// hears about every chunk, `total` being the entry's uncompressed size;
/// returning `false` stops it with a cancellation
/// ([`ArchiveError::is_cancelled`]). `dest` must not exist, and is removed
/// again if anything fails. `max_bytes` bounds both what is downloaded and
/// what is written.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn fetch_entry_to_file(
    agent: &network::HttpAgent,
    index: &PackIndex,
    entry: usize,
    dest: &Path,
    max_bytes: u64,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<u64, ArchiveError> {
    fetch_entry_to_file_with(
        &mut Http::new(agent, index.pack_id),
        index,
        entry,
        dest,
        max_bytes,
        &mut on_progress,
    )
}

/// One song folder, staged at `staging/<name>/`; returns the name it was
/// written under. That is the folder's own name less any trailing dots and
/// spaces, which Windows would drop anyway, and with a `_` in front of a
/// device name such as `CON`.
///
/// The folder's bytes are downloaded to `temp` -- one range when its entries
/// sit together, as they almost always do, else their ranges in one request
/// -- and `temp` is removed afterwards whatever happens.
/// `on_progress(done, total)` follows the download; `false` cancels. Then
/// every entry is extracted under the pack installer's rules: enclosed names
/// only (no `..`, no absolute paths, no drive letters), no symbolic links,
/// nothing two names would make one file, at least one simfile, and
/// `max_bytes` bounding both the download and what is written.
///
/// Everything that can be refused from the directory alone is refused before
/// a byte is downloaded. The song folder is created fresh: one already in
/// `staging` is an error rather than something to write into, and a failed
/// extraction removes only what it created.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn fetch_folder(
    agent: &network::HttpAgent,
    index: &PackIndex,
    folder: usize,
    temp: &Path,
    staging: &Path,
    max_bytes: u64,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<String, ArchiveError> {
    fetch_folder_with(
        &mut Http::new(agent, index.pack_id),
        index,
        folder,
        temp,
        staging,
        max_bytes,
        &mut on_progress,
    )
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// A reply to one ranged GET, described before any of its body is read -- so
/// a reply that is the wrong shape is refused without downloading it.
struct RangeReply {
    status: u16,
    content_type: Option<String>,
    content_range: Option<String>,
    content_length: Option<u64>,
    etag: Option<String>,
    body: Box<dyn Read>,
}

/// One ranged GET of the pack. The network is behind this so everything that
/// decides what to ask for, and what an answer means, runs without one.
trait Transport {
    fn get(&mut self, range: &str, if_range: Option<&str>) -> Result<RangeReply, ArchiveError>;
}

/// The real download URL. Never exercised by the tests, which stand a
/// scripted server in its place.
#[cfg_attr(test, allow(dead_code))]
struct Http<'a> {
    agent: &'a network::HttpAgent,
    url: String,
}

impl<'a> Http<'a> {
    #[cfg_attr(test, allow(dead_code))]
    fn new(agent: &'a network::HttpAgent, pack_id: u64) -> Self {
        Self {
            agent,
            url: pack_url(pack_id),
        }
    }
}

impl Transport for Http<'_> {
    fn get(&mut self, range: &str, if_range: Option<&str>) -> Result<RangeReply, ArchiveError> {
        let mut request = self
            .agent
            .get(self.url.as_str())
            .header("Range", range)
            // A range is a range of the file as stored. Under a compressed
            // transfer it would be a range of something else.
            .header("Accept-Encoding", "identity");
        if let Some(tag) = if_range {
            request = request.header("If-Range", tag);
        }
        let response = request
            .call()
            .map_err(|error| match network::error_from_ureq(error) {
                // A range that fit the file when the index was read and does
                // not now: the file got shorter.
                network::NetworkError::HttpStatus(416) => ArchiveError::Changed,
                other => ArchiveError::Network(other.to_string()),
            })?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = header("content-type");
        let content_range = header("content-range");
        let etag = header("etag");
        let status = response.status().as_u16();
        let content_length = response.body().content_length();
        Ok(RangeReply {
            status,
            content_type,
            content_range,
            content_length,
            etag,
            body: Box::new(response.into_body().into_reader()),
        })
    }
}

/// `bytes a-b/total` as `(a, b + 1, total)`.
fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim();
    let rest = value
        .get(..5)
        .filter(|unit| unit.eq_ignore_ascii_case("bytes"))
        .map(|_| value[5..].trim_start())?;
    let (span, total) = rest.split_once('/')?;
    let (first, last) = span.trim().split_once('-')?;
    let first: u64 = first.trim().parse().ok()?;
    let last: u64 = last.trim().parse().ok()?;
    let total: u64 = total.trim().parse().ok()?;
    (first <= last && last < total).then_some((first, last + 1, total))
}

fn is_multipart(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("multipart/byteranges"))
}

fn multipart_boundary(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        let value = value.trim().trim_matches('"');
        // RFC 2046 caps a boundary at 70 characters.
        (key.trim().eq_ignore_ascii_case("boundary") && !value.is_empty() && value.len() <= 70)
            .then(|| value.to_owned())
    })
}

/// A strong validator, or nothing. A weak one may not be used in `If-Range`.
fn strong_etag(tag: Option<&str>) -> Option<String> {
    tag.map(str::trim)
        .filter(|tag| !tag.is_empty() && !tag.starts_with("W/"))
        .map(str::to_owned)
}

enum Shape {
    Single { start: u64, end: u64 },
    Multipart { boundary: String },
}

/// What a reply to a ranged GET of a known file holds.
fn range_shape(reply: &RangeReply, total: u64) -> Result<Shape, ArchiveError> {
    match reply.status {
        206 => {}
        // Under If-Range a 200 is the server saying the validator no longer
        // matches; without one, a server ignoring Range. Either way the body
        // is the whole pack, and it is dropped unread.
        200 => return Err(ArchiveError::Changed),
        status => return Err(malformed(format!("unexpected HTTP status {status}"))),
    }
    if let Some(kind) = reply.content_type.as_deref()
        && is_multipart(kind)
    {
        let boundary =
            multipart_boundary(kind).ok_or_else(|| malformed("multipart reply has no boundary"))?;
        return Ok(Shape::Multipart { boundary });
    }
    let (start, end, reported) = reply
        .content_range
        .as_deref()
        .and_then(parse_content_range)
        .ok_or_else(|| malformed("206 reply without a usable Content-Range"))?;
    if reported != total {
        return Err(ArchiveError::Changed);
    }
    Ok(Shape::Single { start, end })
}

/// Somewhere for the parts of a reply to go.
trait PartSink {
    /// Take exactly `len` bytes of the pack, starting at offset `start`, from
    /// `body`.
    fn part(&mut self, start: u64, len: u64, body: &mut dyn Read) -> Result<(), ArchiveError>;
}

fn charge(budget: &mut u64, len: u64) -> Result<(), ArchiveError> {
    *budget = budget.checked_sub(len).ok_or(ArchiveError::TooLarge)?;
    Ok(())
}

/// Every part of a ranged reply into `sink`, each charged against `budget`
/// before it is read.
fn receive(
    reply: RangeReply,
    total: u64,
    budget: &mut u64,
    sink: &mut dyn PartSink,
) -> Result<(), ArchiveError> {
    match range_shape(&reply, total)? {
        Shape::Single { start, end } => {
            charge(budget, end - start)?;
            let mut body = reply.body;
            sink.part(start, end - start, &mut body)
        }
        Shape::Multipart { boundary } => {
            let mut body = BufReader::with_capacity(COPY_CHUNK_BYTES, reply.body);
            read_multipart(&mut body, &boundary, total, budget, sink)
        }
    }
}

/// A `multipart/byteranges` body, part by part.
///
/// Each part says where it is with its own Content-Range, and its length is
/// taken from that rather than from hunting for the next boundary in the
/// data -- audio is binary and could contain anything. The delimiter after
/// the data is then checked, so a part shorter or longer than it claimed is
/// caught rather than shifted into the next. Parts may arrive merged or in any
/// order; the caller checks coverage, not the shape.
fn read_multipart(
    body: &mut impl BufRead,
    boundary: &str,
    total: u64,
    budget: &mut u64,
    sink: &mut dyn PartSink,
) -> Result<(), ArchiveError> {
    let delimiter = format!("--{boundary}");
    let closing = format!("--{boundary}--");
    let mut line = Vec::with_capacity(128);
    let mut preamble = 0;
    loop {
        if !read_part_line(body, &mut line)? {
            return Err(malformed("multipart reply ended before its first part"));
        }
        if line == delimiter.as_bytes() {
            break;
        }
        preamble += 1;
        if preamble > MAX_PREAMBLE_LINES {
            return Err(malformed("multipart reply has no delimiter"));
        }
    }
    loop {
        let mut range = None;
        let mut headers = 0;
        loop {
            if !read_part_line(body, &mut line)? {
                return Err(malformed("multipart reply ended inside a part header"));
            }
            if line.is_empty() {
                break;
            }
            headers += 1;
            if headers > MAX_PART_HEADER_LINES {
                return Err(malformed("multipart part header is too long"));
            }
            let text = String::from_utf8_lossy(&line);
            if let Some((name, value)) = text.split_once(':')
                && name.trim().eq_ignore_ascii_case("content-range")
            {
                range = Some(
                    parse_content_range(value)
                        .ok_or_else(|| malformed("multipart part has a bad Content-Range"))?,
                );
            }
        }
        let (start, end, reported) =
            range.ok_or_else(|| malformed("multipart part without a Content-Range"))?;
        if reported != total {
            return Err(ArchiveError::Changed);
        }
        charge(budget, end - start)?;
        sink.part(start, end - start, body)?;
        // The line break that closes the data, then the next delimiter. Some
        // servers leave the break out; none sends more than one.
        let mut blank = 0;
        loop {
            if !read_part_line(body, &mut line)? {
                return Err(malformed(
                    "multipart reply ended without its closing delimiter",
                ));
            }
            if !line.is_empty() {
                break;
            }
            blank += 1;
            if blank > 2 {
                return Err(malformed("multipart part is longer than its Content-Range"));
            }
        }
        if line == closing.as_bytes() {
            return Ok(());
        }
        if line != delimiter.as_bytes() {
            return Err(malformed("multipart part is longer than its Content-Range"));
        }
    }
}

/// One line of a multipart header block, without its line ending or trailing
/// padding. `false` at the end of the body.
fn read_part_line(body: &mut impl BufRead, line: &mut Vec<u8>) -> Result<bool, ArchiveError> {
    line.clear();
    let read = body
        .take(MAX_PART_LINE_BYTES as u64 + 1)
        .read_until(b'\n', line)
        .map_err(transfer_error)?;
    if read == 0 {
        return Ok(false);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    } else if line.len() > MAX_PART_LINE_BYTES {
        return Err(malformed("multipart header line is too long"));
    }
    while line
        .last()
        .is_some_and(|byte| matches!(byte, b'\r' | b' ' | b'\t'))
    {
        line.pop();
    }
    Ok(true)
}

/// Parts kept in memory, for small entries.
struct MemorySink<'s> {
    segments: &'s mut Vec<(u64, Vec<u8>)>,
}

impl PartSink for MemorySink<'_> {
    fn part(&mut self, start: u64, len: u64, body: &mut dyn Read) -> Result<(), ArchiveError> {
        let capacity = usize::try_from(len).map_err(|_| ArchiveError::TooLarge)?;
        let mut bytes = Vec::with_capacity(capacity.min(MAX_RESERVE_BYTES));
        body.take(len)
            .read_to_end(&mut bytes)
            .map_err(transfer_error)?;
        if bytes.len() != capacity {
            return Err(ended_early());
        }
        self.segments.push((start, bytes));
        Ok(())
    }
}

/// Parts appended to a local file, for song folders.
struct FileSink<'f, 'p> {
    file: &'f File,
    /// Where the next part goes in the file.
    at: u64,
    /// `(offset in the pack, offset in the file, length)` of each part.
    pieces: Vec<(u64, u64, u64)>,
    done: u64,
    total: u64,
    on_progress: &'p mut dyn FnMut(u64, u64) -> bool,
    buffer: Vec<u8>,
}

impl PartSink for FileSink<'_, '_> {
    fn part(&mut self, start: u64, len: u64, body: &mut dyn Read) -> Result<(), ArchiveError> {
        let write_failed = io_failure("write the song download");
        let mut file = self.file;
        file.seek(SeekFrom::Start(self.at)).map_err(&write_failed)?;
        let mut left = len;
        while left > 0 {
            let want = usize::try_from(left.min(self.buffer.len() as u64)).unwrap_or(0);
            let read = match body.read(&mut self.buffer[..want]) {
                Ok(0) => return Err(ended_early()),
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(transfer_error(error)),
            };
            file.write_all(&self.buffer[..read])
                .map_err(&write_failed)?;
            left -= read as u64;
            self.done += read as u64;
            if !(self.on_progress)(self.done, self.total.max(self.done)) {
                return Err(cancelled());
            }
        }
        self.pieces.push((start, self.at, len));
        self.at += len;
        Ok(())
    }
}

/// `Range: bytes=a-b,c-d,...` for half-open ranges.
fn range_header(ranges: &[(u64, u64)]) -> String {
    let mut header = String::with_capacity(6 + ranges.len() * 24);
    header.push_str("bytes=");
    for (ix, (start, end)) in ranges.iter().enumerate() {
        if ix > 0 {
            header.push(',');
        }
        write!(header, "{start}-{}", end - 1).expect("writing to a String cannot fail");
    }
    header
}

/// The planned ranges, in as few requests as [`ranges_in_one_request`] allows.
fn request_ranges(
    transport: &mut dyn Transport,
    index: &PackIndex,
    planned: &[(u64, u64)],
    budget: &mut u64,
    sink: &mut dyn PartSink,
) -> Result<(), ArchiveError> {
    let mut rest = planned;
    while !rest.is_empty() {
        let (group, later) = rest.split_at(ranges_in_one_request(rest));
        let reply = transport.get(&range_header(group), index.etag.as_deref())?;
        receive(reply, index.total, budget, sink)?;
        rest = later;
    }
    Ok(())
}

/// How many of `ranges`, from the front, go in one request: as many as fit in
/// [`MAX_RANGE_HEADER_BYTES`] of header, at most [`MAX_RANGES_PER_REQUEST`],
/// and always at least one.
fn ranges_in_one_request(ranges: &[(u64, u64)]) -> usize {
    let mut header = "bytes=".len();
    let mut count = 0;
    for &(start, end) in ranges.iter().take(MAX_RANGES_PER_REQUEST) {
        let item =
            usize::from(count > 0) + decimal_len(start) + 1 + decimal_len(end.saturating_sub(1));
        if count > 0 && header + item > MAX_RANGE_HEADER_BYTES {
            break;
        }
        header += item;
        count += 1;
    }
    count.max(1)
}

fn decimal_len(value: u64) -> usize {
    value
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

/// One ranged GET for exactly `[start, end)`, held in memory.
fn fetch_exact(
    transport: &mut dyn Transport,
    start: u64,
    end: u64,
    total: u64,
    etag: Option<&str>,
) -> Result<Vec<u8>, ArchiveError> {
    let reply = transport.get(&range_header(&[(start, end)]), etag)?;
    let mut segments = Vec::with_capacity(1);
    let mut budget = end - start;
    receive(
        reply,
        total,
        &mut budget,
        &mut MemorySink {
            segments: &mut segments,
        },
    )?;
    match (segments.pop(), segments.is_empty()) {
        (Some((at, bytes)), true) if at == start && bytes.len() as u64 == end - start => Ok(bytes),
        _ => Err(different_range()),
    }
}

// ---------------------------------------------------------------------------
// Range planning
// ---------------------------------------------------------------------------

/// Join ranges no more than `gap` apart, in order.
fn merge_ranges(mut ranges: Vec<(u64, u64)>, gap: u64) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1.saturating_add(gap)
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

/// What was asked for, checked against the file and joined where it overlaps:
/// exactly the bytes wanted, no more.
fn union_ranges(ranges: &[(u64, u64)], total: u64) -> Result<Vec<(u64, u64)>, ArchiveError> {
    if let Some((start, end)) = ranges
        .iter()
        .find(|(start, end)| start >= end || *end > total)
    {
        return Err(malformed(format!(
            "range {start}..{end} is not inside the pack's {total} bytes"
        )));
    }
    Ok(merge_ranges(ranges.to_vec(), 0))
}

/// Half-open byte ranges of the pack.
type Ranges = Vec<(u64, u64)>;

/// Split ranges at the tail: what has to be fetched, and what the tail holds.
fn split_at_tail(ranges: &[(u64, u64)], tail_start: u64) -> (Ranges, Ranges) {
    let mut fetch = Vec::with_capacity(ranges.len());
    let mut held = Vec::new();
    for &(start, end) in ranges {
        if start < tail_start {
            fetch.push((start, end.min(tail_start)));
        }
        if end > tail_start {
            held.push((start.max(tail_start), end));
        }
    }
    (fetch, held)
}

fn span_total(ranges: &[(u64, u64)]) -> u64 {
    ranges
        .iter()
        .fold(0u64, |sum, (start, end)| sum.saturating_add(end - start))
}

/// Whether `have` (any order, overlapping or not) covers every byte of `want`.
fn covers(have: &[(u64, u64)], want: &[(u64, u64)]) -> bool {
    let joined = merge_ranges(have.to_vec(), 0);
    want.iter().all(|&(start, end)| {
        let ix = joined.partition_point(|&(from, _)| from <= start);
        ix > 0 && joined[ix - 1].1 >= end
    })
}

fn fetch_ranges_with(
    transport: &mut dyn Transport,
    index: &PackIndex,
    ranges: &[(u64, u64)],
    max_bytes: u64,
) -> Result<Vec<(u64, Vec<u8>)>, ArchiveError> {
    let wanted = union_ranges(ranges, index.total)?;
    let (fetch, held) = split_at_tail(&wanted, index.tail_start);
    let planned = merge_ranges(fetch.clone(), MERGE_GAP_BYTES);
    let held_bytes = span_total(&held);
    if span_total(&planned).saturating_add(held_bytes) > max_bytes {
        return Err(ArchiveError::TooLarge);
    }
    let mut segments = Vec::with_capacity(planned.len() + held.len());
    for &(start, end) in &held {
        let from = usize::try_from(start - index.tail_start).map_err(|_| ArchiveError::TooLarge)?;
        let to = usize::try_from(end - index.tail_start).map_err(|_| ArchiveError::TooLarge)?;
        let bytes = index
            .tail
            .get(from..to)
            .ok_or_else(|| malformed("the index's tail is shorter than the file"))?;
        segments.push((start, bytes.to_vec()));
    }
    let mut budget = max_bytes - held_bytes;
    request_ranges(
        transport,
        index,
        &planned,
        &mut budget,
        &mut MemorySink {
            segments: &mut segments,
        },
    )?;
    let have: Vec<(u64, u64)> = segments
        .iter()
        .map(|(start, bytes)| (*start, start + bytes.len() as u64))
        .collect();
    if !covers(&have, &fetch) {
        return Err(malformed(
            "the server left out part of a range it was asked for",
        ));
    }
    segments.sort_unstable_by_key(|(start, _)| *start);
    Ok(segments)
}

// ---------------------------------------------------------------------------
// The index
// ---------------------------------------------------------------------------

fn fetch_index_with(
    transport: &mut dyn Transport,
    pack_id: u64,
    song_count: u32,
) -> Result<PackIndex, ArchiveError> {
    let want = tail_request_len(song_count);
    // No If-Range on the first request: there is nothing yet to hold the
    // server to. This reply is what everything after is held to.
    let reply = transport.get(&format!("bytes=-{want}"), None)?;
    let etag = strong_etag(reply.etag.as_deref());
    let (tail_start, total, tail) = receive_tail(reply, want)?;
    let validator = etag.clone();
    build_index(pack_id, total, etag, tail_start, tail, |start, end| {
        fetch_exact(transport, start, end, total, validator.as_deref())
    })
}

/// The reply to the suffix request: where it starts, the file's size, and the
/// bytes.
fn receive_tail(reply: RangeReply, want: u64) -> Result<(u64, u64, Vec<u8>), ArchiveError> {
    match reply.status {
        206 => {
            if reply.content_type.as_deref().is_some_and(is_multipart) {
                return Err(different_range());
            }
            let (start, end, total) = reply
                .content_range
                .as_deref()
                .and_then(parse_content_range)
                .ok_or_else(|| malformed("206 reply without a usable Content-Range"))?;
            // A suffix ends at the end of the file and is no longer than asked.
            if end != total || end - start > want {
                return Err(different_range());
            }
            Ok((start, total, read_body(reply.body, end - start)?))
        }
        // A file shorter than the suffix may come back whole, and then the
        // whole file is the tail. A longer one means Range was ignored: that
        // body is the entire pack, and it is dropped unread.
        200 => match reply.content_length {
            Some(len) if len > 0 && len <= want => Ok((0, len, read_body(reply.body, len)?)),
            _ => Err(ArchiveError::RangeUnsupported),
        },
        status => Err(malformed(format!("unexpected HTTP status {status}"))),
    }
}

fn read_body(body: Box<dyn Read>, len: u64) -> Result<Vec<u8>, ArchiveError> {
    let capacity = usize::try_from(len).map_err(|_| ArchiveError::TooLarge)?;
    let mut bytes = Vec::with_capacity(capacity.min(MAX_RESERVE_BYTES));
    body.take(len)
        .read_to_end(&mut bytes)
        .map_err(transfer_error)?;
    if bytes.len() != capacity {
        return Err(ended_early());
    }
    Ok(bytes)
}

/// The index, from the tail of the file; `fetch_head(start, end)` fetches the
/// part of the central directory the tail missed, if it missed any.
fn build_index(
    pack_id: u64,
    total: u64,
    etag: Option<String>,
    tail_start: u64,
    tail: Vec<u8>,
    fetch_head: impl FnOnce(u64, u64) -> Result<Vec<u8>, ArchiveError>,
) -> Result<PackIndex, ArchiveError> {
    if tail_start.checked_add(tail.len() as u64) != Some(total) {
        return Err(malformed("the tail does not reach the end of the file"));
    }
    let directory = locate_directory(&tail, tail_start, total)?;
    let (tail_start, tail) = if directory.start < tail_start {
        if tail_start - directory.start > MAX_DIRECTORY_BYTES {
            return Err(ArchiveError::TooLarge);
        }
        let head = fetch_head(directory.start, tail_start)?;
        let mut joined = Vec::with_capacity(head.len() + tail.len());
        joined.extend_from_slice(&head);
        joined.extend_from_slice(&tail);
        (directory.start, joined)
    } else {
        (tail_start, tail)
    };
    let from = usize::try_from(directory.start - tail_start).map_err(|_| ArchiveError::TooLarge)?;
    let raw = parse_directory(&tail[from..], directory.count)?;
    let tail: Arc<[u8]> = Arc::from(tail);
    let names = crate_names(&tail, tail_start, total)?;
    let entries = settle_entries(&raw, names, directory.start)?;
    let folders = group_folders(&entries);
    Ok(PackIndex {
        pack_id,
        total,
        etag,
        entries,
        folders,
        tail_start,
        tail,
    })
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let field = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(field.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let field = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(field.try_into().ok()?))
}

fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let field = bytes.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes(field.try_into().ok()?))
}

/// Where the central directory is and how many records it holds.
struct Directory {
    start: u64,
    count: u64,
}

/// The end-of-central-directory record, searched for from the end of the file
/// back across the longest comment the format allows, the way the `zip` crate
/// searches -- so both settle on the same record.
fn locate_directory(tail: &[u8], tail_start: u64, total: u64) -> Result<Directory, ArchiveError> {
    let Some(mut at) = tail.len().checked_sub(EOCD_LEN) else {
        return Err(malformed("the file is too short to be a zip"));
    };
    let lowest = at.saturating_sub(0xFFFF);
    let mut first_error = None;
    loop {
        if read_u32(tail, at) == Some(EOCD_SIGNATURE) {
            match read_end_records(tail, tail_start, total, at) {
                Ok(directory) => return Ok(directory),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if at == lowest {
            break;
        }
        at -= 1;
    }
    Err(first_error.unwrap_or_else(|| malformed("no end-of-central-directory record")))
}

/// The end record at `at` in the tail, and the zip64 records it may point to.
fn read_end_records(
    tail: &[u8],
    tail_start: u64,
    total: u64,
    at: usize,
) -> Result<Directory, ArchiveError> {
    let truncated = || malformed("the end record is cut short");
    let field16 = |offset: usize| read_u16(tail, at + offset).ok_or_else(truncated);
    let field32 = |offset: usize| read_u32(tail, at + offset).ok_or_else(truncated);
    let (disk, directory_disk, count) = (field16(4)?, field16(6)?, field16(10)?);
    let (size, start, comment) = (field32(12)?, field32(16)?, field16(20)?);
    let position = tail_start + at as u64;
    if position + EOCD_LEN as u64 + u64::from(comment) > total {
        return Err(malformed(
            "the end record's comment runs past the end of the file",
        ));
    }
    let may_be_zip64 = count == u16::MAX || size == u32::MAX || start == u32::MAX;
    let zip64 = if may_be_zip64 {
        read_zip64_end(tail, tail_start, at)?
    } else {
        None
    };
    let (disk, directory_disk, count, size, start, boundary) = match zip64 {
        Some(end) => (
            end.disk,
            end.directory_disk,
            end.count,
            end.size,
            end.start,
            end.position,
        ),
        None => (
            u32::from(disk),
            u32::from(directory_disk),
            u64::from(count),
            u64::from(size),
            u64::from(start),
            position,
        ),
    };
    if disk != directory_disk {
        return Err(malformed("multi-disk archives are not supported"));
    }
    if count > MAX_ENTRIES {
        return Err(ArchiveError::TooLarge);
    }
    let directory_end = start
        .checked_add(size)
        .ok_or_else(|| malformed("the central directory's size overflows"))?;
    if directory_end > boundary || (count > 0 && start >= boundary) {
        return Err(malformed("the central directory overlaps its end record"));
    }
    if count.saturating_mul(CENTRAL_HEADER_LEN as u64) > size {
        return Err(malformed(
            "the central directory is smaller than its records",
        ));
    }
    Ok(Directory { start, count })
}

struct Zip64End {
    /// Where the zip64 end record is in the file: the directory ends before it.
    position: u64,
    disk: u32,
    directory_disk: u32,
    count: u64,
    size: u64,
    start: u64,
}

/// The zip64 end record, when a locator sits just before the end record.
/// Without one the 32-bit fields are taken as they are, as the crate does.
fn read_zip64_end(
    tail: &[u8],
    tail_start: u64,
    eocd_at: usize,
) -> Result<Option<Zip64End>, ArchiveError> {
    let Some(locator) = eocd_at.checked_sub(EOCD64_LOCATOR_LEN) else {
        return Ok(None);
    };
    if read_u32(tail, locator) != Some(EOCD64_LOCATOR_SIGNATURE) {
        return Ok(None);
    }
    let truncated = || malformed("the zip64 end record is cut short");
    let position = read_u64(tail, locator + 8).ok_or_else(truncated)?;
    if read_u32(tail, locator + 16).ok_or_else(truncated)? > 1 {
        return Err(malformed("multi-disk archives are not supported"));
    }
    let locator_position = tail_start + locator as u64;
    let at = position
        .checked_sub(tail_start)
        .and_then(|at| usize::try_from(at).ok())
        .filter(|&at| position < locator_position && at + EOCD64_LEN <= locator)
        .ok_or_else(|| malformed("the zip64 end record is not where its locator says"))?;
    if read_u32(tail, at) != Some(EOCD64_SIGNATURE) {
        return Err(malformed(
            "the zip64 end record is not where its locator says",
        ));
    }
    // The record runs right up to the locator; the crate insists on it too.
    let record = read_u64(tail, at + 4).ok_or_else(truncated)?;
    if record.checked_add(12) != Some(locator_position - position) {
        return Err(malformed("the zip64 end record has the wrong length"));
    }
    Ok(Some(Zip64End {
        position,
        disk: read_u32(tail, at + 16).ok_or_else(truncated)?,
        directory_disk: read_u32(tail, at + 20).ok_or_else(truncated)?,
        count: read_u64(tail, at + 32).ok_or_else(truncated)?,
        size: read_u64(tail, at + 40).ok_or_else(truncated)?,
        start: read_u64(tail, at + 48).ok_or_else(truncated)?,
    }))
}

/// One central-directory record, as far as planning needs it.
struct RawEntry {
    /// What the `zip` crate keys the entry by: the raw name, or the Info-ZIP
    /// Unicode path that replaces it.
    key: Vec<u8>,
    compressed: u64,
    uncompressed: u64,
    local_header: u64,
}

/// `count` records from the start of `bytes`, as the crate reads them: by
/// count, not by the directory's stated size.
fn parse_directory(bytes: &[u8], count: u64) -> Result<Vec<RawEntry>, ArchiveError> {
    let count = usize::try_from(count).map_err(|_| ArchiveError::TooLarge)?;
    let cut_short = || malformed("the central directory is cut short");
    let mut entries = Vec::with_capacity(count.min(bytes.len() / CENTRAL_HEADER_LEN));
    let mut at = 0usize;
    for _ in 0..count {
        let header = bytes
            .get(at..at + CENTRAL_HEADER_LEN)
            .ok_or_else(cut_short)?;
        let field16 = |offset: usize| read_u16(header, offset).unwrap_or_default();
        let field32 = |offset: usize| read_u32(header, offset).unwrap_or_default();
        if field32(0) != CENTRAL_HEADER_SIGNATURE {
            return Err(malformed("a central directory record is damaged"));
        }
        let name_at = at + CENTRAL_HEADER_LEN;
        let extra_at = name_at + usize::from(field16(28));
        let comment_at = extra_at + usize::from(field16(30));
        let next = comment_at + usize::from(field16(32));
        if next > bytes.len() {
            return Err(cut_short());
        }
        let mut entry = RawEntry {
            key: bytes[name_at..extra_at].to_vec(),
            compressed: u64::from(field32(20)),
            uncompressed: u64::from(field32(24)),
            local_header: u64::from(field32(42)),
        };
        read_extra_fields(&bytes[extra_at..comment_at], &mut entry)?;
        entries.push(entry);
        at = next;
    }
    Ok(entries)
}

/// The two extra fields that change what an entry is: zip64 sizes and
/// offsets, and a Unicode path. Read the way the crate reads them -- including
/// its habit of taking all three zip64 values whenever the field is long
/// enough to hold them -- so both agree on where every entry is.
fn read_extra_fields(extra: &[u8], entry: &mut RawEntry) -> Result<(), ArchiveError> {
    let mut at = 0;
    while let (Some(kind), Some(len)) = (read_u16(extra, at), read_u16(extra, at + 2)) {
        let data = extra
            .get(at + 4..at + 4 + usize::from(len))
            .ok_or_else(|| malformed("an extra field runs past its record"))?;
        match kind {
            ZIP64_EXTRA_ID => {
                let all = len >= 24;
                let mut cursor = 0;
                let mut next = || {
                    let value = read_u64(data, cursor)
                        .ok_or_else(|| malformed("a zip64 extra field is cut short"))?;
                    cursor += 8;
                    Ok::<u64, ArchiveError>(value)
                };
                let marker = u64::from(u32::MAX);
                if all || entry.uncompressed == marker {
                    entry.uncompressed = next()?;
                }
                if all || entry.compressed == marker {
                    entry.compressed = next()?;
                }
                if all || entry.local_header == marker {
                    entry.local_header = next()?;
                }
            }
            UNICODE_PATH_EXTRA_ID => {
                // A version byte and the CRC of the raw name, then the name.
                let name = data
                    .get(5..)
                    .ok_or_else(|| malformed("a Unicode path field is cut short"))?;
                entry.key = name.to_vec();
            }
            _ => {}
        }
        at += 4 + usize::from(len);
    }
    Ok(())
}

/// The `zip` crate's own reading of the directory: its names, in its
/// `by_index` order. This is what says what each entry is called -- CP437,
/// UTF-8 and Unicode-path names all decoded one way, the crate's -- and its
/// count is what the planner's must match.
fn crate_names(tail: &[u8], tail_start: u64, total: u64) -> Result<Vec<String>, ArchiveError> {
    let reader = SparseReader {
        total,
        pos: 0,
        pieces: vec![Piece::memory(tail_start, tail)],
        stream: None,
    };
    let archive = ZipArchive::with_config(crate_config(), reader).map_err(zip_error)?;
    Ok(archive.file_names().map(str::to_owned).collect())
}

/// The planner's records, settled into the crate's order with the crate's
/// names, each with where its bytes end.
fn settle_entries(
    raw: &[RawEntry],
    names: Vec<String>,
    directory_start: u64,
) -> Result<Vec<ZipEntry>, ArchiveError> {
    // An entry's bytes stop where the next entry's start, whichever record
    // that is -- a repeated name's earlier record included.
    let mut starts: Vec<u64> = raw.iter().map(|entry| entry.local_header).collect();
    starts.sort_unstable();
    starts.dedup();
    // The crate keys entries by name. A repeated name keeps its first
    // position and takes the last record's data, so the same is done here and
    // index `i` stays the crate's `by_index(i)`.
    let mut slots: HashMap<&[u8], usize> = HashMap::with_capacity(raw.len());
    let mut chosen: Vec<usize> = Vec::with_capacity(raw.len());
    for (ix, record) in raw.iter().enumerate() {
        match slots.entry(record.key.as_slice()) {
            Entry::Occupied(slot) => chosen[*slot.get()] = ix,
            Entry::Vacant(slot) => {
                slot.insert(chosen.len());
                chosen.push(ix);
            }
        }
    }
    if chosen.len() != names.len() {
        return Err(malformed(
            "the central directory does not read the same way twice",
        ));
    }
    let mut entries = Vec::with_capacity(chosen.len());
    for (name, ix) in names.into_iter().zip(chosen) {
        let record = &raw[ix];
        if record.local_header >= directory_start {
            return Err(malformed(format!(
                "'{name}' starts inside the central directory"
            )));
        }
        let end = starts
            .get(starts.partition_point(|&start| start <= record.local_header))
            .copied()
            .unwrap_or(directory_start)
            .min(directory_start);
        let room = end - record.local_header;
        if record
            .compressed
            .checked_add(LOCAL_HEADER_LEN)
            .is_none_or(|need| need > room)
        {
            return Err(malformed(format!("'{name}' overlaps the entry after it")));
        }
        entries.push(ZipEntry {
            name,
            compressed: record.compressed,
            uncompressed: record.uncompressed,
            local_header: record.local_header,
            end,
        });
    }
    Ok(entries)
}

fn extension(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
}

/// Song folders: everything under `Root/<folder>/`, in order of first
/// appearance.
///
/// Folder names are compared without case, as Windows would see them on disk.
/// A file directly under the pack root is pack artwork, not a song, and a
/// `__MACOSX` root holds resource forks that only look like songs.
fn group_folders(entries: &[ZipEntry]) -> Vec<SongFolder> {
    let mut folders: Vec<SongFolder> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (ix, entry) in entries.iter().enumerate() {
        let mut parts = entry
            .name
            .split(['/', '\\'])
            .filter(|part| !part.is_empty());
        let (Some(root), Some(folder)) = (parts.next(), parts.next()) else {
            continue;
        };
        if (parts.next().is_none() && !entry.is_dir()) || root.eq_ignore_ascii_case("__MACOSX") {
            continue;
        }
        let key = format!("{}/{}", root.to_lowercase(), folder.to_lowercase());
        let slot = *by_key.entry(key).or_insert_with(|| {
            folders.push(SongFolder {
                name: folder.to_owned(),
                entries: Vec::new(),
                simfile: None,
                audio: Vec::new(),
            });
            folders.len() - 1
        });
        folders[slot].entries.push(ix);
    }
    for folder in &mut folders {
        let (mut ssc, mut sm) = (None, None);
        for &ix in &folder.entries {
            let entry = &entries[ix];
            if entry.is_dir() {
                continue;
            }
            let mut parts = entry
                .name
                .split(['/', '\\'])
                .filter(|part| !part.is_empty());
            let depth = parts.clone().count();
            let Some(file) = parts.next_back() else {
                continue;
            };
            // AppleDouble companions: `._song.sm` is metadata, not a chart.
            if file.starts_with("._") {
                continue;
            }
            let extension = extension(file);
            if depth == 3 {
                if extension.eq_ignore_ascii_case("ssc") {
                    ssc = ssc.or(Some(ix));
                } else if extension.eq_ignore_ascii_case("sm") {
                    sm = sm.or(Some(ix));
                }
            }
            if AUDIO_EXTENSIONS
                .iter()
                .any(|audio| extension.eq_ignore_ascii_case(audio))
            {
                folder.audio.push(ix);
            }
        }
        folder.simfile = ssc.or(sm);
    }
    folders
}

// ---------------------------------------------------------------------------
// The sparse reader
// ---------------------------------------------------------------------------

/// Where some fetched bytes of the pack are kept.
enum Held<'a> {
    Memory(&'a [u8]),
    /// In a local file, starting at `at`.
    File {
        file: &'a File,
        at: u64,
    },
}

/// A run of the pack this side of the network, at its offset in the file.
struct Piece<'a> {
    start: u64,
    len: u64,
    held: Held<'a>,
}

impl<'a> Piece<'a> {
    fn memory(start: u64, bytes: &'a [u8]) -> Self {
        Self {
            start,
            len: bytes.len() as u64,
            held: Held::Memory(bytes),
        }
    }

    fn contains(&self, pos: u64) -> bool {
        pos >= self.start && pos - self.start < self.len
    }
}

/// A run of the pack still arriving: readable forward only, once.
struct Stream<'a> {
    start: u64,
    end: u64,
    /// The offset of the next byte the body will give.
    next: u64,
    body: &'a mut dyn Read,
}

/// Why a read of the pack could not be answered. Carried inside an
/// `io::Error` through the `zip` crate and recovered on the far side.
#[derive(Debug)]
enum Fault {
    NotFetched(u64),
    Rewind(u64),
    Transfer(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFetched(at) => write!(f, "byte {at} of the pack was not fetched"),
            Self::Rewind(at) => {
                write!(f, "byte {at} of the pack was wanted after it streamed past")
            }
            Self::Transfer(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Fault {}

/// `Read + Seek` over the pack as a whole file, answered from whatever has
/// been fetched: the tail, segments in memory, parts in a temporary file, and
/// at most one range still streaming in. A read of anything else is an error
/// -- the decoder never sees a zero where a byte should be.
struct SparseReader<'a> {
    total: u64,
    pos: u64,
    pieces: Vec<Piece<'a>>,
    stream: Option<Stream<'a>>,
}

impl<'a> SparseReader<'a> {
    /// The index's tail, these segments, and optionally a stream.
    fn new(
        index: &'a PackIndex,
        segments: &'a [(u64, Vec<u8>)],
        stream: Option<Stream<'a>>,
    ) -> Self {
        let mut pieces = Vec::with_capacity(segments.len() + 1);
        pieces.push(Piece::memory(index.tail_start, &index.tail));
        pieces.extend(
            segments
                .iter()
                .map(|(start, bytes)| Piece::memory(*start, bytes)),
        );
        Self {
            total: index.total,
            pos: 0,
            pieces,
            stream,
        }
    }
}

impl Read for SparseReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.total {
            return Ok(0);
        }
        let pos = self.pos;
        if let Some(piece) = self.pieces.iter().find(|piece| piece.contains(pos)) {
            let offset = pos - piece.start;
            let want = usize::try_from((piece.len - offset).min(buf.len() as u64)).unwrap_or(0);
            let read = match &piece.held {
                Held::Memory(bytes) => {
                    let from = usize::try_from(offset).unwrap_or(usize::MAX);
                    let source = bytes
                        .get(from..from.saturating_add(want))
                        .ok_or_else(|| io::Error::other(Fault::NotFetched(pos)))?;
                    buf[..want].copy_from_slice(source);
                    want
                }
                Held::File { file, at } => {
                    let mut file: &File = file;
                    file.seek(SeekFrom::Start(*at + offset))?;
                    match file.read(&mut buf[..want])? {
                        0 => return Err(io::Error::other(Fault::NotFetched(pos))),
                        read => read,
                    }
                }
            };
            self.pos += read as u64;
            return Ok(read);
        }
        if let Some(stream) = self.stream.as_mut()
            && (stream.start..stream.end).contains(&pos)
        {
            if pos < stream.next {
                return Err(io::Error::other(Fault::Rewind(pos)));
            }
            let skip = pos - stream.next;
            if skip > 0 {
                let skipped = io::copy(&mut (&mut *stream.body).take(skip), &mut io::sink())
                    .map_err(|error| io::Error::other(Fault::Transfer(error.to_string())))?;
                if skipped != skip {
                    return Err(io::Error::other(Fault::Transfer(
                        "the transfer ended early".to_owned(),
                    )));
                }
                stream.next = pos;
            }
            let want = usize::try_from((stream.end - pos).min(buf.len() as u64)).unwrap_or(0);
            let read = match stream.body.read(&mut buf[..want]) {
                Ok(0) => {
                    return Err(io::Error::other(Fault::Transfer(
                        "the transfer ended early".to_owned(),
                    )));
                }
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Err(error),
                Err(error) => return Err(io::Error::other(Fault::Transfer(error.to_string()))),
            };
            stream.next += read as u64;
            self.pos += read as u64;
            return Ok(read);
        }
        Err(io::Error::other(Fault::NotFetched(pos)))
    }
}

impl Seek for SparseReader<'_> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let next = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.total.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        self.pos = next
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek outside the pack"))?;
        Ok(self.pos)
    }
}

/// The archive starts at byte zero. Said outright so the crate does not go
/// looking for a prefix in bytes that were never fetched.
fn crate_config() -> Config {
    Config {
        archive_offset: ArchiveOffset::Known(0),
    }
}

fn zip_error(error: ZipError) -> ArchiveError {
    match error {
        ZipError::Io(error) => io_fault(error),
        other => malformed(other.to_string()),
    }
}

/// An error out of the decoder, sorted by whose fault it was.
fn io_fault(error: io::Error) -> ArchiveError {
    if let Some(fault) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Fault>())
    {
        return match fault {
            Fault::Transfer(message) => ArchiveError::Network(message.clone()),
            other => ArchiveError::Io(other.to_string()),
        };
    }
    match error.kind() {
        // Inflate refusing its input, or a CRC that does not match.
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => malformed(error.to_string()),
        _ => ArchiveError::Io(format!("Failed to read the pack: {error}")),
    }
}

fn open_archive<'a>(
    index: &PackIndex,
    reader: SparseReader<'a>,
) -> Result<ZipArchive<SparseReader<'a>>, ArchiveError> {
    let archive = ZipArchive::with_config(crate_config(), reader).map_err(zip_error)?;
    if archive.len() != index.entries.len() {
        return Err(malformed("the central directory changed"));
    }
    Ok(archive)
}

fn entry_at(index: &PackIndex, entry: usize) -> Result<&ZipEntry, ArchiveError> {
    index
        .entries
        .get(entry)
        .ok_or_else(|| malformed(format!("the pack has no entry {entry}")))
}

/// Open entry `entry`, checking the crate agrees about which one that is.
fn open_entry<'r, 'a>(
    archive: &'r mut ZipArchive<SparseReader<'a>>,
    index: &PackIndex,
    entry: usize,
) -> Result<ZipFile<'r, SparseReader<'a>>, ArchiveError> {
    let expected = entry_at(index, entry)?;
    let file = archive.by_index(entry).map_err(zip_error)?;
    if file.name() != expected.name {
        return Err(malformed(format!(
            "entry {entry} is '{}', not '{}'",
            file.name(),
            expected.name
        )));
    }
    Ok(file)
}

fn read_open_entry(
    archive: &mut ZipArchive<SparseReader<'_>>,
    index: &PackIndex,
    entry: usize,
    max_bytes: u64,
) -> Result<Vec<u8>, ArchiveError> {
    let meta = entry_at(index, entry)?;
    if meta.uncompressed > max_bytes {
        return Err(ArchiveError::TooLarge);
    }
    let capacity = usize::try_from(meta.uncompressed).map_err(|_| ArchiveError::TooLarge)?;
    let mut file = open_entry(archive, index, entry)?;
    let mut bytes = Vec::with_capacity(capacity.min(MAX_RESERVE_BYTES));
    // One byte past the size, so an entry that inflates to more than its
    // record says is caught; reading on to the end is what checks the CRC.
    (&mut file)
        .take(meta.uncompressed + 1)
        .read_to_end(&mut bytes)
        .map_err(io_fault)?;
    if bytes.len() != capacity {
        return Err(malformed(format!(
            "'{}' is not the size its record says",
            meta.name
        )));
    }
    Ok(bytes)
}

/// An opened entry poured into `out`, checked to come to exactly `expected`
/// bytes. `on_progress` hears about every chunk and may stop it.
fn pour(
    file: &mut impl Read,
    out: &mut File,
    expected: u64,
    name: &str,
    buffer: &mut [u8],
    on_progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<u64, ArchiveError> {
    let write_failed = io_failure("write a song file");
    let mut written = 0u64;
    loop {
        let read = match file.read(buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_fault(error)),
        };
        written += read as u64;
        if written > expected {
            return Err(malformed(format!(
                "'{name}' is larger than its record says"
            )));
        }
        out.write_all(&buffer[..read]).map_err(&write_failed)?;
        if !on_progress(written, expected) {
            return Err(cancelled());
        }
    }
    if written != expected {
        return Err(malformed(format!(
            "'{name}' is smaller than its record says"
        )));
    }
    out.flush().map_err(&write_failed)?;
    Ok(written)
}

fn fetch_entry_to_file_with(
    transport: &mut dyn Transport,
    index: &PackIndex,
    entry: usize,
    dest: &Path,
    max_bytes: u64,
    on_progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<u64, ArchiveError> {
    let meta = entry_at(index, entry)?;
    if meta.uncompressed > max_bytes || meta.compressed > max_bytes {
        return Err(ArchiveError::TooLarge);
    }
    let (start, end) = meta.span();
    // Whatever the tail already holds is read from it, not fetched again.
    let fetch_end = end.min(index.tail_start);
    // Created before anything is fetched, and only ever removed by this call
    // once this call has created it.
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(io_failure("create the song file"))?;
    let result = (|| {
        let mut buffer = vec![0; COPY_CHUNK_BYTES];
        if start >= fetch_end {
            let reader = SparseReader::new(index, &[], None);
            let mut archive = open_archive(index, reader)?;
            let mut file = open_entry(&mut archive, index, entry)?;
            if !on_progress(0, meta.uncompressed) {
                return Err(cancelled());
            }
            return pour(
                &mut file,
                &mut out,
                meta.uncompressed,
                &meta.name,
                &mut buffer,
                on_progress,
            );
        }
        let reply = transport.get(&range_header(&[(start, fetch_end)]), index.etag.as_deref())?;
        match range_shape(&reply, index.total)? {
            Shape::Single {
                start: got_start,
                end: got_end,
            } if got_start == start && got_end == fetch_end => {}
            _ => return Err(different_range()),
        }
        let mut body = reply.body.take(fetch_end - start);
        let stream = Stream {
            start,
            end: fetch_end,
            next: start,
            body: &mut body,
        };
        let mut archive = open_archive(index, SparseReader::new(index, &[], Some(stream)))?;
        let mut file = open_entry(&mut archive, index, entry)?;
        if !on_progress(0, meta.uncompressed) {
            return Err(cancelled());
        }
        pour(
            &mut file,
            &mut out,
            meta.uncompressed,
            &meta.name,
            &mut buffer,
            on_progress,
        )
    })();
    drop(out);
    if result.is_err()
        && let Err(error) = fs::remove_file(dest)
    {
        log::warn!(
            "Could not remove a partial song file '{}': {error}",
            dest.display()
        );
    }
    result
}

// ---------------------------------------------------------------------------
// Song folders
// ---------------------------------------------------------------------------

/// Everything about installing one folder that can be settled from the
/// directory alone, settled before a byte is downloaded.
struct FolderPlan {
    /// The folder's name as it will be written.
    written: String,
    /// `(entry, path relative to the song folder, is a directory)`.
    outputs: Vec<(usize, PathBuf, bool)>,
    /// What has to come over the network: merged, in order, the tail left out.
    fetch: Vec<(u64, u64)>,
}

fn is_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().trim_end();
    smo::WINDOWS_RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
}

/// The name a song folder is written under.
///
/// Windows quietly drops trailing dots and spaces -- `V.L.S.I.` would land as
/// `V.L.S.I` -- and treats device names as devices, so both are settled here,
/// on every platform, rather than left to surprise whoever reads the folder
/// back by the name this returned.
pub(crate) fn written_folder_name(name: &str) -> Result<String, ArchiveError> {
    let trimmed = name.trim_end_matches([' ', '.']);
    if trimmed.is_empty() {
        return Err(ArchiveError::Unsafe(format!(
            "'{name}' is not a usable folder name"
        )));
    }
    if is_reserved_name(trimmed) {
        return Ok(format!("_{trimmed}"));
    }
    Ok(trimmed.to_owned())
}

fn plan_folder(
    index: &PackIndex,
    folder: usize,
    max_bytes: u64,
) -> Result<FolderPlan, ArchiveError> {
    let song = index
        .folders
        .get(folder)
        .ok_or_else(|| malformed(format!("the pack has no song folder {folder}")))?;
    let written = written_folder_name(&song.name)?;
    let mut outputs = Vec::with_capacity(song.entries.len());
    let mut seen = HashSet::with_capacity(song.entries.len());
    let mut spans = Vec::with_capacity(song.entries.len());
    let mut uncompressed = 0u64;
    let mut has_simfile = false;
    for &ix in &song.entries {
        let entry = entry_at(index, ix)?;
        if entry.name.len() > smo::MAX_ARCHIVE_PATH_BYTES {
            return Err(ArchiveError::Unsafe(format!(
                "'{}' has too long a path",
                entry.name
            )));
        }
        // No `..`, no absolute paths, no drive letters, no characters a
        // Windows file name cannot hold: the installer's own rules.
        smo::portable_archive_parts(&entry.name).map_err(smo_error)?;
        let mut relative = PathBuf::new();
        let mut key = String::with_capacity(entry.name.len());
        let mut file_name = "";
        for part in entry
            .name
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .skip(2)
        {
            if cfg!(windows) && is_reserved_name(part) {
                return Err(ArchiveError::Unsafe(format!(
                    "'{}' uses a name Windows reserves for devices",
                    entry.name
                )));
            }
            relative.push(part);
            if !key.is_empty() {
                key.push('/');
            }
            key.push_str(part);
            file_name = part;
        }
        spans.push(entry.span());
        if relative.as_os_str().is_empty() {
            // The folder's own directory record.
            continue;
        }
        if entry.is_dir() {
            outputs.push((ix, relative, true));
            continue;
        }
        uncompressed = uncompressed
            .checked_add(entry.uncompressed)
            .filter(|&sum| sum <= max_bytes)
            .ok_or(ArchiveError::TooLarge)?;
        // Two names one file system would see as one file.
        if !seen.insert(key.to_lowercase()) {
            return Err(ArchiveError::Unsafe(format!(
                "'{}' duplicates another file",
                entry.name
            )));
        }
        has_simfile |= smo::is_simfile(file_name);
        outputs.push((ix, relative, false));
    }
    if !has_simfile {
        return Err(malformed(format!(
            "'{}' has no .sm, .ssc or .dwi simfile",
            song.name
        )));
    }
    // Contiguous entries -- the usual case -- merge into the folder's one span.
    let (fetch, _) = split_at_tail(&merge_ranges(spans, MERGE_GAP_BYTES), index.tail_start);
    if span_total(&fetch) > max_bytes {
        return Err(ArchiveError::TooLarge);
    }
    Ok(FolderPlan {
        written,
        outputs,
        fetch,
    })
}

fn fetch_folder_with(
    transport: &mut dyn Transport,
    index: &PackIndex,
    folder: usize,
    temp: &Path,
    staging: &Path,
    max_bytes: u64,
    on_progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<String, ArchiveError> {
    let plan = plan_folder(index, folder, max_bytes)?;
    remove_stale_file(temp)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(io_failure("create the song download"))?;
    let result = download_and_extract(
        transport,
        index,
        &plan,
        &file,
        staging,
        max_bytes,
        on_progress,
    );
    drop(file);
    if let Err(error) = fs::remove_file(temp) {
        log::warn!(
            "Could not remove the song download '{}': {error}",
            temp.display()
        );
    }
    result.map(|()| plan.written)
}

/// A temporary file left by a run that crashed is the caller's own, and goes.
/// Anything else at that path -- a folder above all -- is a caller pointing
/// at the wrong place, and is refused rather than deleted.
fn remove_stale_file(path: &Path) -> Result<(), ArchiveError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_failure("inspect the song download")(error)),
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(io_failure("remove a stale song download"))
        }
        Ok(_) => Err(ArchiveError::Io(format!(
            "'{}' is in the way of the song download",
            path.display()
        ))),
    }
}

fn download_and_extract(
    transport: &mut dyn Transport,
    index: &PackIndex,
    plan: &FolderPlan,
    file: &File,
    staging: &Path,
    max_bytes: u64,
    on_progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), ArchiveError> {
    let total = span_total(&plan.fetch);
    if !on_progress(0, total) {
        return Err(cancelled());
    }
    let mut sink = FileSink {
        file,
        at: 0,
        pieces: Vec::with_capacity(plan.fetch.len()),
        done: 0,
        total,
        on_progress,
        buffer: vec![0; COPY_CHUNK_BYTES],
    };
    let mut budget = max_bytes;
    request_ranges(transport, index, &plan.fetch, &mut budget, &mut sink)?;
    let pieces = sink.pieces;
    let have: Vec<(u64, u64)> = pieces
        .iter()
        .map(|&(start, _, len)| (start, start + len))
        .collect();
    if !covers(&have, &plan.fetch) {
        return Err(malformed(
            "the server left out part of a range it was asked for",
        ));
    }
    let mut reader = SparseReader::new(index, &[], None);
    reader
        .pieces
        .extend(pieces.iter().map(|&(start, at, len)| Piece {
            start,
            len,
            held: Held::File { file, at },
        }));
    let mut archive = open_archive(index, reader)?;
    extract_folder(&mut archive, index, plan, staging)
}

fn extract_folder(
    archive: &mut ZipArchive<SparseReader<'_>>,
    index: &PackIndex,
    plan: &FolderPlan,
    staging: &Path,
) -> Result<(), ArchiveError> {
    fs::create_dir_all(staging).map_err(io_failure("create the staging folder"))?;
    let root = staging.join(&plan.written);
    // Not create_dir_all: a folder already there belongs to someone else, and
    // must neither be written into nor cleaned up after.
    fs::create_dir(&root).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            ArchiveError::Io(format!("'{}' is already staged", plan.written))
        } else {
            io_failure("create the song folder")(error)
        }
    })?;
    let mut buffer = vec![0; COPY_CHUNK_BYTES];
    let result = plan.outputs.iter().try_for_each(|(ix, relative, is_dir)| {
        extract_one(
            archive,
            index,
            *ix,
            &root.join(relative),
            *is_dir,
            &mut buffer,
        )
    });
    if result.is_err()
        && let Err(error) = smo::remove_temp_path(&root)
    {
        log::warn!("Could not remove a partly extracted song folder: {error}");
    }
    result
}

fn extract_one(
    archive: &mut ZipArchive<SparseReader<'_>>,
    index: &PackIndex,
    ix: usize,
    output: &Path,
    is_dir: bool,
    buffer: &mut [u8],
) -> Result<(), ArchiveError> {
    let create_dir = io_failure("create a song subfolder");
    if is_dir {
        return fs::create_dir_all(output).map_err(create_dir);
    }
    let entry = entry_at(index, ix)?;
    let mut file = open_entry(archive, index, ix)?;
    // The crate's own view of the same rules, on the record it will extract.
    if file.enclosed_name().is_none() {
        return Err(ArchiveError::Unsafe(format!(
            "'{}' escapes its folder",
            entry.name
        )));
    }
    if file.is_symlink() || !smo::safe_unix_entry_type(file.unix_mode(), false) {
        return Err(ArchiveError::Unsafe(format!(
            "'{}' is not a regular file",
            entry.name
        )));
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(create_dir)?;
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(io_failure("create a song file"))?;
    pour(
        &mut file,
        &mut out,
        entry.uncompressed,
        &entry.name,
        buffer,
        &mut |_: u64, _: u64| true,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Simfile tags and song matching
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SimfileTags {
    pub title: String,
    pub translit: String,
    pub artist: String,
    pub music: String,
    pub preview: String,
}

/// The song's own header tags -- `#TITLE`, `#TITLETRANSLIT`, `#ARTIST`,
/// `#MUSIC`, `#PREVIEW` -- out of a `.sm` or `.ssc`, and nothing else. Cheap:
/// one pass that stops at the first chart, with no simfile parser behind it.
///
/// Read the way StepMania's MSD reader reads them: `//` comments run to the
/// end of the line, wherever they are; `\` escapes the next character; a value
/// ends at `;`, or -- for the many files that forget one -- at a `#` that
/// starts its own line. A value's first parameter is the value, so an
/// unescaped `:` ends it, exactly as the game would truncate it. Text is
/// otherwise kept as written: entities are not decoded, and bytes that are not
/// UTF-8 become U+FFFD. Stopping at the first chart (`#NOTES`, `#NOTEDATA`)
/// also means a chart section's own `#MUSIC` cannot override the song's.
pub(crate) fn simfile_tags(bytes: &[u8]) -> SimfileTags {
    let mut tags = SimfileTags::default();
    let mut at = 0;
    while let Some(start) = next_tag(bytes, at) {
        let (name, value_at) = read_tag_name(bytes, start);
        let name = name.trim_ascii();
        let slot = if name.eq_ignore_ascii_case(b"TITLE") {
            Some(&mut tags.title)
        } else if name.eq_ignore_ascii_case(b"TITLETRANSLIT") {
            Some(&mut tags.translit)
        } else if name.eq_ignore_ascii_case(b"ARTIST") {
            Some(&mut tags.artist)
        } else if name.eq_ignore_ascii_case(b"MUSIC") {
            Some(&mut tags.music)
        } else if name.eq_ignore_ascii_case(b"PREVIEW") {
            Some(&mut tags.preview)
        } else if name.eq_ignore_ascii_case(b"NOTES")
            || name.eq_ignore_ascii_case(b"NOTES2")
            || name.eq_ignore_ascii_case(b"NOTEDATA")
        {
            break;
        } else {
            None
        };
        let Some(value_at) = value_at else {
            at = start + 1;
            continue;
        };
        let (value, next) = read_tag_value(bytes, value_at, slot.is_some());
        if let Some(slot) = slot {
            *slot = String::from_utf8_lossy(value.trim_ascii()).into_owned();
        }
        at = next;
    }
    tags
}

/// The next `#` from `at` that is not inside a comment.
fn next_tag(bytes: &[u8], mut at: usize) -> Option<usize> {
    while at < bytes.len() {
        match bytes[at] {
            b'#' => return Some(at),
            b'/' if bytes.get(at + 1) == Some(&b'/') => at = line_end(bytes, at),
            _ => at += 1,
        }
    }
    None
}

/// The newline that ends the line `at` is on, or the end of the text.
fn line_end(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(bytes.len(), |offset| at + offset)
}

/// The name of the tag whose `#` is at `start`, and where its value starts --
/// `None` for a tag with no `:`, which has no value to read.
fn read_tag_name(bytes: &[u8], start: usize) -> (&[u8], Option<usize>) {
    let from = start + 1;
    let len = bytes[from..]
        .iter()
        .position(|byte| matches!(byte, b':' | b';' | b'#' | b'\n'))
        .unwrap_or(bytes.len() - from);
    let name = &bytes[from..from + len];
    let value = (bytes.get(from + len) == Some(&b':')).then_some(from + len + 1);
    (name, value)
}

/// A tag's value from `at`: its first parameter (kept only if `keep`), and
/// where scanning resumes.
fn read_tag_value(bytes: &[u8], mut at: usize, keep: bool) -> (Vec<u8>, usize) {
    let mut value = Vec::new();
    let mut first_param = keep;
    // Only whitespace since the last line break. The value starts mid-line.
    let mut line_blank = false;
    while at < bytes.len() {
        let byte = bytes[at];
        match byte {
            b';' => return (value, at + 1),
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = line_end(bytes, at);
                continue;
            }
            // A tag on a line of its own: the one before it never closed.
            b'#' if line_blank => return (value, at),
            b'\\' if at + 1 < bytes.len() => {
                if first_param {
                    value.push(bytes[at + 1]);
                }
                line_blank = false;
                at += 2;
                continue;
            }
            b':' => first_param = false,
            _ if first_param => value.push(byte),
            _ => {}
        }
        match byte {
            b'\n' | b'\r' => line_blank = true,
            b' ' | b'\t' => {}
            _ => line_blank = false,
        }
        at += 1;
    }
    (value, at)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SongMatch {
    Folder(usize),
    Ambiguous,
    NotFound,
}

/// What two song names are compared by: lowercase letters and digits, the
/// rest dropped -- so `V.L.S.I` and `vlsi` agree, and so do a page's title
/// and a folder name that differs only in punctuation. A name that is all
/// punctuation keeps it, rather than comparing equal to every other one.
fn match_key(text: &str) -> String {
    let key: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric())
        .collect();
    if key.is_empty() {
        text.trim().to_lowercase()
    } else {
        key
    }
}

/// The shortest containment worth believing.
const MIN_CONTAINED_CHARS: usize = 4;

/// How strongly `a` and `b` share a name: the length of whichever contains the
/// other, if it is long enough to mean something.
fn containment(a: &str, b: &str) -> Option<usize> {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let chars = short.chars().count();
    (chars >= MIN_CONTAINED_CHARS && long.contains(short)).then_some(chars)
}

/// The candidates with the best score, if any scored.
fn best_scored(scored: impl Iterator<Item = (usize, usize)>) -> Vec<usize> {
    let mut best = 0;
    let mut out = Vec::new();
    for (folder, score) in scored {
        if score > best {
            best = score;
            out.clear();
        }
        if score == best {
            out.push(folder);
        }
    }
    out
}

/// One candidate is an answer. Several are settled by the artist when exactly
/// one of them has it, and are otherwise ambiguous: equals are never guessed
/// between. None is no answer yet.
fn settle(
    candidates: &[usize],
    artist: &str,
    tags: &impl Fn(usize) -> Option<SimfileTags>,
) -> Option<SongMatch> {
    match candidates {
        [] => None,
        [only] => Some(SongMatch::Folder(*only)),
        _ => {
            let by_artist: Vec<usize> = if artist.is_empty() {
                Vec::new()
            } else {
                candidates
                    .iter()
                    .copied()
                    .filter(|&folder| {
                        tags(folder).is_some_and(|found| match_key(&found.artist) == artist)
                    })
                    .collect()
            };
            Some(match by_artist[..] {
                [only] => SongMatch::Folder(only),
                _ => SongMatch::Ambiguous,
            })
        }
    }
}

/// The song folder a pack page's song title names.
///
/// Names are compared by [`match_key`] -- lowercase letters and digits only.
/// In order, the first rule that names one folder wins:
///
/// 1. the folder's own name equals the title;
/// 2. the folder's `#TITLE` or `#TITLETRANSLIT` equals it, from
///    `tags(folder)` -- `None` for a simfile not fetched;
/// 3. the folder's name contains the title or the title contains it, at least
///    four characters' worth, the longest such;
/// 4. the same containment against `#TITLE` and `#TITLETRANSLIT`. ITL-style
///    packs title a song `[1000] [07] Long Time` in the simfile and keep it in
///    `[07] Long Time (SN) [Feraligatr]`, and a page may add the subtitle on.
///
/// Wherever several folders tie, the one whose `#ARTIST` matches `artist`
/// wins if it is the only one; otherwise the answer is
/// [`SongMatch::Ambiguous`]. Equals are never guessed between. And when
/// several folders carry the title's exact name, nothing weaker than their own
/// tags decides between them -- a containment elsewhere in the pack is not
/// evidence about which of them is meant.
///
/// `artist` is given only for a title the pack's page lists more than once.
/// Then a folder's name is not enough -- `Butterfly` may be the other artist's
/// `Butterfly` -- so before any rule, every folder carrying the title by name
/// or by `#TITLE` is put to the artist, and exactly one with it is the answer.
/// None with it leaves the rules to decide as for any title.
///
/// With `|_| None` only rules 1 and 3 can apply. A rule-1 answer is final
/// either way; anything else is firmer once the simfiles
/// [`folders_needing_tags`] names have been fetched and passed in.
pub(crate) fn match_song(
    index: &PackIndex,
    title: &str,
    artist: &str,
    tags: impl Fn(usize) -> Option<SimfileTags>,
) -> SongMatch {
    let want = match_key(title);
    if want.is_empty() {
        return SongMatch::NotFound;
    }
    let artist = match_key(artist);
    let names: Vec<String> = index
        .folders
        .iter()
        .map(|folder| match_key(&folder.name))
        .collect();
    let named: Vec<usize> = (0..names.len()).filter(|&ix| names[ix] == want).collect();
    let titled_as = |folder: usize| {
        tags(folder).is_some_and(|tags| {
            match_key(&tags.title) == want
                || !tags.translit.is_empty() && match_key(&tags.translit) == want
        })
    };
    if !artist.is_empty() {
        let by_artist: Vec<usize> = (0..names.len())
            .filter(|&ix| names[ix] == want || titled_as(ix))
            .filter(|&ix| tags(ix).is_some_and(|found| match_key(&found.artist) == artist))
            .collect();
        match by_artist[..] {
            [only] => return SongMatch::Folder(only),
            [] => {}
            _ => return SongMatch::Ambiguous,
        }
    }
    if let [only] = named[..] {
        return SongMatch::Folder(only);
    }
    if !named.is_empty() {
        let titled: Vec<usize> = named.iter().copied().filter(|&ix| titled_as(ix)).collect();
        return settle(&titled, &artist, &tags)
            .or_else(|| settle(&named, &artist, &tags))
            .unwrap_or(SongMatch::Ambiguous);
    }
    let titled: Vec<usize> = (0..names.len()).filter(|&ix| titled_as(ix)).collect();
    if let Some(found) = settle(&titled, &artist, &tags) {
        return found;
    }
    let by_folder = best_scored(
        names
            .iter()
            .enumerate()
            .filter_map(|(ix, name)| Some((ix, containment(name, &want)?))),
    );
    if let Some(found) = settle(&by_folder, &artist, &tags) {
        return found;
    }
    let by_title = best_scored((0..names.len()).filter_map(|ix| {
        let tags = tags(ix)?;
        let title = containment(&match_key(&tags.title), &want);
        let translit = (!tags.translit.is_empty())
            .then(|| containment(&match_key(&tags.translit), &want))
            .flatten();
        Some((ix, title.max(translit)?))
    }));
    settle(&by_title, &artist, &tags).unwrap_or(SongMatch::NotFound)
}

/// The folders whose simfile tags [`match_song`] could use for this title.
///
/// None when a folder name settles it; the folders named for it when several
/// are; otherwise -- and always when an `artist` is given -- every folder with
/// a simfile. Fetch those simfiles' ranges in one [`fetch_ranges`] call and
/// match again.
pub(crate) fn folders_needing_tags(index: &PackIndex, title: &str, artist: &str) -> Vec<usize> {
    let with_simfile = || {
        index
            .folders
            .iter()
            .enumerate()
            .filter(|(_, folder)| folder.simfile.is_some())
            .map(|(ix, _)| ix)
            .collect()
    };
    if !match_key(artist).is_empty() {
        return with_simfile();
    }
    let want = match_key(title);
    let named: Vec<usize> = index
        .folders
        .iter()
        .enumerate()
        .filter(|(_, folder)| match_key(&folder.name) == want)
        .map(|(ix, _)| ix)
        .collect();
    match named.len() {
        1 => Vec::new(),
        0 => with_simfile(),
        _ => named
            .into_iter()
            .filter(|&ix| index.folders[ix].simfile.is_some())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    const MAX: u64 = 64 * 1024 * 1024;
    const ETAG: &str = "\"pack-v1\"";
    const BOUNDARY: &str = "3d6b6a416f9b5";
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    fn temp_dir(label: &str) -> PathBuf {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "deadsync-pack-archive-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create fixture root");
        dir
    }

    /// Bytes deflate cannot shrink, so stored fillers stay the size asked for.
    fn noise(len: usize, seed: u32) -> Vec<u8> {
        let mut state = seed.wrapping_mul(2_654_435_761) | 1;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state.to_le_bytes()[0]
            })
            .collect()
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 0 {
                    crc >> 1
                } else {
                    (crc >> 1) ^ 0xEDB8_8320
                };
            }
        }
        !crc
    }

    enum Item<'a> {
        Dir(&'a str),
        Stored(&'a str, &'a [u8]),
        Deflated(&'a str, &'a [u8]),
        Symlink(&'a str, &'a str),
    }

    fn build_zip(items: &[Item<'_>]) -> Vec<u8> {
        build_zip_with(items, |_| {})
    }

    fn build_zip_with(
        items: &[Item<'_>],
        finish: impl FnOnce(&mut ZipWriter<Cursor<Vec<u8>>>),
    ) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for item in items {
            match item {
                Item::Dir(name) => zip
                    .add_directory(*name, SimpleFileOptions::default())
                    .expect("add directory"),
                Item::Stored(name, bytes) | Item::Deflated(name, bytes) => {
                    let method = if matches!(item, Item::Stored(..)) {
                        CompressionMethod::Stored
                    } else {
                        CompressionMethod::Deflated
                    };
                    let options = SimpleFileOptions::default().compression_method(method);
                    zip.start_file(*name, options).expect("start entry");
                    zip.write_all(bytes).expect("write entry");
                }
                Item::Symlink(name, target) => zip
                    .add_symlink(*name, *target, SimpleFileOptions::default())
                    .expect("add symlink"),
            }
        }
        finish(&mut zip);
        zip.finish().expect("finish zip").into_inner()
    }

    /// A stored-only zip written by hand, for shapes `ZipWriter` will not
    /// make: every size and offset behind a zip64 field, or a repeated name.
    fn hand_zip(entries: &[(&str, &[u8])], zip64: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let offset = out.len() as u64;
            let size = data.len() as u64;
            let crc = crc32(data);
            let (size32, offset32) = if zip64 {
                (u32::MAX, u32::MAX)
            } else {
                (size as u32, offset as u32)
            };
            let mut local_extra = Vec::new();
            let mut central_extra = Vec::new();
            if zip64 {
                local_extra.extend_from_slice(&ZIP64_EXTRA_ID.to_le_bytes());
                local_extra.extend_from_slice(&16u16.to_le_bytes());
                local_extra.extend_from_slice(&size.to_le_bytes());
                local_extra.extend_from_slice(&size.to_le_bytes());
                central_extra.extend_from_slice(&ZIP64_EXTRA_ID.to_le_bytes());
                central_extra.extend_from_slice(&24u16.to_le_bytes());
                central_extra.extend_from_slice(&size.to_le_bytes());
                central_extra.extend_from_slice(&size.to_le_bytes());
                central_extra.extend_from_slice(&offset.to_le_bytes());
            }
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x21u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size32.to_le_bytes());
            out.extend_from_slice(&size32.to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(local_extra.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&local_extra);
            out.extend_from_slice(data);

            central.extend_from_slice(&CENTRAL_HEADER_SIGNATURE.to_le_bytes());
            central.extend_from_slice(&45u16.to_le_bytes());
            central.extend_from_slice(&45u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0x21u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&size32.to_le_bytes());
            central.extend_from_slice(&size32.to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&(central_extra.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&offset32.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&central_extra);
        }
        let start = out.len() as u64;
        let size = central.len() as u64;
        let count = entries.len() as u64;
        out.extend_from_slice(&central);
        if zip64 {
            let position = out.len() as u64;
            out.extend_from_slice(&EOCD64_SIGNATURE.to_le_bytes());
            out.extend_from_slice(&44u64.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&start.to_le_bytes());
            out.extend_from_slice(&EOCD64_LOCATOR_SIGNATURE.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&position.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
        }
        let (count16, size32, start32) = if zip64 {
            (u16::MAX, u32::MAX, u32::MAX)
        } else {
            (count as u16, size as u32, start as u32)
        };
        out.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count16.to_le_bytes());
        out.extend_from_slice(&count16.to_le_bytes());
        out.extend_from_slice(&size32.to_le_bytes());
        out.extend_from_slice(&start32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn reply(
        status: u16,
        content_type: &str,
        content_range: Option<String>,
        body: Vec<u8>,
    ) -> RangeReply {
        RangeReply {
            status,
            content_type: Some(content_type.to_owned()),
            content_range,
            content_length: Some(body.len() as u64),
            etag: Some(ETAG.to_owned()),
            body: Box::new(Cursor::new(body)),
        }
    }

    /// The download URL, played from memory: suffix, single and multi-range
    /// requests, If-Range, and a log of every request it was sent.
    struct FakeServer {
        bytes: Vec<u8>,
        etag: Option<String>,
        ignore_ranges: bool,
        requests: Vec<(String, Option<String>)>,
    }

    impl FakeServer {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                etag: Some(ETAG.to_owned()),
                ignore_ranges: false,
                requests: Vec::new(),
            }
        }
    }

    impl Transport for FakeServer {
        fn get(&mut self, range: &str, if_range: Option<&str>) -> Result<RangeReply, ArchiveError> {
            self.requests
                .push((range.to_owned(), if_range.map(str::to_owned)));
            let total = self.bytes.len() as u64;
            let stale = if_range.is_some_and(|tag| Some(tag) != self.etag.as_deref());
            if self.ignore_ranges || stale {
                let mut whole = reply(200, "application/zip", None, self.bytes.clone());
                whole.etag = self.etag.clone();
                return Ok(whole);
            }
            let spec = range.strip_prefix("bytes=").expect("a byte range");
            let ranges: Vec<(u64, u64)> = spec
                .split(',')
                .map(|part| {
                    let (first, last) = part.split_once('-').expect("a range");
                    if first.is_empty() {
                        (total.saturating_sub(last.parse().expect("suffix")), total)
                    } else {
                        let last: u64 = last.parse().expect("last byte");
                        (first.parse().expect("first byte"), (last + 1).min(total))
                    }
                })
                .collect();
            let mut answer = if let [only] = ranges[..] {
                let content_range = format!("bytes {}-{}/{total}", only.0, only.1 - 1);
                reply(
                    206,
                    "application/zip",
                    Some(content_range),
                    slice(&self.bytes, only),
                )
            } else {
                let mut body = Vec::new();
                for &part in &ranges {
                    write!(
                        body,
                        "\r\n--{BOUNDARY}\r\nContent-Type: application/zip\r\n\
                         Content-Range: bytes {}-{}/{total}\r\n\r\n",
                        part.0,
                        part.1 - 1
                    )
                    .expect("write part header");
                    body.extend_from_slice(&slice(&self.bytes, part));
                }
                write!(body, "\r\n--{BOUNDARY}--\r\n").expect("write closing delimiter");
                let kind = format!("multipart/byteranges; boundary={BOUNDARY}");
                reply(206, &kind, None, body)
            };
            answer.etag = self.etag.clone();
            Ok(answer)
        }
    }

    /// A body that must never be read: the whole pack, sent where a range was
    /// asked for.
    struct Untouchable;

    impl Read for Untouchable {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("a body that should have been dropped unread was read");
        }
    }

    /// Replies handed out in order, whatever was asked.
    struct Scripted(Vec<RangeReply>);

    impl Transport for Scripted {
        fn get(&mut self, _: &str, _: Option<&str>) -> Result<RangeReply, ArchiveError> {
            Ok(self.0.remove(0))
        }
    }

    fn whole_file_reply(len: u64) -> RangeReply {
        RangeReply {
            status: 200,
            content_type: Some("application/zip".to_owned()),
            content_range: None,
            content_length: Some(len),
            etag: Some(ETAG.to_owned()),
            body: Box::new(Untouchable),
        }
    }

    fn multipart_reply(content_type: &str, body: &[u8]) -> RangeReply {
        RangeReply {
            status: 206,
            content_type: Some(content_type.to_owned()),
            content_range: None,
            content_length: None,
            etag: None,
            body: Box::new(Cursor::new(body.to_vec())),
        }
    }

    fn receive_all(
        reply: RangeReply,
        total: u64,
        budget: &mut u64,
    ) -> Result<Vec<(u64, Vec<u8>)>, ArchiveError> {
        let mut segments = Vec::new();
        receive(
            reply,
            total,
            budget,
            &mut MemorySink {
                segments: &mut segments,
            },
        )?;
        Ok(segments)
    }

    fn slice(bytes: &[u8], (start, end): (u64, u64)) -> Vec<u8> {
        bytes[start as usize..end as usize].to_vec()
    }

    fn accept_all() -> impl FnMut(u64, u64) -> bool {
        |_, _| true
    }

    const SM_TEXT: &[u8] = b"#TITLE:Alpha;\n#ARTIST:Somebody;\n#MUSIC:alpha.ogg;\n#NOTES:\n";

    #[test]
    fn tail_request_is_sized_to_the_pack() {
        assert_eq!(tail_request_len(0), 64 * 1024);
        assert_eq!(tail_request_len(40), 64 * 1024);
        assert_eq!(tail_request_len(100), 100 * 1536 + 4096);
        assert_eq!(tail_request_len(1_000), 1_000 * 1536 + 4096);
        assert_eq!(tail_request_len(5_000), 2 * 1024 * 1024);
        assert_eq!(tail_request_len(u32::MAX), 2 * 1024 * 1024);
    }

    #[test]
    fn index_lists_entries_and_song_folders_in_directory_order() {
        let ogg = noise(5_000, 1);
        let mp3 = noise(3_000, 2);
        let ssc = b"#VERSION:0.83;\n#TITLE:Alpha;\n#NOTEDATA:;\n".repeat(40);
        let items = [
            Item::Dir("Pack/"),
            Item::Stored("Pack/banner.png", b"png"),
            Item::Dir("Pack/Alpha/"),
            Item::Deflated("Pack/Alpha/alpha.sm", SM_TEXT),
            Item::Deflated("Pack/Alpha/alpha.ssc", &ssc),
            Item::Stored("Pack/Alpha/alpha.ogg", &ogg),
            Item::Deflated("Pack/Bêta Ünïcode/bêta.sm", SM_TEXT),
            Item::Stored("Pack/Bêta Ünïcode/sub/bêta.mp3", &mp3),
            Item::Stored("Pack/Gamma/._gamma.sm", b"apple double"),
            Item::Deflated("Pack/Gamma/gamma.sm", SM_TEXT),
        ];
        let bytes = build_zip(&items);
        let mut server = FakeServer::new(bytes.clone());
        let index = fetch_index_with(&mut server, 7, 3).expect("index");

        // One request, the suffix, with nothing yet to hold the server to.
        assert_eq!(server.requests, vec![("bytes=-65536".to_owned(), None)]);
        assert_eq!(index.pack_id, 7);
        assert_eq!(index.total, bytes.len() as u64);
        assert_eq!(index.etag.as_deref(), Some(ETAG));
        assert_eq!(index.tail_start, 0);

        let mut reference = ZipArchive::new(Cursor::new(bytes.clone())).expect("reference");
        let directory_start = reference.central_directory_start();
        assert_eq!(index.entries.len(), reference.len());
        for (ix, entry) in index.entries.iter().enumerate() {
            let next = index
                .entries
                .get(ix + 1)
                .map_or(directory_start, |next| next.local_header);
            assert_eq!(entry.end, next, "{}", entry.name);
            let mut file = reference.by_index(ix).expect("reference entry");
            assert_eq!(entry.name, file.name());
            assert_eq!(entry.local_header, file.header_start());
            assert_eq!(entry.compressed, file.compressed_size());
            assert_eq!(entry.uncompressed, file.size());
            let mut expected = Vec::new();
            file.read_to_end(&mut expected).expect("reference bytes");
            // Everything is in the tail of a pack this small.
            assert_eq!(read_entry(&index, &[], ix, MAX).expect("entry"), expected);
        }

        let name = |ix: usize| index.entries[ix].name.as_str();
        let folders: Vec<(&str, Option<&str>, Vec<&str>, usize)> = index
            .folders
            .iter()
            .map(|folder| {
                (
                    folder.name.as_str(),
                    folder.simfile.map(name),
                    folder.audio.iter().map(|&ix| name(ix)).collect(),
                    folder.entries.len(),
                )
            })
            .collect();
        assert_eq!(
            folders,
            vec![
                (
                    "Alpha",
                    Some("Pack/Alpha/alpha.ssc"),
                    vec!["Pack/Alpha/alpha.ogg"],
                    4
                ),
                (
                    "Bêta Ünïcode",
                    Some("Pack/Bêta Ünïcode/bêta.sm"),
                    vec!["Pack/Bêta Ünïcode/sub/bêta.mp3"],
                    2
                ),
                ("Gamma", Some("Pack/Gamma/gamma.sm"), Vec::new(), 2),
            ]
        );
    }

    /// A pack whose first song sits well before the tail, and that song's
    /// chart.
    fn front_loaded() -> (Vec<u8>, Vec<u8>) {
        let chart = b"#TITLE:Far Away;\n#MUSIC:song.ogg;\n".repeat(200);
        let bytes = build_zip(&[
            Item::Deflated("Pack/Song/song.ssc", &chart),
            Item::Stored("Pack/Song/song.ogg", &noise(40_000, 3)),
            Item::Stored("Pack/Filler/filler.bin", &noise(200_000, 4)),
            Item::Deflated("Pack/Filler/filler.sm", b"#TITLE:Filler;"),
        ]);
        (bytes, chart)
    }

    #[test]
    fn entries_decode_from_exactly_their_own_bytes() {
        let (bytes, chart) = front_loaded();
        let mut server = FakeServer::new(bytes.clone());
        let index = fetch_index_with(&mut server, 1, 2).expect("index");
        let simfile = index.folders[0].simfile.expect("simfile");
        let audio = index.folders[0].audio[0];
        assert!(index.tail_start > index.entries[audio].end);

        // Nothing fetched: the bytes are not there, and it says so.
        let error = read_entry(&index, &[], simfile, MAX).unwrap_err();
        assert!(
            matches!(&error, ArchiveError::Io(message) if message.contains("not fetched")),
            "{error}"
        );
        // Exactly the entry's own bytes are enough.
        let span = index.entries[simfile].span();
        let exact = vec![(span.0, slice(&bytes, span))];
        assert_eq!(
            read_entry(&index, &exact, simfile, MAX).expect("entry"),
            chart
        );
        // One byte short is not.
        let short = vec![(span.0, slice(&bytes, (span.0, span.1 - 1)))];
        assert!(read_entry(&index, &short, simfile, MAX).is_err());
        // Nor is the caller's bound.
        assert!(matches!(
            read_entry(&index, &exact, simfile, 100),
            Err(ArchiveError::TooLarge)
        ));
        // A damaged byte fails the CRC rather than coming back as data.
        let audio_span = index.entries[audio].span();
        let mut damaged = slice(&bytes, audio_span);
        let last = damaged.len() - 1;
        damaged[last] ^= 0xFF;
        let error = read_entry(&index, &[(audio_span.0, damaged)], audio, MAX).unwrap_err();
        assert!(matches!(error, ArchiveError::Malformed(_)), "{error}");

        // Neighbouring entries come in one request, held to the validator.
        server.requests.clear();
        let segments =
            fetch_ranges_with(&mut server, &index, &[span, audio_span], MAX).expect("ranges");
        assert_eq!(
            server.requests,
            vec![(
                format!("bytes={}-{}", span.0, audio_span.1 - 1),
                Some(ETAG.to_owned())
            )]
        );
        let mut read = Vec::new();
        read_entries(&index, &segments, &[simfile, audio], MAX, |entry, bytes| {
            read.push((entry, bytes));
        })
        .expect("archive");
        assert_eq!(read[0].0, simfile);
        assert_eq!(read[0].1.as_ref().expect("simfile"), &chart);
        assert_eq!(read[1].0, audio);
        assert_eq!(read[1].1.as_ref().expect("audio"), &noise(40_000, 3));

        // An entry the tail holds costs no request at all.
        server.requests.clear();
        let filler = index.folders[1].simfile.expect("filler simfile");
        let held = fetch_ranges_with(&mut server, &index, &[index.entries[filler].span()], MAX)
            .expect("held ranges");
        assert!(server.requests.is_empty());
        assert_eq!(
            read_entry(&index, &held, filler, MAX).expect("filler"),
            b"#TITLE:Filler;"
        );
    }

    #[test]
    fn a_central_directory_longer_than_the_tail_costs_one_more_request() {
        let names: Vec<String> = (0..1_200)
            .map(|ix| {
                format!(
                    "Pack/Folder {:02} with a long name/file {ix:04} padded out somewhat.sm",
                    ix % 40
                )
            })
            .collect();
        let items: Vec<Item<'_>> = names
            .iter()
            .map(|name| Item::Stored(name, b"#TITLE:x;"))
            .collect();
        let bytes = build_zip(&items);
        let mut server = FakeServer::new(bytes.clone());
        let index = fetch_index_with(&mut server, 2, 1).expect("index");

        let reference = ZipArchive::new(Cursor::new(bytes.clone())).expect("reference");
        let directory_start = reference.central_directory_start();
        let tail_start = bytes.len() as u64 - 64 * 1024;
        assert!(directory_start < tail_start);
        assert_eq!(server.requests.len(), 2);
        assert_eq!(
            server.requests[1],
            (
                format!("bytes={directory_start}-{}", tail_start - 1),
                Some(ETAG.to_owned())
            )
        );
        assert_eq!(index.tail_start, directory_start);
        assert_eq!(index.entries.len(), 1_200);
        assert_eq!(index.folders.len(), 40);
        assert!(
            index
                .folders
                .iter()
                .all(|folder| folder.entries.len() == 30)
        );
    }

    #[test]
    fn zip64_records_are_read_the_way_the_crate_reads_them() {
        // The writer's zip64: an end record and its locator.
        let chart = b"#TITLE:Large;\n".repeat(50);
        let audio = noise(2_000, 5);
        let bytes = build_zip_with(
            &[
                Item::Deflated("Pack/Song/song.sm", &chart),
                Item::Stored("Pack/Song/song.ogg", &audio),
            ],
            |zip| zip.set_raw_zip64_extensible_data_sector(Vec::new().into_boxed_slice()),
        );
        let signature = EOCD64_SIGNATURE.to_le_bytes();
        assert!(bytes.windows(4).any(|window| window == signature));
        let index = fetch_index_with(&mut FakeServer::new(bytes), 3, 1).expect("zip64 index");
        assert_eq!(read_entry(&index, &[], 0, MAX).expect("simfile"), chart);
        assert_eq!(read_entry(&index, &[], 1, MAX).expect("audio"), audio);

        // By hand: every size and offset behind 0xFFFFFFFF.
        let audio = noise(1_000, 6);
        let bytes = hand_zip(
            &[
                ("Pack/Song/song.sm", b"#TITLE:By Hand;"),
                ("Pack/Song/song.ogg", &audio),
            ],
            true,
        );
        let mut reference = ZipArchive::new(Cursor::new(bytes.clone())).expect("reference");
        let index = fetch_index_with(&mut FakeServer::new(bytes), 3, 1).expect("hand zip64");
        for (ix, entry) in index.entries.iter().enumerate() {
            let file = reference.by_index(ix).expect("reference entry");
            assert_eq!(entry.local_header, file.header_start());
            assert_eq!(entry.compressed, file.compressed_size());
            assert_eq!(entry.uncompressed, file.size());
        }
        assert_eq!(index.entries[1].local_header, 30 + 17 + 20 + 15);
        assert_eq!(
            read_entry(&index, &[], 0, MAX).expect("simfile"),
            b"#TITLE:By Hand;"
        );
        assert_eq!(read_entry(&index, &[], 1, MAX).expect("audio"), audio);
    }

    #[test]
    fn a_repeated_name_keeps_its_first_place_and_its_last_record() {
        let bytes = hand_zip(
            &[
                ("Pack/Song/song.sm", b"first"),
                ("Pack/Song/song.sm", b"second"),
                ("Pack/Song/song.ogg", b"ogg"),
            ],
            false,
        );
        let reference = ZipArchive::new(Cursor::new(bytes.clone())).expect("reference");
        assert_eq!(reference.len(), 2);
        let index = fetch_index_with(&mut FakeServer::new(bytes), 4, 1).expect("index");
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].name, "Pack/Song/song.sm");
        assert_eq!(index.entries[0].end, index.entries[1].local_header);
        assert_eq!(read_entry(&index, &[], 0, MAX).expect("simfile"), b"second");
        assert_eq!(read_entry(&index, &[], 1, MAX).expect("audio"), b"ogg");
        assert_eq!(index.folders[0].entries, vec![0, 1]);
    }

    fn flat_index(bytes: &[u8], held: usize) -> PackIndex {
        let total = bytes.len() as u64;
        PackIndex {
            pack_id: 9,
            total,
            etag: Some(ETAG.to_owned()),
            entries: Vec::new(),
            folders: Vec::new(),
            tail_start: total - held as u64,
            tail: Arc::from(&bytes[bytes.len() - held..]),
        }
    }

    #[test]
    fn ranges_merge_split_at_the_tail_and_batch() {
        let bytes = noise(24 * 1024 * 1024, 7);
        let total = bytes.len() as u64;
        let index = flat_index(&bytes, 16);
        let mut server = FakeServer::new(bytes.clone());

        // Close neighbours merge; a range reaching into the tail is cut at it.
        let wanted = [(300, 400), (100, 200), (total - 20, total)];
        let segments = fetch_ranges_with(&mut server, &index, &wanted, MAX).expect("ranges");
        assert_eq!(
            server.requests,
            vec![(
                format!("bytes=100-399,{}-{}", total - 20, total - 17),
                Some(ETAG.to_owned())
            )]
        );
        let shape: Vec<(u64, usize)> = segments
            .iter()
            .map(|(start, data)| (*start, data.len()))
            .collect();
        assert_eq!(shape, vec![(100, 300), (total - 20, 4), (total - 16, 16)]);
        for (start, data) in &segments {
            assert_eq!(
                data[..],
                bytes[*start as usize..*start as usize + data.len()]
            );
        }

        // Two hundred and fifty ranges too far apart to merge: two hundred,
        // then fifty.
        server.requests.clear();
        let ranges: Vec<(u64, u64)> = (0..250).map(|ix| (ix * 80_000, ix * 80_000 + 10)).collect();
        let segments = fetch_ranges_with(&mut server, &index, &ranges, MAX).expect("batched");
        assert_eq!(server.requests.len(), 2);
        assert_eq!(server.requests[0].0.matches(',').count(), 199);
        assert_eq!(server.requests[1].0.matches(',').count(), 49);
        assert_eq!(segments.len(), 250);
        assert!(
            segments
                .iter()
                .all(|(start, data)| data[..] == bytes[*start as usize..*start as usize + 10])
        );

        // Refused before anything is sent.
        server.requests.clear();
        assert!(
            fetch_ranges_with(&mut server, &index, &[], MAX)
                .expect("nothing")
                .is_empty()
        );
        assert!(matches!(
            fetch_ranges_with(&mut server, &index, &[(0, 1_000)], 999),
            Err(ArchiveError::TooLarge)
        ));
        assert!(matches!(
            fetch_ranges_with(&mut server, &index, &[(10, 10)], MAX),
            Err(ArchiveError::Malformed(_))
        ));
        assert!(matches!(
            fetch_ranges_with(&mut server, &index, &[(0, total + 1)], MAX),
            Err(ArchiveError::Malformed(_))
        ));
        assert!(server.requests.is_empty());
    }

    #[test]
    fn a_changed_pack_or_an_ignored_range_is_reported_not_read() {
        let bytes = noise(300_000, 8);
        let index = flat_index(&bytes, 16);

        // Replaced since the index: If-Range gets the whole file back.
        let mut server = FakeServer::new(bytes.clone());
        server.etag = Some("\"pack-v2\"".to_owned());
        assert!(matches!(
            fetch_ranges_with(&mut server, &index, &[(100, 200)], MAX),
            Err(ArchiveError::Changed)
        ));
        // A different size under the same validator.
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(matches!(
            fetch_ranges_with(&mut FakeServer::new(longer), &index, &[(100, 200)], MAX),
            Err(ArchiveError::Changed)
        ));
        // The whole file where a range was asked for is never read.
        let mut scripted = Scripted(vec![whole_file_reply(bytes.len() as u64)]);
        assert!(matches!(
            fetch_ranges_with(&mut scripted, &index, &[(100, 200)], MAX),
            Err(ArchiveError::Changed)
        ));
        let mut scripted = Scripted(vec![whole_file_reply(bytes.len() as u64)]);
        assert!(matches!(
            fetch_index_with(&mut scripted, 1, 1),
            Err(ArchiveError::RangeUnsupported)
        ));

        // A pack no bigger than the tail asked for may come back whole.
        let small = build_zip(&[Item::Stored("Pack/Song/song.sm", b"#TITLE:Tiny;")]);
        let mut server = FakeServer::new(small.clone());
        server.ignore_ranges = true;
        let index = fetch_index_with(&mut server, 1, 1).expect("small index");
        assert_eq!((index.tail_start, index.total), (0, small.len() as u64));
        assert_eq!(
            read_entry(&index, &[], 0, MAX).expect("simfile"),
            b"#TITLE:Tiny;"
        );
    }

    #[test]
    fn content_ranges_and_validators_parse_strictly() {
        assert_eq!(parse_content_range("bytes 0-0/1"), Some((0, 1, 1)));
        assert_eq!(
            parse_content_range(" BYTES 10-19/100 "),
            Some((10, 20, 100))
        );
        assert_eq!(parse_content_range("bytes 5-4/10"), None);
        assert_eq!(parse_content_range("bytes 0-10/10"), None);
        assert_eq!(parse_content_range("bytes */10"), None);
        assert_eq!(parse_content_range("bytes 0-1/*"), None);
        assert_eq!(parse_content_range("items 0-1/10"), None);
        assert_eq!(
            multipart_boundary("multipart/byteranges; boundary=\"a b\""),
            Some("a b".to_owned())
        );
        assert_eq!(multipart_boundary("multipart/byteranges"), None);
        assert_eq!(strong_etag(Some("W/\"weak\"")), None);
        assert_eq!(
            strong_etag(Some("\"strong\"")),
            Some("\"strong\"".to_owned())
        );
    }

    #[test]
    fn multipart_bodies_parse_by_their_content_ranges() {
        let kind = "multipart/byteranges; boundary=\"SEPARATOR\"";
        let body = b"\r\n--SEPARATOR\r\ncontent-type: application/zip\r\n\
CONTENT-RANGE: bytes 2-4/10\r\n\r\ncde\r\n--SEPARATOR\r\n\
Content-Range:bytes 7-8/10\r\n\r\nhi\r\n--SEPARATOR--\r\n";
        let mut budget = 100;
        let segments = receive_all(multipart_reply(kind, body), 10, &mut budget).expect("parts");
        assert_eq!(segments, vec![(2, b"cde".to_vec()), (7, b"hi".to_vec())]);
        assert_eq!(budget, 95);

        // Data that looks like a delimiter is data: its length was declared.
        let mut tricky = b"--SEPARATOR\nContent-Range: bytes 0-12/20\n\n".to_vec();
        tricky.extend_from_slice(b"\r\n--SEPARATOR");
        tricky.extend_from_slice(b"\n--SEPARATOR--");
        let segments =
            receive_all(multipart_reply(kind, &tricky), 20, &mut 100).expect("tricky part");
        assert_eq!(segments, vec![(0, b"\r\n--SEPARATOR".to_vec())]);

        // Another file's total.
        let other = b"--SEPARATOR\r\nContent-Range: bytes 2-4/11\r\n\r\ncde\r\n--SEPARATOR--\r\n";
        assert!(matches!(
            receive_all(multipart_reply(kind, other), 10, &mut 100),
            Err(ArchiveError::Changed)
        ));
        // A part shorter than it claimed runs into the delimiter and is caught.
        let short = b"--SEPARATOR\r\nContent-Range: bytes 2-9/10\r\n\r\ncde\r\n--SEPARATOR--\r\n";
        assert!(receive_all(multipart_reply(kind, short), 10, &mut 100).is_err());
        // A body that stops.
        let cut = b"--SEPARATOR\r\nContent-Range: bytes 2-4/10\r\n\r\ncd";
        assert!(receive_all(multipart_reply(kind, cut), 10, &mut 100).is_err());
        // More than the budget allows is refused before it is read.
        assert!(matches!(
            receive_all(multipart_reply(kind, body), 10, &mut 4),
            Err(ArchiveError::TooLarge)
        ));
        // No boundary, no parts.
        assert!(matches!(
            receive_all(multipart_reply("multipart/byteranges", body), 10, &mut 100),
            Err(ArchiveError::Malformed(_))
        ));
        // No delimiter anywhere.
        let none = b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n";
        assert!(matches!(
            receive_all(multipart_reply(kind, none), 10, &mut 100),
            Err(ArchiveError::Malformed(_))
        ));
    }

    #[test]
    fn one_entry_streams_to_a_file() {
        let (bytes, chart) = front_loaded();
        let mut server = FakeServer::new(bytes);
        let index = fetch_index_with(&mut server, 1, 2).expect("index");
        let audio = index.folders[0].audio[0];
        let span = index.entries[audio].span();
        let dir = temp_dir("stream");

        server.requests.clear();
        let dest = dir.join("song.ogg");
        let mut seen = Vec::new();
        let mut watch = |done: u64, total: u64| {
            seen.push((done, total));
            true
        };
        let written = fetch_entry_to_file_with(&mut server, &index, audio, &dest, MAX, &mut watch)
            .expect("audio");
        assert_eq!(written, 40_000);
        assert_eq!(fs::read(&dest).expect("written audio"), noise(40_000, 3));
        assert_eq!(
            server.requests,
            vec![(
                format!("bytes={}-{}", span.0, span.1 - 1),
                Some(ETAG.to_owned())
            )]
        );
        assert_eq!(seen.first(), Some(&(0, 40_000)));
        assert_eq!(seen.last(), Some(&(40_000, 40_000)));
        assert!(seen.windows(2).all(|pair| pair[0].0 <= pair[1].0));

        // Deflated entries inflate on the way through.
        let simfile = index.folders[0].simfile.expect("simfile");
        let chart_dest = dir.join("song.ssc");
        fetch_entry_to_file_with(
            &mut server,
            &index,
            simfile,
            &chart_dest,
            MAX,
            &mut accept_all(),
        )
        .expect("simfile");
        assert_eq!(fs::read(&chart_dest).expect("written chart"), chart);

        // A file already there is somebody else's: refused, and left alone.
        let error =
            fetch_entry_to_file_with(&mut server, &index, audio, &dest, MAX, &mut accept_all())
                .unwrap_err();
        assert!(matches!(error, ArchiveError::Io(_)), "{error}");
        assert_eq!(fs::read(&dest).expect("left alone").len(), 40_000);

        // Cancelling removes what was written.
        let cancelled_dest = dir.join("cancelled.ogg");
        let mut first_only = |done: u64, _: u64| done == 0;
        let error = fetch_entry_to_file_with(
            &mut server,
            &index,
            audio,
            &cancelled_dest,
            MAX,
            &mut first_only,
        )
        .unwrap_err();
        assert!(error.is_cancelled());
        assert!(!cancelled_dest.exists());

        // Bounded before anything is created.
        let bounded = dir.join("bounded.ogg");
        assert!(matches!(
            fetch_entry_to_file_with(
                &mut server,
                &index,
                audio,
                &bounded,
                1_000,
                &mut accept_all()
            ),
            Err(ArchiveError::TooLarge)
        ));
        assert!(!bounded.exists());

        // The tail holds the filler's simfile: no request.
        server.requests.clear();
        let filler = index.folders[1].simfile.expect("filler simfile");
        let filler_dest = dir.join("filler.sm");
        fetch_entry_to_file_with(
            &mut server,
            &index,
            filler,
            &filler_dest,
            MAX,
            &mut accept_all(),
        )
        .expect("filler");
        assert!(server.requests.is_empty());
        assert_eq!(fs::read(&filler_dest).expect("filler"), b"#TITLE:Filler;");

        fs::remove_dir_all(dir).expect("clean fixture root");
    }

    fn song_pack() -> (Vec<u8>, Vec<u8>) {
        let chart = b"#TITLE:Song;\n#MUSIC:song.ogg;\n".repeat(100);
        let bytes = build_zip(&[
            Item::Stored("Pack/Filler/filler.bin", &noise(150_000, 9)),
            Item::Dir("Pack/Song/"),
            Item::Deflated("Pack/Song/song.ssc", &chart),
            Item::Stored("Pack/Song/song.ogg", &noise(30_000, 10)),
            Item::Dir("Pack/Song/extras/"),
            Item::Deflated("Pack/Song/extras/notes.txt", b"hello"),
            Item::Stored("Pack/Split/split.sm", b"#TITLE:Split;"),
            Item::Stored("Pack/Filler/more.bin", &noise(100_000, 11)),
            Item::Stored("Pack/Split/split.ogg", &noise(1_000, 12)),
            Item::Stored("Pack/Filler/last.bin", &noise(100_000, 13)),
            Item::Stored("Pack/Tail/tail.sm", b"#TITLE:Tail;"),
        ]);
        (bytes, chart)
    }

    #[test]
    fn a_song_folder_is_staged_whole() {
        let (bytes, chart) = song_pack();
        let mut server = FakeServer::new(bytes);
        let index = fetch_index_with(&mut server, 4, 4).expect("index");
        let folder = |name: &str| {
            index
                .folders
                .iter()
                .position(|folder| folder.name == name)
                .expect("folder")
        };
        let dir = temp_dir("folder");
        let temp = dir.join("song.part");
        let staging = dir.join("staging");

        server.requests.clear();
        let mut progress = Vec::new();
        let mut watch = |done: u64, total: u64| {
            progress.push((done, total));
            true
        };
        let written = fetch_folder_with(
            &mut server,
            &index,
            folder("Song"),
            &temp,
            &staging,
            MAX,
            &mut watch,
        )
        .expect("song");
        assert_eq!(written, "Song");
        assert_eq!(server.requests.len(), 1);
        assert!(
            !server.requests[0].0.contains(','),
            "a contiguous folder is one range"
        );
        assert!(!temp.exists());
        assert_eq!(
            fs::read(staging.join("Song/song.ssc")).expect("chart"),
            chart
        );
        assert_eq!(
            fs::read(staging.join("Song/song.ogg")).expect("audio"),
            noise(30_000, 10)
        );
        assert_eq!(
            fs::read(staging.join("Song/extras/notes.txt")).expect("notes"),
            b"hello"
        );
        let (done, total) = *progress.last().expect("progress");
        assert_eq!(done, total);
        assert_eq!(progress[0], (0, total));

        // Staged already: refused, and the first copy is left alone.
        let error = fetch_folder_with(
            &mut server,
            &index,
            folder("Song"),
            &temp,
            &staging,
            MAX,
            &mut accept_all(),
        )
        .unwrap_err();
        assert!(
            matches!(&error, ArchiveError::Io(message) if message.contains("already staged")),
            "{error}"
        );
        assert!(staging.join("Song/song.ssc").is_file());

        // Cancelled mid-download: nothing left behind.
        let other = dir.join("other");
        let mut first_only = |done: u64, _: u64| done == 0;
        let error = fetch_folder_with(
            &mut server,
            &index,
            folder("Song"),
            &temp,
            &other,
            MAX,
            &mut first_only,
        )
        .unwrap_err();
        assert!(error.is_cancelled());
        assert!(!temp.exists());
        assert!(!other.join("Song").exists());

        // A folder where the download should go is refused, not deleted.
        let in_the_way = dir.join("in the way");
        fs::create_dir(&in_the_way).expect("create a folder in the way");
        let error = fetch_folder_with(
            &mut server,
            &index,
            folder("Song"),
            &in_the_way,
            &other,
            MAX,
            &mut accept_all(),
        )
        .unwrap_err();
        assert!(matches!(error, ArchiveError::Io(_)), "{error}");
        assert!(in_the_way.is_dir());

        // Too large is settled before a request.
        server.requests.clear();
        assert!(matches!(
            fetch_folder_with(
                &mut server,
                &index,
                folder("Song"),
                &temp,
                &other,
                1_000,
                &mut accept_all()
            ),
            Err(ArchiveError::TooLarge)
        ));
        assert!(server.requests.is_empty());

        // A folder split by somebody else's file: its two runs, one request.
        let written = fetch_folder_with(
            &mut server,
            &index,
            folder("Split"),
            &temp,
            &staging,
            MAX,
            &mut accept_all(),
        )
        .expect("split");
        assert_eq!(written, "Split");
        assert_eq!(server.requests.len(), 1);
        assert_eq!(server.requests[0].0.matches(',').count(), 1);
        assert_eq!(
            fs::read(staging.join("Split/split.ogg")).expect("split audio"),
            noise(1_000, 12)
        );

        // The tail holds this one whole.
        server.requests.clear();
        fetch_folder_with(
            &mut server,
            &index,
            folder("Tail"),
            &temp,
            &staging,
            MAX,
            &mut accept_all(),
        )
        .expect("tail");
        assert!(server.requests.is_empty());
        assert_eq!(
            fs::read(staging.join("Tail/tail.sm")).expect("tail chart"),
            b"#TITLE:Tail;"
        );

        fs::remove_dir_all(dir).expect("clean fixture root");
    }

    /// Stage the first folder of a pack made of `items`, checking a failure
    /// leaves nothing behind. On success, the name written, the staging
    /// folder, and the fixture root to clean up.
    fn stage_first(items: &[Item<'_>]) -> Result<(String, PathBuf, PathBuf), ArchiveError> {
        let mut server = FakeServer::new(build_zip(items));
        let index = fetch_index_with(&mut server, 5, 1)?;
        let dir = temp_dir("stage");
        let staging = dir.join("staging");
        let temp = dir.join("song.part");
        let result = fetch_folder_with(
            &mut server,
            &index,
            0,
            &temp,
            &staging,
            MAX,
            &mut accept_all(),
        );
        assert!(!temp.exists());
        match result {
            Ok(written) => Ok((written, staging, dir)),
            Err(error) => {
                let left = fs::read_dir(&staging).map_or(0, Iterator::count);
                assert_eq!(left, 0, "a failed folder leaves nothing staged");
                fs::remove_dir_all(dir).expect("clean fixture root");
                Err(error)
            }
        }
    }

    fn unsafe_message(items: &[Item<'_>]) -> String {
        match stage_first(items) {
            Err(ArchiveError::Unsafe(message)) => message,
            Err(other) => panic!("expected an unsafe archive, got {other}"),
            Ok((written, ..)) => panic!("expected an unsafe archive, staged '{written}'"),
        }
    }

    #[test]
    fn unsafe_song_folders_are_refused() {
        unsafe_message(&[Item::Stored("Pack/Song/../escape.sm", b"x")]);
        unsafe_message(&[Item::Stored("Pack\\Song\\..\\..\\escape.sm", b"x")]);
        unsafe_message(&[Item::Stored("/Pack/Song/song.sm", b"x")]);
        unsafe_message(&[Item::Stored("C:/Song/song.sm", b"x")]);
        unsafe_message(&[Item::Stored("Pack/Song/a:b.sm", b"x")]);
        let duplicate = unsafe_message(&[
            Item::Stored("Pack/Song/song.sm", b"x"),
            Item::Stored("Pack/Song/SONG.SM", b"y"),
        ]);
        assert!(duplicate.contains("duplicates"), "{duplicate}");
        let link = unsafe_message(&[
            Item::Stored("Pack/Song/song.sm", b"x"),
            Item::Symlink("Pack/Song/link.ogg", "../../../outside"),
        ]);
        assert!(link.contains("not a regular file"), "{link}");

        assert!(matches!(
            stage_first(&[Item::Stored("Pack/Song/song.ogg", b"audio")]),
            Err(ArchiveError::Malformed(message)) if message.contains("no .sm")
        ));
    }

    #[test]
    fn folder_names_are_written_as_windows_would_keep_them() {
        let (written, staging, dir) =
            stage_first(&[Item::Stored("Pack/V.L.S.I./song.sm", b"#TITLE:V.L.S.I.;")])
                .expect("stage");
        assert_eq!(written, "V.L.S.I");
        assert!(staging.join("V.L.S.I/song.sm").is_file());
        fs::remove_dir_all(dir).expect("clean fixture root");

        let (written, staging, dir) =
            stage_first(&[Item::Stored("Pack/con/song.sm", b"#TITLE:Con;")]).expect("stage");
        assert_eq!(written, "_con");
        assert!(staging.join("_con/song.sm").is_file());
        fs::remove_dir_all(dir).expect("clean fixture root");

        assert!(matches!(
            written_folder_name("..."),
            Err(ArchiveError::Unsafe(_))
        ));
        assert_eq!(written_folder_name("COM1.ogg").expect("name"), "_COM1.ogg");
        assert_eq!(written_folder_name("Console").expect("name"), "Console");
    }

    #[test]
    fn simfile_tags_read_the_header_the_way_the_game_does() {
        let sm = b"\xEF\xBB\xBF// #TITLE:Not this;\n\
#TITLE:Grand\\:Slam // the escaped colon is kept\n\
;\n\
#SUBTITLE:(Long Version);\n\
#ARTIST:  Somebody &amp\\; Friends  ;\n\
#TITLETRANSLIT:Gurando Suramu;\n\
#MUSIC:grand slam.ogg\n\
#OFFSET:-0.009;\n\
#NOTES:\n\
     dance-single:\n\
#TITLE:A chart's idea of a title;\n";
        assert_eq!(
            simfile_tags(sm),
            SimfileTags {
                title: "Grand:Slam".to_owned(),
                translit: "Gurando Suramu".to_owned(),
                artist: "Somebody &amp; Friends".to_owned(),
                music: "grand slam.ogg".to_owned(),
                preview: String::new(),
            }
        );

        let ssc = b"#VERSION:0.83;\r\n#TITLE:Re:Start;\r\n#ARTIST:A;\r\n\
#PREVIEWVID:clip.mp4;\r\n#PREVIEW:preview.ogg;\r\n#MUSIC:main.ogg;\r\n\
#NOTEDATA:;\r\n#MUSIC:chart.ogg;\r\n";
        let tags = simfile_tags(ssc);
        // An unescaped colon ends the value, as the game reads it.
        assert_eq!(tags.title, "Re");
        assert_eq!(tags.artist, "A");
        assert_eq!(tags.music, "main.ogg");
        assert_eq!(tags.preview, "preview.ogg");
        assert_eq!(simfile_tags(b""), SimfileTags::default());
        assert_eq!(simfile_tags(b"#TITLE"), SimfileTags::default());
        assert_eq!(simfile_tags(b"#TITLE:Unclosed").title, "Unclosed");
    }

    fn folders_only(names: &[&str]) -> PackIndex {
        PackIndex {
            pack_id: 1,
            total: 0,
            etag: None,
            entries: Vec::new(),
            folders: names
                .iter()
                .map(|name| SongFolder {
                    name: (*name).to_owned(),
                    entries: Vec::new(),
                    simfile: Some(0),
                    audio: Vec::new(),
                })
                .collect(),
            tail_start: 0,
            tail: Arc::from(Vec::new()),
        }
    }

    fn tagged(title: &str, artist: &str) -> SimfileTags {
        SimfileTags {
            title: title.to_owned(),
            artist: artist.to_owned(),
            ..SimfileTags::default()
        }
    }

    #[test]
    fn songs_match_folders_and_never_guess() {
        let none = |_: usize| None;

        // ECS-style folders carry a block and a charter around the title.
        let index = folders_only(&[
            "(14) Kurenai - [Zaia]",
            "(13) Bad Apple!! - [Zaia]",
            "(12) X - [Zaia]",
        ]);
        assert_eq!(
            match_song(&index, "Kurenai", "", none),
            SongMatch::Folder(0)
        );
        assert_eq!(
            match_song(&index, "Bad Apple!!", "", none),
            SongMatch::Folder(1)
        );
        // Too short to trust a containment; the simfile settles it.
        assert_eq!(match_song(&index, "X", "", none), SongMatch::NotFound);
        let x_tags = |ix: usize| (ix == 2).then(|| tagged("X", "Someone"));
        assert_eq!(match_song(&index, "X", "", x_tags), SongMatch::Folder(2));
        let twice = folders_only(&["(14) Kurenai - [Zaia]", "(11) Kurenai - [Zaia]"]);
        assert_eq!(
            match_song(&twice, "Kurenai", "", none),
            SongMatch::Ambiguous
        );

        // Two folders, one name once punctuation goes: their simfiles decide.
        let index = folders_only(&["V.L.S.I", "V.L.S.I."]);
        assert_eq!(
            match_song(&index, "V.L.S.I.", "Foo", none),
            SongMatch::Ambiguous
        );
        let tags = |ix: usize| {
            Some(if ix == 0 {
                tagged("V.L.S.I", "Bar")
            } else {
                tagged("V.L.S.I.", "Foo")
            })
        };
        assert_eq!(
            match_song(&index, "V.L.S.I.", "Foo", tags),
            SongMatch::Folder(1)
        );
        assert_eq!(
            match_song(&index, "V.L.S.I", "Bar", tags),
            SongMatch::Folder(0)
        );
        assert_eq!(
            match_song(&index, "V.L.S.I", "", tags),
            SongMatch::Ambiguous
        );
        assert_eq!(folders_needing_tags(&index, "V.L.S.I", ""), vec![0, 1]);

        // ITL-style: the page names the song by its simfile title.
        let index = folders_only(&[
            "[07] Long Time (SN) [Feraligatr]",
            "[11] Long Time (Expert) [Feraligatr]",
            "[05] Other Song (SN) [Someone]",
        ]);
        let page = "[1000] [07] Long Time (Beginner)";
        assert_eq!(match_song(&index, page, "", none), SongMatch::NotFound);
        assert_eq!(folders_needing_tags(&index, page, ""), vec![0, 1, 2]);
        let exact = |ix: usize| {
            Some(match ix {
                0 => tagged("[1000] [07] Long Time (Beginner)", "Artist"),
                1 => tagged("[1001] [11] Long Time (Expert)", "Artist"),
                _ => tagged("[1002] [05] Other Song", "Someone"),
            })
        };
        assert_eq!(match_song(&index, page, "", exact), SongMatch::Folder(0));
        // ... or by that title with a subtitle added on.
        let titled = |ix: usize| {
            Some(match ix {
                0 => tagged("[1000] [07] Long Time", "Artist"),
                1 => tagged("[1001] [11] Long Time", "Artist"),
                _ => tagged("[1002] [05] Other Song", "Someone"),
            })
        };
        assert_eq!(match_song(&index, page, "", titled), SongMatch::Folder(0));
        assert_eq!(
            match_song(&index, "Long Time", "", titled),
            SongMatch::Ambiguous
        );

        // An exact folder name outranks a simfile that claims the title.
        let index = folders_only(&["Song", "Song (Remix)"]);
        let claims = |ix: usize| (ix == 1).then(|| tagged("Song", ""));
        assert_eq!(match_song(&index, "Song", "", claims), SongMatch::Folder(0));
        assert!(folders_needing_tags(&index, "Song", "").is_empty());
        assert_eq!(match_song(&index, "", "", none), SongMatch::NotFound);
        assert_eq!(
            match_song(&folders_only(&[]), "Song", "", none),
            SongMatch::NotFound
        );

        // A title the page lists twice comes with its artist, and a folder
        // that merely has the name does not win against the artist's own.
        let index = folders_only(&["Butterfly", "Butterfly (NM)"]);
        let both = |ix: usize| {
            Some(if ix == 0 {
                tagged("Butterfly", "Smile.dk")
            } else {
                tagged("Butterfly", "Nekomata Master")
            })
        };
        assert_eq!(
            folders_needing_tags(&index, "Butterfly", "Nekomata Master"),
            vec![0, 1]
        );
        assert_eq!(
            match_song(&index, "Butterfly", "Nekomata Master", both),
            SongMatch::Folder(1)
        );
        assert_eq!(
            match_song(&index, "Butterfly", "smile.dk", both),
            SongMatch::Folder(0)
        );
        // An artist nothing carries leaves the name to decide.
        assert_eq!(
            match_song(&index, "Butterfly", "Somebody", both),
            SongMatch::Folder(0)
        );
    }

    /// A whole-pack title scan goes out as one request, however many songs,
    /// until the `Range` header would grow past what servers take.
    #[test]
    fn ranges_go_out_in_as_few_requests_as_servers_take() {
        let small: Vec<(u64, u64)> = (0..150).map(|ix| (ix * 1000, ix * 1000 + 10)).collect();
        assert_eq!(ranges_in_one_request(&small), 150);
        let many: Vec<(u64, u64)> = (0..500).map(|ix| (ix * 1000, ix * 1000 + 10)).collect();
        assert_eq!(ranges_in_one_request(&many), MAX_RANGES_PER_REQUEST);
        // Sixteen-digit offsets: the header fills before the count does.
        const FAR: u64 = 1_000_000_000_000_000;
        let far: Vec<(u64, u64)> = (0..300)
            .map(|ix| (FAR + ix * 1000, FAR + ix * 1000 + 10))
            .collect();
        let fits = ranges_in_one_request(&far);
        assert!(fits < MAX_RANGES_PER_REQUEST);
        assert!(range_header(&far[..fits]).len() <= MAX_RANGE_HEADER_BYTES);
        assert!(range_header(&far[..=fits]).len() > MAX_RANGE_HEADER_BYTES);
        assert_eq!(ranges_in_one_request(&far[..1]), 1);
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            ArchiveError::Changed.to_string(),
            "The pack changed on StepManiaOnline; try again"
        );
        assert_eq!(cancelled().to_string(), "Cancelled");
        assert!(cancelled().is_cancelled());
        assert!(!ArchiveError::TooLarge.is_cancelled());
        assert_eq!(
            ArchiveError::Malformed("bad".to_owned()).to_string(),
            "Invalid pack archive: bad"
        );
        assert_eq!(
            pack_url(649),
            "https://stepmaniaonline.net/download/pack/649/"
        );
    }
}
