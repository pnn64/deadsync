//! One pack's own page on stepmaniaonline.net.
//!
//! The catalog CSV and the datatables endpoint both describe packs from the
//! outside -- a name, a size, a date. Neither says what is *in* one. The site
//! renders that on `/pack/<id>`: a table of songs with jacket art, artist,
//! length, bpm, chart credits and per-song meters, plus the chart-count
//! histogram the page draws with Chart.js.
//!
//! Read on demand, one pack at a time, because it is only ever wanted for the
//! pack whose detail page is open. Results are cached so backing out and
//! returning is free, and the cache is bounded so a long browse cannot grow
//! without limit.

use deadsync_net as network;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

const PACK_URL_BASE: &str = "https://stepmaniaonline.net/pack/";
const MEDIA_BASE: &str = "https://stepmaniaonline.net";
/// A pack page with a thousand songs in it is a few hundred kilobytes of
/// table. Two megabytes is well clear of the largest and still a bound.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Pack pages held at once. Each is a few tens of kilobytes of parsed text.
const MAX_CACHED: usize = 12;
/// Meters above this are joke charts; they would blow out the histogram scale
/// and print ranges like "1 - 5454".
const METER_MAX: u32 = 30;

/// One row of the pack's song table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SongRow {
    pub title: String,
    pub artist: String,
    /// Playing time as the site prints it, e.g. `1:52`.
    pub length: String,
    /// May itself be a range, e.g. `150-190`.
    pub bpm: String,
    /// Chart authors, comma-joined.
    pub credit: String,
    /// The meters as the site prints them, e.g. `3, 6, 9, 12`.
    pub meters: String,
    /// Absolute URL of the jacket, when the site has one that is not its own
    /// "no banner" placeholder.
    pub image_url: Option<String>,
    /// The styles this song actually charts for -- `dance-single`,
    /// `dance-double` and friends.
    ///
    /// The ITGmania module reads this column as the page's repeated "Filter By
    /// Mode" control and throws it away, because the *images* in the cell are
    /// the same four buttons on every row. The cell's `data-sort` attribute is
    /// not: it carries this song's real styles, and it differs row to row.
    pub styles: Vec<String>,
}

impl SongRow {
    /// The easiest chart in this row.
    ///
    /// The first number the site prints, which is how the meters column is
    /// ordered. This is what a beginner would actually be playing.
    #[must_use]
    pub fn low_meter(&self) -> Option<u32> {
        let meters = self.meters.trim_start();
        let end = meters
            .find(|ch: char| !ch.is_ascii_digit())
            .unwrap_or(meters.len());
        meters[..end].parse().ok()
    }

    /// The hardest meter in the row, which is what tints the meter string.
    #[must_use]
    pub fn top_meter(&self) -> u32 {
        let mut top = 0;
        let mut current: u32 = 0;
        let mut in_number = false;
        for ch in self.meters.chars() {
            if let Some(digit) = ch.to_digit(10) {
                current = current.saturating_mul(10).saturating_add(digit);
                in_number = true;
            } else if in_number {
                top = top.max(current);
                current = 0;
                in_number = false;
            }
        }
        if in_number { top.max(current) } else { top }
    }

    /// `artist  -  150 bpm  -  1:52  -  Charter`, with any part absent.
    #[must_use]
    pub fn subline(&self) -> String {
        let parts = [
            (self.artist.as_str(), ""),
            (self.bpm.as_str(), " bpm"),
            (self.length.as_str(), ""),
            (self.credit.as_str(), ""),
        ]
        .into_iter()
        .filter(|(text, _)| !text.is_empty());
        let capacity = parts
            .clone()
            .map(|(text, suffix)| text.len() + suffix.len() + "  -  ".len())
            .sum::<usize>()
            .saturating_sub("  -  ".len());
        let mut out = String::with_capacity(capacity);
        for (text, suffix) in parts {
            if !out.is_empty() {
                out.push_str("  -  ");
            }
            out.push_str(text);
            out.push_str(suffix);
        }
        out
    }
}

/// Everything the pack page says.
#[derive(Clone, Debug, Default)]
pub struct PackPage {
    pub songs: Vec<SongRow>,
    /// Difficulty meters present in the pack, ascending.
    pub meter_labels: Vec<u32>,
    /// How many charts sit at each of `meter_labels`.
    pub meter_counts: Vec<u32>,
    /// The site's own chart total.
    pub chart_count: Option<String>,
    /// Distinct chart authors, in the order they first appear.
    pub authors: Vec<String>,
    /// Every style any song in the pack charts for, deduplicated.
    pub styles: Vec<String>,
    /// The pack's banner, from the page's `og:image` -- the one place a pack
    /// no list has described still says where its art is. `None` for the
    /// site's "no banner" stand-in, as everywhere else.
    pub banner_url: Option<String>,
}

impl PackPage {
    /// The easiest and hardest meters in the pack.
    #[must_use]
    pub fn difficulty_span(&self) -> Option<(u32, u32)> {
        let low = *self.meter_labels.first()?;
        let high = *self.meter_labels.last()?;
        Some((low, high))
    }

    /// What a player would call this pack: the original's `StyleOf`.
    ///
    /// `None` when the page said nothing about styles, so a caller can fall
    /// back to the catalogue's own column rather than printing a guess.
    #[must_use]
    pub fn style_label(&self) -> Option<&'static str> {
        if self.styles.is_empty() {
            return None;
        }
        if self.styles.iter().any(|style| style.starts_with("kb")) {
            return Some("Keyboard");
        }
        let singles = self.styles.iter().any(|style| style.ends_with("-single"));
        let doubles = self.styles.iter().any(|style| style.ends_with("-double"));
        Some(match (singles, doubles) {
            (true, true) => "Singles + Doubles",
            (false, true) => "Doubles",
            _ => "Singles",
        })
    }

    /// Whether any song here carries a doubles chart.
    #[must_use]
    pub fn has_doubles(&self) -> bool {
        self.styles.iter().any(|style| style.ends_with("-double"))
    }

    /// Whether this is a pack worth handing a beginner.
    ///
    /// The original's test, verbatim: **most of the pack's songs have their
    /// easiest chart rated 1 to 4**. Not "has an easy chart" -- nearly every
    /// pack has one of those, and a single easy chart bolted onto a hard pack
    /// is not something anyone can work through. The majority is what
    /// separates a pack a beginner can play from one they can sample.
    ///
    /// Songs whose meters the site did not print are not counted either way:
    /// they are absent evidence, not evidence of difficulty.
    #[must_use]
    pub fn is_beginner_friendly(&self) -> bool {
        /// The easiest block a beginner chart sits in...
        const FLOOR: u32 = 1;
        /// ...and the hardest.
        const CEIL: u32 = 4;

        let mut easy = 0usize;
        let mut total = 0usize;
        for song in &self.songs {
            let Some(low) = song.low_meter() else {
                continue;
            };
            total += 1;
            if (FLOOR..=CEIL).contains(&low) {
                easy += 1;
            }
        }
        total > 0 && easy * 2 > total
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PagePhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

/// What the browser sees for the pack whose detail page is open.
#[derive(Clone, Debug, Default)]
pub struct PageSnapshot {
    pub phase: PagePhase,
    pub pack_id: u64,
    pub page: Option<Arc<PackPage>>,
    pub message: Option<String>,
    pub revision: u64,
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<PageSnapshot>,
    /// Pack pages already read, with their arrival order for eviction.
    cache: HashMap<u64, Arc<PackPage>>,
    order: Vec<u64>,
    /// Packs whose page could not be read, so a detail page opened twice does
    /// not ask twice.
    failed: HashMap<u64, String>,
    sender: Option<SyncSender<u64>>,
    ready: Option<Receiver<(u64, Result<PackPage, String>)>>,
    revision: u64,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

#[must_use]
pub fn runtime_snapshot() -> Arc<PageSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Ask for one pack's page, and file whatever has arrived.
///
/// Cheap and idempotent: a page already cached is published straight away, a
/// page already in flight is left alone, and a page that has already failed is
/// reported rather than retried. Safe to call every frame.
pub fn runtime_want(pack_id: u64) {
    let mut runtime = lock_runtime();
    drain(&mut runtime);

    if runtime.snapshot.pack_id == pack_id
        && matches!(
            runtime.snapshot.phase,
            PagePhase::Loading | PagePhase::Ready
        )
    {
        return;
    }
    if let Some(page) = runtime.cache.get(&pack_id).cloned() {
        publish(&mut runtime, pack_id, PagePhase::Ready, Some(page), None);
        return;
    }
    if let Some(message) = runtime.failed.get(&pack_id).cloned() {
        publish(&mut runtime, pack_id, PagePhase::Error, None, Some(message));
        return;
    }

    ensure_worker(&mut runtime);
    let Some(sender) = runtime.sender.clone() else {
        return;
    };
    match sender.try_send(pack_id) {
        Ok(()) => publish(&mut runtime, pack_id, PagePhase::Loading, None, None),
        // The reader is moving through packs faster than the site answers.
        // Dropping is right: the next frame asks again for whichever pack they
        // have actually settled on.
        Err(TrySendError::Full(_)) => {}
        Err(TrySendError::Disconnected(_)) => {
            runtime.sender = None;
            runtime.ready = None;
        }
    }
}

/// Nothing is open: stop reporting whichever pack was last read.
pub fn runtime_clear() {
    let mut runtime = lock_runtime();
    drain(&mut runtime);
    if runtime.snapshot.phase == PagePhase::Idle {
        return;
    }
    publish(&mut runtime, 0, PagePhase::Idle, None, None);
}

/// Read the whole pack page again, even though it is cached.
pub fn runtime_refresh(pack_id: u64) {
    {
        let mut runtime = lock_runtime();
        runtime.cache.remove(&pack_id);
        runtime.order.retain(|id| *id != pack_id);
        runtime.failed.remove(&pack_id);
        publish(&mut runtime, 0, PagePhase::Idle, None, None);
    }
    runtime_want(pack_id);
}

fn publish(
    runtime: &mut RuntimeState,
    pack_id: u64,
    phase: PagePhase,
    page: Option<Arc<PackPage>>,
    message: Option<String>,
) {
    runtime.revision = runtime.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(PageSnapshot {
        phase,
        pack_id,
        page,
        message,
        revision: runtime.revision,
    });
}

/// Take everything the worker has finished and file it.
fn drain(runtime: &mut RuntimeState) {
    let mut arrived: Vec<(u64, Result<PackPage, String>)> = Vec::new();
    if let Some(ready) = runtime.ready.as_ref() {
        loop {
            match ready.try_recv() {
                Ok(item) => arrived.push(item),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    runtime.sender = None;
                    runtime.ready = None;
                    break;
                }
            }
        }
    }
    for (pack_id, result) in arrived {
        match result {
            Ok(page) => {
                let page = Arc::new(page);
                runtime.cache.insert(pack_id, Arc::clone(&page));
                runtime.order.retain(|id| *id != pack_id);
                runtime.order.push(pack_id);
                while runtime.order.len() > MAX_CACHED {
                    let oldest = runtime.order.remove(0);
                    runtime.cache.remove(&oldest);
                }
                if runtime.snapshot.pack_id == pack_id {
                    publish(runtime, pack_id, PagePhase::Ready, Some(page), None);
                }
            }
            Err(message) => {
                runtime.failed.insert(pack_id, message.clone());
                if runtime.snapshot.pack_id == pack_id {
                    publish(runtime, pack_id, PagePhase::Error, None, Some(message));
                }
            }
        }
    }
}

fn ensure_worker(runtime: &mut RuntimeState) {
    if runtime.sender.is_some() {
        return;
    }
    let (jobs_tx, jobs_rx) = sync_channel::<u64>(4);
    let (done_tx, done_rx) = sync_channel::<(u64, Result<PackPage, String>)>(4);
    let spawn = thread::Builder::new()
        .name("smo-pack-page".to_owned())
        .spawn(move || {
            let agent = network::get_agent();
            while let Ok(pack_id) = jobs_rx.recv() {
                let result = fetch(&agent, pack_id);
                if done_tx.send((pack_id, result)).is_err() {
                    break;
                }
            }
        });
    if spawn.is_err() {
        log::warn!("Could not start the pack page worker; detail pages will list no songs.");
        return;
    }
    runtime.sender = Some(jobs_tx);
    runtime.ready = Some(done_rx);
}

/// Read one pack's page and parse it, on the calling thread.
///
/// Public within the crate so the beginner walk can read pages without going
/// through this module's single-slot cache, which belongs to whichever detail
/// page is open.
pub(crate) fn fetch_blocking(agent: &network::HttpAgent, pack_id: u64) -> Result<PackPage, String> {
    fetch(agent, pack_id)
}

fn fetch(agent: &network::HttpAgent, pack_id: u64) -> Result<PackPage, String> {
    let url = format!("{PACK_URL_BASE}{pack_id}");
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
    let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
        .map_err(|error| format!("{error:?}"))?;
    Ok(parse(body.as_str()))
}

// --- parsing -----------------------------------------------------------------

/// The text between `open` and the next `close`, starting the search at `from`.
/// Returns the text and where to resume.
fn between<'a>(
    haystack: &'a str,
    from: usize,
    open: &str,
    close: &str,
) -> Option<(&'a str, usize)> {
    let start = haystack.get(from..)?.find(open)? + from + open.len();
    let end = haystack.get(start..)?.find(close)? + start;
    Some((&haystack[start..end], end + close.len()))
}

/// The value of one HTML attribute in a tag, e.g. `src="..."`.
fn attribute<'a>(fragment: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = fragment.find(needle.as_str())? + needle.len();
    let end = fragment.get(start..)?.find('"')? + start;
    Some(&fragment[start..end])
}

/// Strip tags, decode the handful of entities the site emits, and squeeze the
/// whitespace -- the site's own `CleanText`.
fn clean(fragment: &str) -> String {
    let mut text = String::with_capacity(fragment.len());
    let mut in_tag = false;
    for ch in fragment.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            _ => text.push(ch),
        }
    }
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The style tokens inside a song row's `data-sort` list.
///
/// Read as tokens rather than parsed as a list: the attribute is HTML-escaped
/// Python-ish source, and every value that matters is a bare `game-mode` word.
fn style_tokens(attribute_value: &str) -> Vec<String> {
    /// The games the site charts for. Anything else in the attribute is not a
    /// style, and a bare token filter would have let a file path through.
    const GAMES: [&str; 6] = ["dance", "pump", "kb7", "para", "techno", "smx"];
    let decoded = attribute_value.replace("&#x27;", "'").replace("&#39;", "'");
    let mut out: Vec<String> = Vec::new();
    for piece in decoded.split(['[', ']', ',', '\'', '"', ' ']) {
        let piece = piece.trim();
        let known_game = piece
            .split_once('-')
            .is_some_and(|(game, _)| GAMES.contains(&game));
        if !known_game {
            continue;
        }
        if !out.iter().any(|known| known == piece) {
            out.push(piece.to_owned());
        }
    }
    out
}

/// Every integer in a comma-separated Chart.js array literal.
fn numbers(list: &str) -> Vec<u32> {
    list.split(',')
        .filter_map(|piece| {
            let mut digits = piece.bytes().filter(u8::is_ascii_digit);
            let mut value = u32::from(digits.next()? - b'0');
            for digit in digits {
                value = value
                    .checked_mul(10)?
                    .checked_add(u32::from(digit - b'0'))?;
            }
            Some(value)
        })
        .collect()
}

/// Turn a site-relative image path into an absolute URL, dropping the site's
/// own "no banner available" placeholder -- it carries no information, and the
/// screen has its own way of saying a row has no art.
fn media_url(path: &str) -> Option<String> {
    if path.is_empty() || path.contains("nobanner") {
        return None;
    }
    if path.starts_with("http") {
        return Some(path.to_owned());
    }
    Some(format!("{MEDIA_BASE}{path}"))
}

/// One of the page's own headline figures.
///
/// The site renders each as `<small ...>Charts</small>` followed by a div
/// carrying `text-white`, so the label is what locates the value.
fn stat(html: &str, label: &str) -> Option<String> {
    let marker = format!(">{label}</small>");
    let at = html.find(marker.as_str())? + marker.len();
    // Bounded, so a missing value cannot run away and pick up the next stat.
    let window = html.get(at..(at + 400).min(html.len()))?;
    let (value, _) = between(window, 0, "text-white\">", "</div>")?;
    let value = clean(value);
    (!value.is_empty()).then_some(value)
}

/// The pack banner the page advertises for link previews.
///
/// Only a pack image counts: the tag is the site's, and on a page without a
/// banner it could as well name the site's own logo.
fn og_banner(html: &str) -> Option<String> {
    let (_, after) = between(html, 0, "property=\"og:image\"", "content=\"")?;
    let (content, _) = between(html, after - "content=\"".len(), "content=\"", "\"")?;
    if !content.contains("/media/images/packs/") {
        return None;
    }
    media_url(content)
}

fn parse(html: &str) -> PackPage {
    let mut page = PackPage {
        banner_url: og_banner(html),
        ..PackPage::default()
    };

    // The Chart.js block: the meters present, and how many charts at each.
    // `data:` is looked for after `labels:` rather than from the top, so a
    // stray `data:` URI earlier in the document cannot be read as the counts.
    if let Some((labels, after_labels)) = between(html, 0, "labels:", "]")
        && let Some((counts, _)) = between(html, after_labels, "data:", "]")
    {
        let labels = numbers(labels);
        let counts = numbers(counts);
        for (index, meter) in labels.iter().enumerate() {
            if (1..=METER_MAX).contains(meter) {
                page.meter_labels.push(*meter);
                page.meter_counts
                    .push(counts.get(index).copied().unwrap_or(0));
            }
        }
    }

    page.songs = parse_songs(html);

    let mut authors: Vec<String> = Vec::new();
    for song in &page.songs {
        for name in song.credit.split(',') {
            let name = name.trim();
            if !name.is_empty() && !authors.iter().any(|known| known == name) {
                authors.push(name.to_owned());
            }
        }
    }
    page.authors = authors;
    let mut styles: Vec<String> = Vec::new();
    for song in &page.songs {
        for style in &song.styles {
            if !styles.iter().any(|known| known == style) {
                styles.push(style.clone());
            }
        }
    }
    page.styles = styles;
    // The site's own total, which counts charts the histogram drops as jokes.
    // Summing the bars is the fallback, not the answer.
    page.chart_count = stat(html, "Charts").or_else(|| {
        page.meter_counts
            .iter()
            .copied()
            .reduce(u32::saturating_add)
            .map(|total| total.to_string())
    });
    page
}

/// The song table: every `<tr>` that links to a song.
fn parse_songs(html: &str) -> Vec<SongRow> {
    let mut songs = Vec::new();
    let mut at = 0usize;
    while let Some((row, next)) = between(html, at, "<tr>", "</tr>") {
        at = next;
        if !row.contains("/song/") {
            continue;
        }
        let cells = table_cells(row);
        // The site prints eight columns; a row with fewer is a header or a
        // layout row rather than a song.
        if cells.len() < 8 {
            continue;
        }
        let credit = clean(
            cells[5]
                .replace("<br>", ", ")
                .replace("<br/>", ", ")
                .replace("<br />", ", ")
                .as_str(),
        );
        let song = SongRow {
            title: clean(title_of(cells[1]).unwrap_or_default()),
            artist: clean(artist_of(cells[1]).unwrap_or_default()),
            length: clean(cells[3]),
            bpm: clean(cells[4]),
            credit: credit.trim_end_matches([',', ' ']).to_owned(),
            meters: clean(cells[7]),
            image_url: attribute(cells[0], "src").and_then(media_url),
            styles: attribute(cells[6], "data-sort")
                .map(style_tokens)
                .unwrap_or_default(),
        };
        if song.title.is_empty() {
            continue;
        }
        songs.push(song);
    }
    songs
}

/// The row's cells, each as the whole `<td ...>...</td>` element.
///
/// The opening tag is kept because one cell carries its only real content in
/// an attribute: the styles column's text is the page's repeated filter
/// buttons, while its `data-sort` is the song's actual styles.
fn table_cells(row: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let mut at = 0usize;
    while let Some(open) = row.get(at..).and_then(|rest| rest.find("<td")) {
        let tag_start = at + open;
        let Some(end) = row.get(tag_start..).and_then(|rest| rest.find("</td>")) else {
            break;
        };
        let stop = tag_start + end + "</td>".len();
        cells.push(&row[tag_start..stop]);
        at = stop;
    }
    cells
}

/// The song title, which is the text of the `/song/<id>` link.
fn title_of(cell: &str) -> Option<&str> {
    let at = cell.find("/song/")?;
    let close = cell.get(at..)?.find('>')? + at + 1;
    let end = cell.get(close..)?.find("</a>")? + close;
    Some(&cell[close..end])
}

/// The artist, which the site puts in a small grey span under the title.
fn artist_of(cell: &str) -> Option<&str> {
    let at = cell.find("text-gray-400")?;
    let close = cell.get(at..)?.find('>')? + at + 1;
    let end = cell.get(close..)?.find("</span>")? + close;
    Some(&cell[close..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
      <small class="text-gray-400">Charts</small>
      <div class="fs-5 fw-medium text-white">25</div>
      <script>labels: [1, 4, 9, 5454, 12], data: [2, 7, 3, 99, 1]</script>
      <table>
      <tr><th>Art</th></tr>
      <tr>
        <td><img src="/media/images/songs/12.png"></td>
        <td><a href="/song/91">Vertex &amp; Beta</a>
            <span class="small translatable text-gray-400">Kyzentun</span></td>
        <td>subtitle</td>
        <td>1:52</td>
        <td>150-190</td>
        <td>Alice<br>Bob,</td>
        <td data-sort="[&#x27;dance-double&#x27;, &#x27;dance-single&#x27;]"></td>
        <td>3, 6, 9, 12</td>
      </tr>
      <tr>
        <td><img src="/media/images/nobanner.png"></td>
        <td><a href="/song/92">Second</a></td>
        <td></td><td>2:00</td><td>170</td><td>Alice</td>
        <td data-sort="[&#x27;dance-single&#x27;]"></td><td>4, 17</td>
      </tr>
      </table>
    "#;

    #[test]
    fn a_song_row_is_read_whole() {
        let page = parse(PAGE);
        assert_eq!(page.songs.len(), 2);
        let first = &page.songs[0];
        assert_eq!(first.title, "Vertex & Beta");
        assert_eq!(first.artist, "Kyzentun");
        assert_eq!(first.length, "1:52");
        assert_eq!(first.bpm, "150-190");
        // <br> becomes a separator and the trailing comma is dropped
        assert_eq!(first.credit, "Alice, Bob");
        assert_eq!(first.meters, "3, 6, 9, 12");
        assert_eq!(
            first.image_url.as_deref(),
            Some("https://stepmaniaonline.net/media/images/songs/12.png")
        );
        assert_eq!(
            first.subline(),
            "Kyzentun  -  150-190 bpm  -  1:52  -  Alice, Bob"
        );
    }

    /// The site's own placeholder carries no information, and a row showing it
    /// would look like art that failed rather than art that was never there.
    #[test]
    fn the_sites_no_banner_placeholder_is_not_art() {
        let page = parse(PAGE);
        assert!(page.songs[1].image_url.is_none());
    }

    /// A pack no list has described still says where its art is.
    #[test]
    fn the_banner_comes_from_the_pages_link_preview() {
        let page = parse(
            r#"<meta property="og:image" content="https://stepmaniaonline.net/media/images/packs/9957.png">"#,
        );
        assert_eq!(
            page.banner_url.as_deref(),
            Some("https://stepmaniaonline.net/media/images/packs/9957.png")
        );
        // the site's stand-in is no banner, and neither is anything that is
        // not a pack image
        let none = parse(
            r#"<meta property="og:image" content="https://stepmaniaonline.net/media/images/packs/nobanner.png">"#,
        );
        assert!(none.banner_url.is_none());
        let logo = parse(
            r#"<meta property="og:image" content="https://stepmaniaonline.net/static/logo.png">"#,
        );
        assert!(logo.banner_url.is_none());
        assert!(parse("<html></html>").banner_url.is_none());
    }

    /// A joke chart in the thousands would print a range like "1 - 5454" and
    /// flatten every real bar in the histogram to nothing.
    #[test]
    fn joke_meters_are_dropped_from_the_histogram() {
        let page = parse(PAGE);
        assert_eq!(page.meter_labels, vec![1, 4, 9, 12]);
        assert_eq!(page.meter_counts, vec![2, 7, 3, 1]);
        assert_eq!(page.difficulty_span(), Some((1, 12)));
    }

    /// The chart total is the site's own figure, which counts the joke charts
    /// the histogram drops -- summing the bars would under-report it.
    #[test]
    fn the_chart_total_comes_from_the_page_and_falls_back_to_the_bars() {
        assert_eq!(parse(PAGE).chart_count.as_deref(), Some("25"));
        let without = PAGE.replace("Charts</small>", "Nothing</small>");
        assert_eq!(parse(without.as_str()).chart_count.as_deref(), Some("13"));
    }

    #[test]
    fn the_hardest_meter_tints_the_row() {
        let page = parse(PAGE);
        assert_eq!(page.songs[0].top_meter(), 12);
        assert_eq!(page.songs[1].top_meter(), 17);
        assert_eq!(SongRow::default().top_meter(), 0);
    }

    /// Authors are gathered across every row, in first-seen order, so the fact
    /// table can name two of them and say how many more there are.
    /// The styles column is per-song data, not the page's repeated filter
    /// control -- which is what makes "Singles + Doubles" answerable at all.
    #[test]
    fn per_song_styles_come_from_the_sort_attribute() {
        let page = parse(PAGE);
        assert_eq!(page.songs[0].styles, vec!["dance-double", "dance-single"]);
        assert_eq!(page.songs[1].styles, vec!["dance-single"]);
        assert_eq!(page.styles, vec!["dance-double", "dance-single"]);
        assert_eq!(page.style_label(), Some("Singles + Doubles"));
        assert!(page.has_doubles());
    }

    /// A pack the page says nothing about must not be labelled from a guess.
    #[test]
    fn a_page_with_no_styles_reports_none_rather_than_singles() {
        let page = parse("<html></html>");
        assert_eq!(page.style_label(), None);
        assert!(!page.has_doubles());
    }

    #[test]
    fn authors_are_gathered_once_each() {
        let page = parse(PAGE);
        assert_eq!(page.authors, vec!["Alice", "Bob"]);
    }

    /// Most of the pack, not one token chart. A pack with a single easy song
    /// bolted onto twenty hard ones is not a beginner pack, and that is the
    /// distinction the majority test exists to draw.
    #[test]
    fn a_beginner_pack_is_mostly_easy_rather_than_partly_easy() {
        let easy = |meters: &str| SongRow {
            meters: meters.to_owned(),
            title: "x".to_owned(),
            ..SongRow::default()
        };

        // three of four songs start at 4 or below
        let mut page = PackPage {
            songs: vec![easy("1, 5"), easy("3, 8"), easy("4"), easy("9, 12")],
            ..PackPage::default()
        };
        assert!(page.is_beginner_friendly());

        // two of four is not most of four
        page.songs = vec![easy("1"), easy("2"), easy("9"), easy("11")];
        assert!(!page.is_beginner_friendly());

        // one token easy chart in a hard pack
        page.songs = vec![easy("2"), easy("12"), easy("13"), easy("15"), easy("16")];
        assert!(!page.is_beginner_friendly());

        // and a pack the site printed no meters for claims nothing
        page.songs = vec![easy(""), easy("")];
        assert!(!page.is_beginner_friendly());
    }

    #[test]
    fn the_easiest_chart_is_the_first_number_printed() {
        let row = SongRow {
            meters: "3, 6, 9, 12".to_owned(),
            ..SongRow::default()
        };
        assert_eq!(row.low_meter(), Some(3));
        assert_eq!(row.top_meter(), 12);

        // a range, as the site sometimes prints
        let row = SongRow {
            meters: "1-9".to_owned(),
            ..SongRow::default()
        };
        assert_eq!(row.low_meter(), Some(1));

        assert_eq!(SongRow::default().low_meter(), None);
    }

    #[test]
    fn a_page_with_no_song_table_is_empty_rather_than_wrong() {
        let page = parse("<html><body>nothing here</body></html>");
        assert!(page.songs.is_empty());
        assert!(page.meter_labels.is_empty());
        assert_eq!(page.difficulty_span(), None);
    }
}

#[cfg(test)]
#[path = "pack_response_original.rs"]
mod response_original;

#[cfg(test)]
#[path = "pack_response_perf.rs"]
mod response_perf_tests;

#[cfg(test)]
#[path = "pack_meter_perf.rs"]
mod meter_perf_tests;

#[cfg(test)]
#[path = "pack_page_perf.rs"]
mod perf_tests;
