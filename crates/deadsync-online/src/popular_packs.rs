//! What people are actually playing, from arrowcloud.dance.
//!
//! stepmaniaonline.net can say what is *new*. It cannot say what is *good*,
//! because it has no play data -- so the ITGmania browser's featured strip is
//! not sourced from it at all. It comes from arrowcloud's popularity ranking,
//! joined back to the catalogue by pack name.
//!
//! Two things here are deliberately not the original's:
//!
//! * It walks until the API says there is no next page, rather than stopping
//!   after a hardcoded three. The catalogue is 318 packs over four pages
//!   today, so three drops the tail.
//! * It orders by each entry's own `popularity` score rather than by the order
//!   pages happened to arrive in. The score is right there, and using it makes
//!   the ranking independent of how the walk went.
//!
//! Banner artwork comes from the three flat fields, smallest first. The
//! entries also carry a `bannerVariants` list offering 256px JPEGs of about
//! eleven kilobytes, which looks like the obvious thing to prefer -- but those
//! URLs point at a *superseded upload folder* and answer 403. Compare one
//! entry's two paths:
//!
//! ```text
//! bannerUrl   .../packs/1779629235986_tech_spectrum_super/pack-banner.png     200
//! variant sm  .../packs/1753241110612_tech_spectrum_super/pack-banner_sm.jpeg 403
//! ```
//!
//! Measured across the whole featured strip: the flat chain fails zero times
//! out of twenty-four, the variants fail on every entry that has them and no
//! live small field. So the variants are not read at all.

use deadsync_net as network;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::thread;

const POPULAR_URL: &str = "https://api.arrowcloud.dance/packs";
/// The API clamps anything larger to 100 without saying so.
const PAGE_LIMIT: usize = 100;
/// A stop for a walk that would otherwise trust the server's own paging
/// forever. Four pages covers the catalogue today.
const MAX_PAGES: usize = 12;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// One pack in the ranking.
#[derive(Clone, Debug, PartialEq)]
pub struct PopularPack {
    pub name: String,
    /// arrowcloud's own score. Ordering by this rather than by arrival order
    /// is what makes the ranking independent of how the walk went.
    pub popularity: f64,
    pub simfile_count: u32,
    /// The smallest banner the service offers, which is the one worth
    /// fetching for artwork nothing draws above 104px.
    pub banner_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopularPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Clone, Debug, Default)]
pub struct PopularSnapshot {
    pub phase: PopularPhase,
    /// Most played first.
    pub packs: Arc<[PopularPack]>,
    pub revision: u64,
    pub message: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    snapshot: Arc<PopularSnapshot>,
    generation: u64,
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

fn lock_runtime() -> MutexGuard<'static, RuntimeState> {
    RUNTIME.lock().unwrap_or_else(|error| error.into_inner())
}

#[must_use]
pub fn runtime_snapshot() -> Arc<PopularSnapshot> {
    Arc::clone(&lock_runtime().snapshot)
}

/// Fetch the ranking once. Ignores repeat calls while loading or once ready,
/// so a screen may call this every frame.
pub fn runtime_ensure_popular() {
    start_request(false);
}

/// Fetch it again even though it is already loaded.
pub fn runtime_refresh_popular() {
    start_request(true);
}

fn start_request(force: bool) {
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.phase == PopularPhase::Loading
            || !force && runtime.snapshot.phase == PopularPhase::Ready
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = PopularPhase::Loading;
        snapshot.message = Some("Loading popular packs...".to_owned());
        snapshot.revision = snapshot.revision.wrapping_add(1);
        runtime.snapshot = Arc::new(snapshot);
        generation
    };

    let spawn = thread::Builder::new()
        .name("arrowcloud-popular".to_owned())
        .spawn(move || finish(generation, fetch_all()));
    if let Err(error) = spawn {
        finish(
            generation,
            Err(format!("start the popular worker: {error}")),
        );
    }
}

fn finish(generation: u64, result: Result<Vec<PopularPack>, String>) {
    let mut runtime = lock_runtime();
    if runtime.generation != generation {
        return;
    }
    let mut snapshot = (*runtime.snapshot).clone();
    match result {
        Ok(packs) => {
            log::info!("Loaded {} packs from the arrowcloud ranking.", packs.len());
            snapshot.phase = PopularPhase::Ready;
            snapshot.packs = Arc::from(packs);
            snapshot.message = None;
        }
        Err(message) => {
            // The strip degrades to the newest packs rather than vanishing, so
            // this is a warning rather than a failure of the screen.
            log::warn!("Could not load the arrowcloud ranking: {message}");
            snapshot.phase = PopularPhase::Error;
            snapshot.message = Some(message);
        }
    }
    snapshot.revision = snapshot.revision.wrapping_add(1);
    runtime.snapshot = Arc::new(snapshot);
}

fn fetch_all() -> Result<Vec<PopularPack>, String> {
    let agent = network::get_agent();
    let mut packs: Vec<PopularPack> = Vec::new();

    for page in 1..=MAX_PAGES {
        let url = page_url(page);
        let response = agent
            .get(url)
            .call()
            .map_err(|error| format!("{:?}", network::error_from_ureq(error)))?;
        let body = network::read_text_body_bounded(response, MAX_BODY_BYTES)
            .map_err(|error| format!("{error:?}"))?;
        let parsed: Response =
            serde_json::from_str(body.as_str()).map_err(|error| format!("ranking: {error}"))?;

        let count = parsed.data.len();
        for entry in parsed.data {
            packs.push(PopularPack {
                name: entry.name.clone(),
                popularity: entry.popularity,
                simfile_count: entry.simfile_count,
                banner_url: entry.best_banner(),
            });
        }
        // An out-of-range page answers 200 with an empty array rather than an
        // error, so the walk has to stop on content, not on status.
        if count == 0 || !parsed.meta.has_next_page {
            break;
        }
    }

    if packs.is_empty() {
        return Err("the ranking returned no packs".to_owned());
    }
    // The score decides, not the order the pages landed in.
    packs.sort_by(|a, b| {
        b.popularity
            .partial_cmp(&a.popularity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(packs)
}

fn page_url(page: usize) -> String {
    format!("{POPULAR_URL}?page={page}&limit={PAGE_LIMIT}&orderBy=popularity&orderDirection=desc")
}

// --- the response -------------------------------------------------------------

#[derive(serde::Deserialize)]
struct Response {
    #[serde(default)]
    data: Vec<Entry>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default)]
    has_next_page: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    name: String,
    #[serde(default)]
    popularity: f64,
    #[serde(default)]
    simfile_count: u32,
    #[serde(default)]
    banner_url: Option<String>,
    #[serde(default)]
    md_banner_url: Option<String>,
    #[serde(default)]
    sm_banner_url: Option<String>,
}

impl Entry {
    /// The smallest banner this pack actually serves.
    ///
    /// Only the flat fields. `bannerVariants` advertises much smaller JPEGs
    /// but points them at an older upload folder that answers 403, so reading
    /// it produced cards that spun forever -- see the note at the top of this
    /// file. When the small and medium fields are null, as they are for most
    /// entries, the full-size PNG is the one that exists.
    fn best_banner(&self) -> Option<String> {
        self.sm_banner_url
            .clone()
            .or_else(|| self.md_banner_url.clone())
            .or_else(|| self.banner_url.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"{
      "data": [
        {"id": 304, "name": "ITL Online 2026", "popularity": 8011.25, "simfileCount": 310,
         "bannerUrl": "https://a.test/big.png", "mdBannerUrl": null, "smBannerUrl": null},
        {"id": 86, "name": "Tech Spectrum Super", "popularity": 90.5, "simfileCount": 375,
         "bannerUrl": "https://a.test/full.png", "mdBannerUrl": null, "smBannerUrl": null,
         "bannerVariants": {
           "sm": [{"url": "https://a.test/stale_sm.jpeg", "format": "jpeg"}],
           "md": [{"url": "https://a.test/stale_md.jpeg", "format": "jpeg"}],
           "original": [], "all": []}},
        {"id": 42, "name": "Has A Small One", "popularity": 5.0, "simfileCount": 9,
         "bannerUrl": "https://a.test/full2.png",
         "mdBannerUrl": "https://a.test/md2.jpeg",
         "smBannerUrl": "https://a.test/sm2.jpeg"},
        {"id": 137, "name": "No Art At All", "popularity": 1.0, "simfileCount": 4,
         "bannerUrl": null, "mdBannerUrl": null, "smBannerUrl": null}
      ],
      "meta": {"page": 1, "limit": 100, "total": 318, "totalPages": 4, "hasNextPage": true},
      "filters": {"orderBy": "popularity", "orderDirection": "desc"}
    }"#;

    fn parse(body: &str) -> Response {
        serde_json::from_str(body).expect("the ranking's own shape")
    }

    #[test]
    fn the_envelope_is_the_services_own() {
        let parsed = parse(PAGE);
        assert_eq!(parsed.data.len(), 4);
        assert!(
            parsed.meta.has_next_page,
            "the walk must not stop at page 1"
        );
        assert_eq!(parsed.data[0].name, "ITL Online 2026");
        assert!((parsed.data[0].popularity - 8011.25).abs() < 1e-9);
        assert_eq!(parsed.data[0].simfile_count, 310);
    }

    /// The smallest flat field wins, and the variants are never read.
    ///
    /// Those variant URLs point at a superseded upload folder and answer 403;
    /// preferring them is what left three of the twenty-four featured cards
    /// spinning. Measured live: the flat chain fails zero times out of
    /// twenty-four.
    #[test]
    fn the_smallest_banner_that_actually_serves_is_preferred() {
        let parsed = parse(PAGE);

        // small and medium are null, so the full-size PNG is the one that
        // exists -- NOT the small jpeg the variants advertise
        assert_eq!(
            parsed.data[1].best_banner().as_deref(),
            Some("https://a.test/full.png"),
            "a stale variant must never be chosen over a live full-size url"
        );

        // where the flat small field is populated it is genuinely smaller and
        // genuinely there, so it leads
        assert_eq!(
            parsed.data[2].best_banner().as_deref(),
            Some("https://a.test/sm2.jpeg")
        );

        // no variants at all: the flat chain still answers
        assert_eq!(
            parsed.data[0].best_banner().as_deref(),
            Some("https://a.test/big.png")
        );
        // and a pack with no artwork anywhere says so rather than guessing
        assert_eq!(parsed.data[3].best_banner(), None);
    }

    /// The page number is the whole query, and getting it wrong would silently
    /// re-fetch page one forever.
    #[test]
    fn the_page_url_asks_for_the_ranking_it_says_it_does() {
        let url = page_url(3);
        assert!(url.contains("page=3"));
        assert!(url.contains("limit=100"));
        assert!(url.contains("orderBy=popularity"));
        assert!(url.contains("orderDirection=desc"));
    }

    /// An entry the service has no banner for must not be dropped here: the
    /// screen decides whether a card without artwork is worth showing.
    #[test]
    fn a_missing_meta_block_does_not_fail_the_parse() {
        let parsed = parse(r#"{"data": [{"name": "X", "popularity": 1.0}]}"#);
        assert_eq!(parsed.data.len(), 1);
        assert!(
            !parsed.meta.has_next_page,
            "no meta means no next page, so the walk stops"
        );
    }
}
