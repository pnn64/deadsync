//! When a chart came out, for the eras a rip can keep to and the year a card
//! shows: relay `lib/packdates.ts`, ported rule for rule.
//!
//! A pack's date comes first, from what there is to go on, none of it trusted
//! alone: itgdb's release date, which people curate; SMO's upload date, never
//! before the pack came out but often years after it; the year an official
//! game came out, or the pack's `Pack.ini` Year, or its name's; and the dates
//! the files in its zip carry, which re-zips, re-encodes and resyncs only ever
//! move later -- with the signs of age among them. Then a song's own date,
//! when its files say it is much older than its pack, and a chart's: a
//! compilation or a tournament gathers charts that came out long before it,
//! so a chart is as old as the first pack it is found in, or the year its
//! title carries.
//!
//! The site's `pack-dates.json` and `chart-firsts.json` hold what would
//! otherwise take a crawl; the rest is read from the pack's index when a rip
//! opens it.
//!
//! The relay looks at a zip through its own index, which sees song folders a
//! little differently from [`PackIndex`]'s: a song there is a folder at
//! exactly `Pack/Song/`, keyed by its name flattened to letters and digits,
//! and its audio is only the `.mp3` and `.ogg` files with bytes in them. That
//! view is rebuilt here from the index's entries, because DeadSync's own
//! folders -- more kinds of audio, at any depth -- would date some packs and
//! songs differently from the ITGmania version.
//!
//! Dates are `YYYYMMDD` numbers and are compared as numbers, as the relay
//! compares them; only differences in days go through [`day_number`].

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Utc};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use super::lists::{ChartFirsts, PackDates};
use crate::pack_archive::{PackIndex, ZipEntry};

/// Days: "more than a year" apart.
pub(super) const YEAR_DAYS: i64 = 366;

/// `.dwi` charts are in 16% of packs from before 2020 and 1% of those since,
/// so a pack carrying them is taken to be no later than 2019.
const DWI_UNTIL: u32 = 20_191_231;

/// The earliest date a zip or SMO could mean: DDR's PC days.
const EARLIEST: u32 = 19_990_101;

/// A date for the era test and the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PackDate {
    /// `YYYYMMDD`: what the era is judged by -- the date, or under a ceiling
    /// the latest it can be.
    pub day: u32,
    /// The year a card shows: the date's, or under a ceiling the best guess.
    pub year: u32,
    /// Only that it is no later than `day` is known.
    pub ceiling: bool,
}

impl PackDate {
    /// A date known to the day.
    const fn on(day: u32) -> Self {
        Self {
            day,
            year: year_of(day),
            ceiling: false,
        }
    }
}

/// What a pack zip says of itself, worked out once per index (relay
/// `ZipFacts`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ZipFacts {
    /// Its dates are one stamp: every file within two days of the rest, or a
    /// resync's re-zip.
    pub stamped: bool,
    /// The median date of its songs' audio; 0 when stamped.
    pub median: u32,
    /// When its audio was redone, so only a ceiling: the day 80% or more of it
    /// carries, or the later of two days 180 or more apart that 80% does
    /// between them; else 0.
    pub batch: u32,
    /// When a fifth or more of its songs are over two years older than that
    /// median by their audio and chart both, their median date; else 0.
    pub old_songs: u32,
    /// The median date of its songs' simfiles.
    pub charts: u32,
    /// SMO's resync left an `.oldsync` for a fifth or more of its songs: those
    /// dates are the resync's.
    pub resynced: bool,
    /// The older of its images' and its text files' median dates, which are
    /// seldom redone.
    pub art: u32,
    /// Whether it has `.dwi` charts.
    pub dwi: bool,
    /// The median date of all its files, for a guess when nothing better will
    /// do.
    pub stamp_day: u32,
}

/// Days since 1970 for a `YYYYMMDD` date, or a `YYYY` one as its first day.
/// A day past its month's end runs into the next month, as JavaScript's
/// `Date.UTC` does.
#[must_use]
pub(super) fn day_number(date: u32) -> i64 {
    let date = if date < 10_000 {
        date * 10_000 + 101
    } else {
        date
    };
    let (year, month, day) = (
        i64::from(date / 10_000),
        i64::from(date / 100 % 100),
        i64::from(date % 100),
    );
    // Days from civil, months counted from March (Howard Hinnant's), with
    // the month and day allowed to overflow as Date.UTC allows them.
    let months = year * 12 + (month - 1);
    let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + (day - 1)
}

/// The `YYYYMMDD` of a day counted from 1970: [`day_number`]'s inverse, which
/// always gives a real date (relay `ymd`). Civil from days, Howard Hinnant's.
fn ymd(days: i64) -> u32 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    u32::try_from(year * 10_000 + month * 100 + day).unwrap_or(0)
}

const fn year_of(day: u32) -> u32 {
    day / 10_000
}

/// Today, UTC, as `YYYYMMDD`.
#[must_use]
pub(super) fn today() -> u32 {
    let now = Utc::now().date_naive();
    u32::try_from(now.year()).unwrap_or(0) * 10_000 + now.month() * 100 + now.day()
}

/// The latest date that is believed: tomorrow's, as SMO dates one pack 2097.
fn latest(today: u32) -> u32 {
    ymd(day_number(today) + 1)
}

/// Whether a date is one a zip or SMO could mean: after DDR's PC days, and
/// not after `latest` (relay `sane`).
fn sane(day: u32, latest: u32) -> bool {
    (EARLIEST..=latest).contains(&day)
}

/// The earlier of a date and the upload, when there is one.
fn no_later_than(day: u32, up: u32) -> u32 {
    if up == 0 { day } else { day.min(up) }
}

// ---------------------------------------------------------------------------
// text, as JavaScript reads it

/// JavaScript's `\s`, and what `trim` takes off: its white space and line
/// terminators. Not quite Rust's: no U+0085, and U+FEFF.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// What a JavaScript `.` does not match.
fn js_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Whether text ends with this ASCII, case aside. A JavaScript `/i` test of an
/// ASCII pattern is exactly this: no other letter folds onto an ASCII one.
fn ends_with_ascii(text: &str, suffix: &str) -> bool {
    let (text, suffix) = (text.as_bytes(), suffix.as_bytes());
    text.len() >= suffix.len() && text[text.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Titles and folder names flattened to bare lowercase ASCII letters and
/// digits (relay `norm`): how the relay keys song folders and compares titles.
fn norm(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

/// Every `[tag]` in a title, each made a space: `"[2004] Pandemonium"`,
/// `"[17] [215] Chiron"`.
fn untagged(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        // With no `]` after this `[`, there is none after any later one.
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push(' ');
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A title without its tags, flattened (relay `bare`).
fn bare(title: &str) -> String {
    norm(&untagged(title))
}

/// JavaScript's `\p{L}` or `\p{N}`: a letter or a number of any script.
///
/// Rust's `is_alphabetic` is the wider Alphabetic property; what it adds to
/// the letters is the combining marks some scripts write vowels with, and the
/// circled and squared Latin letters, none of which `\p{L}` takes. The
/// numbers are the same three categories either way.
fn letter_or_number(c: char) -> bool {
    c.is_numeric()
        || c.is_alphabetic()
            && !is_combining_mark(c)
            && !matches!(c, '\u{24B6}'..='\u{24E9}' | '\u{1F130}'..='\u{1F189}')
}

/// Any script's letters and digits, case and width folded (relay `fold`).
fn fold(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .chars()
        .filter(|&c| letter_or_number(c))
        .collect()
}

/// The key `chart-firsts.json` files a song by a charter under (relay
/// `chartKey`): the title without its `[tags]`, then `|`, then the credit,
/// each NFKC-normalised, lowercased and kept to letters and digits of any
/// script.
#[must_use]
pub(super) fn chart_key(title: &str, credit: &str) -> String {
    let mut key = fold(&untagged(title));
    key.push('|');
    key.push_str(&fold(credit));
    key
}

/// The `Year=` a `Pack.ini` gives (relay `packinfo.ts`), read as Latin-1.
///
/// The relay's `/^\s*year\s*=\s*(\d{4})/im`: the first line -- spaces and
/// blank lines before it allowed -- that starts `Year`, any case, then `=`,
/// then four digits, spaces between any of them and line breaks too. The
/// first such line decides, and a year of 0 is none.
#[must_use]
pub(super) fn ini_year(bytes: &[u8]) -> Option<u32> {
    // Latin-1's share of `\s`; of its line terminators, only these two.
    let space = |b: u8| matches!(b, b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b' ' | 0xA0);
    let past_spaces = |mut at: usize| {
        while bytes.get(at).copied().is_some_and(space) {
            at += 1;
        }
        at
    };
    // Every line start inside one run of spaces lands on the same first
    // non-space, so each run is tried once.
    let mut tried: Option<usize> = None;
    for start in 0..=bytes.len() {
        if start > 0 && !matches!(bytes[start - 1], b'\n' | b'\r') {
            continue;
        }
        if tried.is_some_and(|at| start <= at) {
            continue;
        }
        let at = past_spaces(start);
        tried = Some(at);
        if !bytes
            .get(at..at + 4)
            .is_some_and(|word| word.eq_ignore_ascii_case(b"year"))
        {
            continue;
        }
        let at = past_spaces(at + 4);
        if bytes.get(at) != Some(&b'=') {
            continue;
        }
        let at = past_spaces(at + 1);
        let Some(digits) = bytes.get(at..at + 4) else {
            continue;
        };
        if !digits.iter().all(u8::is_ascii_digit) {
            continue;
        }
        let year = digits
            .iter()
            .fold(0, |year, &digit| year * 10 + u32::from(digit - b'0'));
        return (year != 0).then_some(year);
    }
    None
}

// ---------------------------------------------------------------------------
// the zip, as the relay's index sees it

/// A file the relay's index lists: every entry but a directory record.
fn is_file(entry: &ZipEntry) -> bool {
    !entry.name.is_empty() && !entry.name.ends_with('/')
}

/// A path's extension as the relay reads it, `/\.([a-z0-9]+)$/i`: ASCII
/// letters and digits after the last dot, else none.
fn extension(path: &str) -> &str {
    let ext = path.rsplit_once('.').map_or("", |(_, ext)| ext);
    if ext.bytes().all(|b| b.is_ascii_alphanumeric()) {
        ext
    } else {
        ""
    }
}

fn is_image(path: &str) -> bool {
    [".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"]
        .iter()
        .any(|ext| ends_with_ascii(path, ext))
}

fn is_text(path: &str) -> bool {
    [
        ".txt", ".md", ".nfo", ".rtf", ".htm", ".html", ".pdf", ".doc", ".docx",
    ]
    .iter()
    .any(|ext| ends_with_ascii(path, ext))
}

/// A song as the relay's index has one (relay `SongEntry`).
struct Song<'a> {
    /// `Pack/Song/`, as the first of its files has it.
    dir: &'a str,
    /// Its `.mp3` and `.ogg` files: the song, and any preview clip beside it.
    audio: Vec<&'a ZipEntry>,
}

/// Where the relay's index files an entry among its songs: an `.mp3`,
/// `.ogg`, `.sm` or `.ssc` with bytes in it, at exactly `Pack/Song/file`.
/// The song folder's name, its `Pack/Song/`, and whether the file is audio.
fn song_file(entry: &ZipEntry) -> Option<(&str, &str, bool)> {
    let name = entry.name.as_str();
    if !is_file(entry) || entry.compressed == 0 {
        return None;
    }
    let audio = ends_with_ascii(name, ".mp3") || ends_with_ascii(name, ".ogg");
    if !audio && !ends_with_ascii(name, ".sm") && !ends_with_ascii(name, ".ssc") {
        return None;
    }
    let mut parts = name.split('/');
    let (Some(root), Some(folder), Some(_), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some((folder, &name[..root.len() + folder.len() + 2], audio))
}

/// Every song the relay's index would list, a folder with no audio being
/// none. Folders whose names flatten alike are one song there, so they are
/// one here.
fn songs(index: &PackIndex) -> Vec<Song<'_>> {
    let mut songs: Vec<Song<'_>> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for entry in &index.entries {
        let Some((folder, dir, audio)) = song_file(entry) else {
            continue;
        };
        let slot = *by_key.entry(norm(folder)).or_insert_with(|| {
            songs.push(Song {
                dir,
                audio: Vec::new(),
            });
            songs.len() - 1
        });
        if audio {
            songs[slot].audio.push(entry);
        }
    }
    songs.retain(|song| !song.audio.is_empty());
    songs
}

/// The relay's song for one of [`PackIndex`]'s folders: the one its name
/// flattens to. A name that flattens to nothing is a song the relay never
/// offers; it is taken as its own folder alone.
fn song_named<'a>(index: &'a PackIndex, name: &str) -> Option<Song<'a>> {
    let key = norm(name);
    let lower = name.to_lowercase();
    let mut song: Option<Song<'a>> = None;
    for entry in &index.entries {
        let Some((folder, dir, audio)) = song_file(entry) else {
            continue;
        };
        let ours = if key.is_empty() {
            folder.to_lowercase() == lower
        } else {
            norm(folder) == key
        };
        if !ours {
            continue;
        }
        let song = song.get_or_insert_with(|| Song {
            dir,
            audio: Vec::new(),
        });
        if audio {
            song.audio.push(entry);
        }
    }
    song.filter(|song| !song.audio.is_empty())
}

/// The song's own audio: the biggest file, not a preview clip beside it, the
/// first of equals (relay `audioDay`). 0 for none.
fn audio_day(audio: &[&ZipEntry]) -> u32 {
    let Some(mut best) = audio.first().copied() else {
        return 0;
    };
    for &file in audio {
        if file.uncompressed > best.uncompressed {
            best = file;
        }
    }
    best.day
}

/// The date of the simfile the game would load from `Pack/Song/`: the first
/// `.ssc` directly in it, else the first `.sm` (relay `simfileOf`). 0 for
/// none.
fn simfile_day(index: &PackIndex, dir: &str) -> u32 {
    let mut sm = None;
    for entry in index.entries.iter().filter(|entry| is_file(entry)) {
        let Some(rest) = entry.name.strip_prefix(dir) else {
            continue;
        };
        if rest.contains('/') {
            continue;
        }
        if ends_with_ascii(&entry.name, ".ssc") {
            return entry.day;
        }
        if sm.is_none() && ends_with_ascii(&entry.name, ".sm") {
            sm = Some(entry.day);
        }
    }
    sm.unwrap_or(0)
}

/// A pack whose audio all carries one date that its `.oldsync` backups share:
/// SMO's resync re-zipped it, and every date in it is the re-zip's.
fn is_rezipped(index: &PackIndex, songs: &[Song<'_>]) -> bool {
    let mut span: Option<(i64, i64)> = None;
    for song in songs {
        let day = audio_day(&song.audio);
        if day == 0 {
            continue;
        }
        let day = day_number(day);
        span = Some(span.map_or((day, day), |(lo, hi)| (lo.min(day), hi.max(day))));
    }
    let Some((lo, hi)) = span else {
        return false;
    };
    if hi - lo > 3 {
        return false;
    }
    index.entries.iter().any(|entry| {
        is_file(entry)
            && ends_with_ascii(&entry.name, ".oldsync")
            && entry.day != 0
            && (day_number(entry.day) - lo).abs() <= 3
    })
}

/// The median of some dates as the relay takes it -- of an even count, the
/// later of the middle two -- and 0 for none.
fn middle(days: &mut [u32]) -> u32 {
    days.sort_unstable();
    days.get(days.len() / 2).copied().unwrap_or(0)
}

/// The days that hold the most of a list of dates, a day either side of each:
/// `(YYYYMMDD, how many)`, the busiest first.
///
/// Ties go to the day first met in `days`, as the relay meets them: in the
/// sorted order its median left the list in.
fn busiest(days: &[u32], k: usize) -> Vec<(u32, usize)> {
    let mut order: Vec<i64> = Vec::new();
    let mut count: HashMap<i64, usize> = HashMap::new();
    for &day in days {
        let day = day_number(day);
        *count.entry(day).or_insert_with(|| {
            order.push(day);
            0
        }) += 1;
    }
    let mut taken: HashSet<i64> = HashSet::new();
    let mut out = Vec::with_capacity(k);
    for _ in 0..k {
        let (mut best, mut at) = (0, 0);
        for &day in &order {
            if taken.contains(&day) {
                continue;
            }
            let near: usize = [day - 1, day, day + 1]
                .iter()
                .filter(|near| !taken.contains(near))
                .map(|near| count.get(near).copied().unwrap_or(0))
                .sum();
            if near > best {
                (best, at) = (near, day);
            }
        }
        if best == 0 {
            break;
        }
        taken.extend([at - 1, at, at + 1]);
        out.push((ymd(at), best));
    }
    out
}

/// What a pack zip says of itself (relay `zipFacts`). `today` bounds the
/// dates believed, as everywhere here.
#[must_use]
pub(super) fn zip_facts(index: &PackIndex, today: u32) -> ZipFacts {
    let latest = latest(today);
    let songs = songs(index);

    let mut span: Option<(i64, i64)> = None;
    let (mut dwi, mut oldsyncs) = (false, 0usize);
    let (mut all, mut images, mut texts) = (Vec::new(), Vec::new(), Vec::new());
    // song folder -> the rank and date of its simfile: .ssc, .sm, .dwi, the
    // order the game loads them in
    let mut charts: HashMap<&str, (u8, u32)> = HashMap::new();
    for entry in index.entries.iter().filter(|entry| is_file(entry)) {
        let path = entry.name.as_str();
        let ext = extension(path);
        let rank = ["ssc", "sm", "dwi"]
            .iter()
            .position(|chart| ext.eq_ignore_ascii_case(chart));
        if let Some(rank) = rank {
            let rank = rank as u8;
            dwi |= rank == 2;
            let dir = &path[..path.rfind('/').map_or(0, |at| at + 1)];
            match charts.entry(dir) {
                Entry::Occupied(mut have) => {
                    if rank < have.get().0 {
                        have.insert((rank, entry.day));
                    }
                }
                Entry::Vacant(slot) => {
                    slot.insert((rank, entry.day));
                }
            }
        }
        if ext.eq_ignore_ascii_case("oldsync") {
            oldsyncs += 1;
        }
        if !sane(entry.day, latest) {
            continue;
        }
        let day = day_number(entry.day);
        span = Some(span.map_or((day, day), |(lo, hi)| (lo.min(day), hi.max(day))));
        all.push(entry.day);
        if is_image(path) {
            images.push(entry.day);
        } else if is_text(path) {
            texts.push(entry.day);
        }
    }

    let (mut audio, mut chart_days, mut own) = (Vec::new(), Vec::new(), Vec::new());
    for song in &songs {
        let chart = charts.get(song.dir).map_or(0, |&(_, day)| day);
        if sane(chart, latest) {
            chart_days.push(chart);
        }
        let day = audio_day(&song.audio);
        if !sane(day, latest) {
            continue;
        }
        audio.push(day);
        if sane(chart, latest) {
            own.push(day.max(chart));
        }
    }
    let stamped = span.is_none_or(|(lo, hi)| hi - lo <= 2) || is_rezipped(index, &songs);
    let median = if stamped { 0 } else { middle(&mut audio) };

    let mut batch = 0;
    if median != 0 && audio.len() >= 3 {
        let most = 0.8 * audio.len() as f64;
        let busy = busiest(&audio, 2);
        if let Some(&(one_day, one)) = busy.first() {
            if one as f64 >= most {
                batch = one_day;
            } else if let Some(&(two_day, two)) = busy.get(1)
                && (one + two) as f64 >= most
                && (day_number(one_day) - day_number(two_day)).abs() >= 180
            {
                batch = one_day.max(two_day);
            }
        }
    }
    let mut old_songs = 0;
    if median != 0 {
        let mut old: Vec<u32> = own
            .iter()
            .copied()
            .filter(|&day| day_number(median) - day_number(day) > 2 * YEAR_DAYS)
            .collect();
        if !old.is_empty() && old.len() as f64 >= 0.2 * own.len() as f64 {
            old_songs = middle(&mut old);
        }
    }
    let art = [middle(&mut images), middle(&mut texts)]
        .into_iter()
        .filter(|&day| day > 0)
        .min()
        .unwrap_or(0);

    ZipFacts {
        stamped,
        median,
        batch,
        old_songs,
        charts: middle(&mut chart_days),
        resynced: oldsyncs as f64 >= 0.2 * songs.len() as f64,
        art,
        dwi,
        stamp_day: middle(&mut all),
    }
}

// ---------------------------------------------------------------------------
// a pack's date

/// What the weekly file alone says of a pack (relay `listedDate`): itgdb's
/// release -- unless it comes more than a month after SMO's upload, when it
/// must be some later release's, and the upload is the date, the pack having
/// been out by then. Without a release, only the ceiling: the upload. `None`
/// for a pack newer than the file, or one with neither date.
#[must_use]
pub(super) fn listed_date(dates: Option<&PackDates>, pack_id: u64, today: u32) -> Option<PackDate> {
    let &(up, release) = dates?.packs.get(&pack_id)?;
    let latest = latest(today);
    let up = if sane(up, latest) { up } else { 0 };
    // a release known only to the year is its first day
    let release = if release > 0 && release < 10_000 {
        release * 10_000 + 101
    } else {
        release
    };
    if sane(release, latest) {
        let day = if up != 0 && day_number(release) > day_number(up) + 30 {
            up
        } else {
            release
        };
        return Some(PackDate::on(day));
    }
    (up != 0).then_some(PackDate {
        day: up,
        year: year_of(up),
        ceiling: true,
    })
}

/// The year a pack's name gives, as many do -- "File of the Week 2007" -- the
/// latest when it names several, 0 for none; and whether it is only a bound:
/// "Individual Files Pre-2016" gives 2015, as the latest it can be. Where
/// itgdb dates such a pack, it came out in that year or the next in 89 of 91.
///
/// The relay's `/(^|\D)((?:19|20)\d\d)(?!\d)/g`: four digits starting 19 or
/// 20, no digit either side; from 1999 to this year. A year counts as a bound
/// when what comes before it ends in "pre" or "before", any case, and then
/// only spaces and hyphens.
fn name_year(name: &str, today: u32) -> (u32, bool) {
    let chars: Vec<char> = name.chars().collect();
    let digit = |at: usize| chars.get(at).is_some_and(char::is_ascii_digit);
    let mut best = (0, false);
    for at in 0..chars.len() {
        if at > 0 && digit(at - 1) {
            continue;
        }
        let century = (chars[at], chars.get(at + 1).copied());
        if !matches!(century, ('1', Some('9')) | ('2', Some('0')))
            || !digit(at + 2)
            || !digit(at + 3)
            || digit(at + 4)
        {
            continue;
        }
        let year = chars[at..at + 4]
            .iter()
            .fold(0, |year, &c| year * 10 + c.to_digit(10).unwrap_or(0));
        if year < 1999 || year > year_of(today) {
            continue;
        }
        let before: String = chars[..at].iter().collect();
        let before = before.trim_end_matches(|c| js_space(c) || c == '-');
        let bound = ends_with_ascii(before, "pre") || ends_with_ascii(before, "before");
        let year = if bound { year - 1 } else { year };
        if year > best.0 {
            best = (year, bound);
        }
    }
    best
}

/// One alternative of the relay's game patterns, against a name with its
/// ASCII lowercased.
enum Pattern {
    /// The text anywhere.
    Has(&'static str),
    /// The first text, then the second later on the same line: `A.*B`.
    Then(&'static str, &'static str),
    /// The text, not followed by any of these: `A(?![23])`.
    NotBefore(&'static str, &'static [u8]),
    /// The text at the start of the name: `^A`.
    Starts(&'static str),
}

impl Pattern {
    fn found(&self, name: &str) -> bool {
        match *self {
            Self::Has(text) => name.contains(text),
            Self::Then(first, then) => name.match_indices(first).any(|(at, _)| {
                let rest = &name[at + first.len()..];
                for (at, c) in rest.char_indices() {
                    if rest[at..].starts_with(then) {
                        return true;
                    }
                    if js_line_end(c) {
                        break;
                    }
                }
                false
            }),
            Self::NotBefore(text, not) => name.match_indices(text).any(|(at, _)| {
                name.as_bytes()
                    .get(at + text.len())
                    .is_none_or(|next| !not.contains(next))
            }),
            Self::Starts(text) => name.starts_with(text),
        }
    }
}

use Pattern::{Has, NotBefore, Starts, Then};

/// The official games SMO has ripped, named for their platform -- "Dance
/// Dance Revolution 3rdMIX (AC) (JP)" -- and the year each came out: their
/// charts are that old, whenever the rip was made. The first that matches
/// is the one (relay `GAMES`).
const GAMES: &[(&[Pattern], u32)] = &[
    (&[Has("extreme2")], 2005),
    (&[Then("extreme", "(ps2) (na)")], 2004),
    (&[Then("extreme", "(ps2)")], 2003),
    (&[Has("extreme")], 2002),
    (&[Then("ddrmax2", "(na)")], 2003),
    (&[Has("ddrmax2")], 2002),
    (&[Then("ddrmax", "(na)")], 2002),
    (&[Has("ddrmax")], 2001),
    (&[Has("supernova2"), Has("supernova 2")], 2007),
    (&[Has("supernova"), Has("str!ke")], 2006),
    (&[Has("5thmix")], 2001),
    (
        &[
            Has("4thmix"),
            Has("3rdmix plus"),
            Has("3rdmix (playstation)"),
        ],
        2000,
    ),
    (&[Has("3rdmix")], 1999),
    (
        &[
            Has("2ndremix"),
            Has("2nd remix"),
            Has("2ndre mix"),
            Has("2nd re mix"),
            Has("2ndmix"),
            Has("solo 2000"),
            Has("solo bassmix"),
            Has("internet ranking"),
            Has("internet r@nking"),
        ],
        1999,
    ),
    (&[Has("gb3"), Has("oha-sta")], 2001),
    (
        &[
            Has("gb2"),
            Has("gb disney"),
            Has("gb ("),
            Has("disney dancing museum"),
            Has("disney's rave"),
            Has("tokimeki"),
            Has("kids"),
        ],
        2000,
    ),
    (&[Then("dreams come true", "(playstation)")], 2000),
    (&[Has("dreams come true"), Has("true kiss")], 1999),
    (
        &[Has("konamix"), Has("megamix"), Has("party edition")],
        2002,
    ),
    (
        &[
            Has("party collection"),
            Has("fever"),
            NotBefore("ultramix", b"23"),
        ],
        2003,
    ),
    (
        &[
            Has("ultramix2"),
            Has("fusion"),
            Has("festival"),
            NotBefore("unleashed", b"23"),
        ],
        2004,
    ),
    (
        &[
            Has("ultramix3"),
            Has("mario mix"),
            Has("unleashed2"),
            Has("dancing stage max"),
        ],
        2005,
    ),
    (&[Has("unleashed3")], 2006),
    (&[Has("universe2"), Has("universe3")], 2008),
    (&[Has("universe")], 2007),
    (&[Has("s+")], 2011),
    (&[Starts("dance dance revolution (ac)")], 1998),
];

/// The names an official game's rip goes by.
const OFFICIAL: [&str; 6] = [
    "dancedancerevolution",
    "dance dancerevolution",
    "dancedance revolution",
    "dance dance revolution",
    "ddr",
    "dancing stage",
];

/// The platforms a rip of one is named for.
const PLATFORMS: [&str; 12] = [
    "(ac)",
    "(ps2)",
    "(playstation)",
    "(xbox)",
    "(xbox 360)",
    "(gamecube)",
    "(n64)",
    "(gameboy color)",
    "(mobile)",
    "(wii)",
    "(psp)",
    "(ds)",
];

/// Whether a pack is a rip of an official game: named for one and for its
/// platform.
fn is_official(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    OFFICIAL.iter().any(|game| name.contains(game))
        && PLATFORMS.iter().any(|platform| name.contains(platform))
}

/// The year the official game a pack rips came out; 0 for any other pack.
fn game_year(name: &str) -> u32 {
    if !is_official(name) {
        return 0;
    }
    let name = name.to_ascii_lowercase();
    GAMES
        .iter()
        .find(|(patterns, _)| patterns.iter().any(|pattern| pattern.found(&name)))
        .map_or(0, |&(_, year)| year)
}

/// The first of July of a game's year: a date in it, whatever its month.
const fn game_date(year: u32) -> PackDate {
    PackDate {
        day: year * 10_000 + 701,
        year,
        ceiling: false,
    }
}

/// Whether [`pack_date`] would look at the pack's `Pack.ini` Year at all: not
/// for an official game's rip, nor a pack its listing dates. The relay reads
/// the file only past those two, and reading it can cost a request, so a rip
/// need not read it for them.
#[must_use]
pub(super) fn wants_ini_year(
    dates: Option<&PackDates>,
    pack_id: u64,
    name: &str,
    today: u32,
) -> bool {
    game_year(name) == 0 && listed_date(dates, pack_id, today).is_none_or(|listed| listed.ceiling)
}

/// Relay `packDate`. `ini_year` is the pack's `Pack.ini` Year when it was
/// read; `None` when it was not (or says none).
///
/// When a pack came out: an official game's year; else as its listing has it
/// when that is a date; else as its `Pack.ini`'s Year has it (the same as
/// itgdb's year in 263 of 271 packs); else by its zip. The zip's date is the
/// median of its audio, no later than the upload, with these against it:
///
/// * audio redone at once: a ceiling, not a date;
/// * a fifth of its songs over two years older than that: the pack is as old
///   as they are;
/// * charts finished after the audio, up to two years later, and not
///   resynced since: their median, which is when the pack was done;
/// * an upload on one of SMO's bulk days with dates within a year of it is
///   how an old pack's re-dated files look: a ceiling;
/// * a year in the name (no later than the upload) overrules a date after the
///   year that follows it, or -- unless it is only a bound -- one over a year
///   before it (a tournament pack's audio comes from every year), and stands
///   in for none;
/// * `.dwi` charts: no later than 2019;
/// * an official game the table does not know: only the upload;
/// * art or readmes over two years older than the audio: their date.
///
/// A ceiling's year is the best guess under it; there is always some year.
#[must_use]
pub(super) fn pack_date(
    dates: Option<&PackDates>,
    pack_id: u64,
    name: &str,
    facts: &ZipFacts,
    ini_year: Option<u32>,
    today: u32,
) -> PackDate {
    let game = game_year(name);
    if game != 0 {
        return game_date(game);
    }
    let listed = listed_date(dates, pack_id, today);
    if let Some(listed) = listed
        && !listed.ceiling
    {
        return listed;
    }
    let up = listed.map_or(0, |listed| listed.day);
    let up_year = if up == 0 { year_of(today) } else { year_of(up) };

    if let Some(year) = ini_year
        && (1999..=up_year).contains(&year)
    {
        return PackDate::on(no_later_than(year * 10_000 + 701, up));
    }

    // the best guess, and whether it is a date or only a guess under the
    // ceiling
    let mut est = if facts.median == 0 {
        facts.stamp_day
    } else {
        facts.median
    };
    let mut sure = facts.median > 0;
    let mut ceiling = up;
    if sure && facts.batch != 0 {
        ceiling = no_later_than(facts.batch, ceiling);
        sure = false;
    } else if sure && facts.old_songs != 0 {
        est = facts.old_songs;
    } else if sure
        && facts.charts != 0
        && !facts.resynced
        && (31..=2 * YEAR_DAYS).contains(&(day_number(facts.charts) - day_number(est)))
    {
        est = facts.charts;
    }
    if est != 0 && up != 0 {
        est = est.min(up);
        let bulk = dates.is_some_and(|dates| dates.bulk.contains(&up));
        if sure && bulk && day_number(up) - day_number(est) <= YEAR_DAYS {
            sure = false;
        }
    }
    let (named, bound) = name_year(name, today);
    if named != 0
        && named <= up_year
        && (!sure
            || est > (named + 1) * 10_000 + 1231
            || !bound && est < (named - 1) * 10_000 + 101)
    {
        return PackDate::on(no_later_than(named * 10_000 + 1231, up));
    }
    if sure && facts.dwi && est > DWI_UNTIL {
        ceiling = no_later_than(DWI_UNTIL, ceiling);
        sure = false;
    }
    if sure && is_official(name) {
        sure = false;
    }
    if est != 0 && facts.art != 0 && day_number(est) - day_number(facts.art) > 2 * YEAR_DAYS {
        est = facts.art;
    }
    if sure {
        return PackDate::on(est);
    }

    let limit = [ceiling, est, today]
        .into_iter()
        .find(|&day| day != 0)
        .unwrap_or(0);
    PackDate {
        day: limit,
        year: year_of(if est == 0 { limit } else { est.min(limit) }),
        ceiling: true,
    }
}

/// Relay `songDate`: the song's own date from its folder's audio and the
/// date of the simfile the game would load (`.ssc` before `.sm`).
///
/// A song is dated by its pack -- unless its own files are more than a year
/// older than that: an old song in a newer pack whose files say so. Its own
/// date is the later of its audio's and its chart file's, as re-saving only
/// moves those later; a pack whose dates are one stamp says nothing of its
/// songs. `folder` is one of the index's folders, seen as the relay sees it.
#[must_use]
pub(super) fn song_date(
    pack: Option<PackDate>,
    facts: &ZipFacts,
    index: &PackIndex,
    folder: usize,
    today: u32,
) -> Option<PackDate> {
    let date = pack?;
    if facts.stamped {
        return pack;
    }
    let Some(song) = index
        .folders
        .get(folder)
        .and_then(|folder| song_named(index, &folder.name))
    else {
        return pack;
    };
    let latest = latest(today);
    let audio = audio_day(&song.audio);
    let chart = simfile_day(index, song.dir);
    if !sane(audio, latest) || !sane(chart, latest) {
        return pack;
    }
    let own = audio.max(chart);
    if day_number(date.day) - day_number(own) > YEAR_DAYS {
        Some(PackDate::on(own))
    } else {
        pack
    }
}

// ---------------------------------------------------------------------------
// a chart's first appearance

/// A difficulty name, a blank or "Various" where a name should be is nobody
/// (as the relay's author list has it).
const NOT_A_NAME: [&str; 35] = [
    "",
    "-",
    "?",
    "n/a",
    "na",
    "none",
    "unknown",
    "blank",
    "anonymous",
    "various",
    "various artists",
    "va",
    "v.a.",
    "multiple",
    "community",
    "everyone",
    "beginner",
    "novice",
    "easy",
    "basic",
    "light",
    "medium",
    "standard",
    "trick",
    "another",
    "difficult",
    "hard",
    "heavy",
    "maniac",
    "ssr",
    "challenge",
    "expert",
    "oni",
    "smaniac",
    "edit",
];

/// Whether a trimmed credit names somebody (relay `isName`): 2 to 40
/// JavaScript characters, not one of the non-names, not only digits.
fn is_name(credit: &str) -> bool {
    let len = credit.encode_utf16().count();
    (2..=40).contains(&len)
        && !NOT_A_NAME.contains(&credit.to_lowercase().as_str())
        && !credit.bytes().all(|b| b.is_ascii_digit())
}

/// The year a title carries at its head, as a compilation may put it:
/// `"[2004] Pandemonium"`.
fn title_year(title: &str) -> Option<u32> {
    let rest = title.trim_start_matches(js_space).as_bytes();
    let tag = rest.get(..6)?;
    let year_ok = matches!(tag[1..3], [b'1', b'9'] | [b'2', b'0'])
        && tag[3].is_ascii_digit()
        && tag[4].is_ascii_digit();
    (tag[0] == b'[' && year_ok && tag[5] == b']').then(|| {
        tag[1..5]
            .iter()
            .fold(0, |year, &digit| year * 10 + u32::from(digit - b'0'))
    })
}

/// What can be said of another pack without opening it (relay `otherDate`):
/// the official game it rips, its listing, or its name's year -- else its
/// upload, as a ceiling.
fn other_date(dates: Option<&PackDates>, pack_id: u64, name: &str, today: u32) -> Option<PackDate> {
    let game = game_year(name);
    if game != 0 {
        return Some(game_date(game));
    }
    let listed = listed_date(dates, pack_id, today);
    if let Some(listed) = listed
        && !listed.ceiling
    {
        return Some(listed);
    }
    let up = listed.map_or(0, |listed| listed.day);
    let (named, bound) = name_year(name, today);
    if named != 0 && (up == 0 || named <= year_of(up)) {
        let day = no_later_than(named * 10_000 + 1231, up);
        return Some(PackDate {
            day,
            year: year_of(day),
            ceiling: bound,
        });
    }
    listed
}

/// Where else the same chart is known to be.
#[derive(Clone, Copy, Default)]
pub(super) struct Appearances<'a> {
    pub firsts: Option<&'a ChartFirsts>,
    /// This credit's exact search, when the rip already made it (an author
    /// rip makes its authors'): each dance pack it found, and the songs it
    /// matched there.
    pub made: &'a [(u64, Vec<String>)],
}

/// Relay `chartDate`.
///
/// A song's chart, dated by its first appearance: a `[2004]` at the head of
/// its title, as a compilation may put it, and the same song by the same
/// charter in any other pack, as the chart index has them -- or in an author
/// rip, as the charter search it made already has them -- whichever is
/// earliest, and never later than the date the song has already. A
/// compilation's charts and most of a tournament's came out long before the
/// pack they are in.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub(super) fn chart_date(
    date: Option<PackDate>,
    pack_id: u64,
    title: &str,
    folder: &str,
    credit: &str,
    seen: Appearances<'_>,
    dates: Option<&PackDates>,
    name_of: &dyn Fn(u64) -> String,
    today: u32,
) -> Option<PackDate> {
    let latest = latest(today);
    let mut best = date;
    let mut take = |found: Option<PackDate>| {
        if let Some(found) = found
            && sane(found.day, latest)
            && best.is_none_or(|best| found.day < best.day)
        {
            best = Some(found);
        }
    };
    if let Some(year) = title_year(title) {
        take(Some(PackDate::on(year * 10_000 + 1231)));
    }
    if !is_name(credit.trim_matches(js_space)) {
        return best;
    }

    // the other packs, in the order they were found: the first of equal dates
    // stands
    let mut others: Vec<u64> = Vec::new();
    let mut found: HashSet<u64> = HashSet::new();
    if let Some(firsts) = seen.firsts {
        for text in [title, folder] {
            for &id in firsts.packs(&chart_key(text, credit)) {
                if found.insert(id) {
                    others.push(id);
                }
            }
        }
    }
    if !seen.made.is_empty() {
        let want: Vec<String> = [bare(title), bare(folder)]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect();
        for (id, songs) in seen.made {
            if songs.iter().any(|song| want.contains(&bare(song))) && found.insert(*id) {
                others.push(*id);
            }
        }
    }
    for id in others {
        if id != pack_id {
            take(other_date(dates, id, &name_of(id), today));
        }
    }
    best
}

// ---------------------------------------------------------------------------

/// Relay `inEras`: whether a date is in any of the eras. A ceiling is only
/// when everything before it is: eras that meet count as one.
#[must_use]
pub(super) fn in_eras(date: PackDate, eras: &[(u32, u32)]) -> bool {
    if !date.ceiling {
        return eras
            .iter()
            .any(|&(from, until)| date.day >= from && date.day < until);
    }
    let mut sorted = eras.to_vec();
    sorted.sort_by_key(|&(from, _)| from);
    let mut reach = 0;
    for (from, until) in sorted {
        if from > reach {
            break;
        }
        reach = reach.max(until);
    }
    date.day < reach
}

/// Relay `mayBeIn`: whether a pack can be in the eras before it is opened, by
/// the weekly file and its name. A date in one of them -- its game's, or its
/// listing's -- or else no ceiling (its upload, the end of the year after its
/// name's) before the first begins. A pack the file does not list yet is
/// judged when opened.
#[must_use]
pub(super) fn may_be_in(
    dates: Option<&PackDates>,
    pack_id: u64,
    name: &str,
    eras: &[(u32, u32)],
    today: u32,
) -> bool {
    let game = game_year(name);
    if game != 0 {
        return in_eras(game_date(game), eras);
    }
    let listed = listed_date(dates, pack_id, today);
    if let Some(listed) = listed
        && !listed.ceiling
    {
        return in_eras(listed, eras);
    }
    let mut ceiling = listed.map_or(99_999_999, |listed| listed.day);
    let (named, _) = name_year(name, today);
    if named != 0 && named <= year_of(ceiling) {
        ceiling = ceiling.min((named + 1) * 10_000 + 1231);
    }
    eras.iter().any(|&(from, _)| from <= ceiling)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::pack_archive::SongFolder;

    const TODAY: u32 = 20_261_017;
    const VINTAGE: (u32, u32) = (0, 20_150_101);
    const MID: (u32, u32) = (20_150_101, 20_200_101);
    const MODERN: (u32, u32) = (20_200_101, 99_999_999);

    fn file(name: &str, day: u32) -> ZipEntry {
        ZipEntry {
            name: name.to_owned(),
            compressed: 1000,
            uncompressed: 1000,
            local_header: 0,
            end: 0,
            day,
            crc: 0,
            site_name: None,
        }
    }

    /// An index of these entries, with a song folder for each name that
    /// `Pack/<name>/...` gives.
    fn index(entries: Vec<ZipEntry>) -> PackIndex {
        let mut folders: Vec<SongFolder> = Vec::new();
        for entry in &entries {
            let mut parts = entry.name.split('/');
            if let (Some(_), Some(folder), Some(_)) = (parts.next(), parts.next(), parts.next())
                && !folders.iter().any(|have| have.name == folder)
            {
                folders.push(SongFolder {
                    name: folder.to_owned(),
                    entries: Vec::new(),
                    simfile: None,
                    audio: Vec::new(),
                });
            }
        }
        PackIndex {
            pack_id: 1,
            total: 0,
            etag: None,
            entries,
            folders,
            tail_start: 0,
            tail: Arc::from(Vec::<u8>::new()),
        }
    }

    /// Songs `Pack/Song <n>/`, each its audio's date and its simfile's.
    fn songs_dated(days: &[(u32, u32)]) -> Vec<ZipEntry> {
        days.iter()
            .enumerate()
            .flat_map(|(n, &(audio, chart))| {
                [
                    file(&format!("Pack/Song {n}/song.ogg"), audio),
                    file(&format!("Pack/Song {n}/song.ssc"), chart),
                ]
            })
            .collect()
    }

    /// Songs whose audio and simfile both carry these dates.
    fn songs_on(days: &[u32]) -> Vec<ZipEntry> {
        songs_dated(&days.iter().map(|&day| (day, day)).collect::<Vec<_>>())
    }

    fn facts(entries: Vec<ZipEntry>) -> ZipFacts {
        zip_facts(&index(entries), TODAY)
    }

    /// Songs spread over 2010, their median the 20th of May.
    fn of_2010() -> ZipFacts {
        facts(songs_on(&[
            20100105, 20100310, 20100520, 20100801, 20101001,
        ]))
    }

    fn listing(rows: &[(u64, u32, u32)], bulk: &[u32]) -> PackDates {
        PackDates {
            packs: rows
                .iter()
                .map(|&(id, up, release)| (id, (up, release)))
                .collect(),
            bulk: bulk.iter().copied().collect(),
        }
    }

    fn on(day: u32) -> PackDate {
        PackDate::on(day)
    }

    fn under(day: u32, year: u32) -> PackDate {
        PackDate {
            day,
            year,
            ceiling: true,
        }
    }

    /// `pack_date` for pack 1, with no `Pack.ini` year.
    fn dated(dates: Option<&PackDates>, name: &str, facts: &ZipFacts) -> PackDate {
        pack_date(dates, 1, name, facts, None, TODAY)
    }

    #[test]
    fn day_numbers_run_over_as_date_utc_does() {
        assert_eq!(day_number(19700101), 0);
        assert_eq!(day_number(2015), day_number(20150101));
        assert_eq!(day_number(20250231), day_number(20250303));
        assert_eq!(day_number(20240231), day_number(20240302));
        assert_eq!(day_number(20251300), day_number(20251231));
        assert_eq!(ymd(day_number(20250231)), 20250303);
        for day in [19990101, 20000229, 20191231, 20200101, 20261017] {
            assert_eq!(ymd(day_number(day)), day);
        }
        assert_eq!(latest(20261231), 20270101);
    }

    #[test]
    fn today_is_a_real_date() {
        let today = today();
        assert!(today > 20260000, "{today}");
        assert_eq!(ymd(day_number(today)), today);
    }

    #[test]
    fn chart_keys_fold_width_case_accents_and_tags() {
        assert_eq!(
            chart_key("ＰＡＮＤＥＭＯＮＩＵＭ", "Ｍａｄ Matt"),
            "pandemonium|madmatt"
        );
        assert_eq!(chart_key("[2004] [17] Café Rêve!", "Kyle"), "caférêve|kyle");
        // a decomposed accent composes to the same letter
        assert_eq!(chart_key("Cafe\u{301} Rêve", "kyle"), "caférêve|kyle");
        assert_eq!(
            chart_key("千本桜 (Long ver.)", "ゆう"),
            "千本桜longver|ゆう"
        );
        // a vowel sign is a mark, not a letter
        assert_eq!(chart_key("\u{928}\u{93F}", "x"), "\u{928}|x");
        assert_eq!(chart_key("[Tag]", "Mad Matt"), "|madmatt");
        assert_eq!(bare("[17] Pandemonium (Ｘ)"), "pandemonium");
    }

    #[test]
    fn ini_year_takes_the_first_year_line() {
        assert_eq!(ini_year(b"[Group]\r\nName=X\r\nYear=2009\r\n"), Some(2009));
        assert_eq!(ini_year(b"  yEaR = 2010"), Some(2010));
        assert_eq!(ini_year(b"Year\n=\n2012"), Some(2012));
        assert_eq!(ini_year(b"\xA0Year=2011"), Some(2011));
        assert_eq!(ini_year(b"Year=20091"), Some(2009));
        assert_eq!(ini_year(b"Year=abcd\nYear=2013"), Some(2013));
        // the first match decides, even when it says none
        assert_eq!(ini_year(b"Year=0000\nYear=2013"), None);
        assert_eq!(ini_year(b"MyYear=2009"), None);
        assert_eq!(ini_year(b"Year=99"), None);
        assert_eq!(ini_year(b"\x85Year=2009"), None);
        assert_eq!(ini_year(b""), None);
    }

    #[test]
    fn names_give_years_and_bounds() {
        assert_eq!(name_year("File of the Week 2007", TODAY), (2007, false));
        assert_eq!(name_year("Individual Files Pre-2016", TODAY), (2015, true));
        assert_eq!(name_year("Stuff before - 2013", TODAY), (2012, true));
        assert_eq!(name_year("pre2010 pack", TODAY), (2009, true));
        assert_eq!(name_year("Prelude 2013", TODAY), (2013, false));
        assert_eq!(name_year("Pack 2015-2018", TODAY), (2018, false));
        assert_eq!(name_year("2019 Pack 2014", TODAY), (2019, false));
        // a bound's year and a plain one alike: the first stands
        assert_eq!(name_year("Pre-2016 and 2015", TODAY), (2015, true));
        assert_eq!(name_year("Pack 19992000", TODAY), (0, false));
        assert_eq!(name_year("Pack 1998", TODAY), (0, false));
        assert_eq!(name_year("Pack 2030", TODAY), (0, false));
    }

    #[test]
    fn official_games_give_their_years() {
        let year = |name: &str| game_year(name);
        assert_eq!(year("Dance Dance Revolution EXTREME (PS2) (NA)"), 2004);
        assert_eq!(year("Dance Dance Revolution EXTREME (PS2) (JP)"), 2003);
        // `.*` stops at a line break
        assert_eq!(year("Dance Dance Revolution EXTREME\n(PS2) (NA)"), 2002);
        assert_eq!(year("DDRMAX (PS2) (NA)"), 2002);
        assert_eq!(year("Dance Dance Revolution (AC)"), 1998);
        assert_eq!(year("Dance Dance Revolution 3rdMIX (AC) (JP)"), 1999);
        assert_eq!(year("Dance Dance Revolution ULTRAMIX (Xbox)"), 2003);
        assert_eq!(year("Dance Dance Revolution ULTRAMIX2 (Xbox)"), 2004);
        assert_eq!(year("Dance Dance Revolution ULTRAMIX3 (Xbox)"), 2005);
        assert_eq!(year("Dancing Stage Unleashed (Xbox)"), 2004);
        assert_eq!(year("Dancing Stage Unleashed3 (Xbox)"), 2006);
        assert_eq!(year("Dance Dance Revolution UNIVERSE3 (Xbox 360)"), 2008);
        assert_eq!(year("Dance Dance Revolution S+ (Mobile)"), 2011);
        // named for no platform: not a rip of the game
        assert_eq!(year("DDR Extreme Remixes"), 0);
        assert_eq!(year("DDR Hottest Party (Wii)"), 0);
        assert!(is_official("DDR Hottest Party (Wii)"));
    }

    #[test]
    fn the_relay_index_counts_only_its_own_songs() {
        let entries = vec![
            file("Pack/", 0),
            file("Pack/Song 1/", 0),
            file("Pack/Song 1/a.ogg", 20100101),
            // flattens alike: the same song
            file("Pack/Song-1/b.MP3", 20100101),
            // only .wav, too deep, or empty: none
            file("Pack/Wave/song.wav", 20100101),
            file("Pack/Deep/sub/song.ogg", 20100101),
            ZipEntry {
                compressed: 0,
                ..file("Pack/Empty/song.ogg", 20100101)
            },
            // a chart with no audio is not a song
            file("Pack/Chart/song.ssc", 20100101),
            file("Pack/Song 2/song.ogg", 20100101),
        ];
        let index = index(entries);
        let songs = songs(&index);
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].dir, "Pack/Song 1/");
        assert_eq!(songs[0].audio.len(), 2);
    }

    #[test]
    fn zip_dates_in_one_stamp_say_nothing() {
        let stamp = facts(songs_on(&[20200101, 20200102, 20200103]));
        assert!(stamp.stamped);
        assert_eq!(stamp.median, 0);
        assert_eq!(stamp.stamp_day, 20200102);

        // audio within three days and a resync's backup beside it: re-zipped
        let mut entries = songs_dated(&[
            (20200101, 20150101),
            (20200102, 20150601),
            (20200104, 20160101),
        ]);
        let unzipped = facts(entries.clone());
        assert!(!unzipped.stamped);
        entries.push(file("Pack/Song 0/song.ssc.oldsync", 20200104));
        assert!(facts(entries).stamped);
    }

    #[test]
    fn the_busiest_days_count_their_neighbours() {
        assert_eq!(
            busiest(&[20200101, 20200102, 20200103, 20200110], 2),
            vec![(20200102, 3), (20200110, 1)]
        );
        // the 31st of February is the 3rd of March
        assert_eq!(busiest(&[20250231, 20250303], 2), vec![(20250303, 2)]);
        assert!(busiest(&[], 2).is_empty());
    }

    #[test]
    fn listings_give_a_date_or_a_ceiling() {
        let dates = listing(
            &[
                (1, 20150301, 20140601),
                (2, 20160913, 2014),
                (3, 20150301, 20150501),
                (4, 20150301, 20150325),
                (5, 20150301, 0),
                (6, 20970101, 20100505),
                (7, 20970101, 0),
            ],
            &[],
        );
        let listed = |id| listed_date(Some(&dates), id, TODAY);
        assert_eq!(listed(1), Some(on(20140601)));
        assert_eq!(listed(2), Some(on(20140101)));
        // released over a month after the upload: some later release's date
        assert_eq!(listed(3), Some(on(20150301)));
        assert_eq!(listed(4), Some(on(20150325)));
        assert_eq!(listed(5), Some(under(20150301, 2015)));
        assert_eq!(listed(6), Some(on(20100505)));
        assert_eq!(listed(7), None);
        assert_eq!(listed(8), None);
        assert_eq!(listed_date(None, 1, TODAY), None);
    }

    #[test]
    fn a_game_or_a_firm_listing_dates_a_pack_first() {
        let dates = listing(&[(1, 20150301, 20140601)], &[]);
        let game = "Dance Dance Revolution EXTREME (PS2) (NA)";
        assert_eq!(
            pack_date(Some(&dates), 1, game, &of_2010(), Some(2012), TODAY),
            on(20040701)
        );
        assert_eq!(
            pack_date(Some(&dates), 1, "Pack", &of_2010(), Some(2012), TODAY),
            on(20140601)
        );
        // neither looks at the Pack.ini; an upload alone does
        let uploaded = listing(&[(1, 20150301, 20140601), (2, 20150301, 0)], &[]);
        assert!(!wants_ini_year(Some(&uploaded), 3, game, TODAY));
        assert!(!wants_ini_year(Some(&uploaded), 1, "Pack", TODAY));
        assert!(wants_ini_year(Some(&uploaded), 2, "Pack", TODAY));
        assert!(wants_ini_year(Some(&uploaded), 3, "Pack", TODAY));
        assert!(wants_ini_year(None, 1, "Pack", TODAY));
    }

    #[test]
    fn a_pack_ini_year_dates_a_pack_no_later_than_its_upload() {
        let ini = |rows: &[(u64, u32, u32)], year| {
            pack_date(
                Some(&listing(rows, &[])),
                1,
                "Pack",
                &ZipFacts::default(),
                Some(year),
                TODAY,
            )
        };
        assert_eq!(ini(&[(1, 20200101, 0)], 2012), on(20120701));
        assert_eq!(ini(&[(1, 20120301, 0)], 2012), on(20120301));
        // later than the upload, or before DDR's PC days: not believed
        assert_eq!(ini(&[(1, 20110301, 0)], 2012), under(20110301, 2011));
        assert_eq!(ini(&[(1, 20110301, 0)], 1998), under(20110301, 2011));
        assert_eq!(ini(&[], 2026), on(20260701));
        assert_eq!(ini(&[], 2027), under(TODAY, 2026));
    }

    #[test]
    fn a_zip_dates_a_pack_by_its_audio_median() {
        let facts = of_2010();
        assert!(!facts.stamped);
        assert_eq!(facts.median, 20100520);
        assert_eq!(dated(None, "Some Pack", &facts), on(20100520));
        // no later than the upload
        let dates = listing(&[(1, 20100401, 0)], &[]);
        assert_eq!(dated(Some(&dates), "Some Pack", &facts), on(20100401));
    }

    #[test]
    fn audio_redone_at_once_is_only_a_ceiling() {
        let one_day = facts(songs_on(&[
            20170101, 20180505, 20180505, 20180505, 20180505,
        ]));
        assert_eq!(one_day.batch, 20180505);
        assert_eq!(dated(None, "Pack", &one_day), under(20180505, 2018));
        let dates = listing(&[(1, 20180101, 0)], &[]);
        assert_eq!(dated(Some(&dates), "Pack", &one_day), under(20180101, 2018));

        // two days far apart that hold 80% between them: the later
        let two_days = facts(songs_on(&[
            20120101, 20120101, 20120101, 20150101, 20170101, 20170101,
        ]));
        assert_eq!(two_days.batch, 20170101);
        assert_eq!(dated(None, "Pack", &two_days), under(20170101, 2015));
    }

    #[test]
    fn a_fifth_of_old_songs_makes_the_pack_old() {
        let facts = facts(songs_on(&[
            20100105, 20100605, 20101105, 20150105, 20150305, 20150505, 20150705, 20150905,
            20151105, 20160105,
        ]));
        assert_eq!(facts.median, 20150505);
        assert_eq!(facts.old_songs, 20100605);
        assert_eq!(dated(None, "Pack", &facts), on(20100605));
    }

    #[test]
    fn charts_finished_after_the_audio_date_the_pack() {
        let audio = [20120105, 20120205, 20120305, 20120405, 20120505];
        let later =
            |chart: u32| songs_dated(&audio.iter().map(|&day| (day, chart)).collect::<Vec<_>>());
        assert_eq!(dated(None, "Pack", &facts(later(20120905))), on(20120905));
        // over two years later is a remake, not the pack being finished
        assert_eq!(dated(None, "Pack", &facts(later(20150905))), on(20120305));
        // and a resync's dates are the resync's
        let mut resynced = later(20120905);
        resynced.extend(
            (0..audio.len()).map(|n| file(&format!("Pack/Song {n}/song.ssc.oldsync"), 20200101)),
        );
        let resynced = facts(resynced);
        assert!(resynced.resynced);
        assert_eq!(dated(None, "Pack", &resynced), on(20120305));
    }

    #[test]
    fn a_bulk_upload_day_makes_a_close_date_a_ceiling() {
        let facts = facts(songs_on(&[
            20141101, 20141115, 20141201, 20141215, 20150101,
        ]));
        let bulk = listing(&[(1, 20150601, 0)], &[20150601]);
        assert_eq!(dated(Some(&bulk), "Pack", &facts), under(20150601, 2014));
        let quiet = listing(&[(1, 20150601, 0)], &[]);
        assert_eq!(dated(Some(&quiet), "Pack", &facts), on(20141201));
    }

    #[test]
    fn a_name_year_overrules_a_date_far_from_it() {
        let facts = of_2010();
        assert_eq!(dated(None, "File of the Week 2007", &facts), on(20071231));
        assert_eq!(dated(None, "Tournament 2016", &facts), on(20161231));
        assert_eq!(dated(None, "Pack 2009", &facts), on(20100520));
        // and stands in for none, no later than the upload
        let none = ZipFacts::default();
        assert_eq!(dated(None, "File of the Week 2007", &none), on(20071231));
        let up = listing(&[(1, 20070301, 0)], &[]);
        assert_eq!(
            dated(Some(&up), "File of the Week 2007", &none),
            on(20070301)
        );
        // a year after the upload's is not believed
        let early = listing(&[(1, 20060301, 0)], &[]);
        assert_eq!(
            dated(Some(&early), "File of the Week 2007", &none),
            under(20060301, 2006)
        );
    }

    #[test]
    fn a_pre_year_only_bounds_the_date() {
        let name = "Individual Files Pre-2016";
        assert_eq!(dated(None, name, &of_2010()), on(20100520));
        let later = facts(songs_on(&[
            20180105, 20180305, 20180505, 20180705, 20180905,
        ]));
        assert_eq!(dated(None, name, &later), on(20151231));
    }

    #[test]
    fn dwi_charts_cap_a_pack_at_2019() {
        let mut entries = songs_on(&[20210105, 20210305, 20210505, 20210705, 20210905]);
        entries.push(file("Pack/Song 0/song.dwi", 20210105));
        let facts = facts(entries);
        assert!(facts.dwi);
        assert_eq!(dated(None, "Pack", &facts), under(20191231, 2019));
    }

    #[test]
    fn an_unknown_official_game_is_only_a_guess() {
        assert_eq!(
            dated(None, "DDR Hottest Party (Wii)", &of_2010()),
            under(20100520, 2010)
        );
    }

    #[test]
    fn old_art_dates_a_pack() {
        let audio = [20150105, 20150305, 20150505, 20150705, 20150905];
        let mut entries = songs_on(&audio);
        entries.push(file("Pack/banner.png", 20100101));
        entries.push(file("Pack/Song 0/bg.JPG", 20100101));
        entries.push(file("Pack/readme.txt", 20120101));
        let old = facts(entries);
        assert_eq!(old.art, 20100101);
        assert_eq!(dated(None, "Pack", &old), on(20100101));

        let mut entries = songs_on(&audio);
        entries.push(file("Pack/readme.txt", 20140101));
        assert_eq!(dated(None, "Pack", &facts(entries)), on(20150505));
    }

    #[test]
    fn a_ceiling_guesses_its_year() {
        let stamp = facts(songs_on(&[20200303, 20200303]));
        assert!(stamp.stamped);
        let up = listing(&[(1, 20210101, 0)], &[]);
        assert_eq!(dated(Some(&up), "Pack", &stamp), under(20210101, 2020));
        assert_eq!(
            dated(None, "Pack", &ZipFacts::default()),
            under(TODAY, 2026)
        );
    }

    #[test]
    fn an_old_song_keeps_its_own_date() {
        let mut entries = songs_on(&[20200105, 20200305, 20200505, 20200705, 20200905]);
        entries.extend([
            file("Pack/Old/song.sm", 20090101),
            file("Pack/Old/song.ogg", 20100101),
            // the game loads the .ssc
            file("Pack/Old/song.ssc", 20110101),
            file("Pack/Recent/song.ogg", 20190901),
            file("Pack/Recent/song.ssc", 20190901),
            file("Pack/Bare/song.ogg", 20100101),
            file("Pack/Wave/song.wav", 20100101),
            file("Pack/Wave/song.ssc", 20100101),
        ]);
        let index = index(entries);
        let facts = zip_facts(&index, TODAY);
        let pack = Some(on(20200505));
        let folder = |name: &str| {
            index
                .folders
                .iter()
                .position(|folder| folder.name == name)
                .unwrap()
        };
        let song = |name: &str| song_date(pack, &facts, &index, folder(name), TODAY);
        assert_eq!(song("Old"), Some(on(20110101)));
        // within a year of the pack, with no simfile, or with no audio the
        // relay knows: the pack's
        assert_eq!(song("Recent"), pack);
        assert_eq!(song("Bare"), pack);
        assert_eq!(song("Wave"), pack);
        assert_eq!(song_date(None, &facts, &index, folder("Old"), TODAY), None);
        let stamped = ZipFacts {
            stamped: true,
            ..facts
        };
        assert_eq!(
            song_date(pack, &stamped, &index, folder("Old"), TODAY),
            pack
        );
    }

    #[test]
    fn a_songs_audio_is_its_biggest_file() {
        let mut entries = songs_on(&[20200105, 20200305, 20200505]);
        entries.extend([
            ZipEntry {
                uncompressed: 10,
                ..file("Pack/Old/preview.ogg", 20100101)
            },
            ZipEntry {
                uncompressed: 5000,
                ..file("Pack/Old/song.ogg", 20200101)
            },
            file("Pack/Old/song.ssc", 20100101),
        ]);
        let index = index(entries);
        let facts = zip_facts(&index, TODAY);
        let old = index.folders.iter().position(|f| f.name == "Old").unwrap();
        let pack = Some(on(20200505));
        assert_eq!(song_date(pack, &facts, &index, old, TODAY), pack);
    }

    #[test]
    fn a_chart_is_as_old_as_its_first_appearance() {
        let names = |id: u64| match id {
            9 => "Pack 2005".to_owned(),
            11 => "Individual Files Pre-2016".to_owned(),
            _ => format!("Pack {id}"),
        };
        let dates = listing(
            &[
                (1, 20010101, 20000101),
                (5, 20060101, 20050505),
                (7, 20090101, 20080505),
            ],
            &[],
        );
        let mut firsts = ChartFirsts::default();
        firsts
            .charts
            .insert(chart_key("Pandemonium", "Kyle"), vec![1, 7, 9]);
        firsts
            .charts
            .insert(chart_key("Pandemonium (Folder)", "Kyle"), vec![11]);
        let seen = Appearances {
            firsts: Some(&firsts),
            made: &[],
        };
        let first = |date, title: &str, folder: &str, credit: &str, seen: Appearances<'_>| {
            chart_date(
                date,
                1,
                title,
                folder,
                credit,
                seen,
                Some(&dates),
                &names,
                TODAY,
            )
        };
        let now = Some(on(20200101));
        let unseen = Appearances::default();

        // a year at the head of the title, whoever charted it
        let tagged = first(now, "  [2004] Pandemonium", "x", "Expert", unseen);
        assert_eq!(tagged, Some(on(20041231)));
        let older = Some(on(20000101));
        assert_eq!(
            first(older, "[2004] Pandemonium", "x", "Kyle", unseen),
            older
        );
        // the first pack it is in, its own aside
        assert_eq!(
            first(now, "Pandemonium", "x", "Kyle", seen),
            Some(on(20051231))
        );
        assert_eq!(
            first(None, "Pandemonium", "x", "Kyle", seen),
            Some(on(20051231))
        );
        assert_eq!(
            first(now, "Song", "Pandemonium (Folder)", "Kyle", seen),
            Some(under(20151231, 2015))
        );
        // nobody's chart is not looked for elsewhere
        assert_eq!(first(now, "Pandemonium", "x", " Expert ", seen), now);
        // never later than the date the song has
        let early = Some(on(20050101));
        assert_eq!(first(early, "Pandemonium", "x", "Kyle", seen), early);

        // an author rip's own search
        let made = [
            (5, vec!["[17] PANDEMONIUM".to_owned()]),
            (7, vec!["Other".to_owned()]),
        ];
        let searched = Appearances {
            firsts: None,
            made: &made,
        };
        assert_eq!(
            first(now, "Pandemonium", "x", "Kyle", searched),
            Some(on(20050505))
        );
    }

    #[test]
    fn a_ceiling_is_in_eras_only_when_all_before_it_is() {
        assert!(in_eras(on(20160101), &[MID]));
        assert!(!in_eras(on(20160101), &[VINTAGE, MODERN]));
        let ceiling = under(20180101, 2017);
        assert!(in_eras(ceiling, &[VINTAGE, MID]));
        assert!(in_eras(ceiling, &[MID, VINTAGE]));
        assert!(!in_eras(ceiling, &[MID]));
        assert!(!in_eras(ceiling, &[VINTAGE]));
        assert!(!in_eras(ceiling, &[VINTAGE, MODERN]));
        assert!(in_eras(under(20120101, 2011), &[VINTAGE, MODERN]));
        assert!(!in_eras(under(20150101, 2014), &[VINTAGE]));
    }

    #[test]
    fn packs_are_passed_over_before_opening_only_when_they_cannot_fit() {
        let dates = listing(&[(1, 20200101, 20120101), (2, 20120101, 0)], &[]);
        let may =
            |id, name: &str, eras: &[(u32, u32)]| may_be_in(Some(&dates), id, name, eras, TODAY);
        let game = "Dance Dance Revolution EXTREME (PS2) (NA)";
        assert!(may(3, game, &[VINTAGE]));
        assert!(!may(3, game, &[MODERN]));
        assert!(may(1, "Pack", &[VINTAGE]));
        assert!(!may(1, "Pack", &[MODERN]));
        // uploaded in 2012: nothing later
        assert!(may(2, "Pack", &[VINTAGE]));
        assert!(!may(2, "Pack", &[MID, MODERN]));
        // not listed, but named for 2012: nothing after 2013
        assert!(!may(3, "File of the Week 2012", &[MID]));
        assert!(may(3, "File of the Week 2012", &[VINTAGE]));
        // not listed yet: judged when opened
        assert!(may(3, "Pack", &[MODERN]));
        assert!(may_be_in(None, 2, "Pack", &[MODERN], TODAY));
    }

    #[test]
    fn credits_that_name_nobody() {
        assert!(is_name("Kyle"));
        assert!(!is_name("Expert"));
        assert!(!is_name("UNKNOWN"));
        assert!(!is_name("x"));
        assert!(!is_name("1234"));
        assert!(!is_name(&"a".repeat(41)));
        assert!(is_name("😀"));
    }
}
