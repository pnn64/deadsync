//! Pack detail enrichment for stepmaniaonline.net.
//!
//! `stepmaniaonline::fetch_catalog` reads `/api/packs`, a CSV of every pack. It
//! is complete and cheap and it is missing three things the browser needs: when
//! a pack was added, what its banner is, and which game types it charts for.
//!
//! Those live behind `/api/packs/datatables`, the endpoint the website's own
//! table is built from. It answers with rows of *rendered HTML* rather than
//! data, which is why this module scrapes rather than deserialises: there is no
//! JSON form of it to ask for. The upside is that the same endpoint sorts and
//! pages server-side, so "newest first" costs one request rather than a walk of
//! nine thousand rows.
//!
//! Enrichment is additive and always optional. A pack with no detail row still
//! lists, still downloads, and simply shows no banner or date -- the browser is
//! never blocked on this.

use deadsync_net as network;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const DETAILS_URL: &str = "https://stepmaniaonline.net/api/packs/datatables";
const MEDIA_BASE: &str = "https://stepmaniaonline.net";

/// One page of the table, and what the browser holds until somebody asks for
/// more.
///
/// Two hundred is about thirty screens of list. The catalogue is nine and a
/// half thousand packs, and holding all of them meant every republish filtered
/// and sorted the lot -- for rows nobody had scrolled to. The site is happy to
/// serve a page at a time and the browser is happy to ask.
pub const PAGE_LEN: usize = 200;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Pages the doubles pass will walk. That view is a filtered slice of the same
/// table and comes to a couple of hundred rows, so this is a bound rather than
/// a plan.
const MAX_PAGES: usize = 24;

/// The columns of the rendered table, in the order the endpoint emits them.
/// Named rather than numbered at the use site, because a silent column shift is
/// the most likely way this breaks.
mod column {
    pub const BANNER: usize = 0;
    pub const NAME: usize = 1;
    pub const DATE: usize = 5;
    pub const TYPES: usize = 4;
    pub const COUNT: usize = 7;
}

/// What the detail endpoint knows that the CSV does not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackDetails {
    /// Absolute URL of the pack banner, when it has one.
    pub banner_url: Option<String>,
    /// Date the pack was added to the site, `YYYY-MM-DD`. Kept as text because
    /// it is only ever compared and displayed, never arithmetic.
    pub date_added: Option<String>,
    /// Game types the pack charts for: `dance`, `pump`, `smx` and friends.
    pub chart_types: Vec<String>,
}

impl PackDetails {
    /// Whether this pack is *only* dance charts.
    ///
    /// Every reported game type has to be dance, not merely one of them: a pack
    /// carrying both dance and pump is a multi-game pack, and putting it in the
    /// pad view means most of what a player downloads is unplayable on a dance
    /// pad. This is the rule the ITGmania browser uses.
    ///
    /// A pack the site reported nothing for is kept. Absent data is not
    /// evidence of a foreign game, and hiding a playable pack is the worse
    /// mistake.
    pub fn is_dance_only(&self) -> bool {
        self.chart_types.is_empty() || self.chart_types.iter().all(|kind| kind == "dance")
    }

    /// The year portion of `date_added`, for the year view.
    pub fn year(&self) -> Option<u16> {
        self.date_added.as_deref()?.get(..4)?.parse().ok()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailsPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug)]
pub struct DetailsSnapshot {
    pub phase: DetailsPhase,
    /// Pack id -> what the detail endpoint said about it.
    pub by_id: Arc<HashMap<u64, PackDetails>>,
    /// Pack ids in the order the site lists them newest-first. This is the
    /// server's own ordering, not ours, so "recently added" means the same
    /// thing here as it does on the website.
    pub newest_first: Arc<[u64]>,
    /// Packs with at least one doubles chart, per the site's own style filter.
    /// Candidates rather than answers: most are singles packs carrying a token
    /// double, which is why the doubles view puts itgdb's curated list first.
    pub doubles: Arc<std::collections::HashSet<u64>>,
    /// Whether the site has more rows past the ones fetched so far.
    ///
    /// The browser walks this a page at a time on demand rather than pulling
    /// ten thousand rows at startup. Nobody reads past the first page or two,
    /// and every one of them costs a filter and a sort over the whole set on
    /// the screen that shows them.
    pub has_more: bool,
    pub revision: u64,
    pub message: Option<String>,
}

impl Default for DetailsSnapshot {
    fn default() -> Self {
        Self {
            phase: DetailsPhase::Idle,
            by_id: Arc::new(HashMap::new()),
            newest_first: Arc::from(Vec::new()),
            doubles: Arc::new(std::collections::HashSet::new()),
            has_more: true,
            revision: 0,
            message: None,
        }
    }
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<DetailsSnapshot>,
    generation: u64,
    /// The walk so far, kept whole here so another page can be appended
    /// without rebuilding it from the published snapshot.
    by_id: HashMap<u64, PackDetails>,
    newest_first: Vec<u64>,
    /// What `newest_first` holds, so a row is placed in the order once. Kept
    /// apart from `by_id`, which also describes rows that are not in the
    /// order at all.
    listed: HashSet<u64>,
    /// Where the next page starts. Rows already fetched are never re-fetched.
    next_start: usize,
    /// When the last request failed, so recovering from it waits a moment
    /// instead of hammering a site that is down.
    failed_at: Option<Instant>,
}

/// How long a failed request waits before it is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

pub fn runtime_snapshot() -> Arc<DetailsSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Fetch the first page if nothing has been fetched. Ignores repeat calls
/// while loading or once ready, so a screen may call this every frame.
pub fn runtime_ensure_details() {
    start_request(false);
}

/// Throw the walk away and start it again from the first page.
/// Try again after a failure, once the pause has passed. Cheap to call every
/// frame: it does nothing unless the service is in error and due.
///
/// A failed first page starts over. A failed later page keeps what was
/// fetched -- those pages are still on the reader's screen -- and goes back
/// to Ready, so the next page can be asked for again.
pub fn runtime_recover() {
    let restart = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase != DetailsPhase::Error
            || runtime
                .failed_at
                .is_some_and(|at| at.elapsed() < RETRY_AFTER)
        {
            return;
        }
        runtime.failed_at = None;
        if runtime.newest_first.is_empty() {
            true
        } else {
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = DetailsPhase::Ready;
            snapshot.message = None;
            snapshot.revision = snapshot.revision.wrapping_add(1);
            runtime.snapshot = Arc::new(snapshot);
            false
        }
    };
    if restart {
        start_request(true);
    }
}

pub fn runtime_refresh_details() {
    start_request(true);
}

/// Fetch the next page and append it.
///
/// Does nothing while a page is in flight, or once the site has said there is
/// nothing further. Safe to call from a keypress.
pub fn runtime_load_more() {
    let (generation, start) = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase != DetailsPhase::Ready || !runtime.snapshot.has_more {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = DetailsPhase::Loading;
        snapshot.revision = snapshot.revision.wrapping_add(1);
        runtime.snapshot = Arc::new(snapshot);
        (runtime.generation, runtime.next_start)
    };
    spawn_page(generation, start, false);
}

fn start_request(force: bool) {
    let (generation, start) = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == DetailsPhase::Loading
            || !force && runtime.snapshot.phase == DetailsPhase::Ready
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        // A refresh starts over rather than appending to a stale walk.
        runtime.by_id.clear();
        runtime.newest_first.clear();
        runtime.listed.clear();
        runtime.next_start = 0;
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = DetailsPhase::Loading;
        snapshot.message = Some("Loading pack details...".to_owned());
        runtime.snapshot = Arc::new(snapshot);
        (generation, 0)
    };
    spawn_page(generation, start, true);
}

/// Fetch one page on a worker. `first` also fetches the doubles candidates,
/// which are a filtered view of the same table and only wanted once.
fn spawn_page(generation: u64, start: usize, first: bool) {
    let spawn = thread::Builder::new()
        .name("smo-details".to_owned())
        .spawn(move || finish_request(generation, fetch_page(start, first)));
    if let Err(error) = spawn {
        finish_request(
            generation,
            Err(format!("start the details worker: {error}")),
        );
    }
}

struct Fetched {
    rows: Vec<(u64, PackDetails)>,
    /// Whether the page came back full, which is the only evidence the site
    /// gives that there is another one.
    has_more: bool,
    /// Only the first page carries these.
    doubles: Option<std::collections::HashSet<u64>>,
    /// The doubles pass reads whole rows of the same table, so it knows those
    /// packs' banners and dates too.
    ///
    /// It used to throw all of that away and keep the ids. That cost nothing
    /// while the browser held the entire catalogue -- the main walk had
    /// already described every pack. Now that it holds a page, a doubles pack
    /// outside that page has no banner URL anywhere, and its row sat there
    /// spinning for artwork nothing had asked for.
    doubles_rows: Vec<(u64, PackDetails)>,
}

type FetchResult = Result<Fetched, String>;

fn finish_request(generation: u64, result: FetchResult) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        log::debug!("Discarding stale StepManiaOnline details generation {generation}.");
        return;
    }
    let fetched = match result {
        Ok(fetched) => fetched,
        Err(message) => {
            log::warn!("Could not load StepManiaOnline pack details: {message}");
            runtime.failed_at = Some(Instant::now());
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = DetailsPhase::Error;
            snapshot.message = Some(message);
            snapshot.revision = snapshot.revision.wrapping_add(1);
            runtime.snapshot = Arc::new(snapshot);
            return;
        }
    };

    runtime.next_start += fetched.rows.len();
    let RuntimeState {
        by_id,
        newest_first,
        listed,
        ..
    } = &mut *runtime;
    merge_page(
        by_id,
        newest_first,
        listed,
        fetched.rows,
        fetched.doubles_rows,
    );

    // Built while the lock is held, which is fine here: this runs once per
    // page turn rather than once per frame, and the page is at most a couple
    // of hundred rows.
    let by_id = Arc::new(runtime.by_id.clone());
    let newest_first: Arc<[u64]> = Arc::from(runtime.newest_first.clone());

    let mut snapshot = (*runtime.snapshot).clone();
    snapshot.phase = DetailsPhase::Ready;
    snapshot.by_id = by_id;
    snapshot.newest_first = newest_first;
    snapshot.has_more = fetched.has_more;
    if let Some(doubles) = fetched.doubles {
        snapshot.doubles = Arc::new(doubles);
    }
    snapshot.message = None;
    snapshot.revision = snapshot.revision.wrapping_add(1);
    log::info!(
        "StepManiaOnline details: {} packs known, more available: {}",
        snapshot.by_id.len(),
        snapshot.has_more
    );
    runtime.snapshot = Arc::new(snapshot);
}

/// One page of the table, newest first.
///
/// The order parameters are datatables' own array syntax, percent-encoded:
/// `order[0][column]=5&order[0][dir]=desc`. Column 5 is the date added, and
/// getting that number wrong is the one mistake this endpoint does not report:
/// an out-of-range column silently falls back to a name sort, which looks
/// exactly like a working list in the wrong order.
fn page_url(start: usize, length: usize) -> String {
    format!(
        "{DETAILS_URL}?start={start}&length={length}&order%5B0%5D%5Bcolumn%5D={}&order%5B0%5D%5Bdir%5D=desc",
        column::DATE
    )
}

/// Walk the table newest-first until the server runs out of rows.
///
/// Ordering by the date column server-side is what makes `newest_first`
/// trustworthy: the CSV has no date at all, so without this the browser could
/// only guess at "recent" from the pack id, which is close but wrong whenever
/// a pack is re-uploaded.
/// Fetch one page of the table, newest first.
///
/// One page, not a walk. The browser shows a couple of hundred packs and asks
/// for the next lot when the reader gets to the bottom -- pulling ten thousand
/// rows up front cost a filter and a sort over all of them every time anything
/// republished, which is what made changing tabs stutter.
fn fetch_page(start: usize, first: bool) -> FetchResult {
    let agent = network::get_agent();
    let url = page_url(start, PAGE_LEN);
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
    let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
        .map_err(|error| format!("{error:?}"))?;

    let rows = parse_page(body.as_str())?;
    if first && rows.is_empty() {
        return Err("the details endpoint returned no rows".to_owned());
    }
    // A short page is the only evidence the site gives that it has run out.
    let has_more = rows.len() >= PAGE_LEN;

    // The doubles candidates are a filtered view of the same table and are
    // wanted once, so they ride along with the first page. A failure here
    // costs the Doubles tab and nothing else.
    let (doubles, doubles_rows) = if first {
        match fetch_doubles(&agent) {
            Ok((ids, rows)) => (Some(ids), rows),
            Err(error) => {
                log::warn!("Could not read the doubles candidates: {error}");
                (Some(std::collections::HashSet::new()), Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };

    Ok(Fetched {
        rows,
        has_more,
        doubles,
        doubles_rows,
    })
}

/// Packs the site says contain a doubles chart.
///
/// `style_filters=dance-double` alone returns some two and a half thousand --
/// nearly every pack has one doubles chart somewhere. Narrowing by the three
/// substyles that actually produce doubles content brings it to a couple of
/// hundred, which is the same pair of parameters the ITGmania browser used.
type DoublesPass = (std::collections::HashSet<u64>, Vec<(u64, PackDetails)>);

fn fetch_doubles(agent: &network::HttpAgent) -> Result<DoublesPass, String> {
    let mut ids = std::collections::HashSet::new();
    let mut described: Vec<(u64, PackDetails)> = Vec::new();
    for page in 0..MAX_PAGES {
        let start = page * PAGE_LEN;
        let url = format!(
            "{DETAILS_URL}?start={start}&length={PAGE_LEN}&style_filters=dance-double&pack_substyle_filters=technical,mods,stamina"
        );
        let response = agent
            .get(url)
            .call()
            .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
        let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
            .map_err(|error| format!("{error:?}"))?;
        let rows = parse_page(body.as_str())?;
        if rows.is_empty() {
            break;
        }
        let count = rows.len();
        for (id, details) in rows {
            ids.insert(id);
            described.push((id, details));
        }
        if count < PAGE_LEN {
            break;
        }
    }
    Ok((ids, described))
}

/// Pull the rows out of one datatables response.
///
/// Deliberately hand-written rather than a JSON derive: `data` is an array of
/// arrays of HTML strings, and every field wanted from it is an attribute or a
/// text node inside that HTML.
pub(crate) fn parse_page(body: &str) -> Result<Vec<(u64, PackDetails)>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("details response: {error}"))?;
    let rows = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "details response has no data array".to_owned())?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(cells) = row.as_array() else {
            continue;
        };
        if cells.len() < column::COUNT {
            continue;
        }
        let cell = |index: usize| cells.get(index).and_then(serde_json::Value::as_str);
        let Some(name_cell) = cell(column::NAME) else {
            continue;
        };
        let Some(id) = pack_id(name_cell) else {
            continue;
        };
        out.push((
            id,
            PackDetails {
                banner_url: cell(column::BANNER).and_then(banner_url),
                date_added: cell(column::DATE).and_then(inner_text).filter(is_iso_date),
                chart_types: cell(column::TYPES).map(chart_types).unwrap_or_default(),
            },
        ));
    }
    Ok(out)
}

/// `<a ... href="/pack/5262">Name</a>` -> 5262
fn pack_id(cell: &str) -> Option<u64> {
    let start = cell.find("/pack/")? + "/pack/".len();
    let rest = &cell[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// `<img ... data-src="/media/images/packs/<hash>.jpg">` -> absolute URL
/// Fold one fetched page into the walk.
///
/// Appended, not replaced: this is one page of a walk the reader is
/// extending, and the pages before it are still on their screen.
///
/// Whether a row is in the order is `listed`'s question, not `by_id`'s. The
/// doubles pass describes rows from all over the table, so a pack it already
/// described is not thereby already listed -- asking `by_id` dropped every
/// such pack from the page it really belongs to.
fn merge_page(
    by_id: &mut HashMap<u64, PackDetails>,
    newest_first: &mut Vec<u64>,
    listed: &mut HashSet<u64>,
    rows: Vec<(u64, PackDetails)>,
    doubles_rows: Vec<(u64, PackDetails)>,
) {
    for (id, details) in rows {
        by_id.insert(id, details);
        if listed.insert(id) {
            newest_first.push(id);
        }
    }
    // Described, but not placed in the page. `by_id` is a lookup and
    // `newest_first` is the list's order -- these rows come from a filtered
    // view of the table and are not in the main walk's date order, so putting
    // them in the order would interleave them wrongly.
    for (id, details) in doubles_rows {
        by_id.entry(id).or_insert(details);
    }
}

fn banner_url(cell: &str) -> Option<String> {
    let path = attribute(cell, "data-src")?;
    // The site's stand-in for a pack with no banner is an image that says
    // "NO BANNER". It is not the pack's art, so it is no art -- as the
    // original reads it.
    if path.is_empty() || path.contains("nobanner") {
        return None;
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return Some(path.to_owned());
    }
    Some(format!("{MEDIA_BASE}{path}"))
}

/// `data-sort="[&#x27;dance&#x27;, &#x27;pump&#x27;]"` -> ["dance", "pump"]
///
/// The attribute is a Python list literal that has been HTML-escaped, so the
/// quotes arrive as entities. Reading the `alt` attributes of the type icons
/// instead would be prettier and is not reliable: a pack with many types only
/// renders the first few.
fn chart_types(cell: &str) -> Vec<String> {
    let Some(raw) = attribute(cell, "data-sort") else {
        return Vec::new();
    };
    let decoded = decode_entities(raw);
    decoded
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|part| part.trim().trim_matches('\'').trim_matches('"').to_owned())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The value of one attribute, without pulling in an HTML parser for what is
/// always a single well-formed tag written by the same template.
fn attribute<'a>(cell: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = cell.find(needle.as_str())? + needle.len();
    let rest = &cell[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The text between the first `>` and the following `<`.
fn inner_text(cell: &str) -> Option<String> {
    let start = cell.find('>')? + 1;
    let rest = &cell[start..];
    let end = rest.find('<')?;
    let text = rest[..end].trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn is_iso_date(value: &String) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// Just enough entity decoding for the attributes this endpoint emits.
fn decode_entities(raw: &str) -> String {
    raw.replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = r#"{"data": [[
        "<img class=\"lazy-img\" data-src=\"/media/images/packs/abc123.jpg\">",
        "<a class=\"small\" href=\"/pack/5262\">#1 Extra-Terrestrial</a>",
        "<span data-sort=\"194849003\" class=\"small\">185.8 MB</span>",
        "<span class=\"small\">34</span>",
        "<span data-sort=\"[&#x27;dance&#x27;, &#x27;pump&#x27;]\" class=\"small\"><img alt=\"dance\"></span>",
        "<span class=\"small\">2022-08-28</span>",
        "<a href=\"/download/pack/5262/\"><strong></strong></a>"
    ]]}"#;

    #[test]
    fn a_row_yields_the_three_things_the_csv_lacks() {
        let rows = parse_page(ROW).expect("parse");
        assert_eq!(rows.len(), 1);
        let (id, details) = &rows[0];
        assert_eq!(*id, 5262);
        assert_eq!(
            details.banner_url.as_deref(),
            Some("https://stepmaniaonline.net/media/images/packs/abc123.jpg")
        );
        assert_eq!(details.date_added.as_deref(), Some("2022-08-28"));
        assert_eq!(details.chart_types, vec!["dance", "pump"]);
        assert_eq!(details.year(), Some(2022));
        // dance AND pump is a multi-game pack, not a pad pack
        assert!(!details.is_dance_only());
    }

    /// A row missing the pieces we scrape must be skipped, not panic and not
    /// poison the rest of the page.
    #[test]
    fn rows_without_an_id_are_skipped_rather_than_fatal() {
        let body = r#"{"data": [["", "<a href=\"/nope\">x</a>", "", "", "", "", ""]]}"#;
        assert!(parse_page(body).expect("parse").is_empty());
    }

    #[test]
    fn a_missing_banner_or_date_is_absent_rather_than_wrong() {
        let body = r#"{"data": [[
            "<img class=\"lazy-img\">",
            "<a href=\"/pack/12\">Name</a>",
            "<span>1 MB</span>", "<span>2</span>", "<span>x</span>",
            "<span>not a date</span>", "<a></a>"
        ]]}"#;
        let rows = parse_page(body).expect("parse");
        let (_, details) = &rows[0];
        assert!(details.banner_url.is_none());
        assert!(details.date_added.is_none());
        assert!(details.year().is_none());
        // no type information at all still counts as playable on a pad
        assert!(details.is_dance_only());

        // and the site's "NO BANNER" picture is not the pack's banner
        let placeholder = r#"{"data": [[
            "<img class=\"lazy-img\" data-src=\"/media/images/packs/nobanner.png\">",
            "<a href=\"/pack/13\">Name</a>",
            "<span>1 MB</span>", "<span>2</span>", "<span>x</span>",
            "<span>2026-01-01</span>", "<a></a>"
        ]]}"#;
        let rows = parse_page(placeholder).expect("parse");
        assert!(rows[0].1.banner_url.is_none());
    }

    /// A pack the doubles pass described is still listed when its own page
    /// arrives. Treating "described" as "listed" dropped it from the page.
    #[test]
    fn a_pack_described_early_is_still_listed_on_its_own_page() {
        let mut by_id = HashMap::new();
        let mut order = Vec::new();
        let mut listed = HashSet::new();
        // The first page, with the doubles pass describing pack 900 early.
        merge_page(
            &mut by_id,
            &mut order,
            &mut listed,
            vec![(1, PackDetails::default())],
            vec![(900, PackDetails::default())],
        );
        assert_eq!(order, vec![1], "described is not listed");
        // Pack 900's own page.
        merge_page(
            &mut by_id,
            &mut order,
            &mut listed,
            vec![(900, PackDetails::default()), (2, PackDetails::default())],
            Vec::new(),
        );
        assert_eq!(order, vec![1, 900, 2]);
        // and a row that turns up on two pages is listed once
        merge_page(
            &mut by_id,
            &mut order,
            &mut listed,
            vec![(2, PackDetails::default())],
            Vec::new(),
        );
        assert_eq!(order, vec![1, 900, 2]);
    }

    /// A URL with a stray space in it fails only against the real server, and
    /// only once the whole feature is wired up.
    /// A page is what the reader can see plus a margin, not the catalogue.
    /// The whole point of paging is that nothing scans nine thousand packs to
    /// draw seven rows.
    #[test]
    fn a_page_is_a_screenful_rather_than_the_catalogue() {
        const { assert!(PAGE_LEN >= 100, "enough that paging is rare") };
        const { assert!(PAGE_LEN <= 500, "and never the whole catalogue") };
    }

    #[test]
    fn the_request_url_is_well_formed_and_sorted_by_date() {
        let url = page_url(1000, PAGE_LEN);
        assert!(!url.contains(' '), "url has whitespace in it: {url}");
        assert!(!url.contains('\n'));
        assert!(url.starts_with(DETAILS_URL));
        assert!(url.contains("start=1000"));
        assert!(url.contains(format!("length={PAGE_LEN}").as_str()));
        // order[0][column]=5 -> the date column, descending
        assert!(url.contains("order%5B0%5D%5Bcolumn%5D=5"));
        assert!(url.contains("order%5B0%5D%5Bdir%5D=desc"));
    }

    #[test]
    fn only_a_dance_pack_counts_as_a_pad_pack() {
        let pump = PackDetails {
            chart_types: vec!["pump".to_owned()],
            ..PackDetails::default()
        };
        assert!(!pump.is_dance_only());

        let dance = PackDetails {
            chart_types: vec!["dance".to_owned()],
            ..PackDetails::default()
        };
        assert!(dance.is_dance_only());

        let mixed = PackDetails {
            chart_types: vec!["dance".to_owned(), "smx".to_owned()],
            ..PackDetails::default()
        };
        assert!(!mixed.is_dance_only());
    }

    /// A default snapshot must be safe to read from before anything loads.
    #[test]
    fn an_empty_snapshot_claims_nothing() {
        let snapshot = DetailsSnapshot::default();
        assert!(snapshot.by_id.is_empty());
        assert!(snapshot.doubles.is_empty());
        assert!(snapshot.newest_first.is_empty());
    }

    #[test]
    fn the_response_shape_is_checked_before_it_is_trusted() {
        assert!(parse_page("not json").is_err());
        assert!(parse_page(r#"{"nope": 1}"#).is_err());
    }
}
