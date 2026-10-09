//! Searching stepmaniaonline.net, which is three questions rather than one.
//!
//! "rosewood" is not the name of a pack. It is the name of somebody who makes
//! charts, and the packs worth finding are the ones with their charts in them.
//! A search that only matches pack names answers nothing for that query, which
//! is what the ITGmania browser's three passes exist to fix:
//!
//! * **names** -- the catalogue's own pack names, matched here rather than at
//!   the server, because unlike the Lua we already hold the whole catalogue and
//!   a local scan beats a round trip.
//! * **credits** -- `/api/search/?type=credit`, the site's index of who charted
//!   what. This is the pass that answers a charter's name.
//! * **titles** -- `/api/search/?type=title`, for a song somebody half
//!   remembers.
//!
//! Each publishes as it lands, so the name matches are on screen immediately
//! and the other two fill in underneath them.
//!
//! One thing the original does is deliberately not carried over: it re-checks
//! every partial credit hit against `/api/credits/<id>`. That endpoint now
//! answers 404, so the check would fail for every pack and cost a request each
//! to learn nothing. The scores below therefore come from the search response
//! alone, which is what the original falls back to anyway.

use crate::smo_details;
use crate::stepmaniaonline;
use deadsync_net as network;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

const SEARCH_URL: &str = "https://stepmaniaonline.net/api/search/";
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Below this a query is not run: one character matches most of the catalogue.
pub const MIN_CHARS: usize = 2;
/// The original's `SEARCH.MAX_ROWS`. More than this is not a search result, it
/// is the catalogue with extra steps.
const MAX_ROWS: usize = 200;

/// The original's `SearchScores`, verbatim.
mod score {
    /// The query is how the pack's name starts.
    pub const NAME_PREFIX: i32 = 100;
    /// The query is somewhere in the pack's name.
    pub const NAME_MATCH: i32 = 80;
    /// Somebody in this pack is credited with the query.
    pub const CREDIT: i32 = 60;
    /// A song in this pack is called something like the query.
    pub const TITLE: i32 = 40;
    /// The most a credit hit can earn on top of `CREDIT`.
    pub const CREDIT_SHARE: i32 = 100;
    /// What a pack matched more than one way is nudged by, so a pack found
    /// twice outranks one found once at the same score.
    pub const BOTH_WAYS: i32 = 5;
}

/// One pack the search found, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub pack_id: u64,
    pub score: i32,
    /// What matched, in the reader's words -- "all charts by rosewood",
    /// "3 of 11 by rosewood", "song: RED Zone.". Shown on the pack's row,
    /// because a result whose reason is invisible looks like a wrong answer.
    pub why: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct SearchSnapshot {
    pub phase: SearchPhase,
    /// The query these hits answer. A reader who has typed a new one must not
    /// be shown the old one's results, so the query travels with them.
    pub query: String,
    pub hits: Arc<[SearchHit]>,
    /// Whether more matched than `MAX_ROWS`, which is the only thing the "+"
    /// in the readout means.
    pub capped: bool,
    pub revision: u64,
    pub message: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<SearchSnapshot>,
    generation: u64,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

#[must_use]
pub fn runtime_snapshot() -> Arc<SearchSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Run a search, unless this exact query is already running or answered.
///
/// Safe to call every frame: the query is the identity of the work, so a
/// screen that keeps asking for the same one costs nothing.
pub fn runtime_search(query: &str) {
    let query = query.trim().to_owned();
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.query == query
            && matches!(
                runtime.snapshot.phase,
                SearchPhase::Loading | SearchPhase::Ready
            )
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        publish(
            &mut runtime,
            SearchSnapshot {
                phase: if query.is_empty() {
                    SearchPhase::Idle
                } else {
                    SearchPhase::Loading
                },
                query: query.clone(),
                hits: Arc::from(Vec::new()),
                capped: false,
                revision: 0,
                message: None,
            },
        );
        generation
    };

    if query.chars().count() < MIN_CHARS {
        // Too short to run, but the empty result still belongs to this query
        // rather than to whatever was searched before it.
        let mut runtime = lock_runtime();
        if runtime.generation == generation {
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = SearchPhase::Ready;
            publish(&mut runtime, snapshot);
        }
        return;
    }

    let spawn = thread::Builder::new()
        .name("smo-search".to_owned())
        .spawn(move || run(generation, query));
    if spawn.is_err() {
        let mut runtime = lock_runtime();
        if runtime.generation == generation {
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = SearchPhase::Error;
            snapshot.message = Some("could not start the search".to_owned());
            publish(&mut runtime, snapshot);
        }
    }
}

fn publish(runtime: &mut RuntimeState, mut snapshot: SearchSnapshot) {
    snapshot.revision = runtime.snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

/// One accumulating answer, kept sorted only when it is published.
#[derive(Default)]
struct Accumulator {
    hits: Vec<SearchHit>,
    capped: bool,
}

impl Accumulator {
    /// Seed the name pass from the catalogue, whose parser rejects duplicate IDs.
    fn from_catalog(catalog: &[stepmaniaonline::PackInfo], needle: &str) -> Self {
        let mut acc = Self::default();
        for pack in catalog.iter() {
            let name = pack.name.to_lowercase();
            let points = if name.starts_with(needle) {
                score::NAME_PREFIX
            } else if name.contains(needle) {
                score::NAME_MATCH
            } else {
                continue;
            };
            acc.hits.push(SearchHit {
                pack_id: pack.id,
                score: points,
                why: "pack name".to_owned(),
            });
        }
        acc
    }

    /// Add a match, or improve one already found.
    ///
    /// A pack matched more than one way keeps the better reason and gets a
    /// nudge, so finding it twice counts for something.
    fn add(&mut self, pack_id: u64, score: i32, why: String) {
        if let Some(existing) = self.hits.iter_mut().find(|hit| hit.pack_id == pack_id) {
            if score > existing.score {
                existing.why = why;
            }
            existing.score = existing.score.max(score) + score::BOTH_WAYS;
            return;
        }
        self.hits.push(SearchHit {
            pack_id,
            score,
            why,
        });
    }
}

fn run(generation: u64, query: String) {
    let needle = query.to_lowercase();
    // Pass one: local names, before asking the network.
    let catalog = stepmaniaonline::runtime_snapshot();
    let mut acc = Accumulator::from_catalog(&catalog.catalog, &needle);
    if !publish_pass(generation, &mut acc, SearchPhase::Loading) {
        return;
    }

    let agent = network::get_agent();

    // Pass two: who charted what. This is the pass that answers a name.
    match fetch(&agent, query.as_str(), "credit") {
        Ok(results) => {
            for result in results {
                let (points, why) = credit_score(&result, query.as_str());
                acc.add(result.id, points, why);
            }
        }
        Err(error) => log::debug!("Credit search for {query:?} failed: {error}"),
    }
    if !publish_pass(generation, &mut acc, SearchPhase::Loading) {
        return;
    }

    // Pass three: song titles.
    match fetch(&agent, query.as_str(), "title") {
        Ok(results) => {
            for result in results {
                acc.add(result.id, score::TITLE, title_why(&result));
            }
        }
        Err(error) => log::debug!("Title search for {query:?} failed: {error}"),
    }
    publish_pass(generation, &mut acc, SearchPhase::Ready);
}

/// Sort, cap and hand over. Returns whether this search is still the current
/// one -- a stale pass must not repaint over a query the reader has moved on
/// from.
fn publish_pass(generation: u64, acc: &mut Accumulator, phase: SearchPhase) -> bool {
    let details = smo_details::runtime_snapshot();
    // Score first, then newest, then by name -- so equal-scoring packs come
    // out in a stable and useful order rather than in whatever order the
    // passes happened to add them.
    let date_of = |pack_id: u64| {
        details
            .by_id
            .get(&pack_id)
            .and_then(|entry| entry.date_added.clone())
            .unwrap_or_default()
    };
    acc.hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| date_of(b.pack_id).cmp(&date_of(a.pack_id)))
            .then_with(|| a.pack_id.cmp(&b.pack_id))
    });
    acc.capped = acc.hits.len() > MAX_ROWS;
    acc.hits.truncate(MAX_ROWS);

    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        return false;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    snapshot.phase = phase;
    snapshot.hits = Arc::from(acc.hits.clone());
    snapshot.capped = acc.capped;
    publish(&mut runtime, snapshot);
    true
}

/// A credit hit's score and its reason, from the original's own rules.
///
/// The majority bonus is flat rather than proportional on purpose: among packs
/// where somebody charted most of it, recency decides, not pack size.
fn credit_score(result: &SearchResult, query: &str) -> (i32, String) {
    let matched = result.matching_songs.len();
    let total = result.songs_count;
    let bonus = if total > 0 && matched * 2 > total {
        score::CREDIT_SHARE
    } else if total > 0 && matched > 0 {
        let share = (matched as f64 / total as f64).min(0.5);
        (share * f64::from(score::CREDIT_SHARE)) as i32
    } else {
        0
    };
    let why = if total > 0 && matched >= total {
        format!("all charts by {query}")
    } else if matched == 1 {
        format!("1 chart by {query}")
    } else if matched > 0 {
        format!("{matched} of {total} by {query}")
    } else {
        format!("charts by {query}")
    };
    (score::CREDIT + bonus, why)
}

fn title_why(result: &SearchResult) -> String {
    match result.matching_songs.first() {
        Some(first) => format!("song: {}", clean_song_label(first)),
        None => "matching song".to_owned(),
    }
}

/// The site labels a matching song several ways at once.
///
/// Some carry a bracketed index (`[11] RED Zone.`), some a pipe-separated
/// path (`316|TECH SOUP|[lv.Rosewood] VAY`). The original strips the bracket
/// twice because it has seen it doubled; the pipe form it never had.
fn clean_song_label(raw: &str) -> String {
    let tail = raw.rsplit('|').next().unwrap_or(raw);
    let mut text = tail.trim();
    for _ in 0..2 {
        text = strip_index(text);
    }
    text.trim().to_owned()
}

fn strip_index(text: &str) -> &str {
    let text = text.trim_start();
    let Some(rest) = text.strip_prefix('[') else {
        return text;
    };
    let Some(close) = rest.find(']') else {
        return text;
    };
    if rest[..close].is_empty() || !rest[..close].chars().all(|ch| ch.is_ascii_digit()) {
        return text;
    }
    rest[close + 1..].trim_start()
}

// --- the endpoint -------------------------------------------------------------

#[derive(Clone, Debug, serde::Deserialize)]
struct SearchResult {
    id: u64,
    #[serde(default)]
    songs_count: usize,
    #[serde(default)]
    matching_songs: Vec<String>,
}

#[derive(serde::Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

fn fetch(agent: &network::HttpAgent, query: &str, kind: &str) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "{SEARCH_URL}?query={}&type={kind}&exact=0",
        percent_encode(query)
    );
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
    let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
        .map_err(|error| format!("{error:?}"))?;
    let parsed: SearchResponse =
        serde_json::from_str(body.as_str()).map_err(|error| format!("search response: {error}"))?;
    Ok(parsed.results)
}

/// Percent-encode a query for a URL.
///
/// Written out rather than pulled in: the only thing going through here is a
/// player's search term, and the rule for one is short enough to read.
pub(crate) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(format!("%{byte:02X}").as_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(id: u64, total: usize, matched: usize) -> SearchResult {
        SearchResult {
            id,
            songs_count: total,
            matching_songs: (0..matched).map(|n| format!("song {n}")).collect(),
        }
    }

    /// A charter who did most of a pack gets the flat majority bonus, so
    /// recency decides among them rather than pack size -- a 35-of-35 pack
    /// and a 6-of-10 pack score the same, and the newer one leads.
    #[test]
    fn a_majority_of_a_pack_earns_the_flat_bonus() {
        let (score, why) = credit_score(&result(1, 35, 35), "rosewood");
        assert_eq!(score, 160);
        assert_eq!(why, "all charts by rosewood");

        let (majority, _) = credit_score(&result(2, 10, 6), "rosewood");
        assert_eq!(majority, 160, "flat, not proportional");
    }

    /// A handful of charts in a big pack still counts, in proportion, and
    /// never earns more than half the bonus.
    #[test]
    fn a_minority_of_a_pack_earns_a_share() {
        let (score, why) = credit_score(&result(3, 11, 3), "rosewood");
        assert_eq!(score, 60 + 27, "3/11 of a 100 bonus");
        assert_eq!(why, "3 of 11 by rosewood");

        let (one, why) = credit_score(&result(4, 90, 1), "rosewood");
        assert_eq!(one, 61);
        assert_eq!(why, "1 chart by rosewood");
    }

    /// A pack found by two passes keeps the better reason and outranks one
    /// found by a single pass at the same score.
    #[test]
    fn a_pack_found_twice_is_nudged_above_one_found_once() {
        let mut acc = Accumulator::default();
        acc.add(1, score::TITLE, "song: X".to_owned());
        acc.add(1, score::NAME_PREFIX, "pack name".to_owned());
        acc.add(2, score::NAME_PREFIX, "pack name".to_owned());

        let both = acc.hits.iter().find(|hit| hit.pack_id == 1).unwrap();
        let once = acc.hits.iter().find(|hit| hit.pack_id == 2).unwrap();
        assert_eq!(both.score, score::NAME_PREFIX + score::BOTH_WAYS);
        assert!(both.score > once.score);
        assert_eq!(both.why, "pack name", "the better reason wins");
    }

    /// The site labels matching songs three different ways, and the row shows
    /// whichever of them is the actual title.
    #[test]
    fn a_song_label_is_reduced_to_its_title() {
        assert_eq!(clean_song_label("[11] RED Zone."), "RED Zone.");
        assert_eq!(
            clean_song_label("316|TECH SOUP|[lv.Rosewood] VAY"),
            "[lv.Rosewood] VAY",
            "a bracket that is not an index is part of the title"
        );
        assert_eq!(clean_song_label("[3] [12] Doubled"), "Doubled");
        assert_eq!(clean_song_label("  Plain  "), "Plain");
    }

    /// A search term is somebody's typing, so everything outside the unreserved
    /// set has to survive the trip.
    #[test]
    fn a_query_is_encoded_for_a_url() {
        assert_eq!(percent_encode("rosewood"), "rosewood");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("d&d?x=1"), "d%26d%3Fx%3D1");
        assert_eq!(percent_encode("caf\u{e9}"), "caf%C3%A9");
    }

    #[test]
    fn the_response_shape_is_the_sites_own() {
        let body = r#"{"results":[{"id":8851,"name":"4 Arrows","songs_count":11,
            "type":["dance"],"matching_songs":["[11] RED Zone.","[12] Cowbell"]}]}"#;
        let parsed: SearchResponse = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].id, 8851);
        assert_eq!(parsed.results[0].songs_count, 11);
        assert_eq!(parsed.results[0].matching_songs.len(), 2);
    }
}

#[cfg(test)]
#[path = "smo_catalog_perf.rs"]
mod catalog_perf_tests;
