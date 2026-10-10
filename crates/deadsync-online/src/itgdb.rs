//! itgdb.net — the doubles category.
//!
//! Neither stepmaniaonline.net endpoint says whether a pack's charts are
//! singles or doubles. Its `style_filters=dance-double` narrows the catalog to
//! packs containing *at least one* doubles chart, which is a couple of thousand
//! and mostly singles packs with a token double on the end.
//!
//! itgdb.net keeps a hand-curated category for packs built specifically for
//! doubles. It is about thirty entries on a single page, and it is the only
//! published answer to "which packs are actually for doubles players".
//!
//! The page is HTML meant for a browser, so this scrapes the one thing it needs
//! -- the pack names -- and matches them against SMO's catalog by name, because
//! itgdb's ids are its own and mean nothing to the download host.

use deadsync_net as network;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

/// Category 5 is "Doubles" in itgdb's own taxonomy. Sorted by name so the
/// response is stable between fetches.
const DEDICATED_URL: &str = "https://itgdb.net/pack_search/\
                             ?search_by=name&order_by=name&order_dir=asc&category=5";

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItgdbPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    /// The site could not be reached. The doubles view still works from SMO's
    /// side alone, with the curated list missing -- so this is a degradation
    /// rather than a failure.
    Error,
}

#[derive(Clone, Debug)]
pub struct ItgdbSnapshot {
    pub phase: ItgdbPhase,
    /// Normalised names of packs itgdb files under Doubles.
    pub dedicated: Arc<HashSet<String>>,
    pub revision: u64,
    pub message: Option<String>,
}

impl Default for ItgdbSnapshot {
    fn default() -> Self {
        Self {
            phase: ItgdbPhase::Idle,
            dedicated: Arc::new(HashSet::new()),
            revision: 0,
            message: None,
        }
    }
}

impl ItgdbSnapshot {
    /// Whether itgdb files this pack under Doubles.
    pub fn is_dedicated(&self, pack_name: &str) -> bool {
        self.dedicated.contains(&normalize_name(pack_name))
    }
}

/// Fold a pack name to something two catalogs can be joined on.
///
/// itgdb and stepmaniaonline disagree about punctuation, spacing and case for
/// the same pack constantly, so the join has to be looser than equality: this
/// keeps letters and digits and throws the rest away.
pub fn normalize_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<ItgdbSnapshot>,
    generation: u64,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

pub fn runtime_snapshot() -> Arc<ItgdbSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Start a fetch if one has not run. Ignores repeat calls while loading or
/// once ready, so a screen may call this every frame.
pub fn runtime_ensure_dedicated() {
    start_request(false);
}

pub fn runtime_refresh_dedicated() {
    start_request(true);
}

fn start_request(force: bool) {
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == ItgdbPhase::Loading
            || !force && runtime.snapshot.phase == ItgdbPhase::Ready
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = ItgdbPhase::Loading;
        runtime.snapshot = Arc::new(snapshot);
        generation
    };

    let spawn = thread::Builder::new()
        .name("itgdb-doubles".to_owned())
        .spawn(move || finish_request(generation, fetch_dedicated()));
    if let Err(error) = spawn {
        finish_request(generation, Err(format!("start the itgdb worker: {error}")));
    }
}

fn finish_request(generation: u64, result: Result<HashSet<String>, String>) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        log::debug!("Discarding stale itgdb generation {generation}.");
        return;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    match result {
        Ok(names) => {
            log::info!("itgdb lists {} dedicated doubles packs.", names.len());
            snapshot.phase = ItgdbPhase::Ready;
            snapshot.dedicated = Arc::new(names);
            snapshot.message = None;
        }
        Err(message) => {
            log::warn!("Could not read the itgdb doubles category: {message}");
            snapshot.phase = ItgdbPhase::Error;
            snapshot.message = Some(message);
        }
    }
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

fn fetch_dedicated() -> Result<HashSet<String>, String> {
    let response = network::get_agent()
        .get(DEDICATED_URL)
        .call()
        .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
    let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
        .map_err(|error| format!("{error:?}"))?;
    let names = parse_dedicated(body.as_str());
    if names.is_empty() {
        return Err("the doubles category page listed no packs".to_owned());
    }
    Ok(names)
}

/// Pull pack names out of the category page.
///
/// Every pack on it is a `<a href="/packs/<id>/">Name</a>`, sometimes with
/// markup inside the anchor, so the tags are stripped from whatever it wraps.
fn parse_dedicated(body: &str) -> HashSet<String> {
    const NEEDLE: &str = "href=\"/packs/";
    let mut names = HashSet::new();
    let mut rest = body;

    while let Some(at) = rest.find(NEEDLE) {
        rest = &rest[at + NEEDLE.len()..];
        // the id, then the closing quote of the href
        let Some(quote) = rest.find('"') else { break };
        if !rest[..quote]
            .trim_end_matches('/')
            .chars()
            .all(|c| c.is_ascii_digit())
        {
            continue;
        }
        // the anchor's own closing bracket, then its text
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let after = &rest[open_end + 1..];
        let Some(close) = after.find("</a>") else {
            continue;
        };
        let text = strip_tags(&after[..close]);
        let text = decode_entities(text.trim());
        if !text.is_empty() {
            names.insert(normalize_name(text.as_str()));
        }
        rest = after;
    }
    names
}

fn strip_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut depth = 0usize;
    for ch in raw.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_entities(raw: &str) -> String {
    raw.replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
      <div class="results">
        <a href="/packs/12/">8 Button</a>
        <a href="/packs/34/"><span class="x">dekw&#x27;s double-deckered detritus</span></a>
        <a href="/packs/56/">  Double Raccoon  </a>
        <a href="/about/">Not a pack</a>
        <a href="/packs/78/"></a>
      </div>
    "#;

    #[test]
    fn pack_anchors_yield_names_and_everything_else_is_ignored() {
        let names = parse_dedicated(PAGE);
        assert!(names.contains(&normalize_name("8 Button")));
        assert!(names.contains(&normalize_name("Double Raccoon")));
        // markup inside the anchor is stripped, entities decoded
        assert!(names.contains(&normalize_name("dekw's double-deckered detritus")));
        // a non-pack link and an empty anchor contribute nothing
        assert_eq!(names.len(), 3);
    }

    /// The whole point of normalising: itgdb and SMO spell the same pack
    /// differently, and the join is by name because ids are not shared.
    #[test]
    fn the_join_survives_punctuation_case_and_spacing() {
        assert_eq!(
            normalize_name("DBK2 (Doubles Only)"),
            normalize_name("dbk2 doubles only")
        );
        assert_eq!(
            normalize_name("dekw's  detritus"),
            normalize_name("DEKWS DETRITUS")
        );
        assert_ne!(
            normalize_name("Double Raccoon"),
            normalize_name("Double Raccoon 2")
        );
    }

    #[test]
    fn a_snapshot_answers_by_name() {
        let snapshot = ItgdbSnapshot {
            phase: ItgdbPhase::Ready,
            dedicated: Arc::new(parse_dedicated(PAGE)),
            revision: 1,
            message: None,
        };
        assert!(snapshot.is_dedicated("8  BUTTON"));
        assert!(!snapshot.is_dedicated("Some Other Pack"));
    }

    /// The URL is built with a `` line continuation, which strips the
    /// newline AND the following indentation. Getting that wrong yields a URL
    /// with whitespace in it that fails only against the real site.
    #[test]
    fn the_category_url_survives_its_line_continuation() {
        assert!(
            !DEDICATED_URL.contains(char::is_whitespace),
            "{DEDICATED_URL}"
        );
        assert!(DEDICATED_URL.ends_with("category=5"));
        assert!(DEDICATED_URL.starts_with("https://itgdb.net/pack_search/?"));
    }

    #[test]
    fn an_empty_page_is_an_error_rather_than_an_empty_answer() {
        // silently reporting "no doubles packs exist" would be worse than
        // saying the source could not be read
        assert!(parse_dedicated("<html><body>nothing</body></html>").is_empty());
    }
}
