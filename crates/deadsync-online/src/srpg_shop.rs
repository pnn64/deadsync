use deadsync_net::{self as network, AgentConfig, HttpAgent};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::fmt::{self, Display, Formatter};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

const BASE_URL: &str = "https://srpg10.groovestats.com/";
const BASE_ORIGIN: &str = "https://srpg10.groovestats.com";
const LOGIN_URL: &str = "https://srpg10.groovestats.com/index.php?action=login";
const CATALOG_API: &str = "https://srpg10.groovestats.com/api/gen-shop-list-update.php";
const DOWNLOADS_API: &str = "https://srpg10.groovestats.com/api/gen-shop-downloads.php";
const PURCHASE_API: &str = "https://srpg10.groovestats.com/api/gen-shop-buy-sell.php";
const USER_AGENT: &str = "DeadSync SRPG10 Shop/1.0";
const UNLOCK_PACK_NAME: &str = "Stamina RPG 10 Unlocks";
pub const SRPG_SHOP_IDS: [u32; 4] = [0, 2, 3, 4];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SrpgShopPhase {
    #[default]
    Idle,
    Loading,
    Ready,
    Purchasing,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SrpgShopItemKind {
    Song,
    Relic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrpgShopItem {
    pub item_id: String,
    pub kind: SrpgShopItemKind,
    pub name: String,
    pub description: String,
    pub effect: String,
    pub cost: Option<u64>,
    pub difficulty: Option<u32>,
    pub bpm: Option<u32>,
    pub type_id: u8,
    pub owned: bool,
    pub site_downloaded: bool,
    pub download_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrpgShop {
    pub id: u32,
    pub balance: u64,
    pub items: Vec<SrpgShopItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SrpgShopSnapshot {
    pub phase: SrpgShopPhase,
    pub shops: Vec<SrpgShop>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SrpgShopError {
    Timeout,
    HttpStatus(u16),
    Request(String),
    InvalidResponse(String),
    LoginFailed,
    MissingEntrant,
}

impl Display for SrpgShopError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => f.write_str("SRPG10 request timed out"),
            Self::HttpStatus(status) => write!(f, "SRPG10 returned HTTP {status}"),
            Self::Request(message) | Self::InvalidResponse(message) => f.write_str(message),
            Self::LoginFailed => f.write_str("SRPG10 login failed; check Username and Password"),
            Self::MissingEntrant => f.write_str("SRPG10 did not return an entrant ID"),
        }
    }
}

impl std::error::Error for SrpgShopError {}

#[derive(Clone)]
struct ShopSession {
    agent: HttpAgent,
    entrant_id: String,
}

struct RuntimeState {
    generation: u64,
    snapshot: Arc<SrpgShopSnapshot>,
    session: Option<ShopSession>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            generation: 0,
            snapshot: Arc::new(SrpgShopSnapshot::default()),
            session: None,
        }
    }
}

static RUNTIME: LazyLock<Mutex<RuntimeState>> =
    LazyLock::new(|| Mutex::new(RuntimeState::default()));

pub fn runtime_snapshot() -> Arc<SrpgShopSnapshot> {
    Arc::clone(&RUNTIME.lock().unwrap().snapshot)
}

pub fn runtime_refresh(username: String, password: String) {
    let username = username.trim().to_string();
    if username.is_empty() || password.is_empty() {
        let mut runtime = RUNTIME.lock().unwrap();
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.session = None;
        runtime.snapshot = Arc::new(SrpgShopSnapshot {
            phase: SrpgShopPhase::Error,
            shops: Vec::new(),
            message: Some(
                "Add Username=... and Password=... to this profile's groovestats.ini.".to_string(),
            ),
        });
        return;
    }

    let generation = {
        let mut runtime = RUNTIME.lock().unwrap();
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.session = None;
        runtime.snapshot = Arc::new(SrpgShopSnapshot {
            phase: SrpgShopPhase::Loading,
            shops: Vec::new(),
            message: Some("Signing in to SRPG10...".to_string()),
        });
        runtime.generation
    };

    thread::spawn(move || {
        let result = login(&username, &password).and_then(|(session, shop_zero_html)| {
            fetch_snapshot(&session, Some(shop_zero_html)).map(|snapshot| (session, snapshot))
        });
        let mut runtime = RUNTIME.lock().unwrap();
        if runtime.generation != generation {
            return;
        }
        match result {
            Ok((session, snapshot)) => {
                runtime.session = Some(session);
                runtime.snapshot = Arc::new(snapshot);
            }
            Err(error) => {
                runtime.session = None;
                runtime.snapshot = Arc::new(SrpgShopSnapshot {
                    phase: SrpgShopPhase::Error,
                    shops: Vec::new(),
                    message: Some(error.to_string()),
                });
            }
        }
    });
}

pub fn runtime_purchase(shop_id: u32, item_id: String, type_id: u8) {
    let (generation, session, mut previous) = {
        let mut runtime = RUNTIME.lock().unwrap();
        let Some(session) = runtime.session.clone() else {
            return;
        };
        if runtime.snapshot.phase != SrpgShopPhase::Ready {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        let mut snapshot = (*runtime.snapshot).clone();
        snapshot.phase = SrpgShopPhase::Purchasing;
        snapshot.message = Some("Confirming purchase with SRPG10...".to_string());
        let previous = snapshot.clone();
        runtime.snapshot = Arc::new(snapshot);
        (generation, session, previous)
    };

    thread::spawn(move || {
        let purchase = purchase(&session, shop_id, &item_id, type_id);
        let result = match purchase {
            Ok(result) if result.errors.is_empty() => {
                if let Some(download) = result.download.as_ref() {
                    crate::runtime::queue_event_unlock_download(
                        &download.url,
                        &download.name,
                        UNLOCK_PACK_NAME,
                    );
                }
                let notice = result.download.map_or_else(
                    || "Purchase complete.".to_string(),
                    |download| format!("Unlocked {} and queued its download.", download.name),
                );
                fetch_snapshot(&session, None).map(|mut snapshot| {
                    snapshot.message = Some(notice);
                    snapshot
                })
            }
            Ok(result) => Err(SrpgShopError::InvalidResponse(result.errors.join(" "))),
            Err(error) => Err(error),
        };

        let mut runtime = RUNTIME.lock().unwrap();
        if runtime.generation != generation {
            return;
        }
        runtime.session = Some(session);
        match result {
            Ok(snapshot) => runtime.snapshot = Arc::new(snapshot),
            Err(error) => {
                previous.phase = SrpgShopPhase::Ready;
                previous.message = Some(format!("Purchase failed: {error}"));
                runtime.snapshot = Arc::new(previous);
            }
        }
    });
}

fn login(username: &str, password: &str) -> Result<(ShopSession, String), SrpgShopError> {
    let agent = network::build_agent(AgentConfig {
        timeout: network::GROOVESTATS_REQUEST_TIMEOUT,
    });
    let _ = get_text(&agent, BASE_URL)?;
    post_form(
        &agent,
        LOGIN_URL,
        &[
            ("name", username.to_string()),
            ("pass", password.to_string()),
        ],
        BASE_URL,
        false,
    )?;
    let shop_html = get_text(&agent, &shop_url(0))?;
    if looks_logged_out(&shop_html) {
        return Err(SrpgShopError::LoginFailed);
    }
    let entrant_id =
        find_number_after_key(&shop_html, "entrantid").ok_or(SrpgShopError::MissingEntrant)?;
    Ok((ShopSession { agent, entrant_id }, shop_html))
}

fn fetch_snapshot(
    session: &ShopSession,
    shop_zero_html: Option<String>,
) -> Result<SrpgShopSnapshot, SrpgShopError> {
    let mut workers = Vec::with_capacity(SRPG_SHOP_IDS.len());
    for shop_id in SRPG_SHOP_IDS {
        let session = session.clone();
        let page = (shop_id == 0).then(|| shop_zero_html.clone()).flatten();
        workers.push(thread::spawn(move || fetch_shop(&session, shop_id, page)));
    }

    let mut shops = Vec::with_capacity(SRPG_SHOP_IDS.len());
    for worker in workers {
        shops.push(worker.join().map_err(|_| {
            SrpgShopError::Request("SRPG10 shop worker stopped unexpectedly".to_string())
        })??);
    }
    shops.sort_unstable_by_key(|shop| {
        SRPG_SHOP_IDS
            .iter()
            .position(|id| id == &shop.id)
            .unwrap_or(usize::MAX)
    });
    Ok(SrpgShopSnapshot {
        phase: SrpgShopPhase::Ready,
        shops,
        message: None,
    })
}

fn fetch_shop(
    session: &ShopSession,
    shop_id: u32,
    page: Option<String>,
) -> Result<SrpgShop, SrpgShopError> {
    let page = match page {
        Some(page) => page,
        None => get_text(&session.agent, &shop_url(shop_id))?,
    };
    if looks_logged_out(&page) {
        return Err(SrpgShopError::LoginFailed);
    }
    let balance = find_number_after_key(&page, "var currentcurrency")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let common = [
        ("entrantid", session.entrant_id.clone()),
        ("shop", shop_id.to_string()),
    ];
    let mut catalog_params = vec![
        ("draw", "1".to_string()),
        ("start", "0".to_string()),
        ("length", "5000".to_string()),
        ("search[value]", String::new()),
        ("search[regex]", "false".to_string()),
    ];
    catalog_params.extend(common.clone());
    catalog_params.push(("type", "buy".to_string()));
    let referer = shop_url(shop_id);
    let catalog = match get_form(&session.agent, CATALOG_API, &catalog_params, &referer, true) {
        Ok(body) if parse_catalog(&body).is_ok() => body,
        _ => post_form(&session.agent, CATALOG_API, &catalog_params, &referer, true)?,
    };
    let mut download_params = common.to_vec();
    download_params.push(("type", "unlocks".to_string()));
    let downloads = post_form(
        &session.agent,
        DOWNLOADS_API,
        &download_params,
        &referer,
        true,
    )?;
    let mut items = parse_catalog(&catalog)?;
    merge_downloads(&mut items, parse_downloads(&downloads)?);
    items.sort_by(|a, b| {
        b.owned.cmp(&a.owned).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    Ok(SrpgShop {
        id: shop_id,
        balance,
        items,
    })
}

struct PurchaseDownload {
    name: String,
    url: String,
}

struct PurchaseResult {
    errors: Vec<String>,
    download: Option<PurchaseDownload>,
}

fn purchase(
    session: &ShopSession,
    shop_id: u32,
    item_id: &str,
    type_id: u8,
) -> Result<PurchaseResult, SrpgShopError> {
    let body = post_form(
        &session.agent,
        PURCHASE_API,
        &[
            ("entrantid", session.entrant_id.clone()),
            ("action", "buy".to_string()),
            ("itemid", item_id.to_string()),
            ("changequant", "1".to_string()),
            ("typeid", type_id.to_string()),
            ("shop", shop_id.to_string()),
        ],
        &shop_url(shop_id),
        true,
    )?;
    parse_purchase(&body)
}

fn parse_purchase(body: &str) -> Result<PurchaseResult, SrpgShopError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let download = value
        .get("unlocks")
        .and_then(Value::as_object)
        .and_then(download_from_object)
        .map(|download| PurchaseDownload {
            name: download.name,
            url: download.url,
        });
    Ok(PurchaseResult { errors, download })
}

#[derive(Deserialize)]
struct DownloadResponse {
    #[serde(default)]
    unlocks: Vec<DownloadRow>,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Deserialize)]
struct DownloadRow {
    id: Value,
    #[serde(default)]
    data: String,
    song: String,
    url: String,
    #[serde(default)]
    dled: u8,
}

struct ParsedDownload {
    item_id: String,
    name: String,
    details: String,
    url: String,
    site_downloaded: bool,
}

fn parse_downloads(body: &str) -> Result<Vec<ParsedDownload>, SrpgShopError> {
    let response: DownloadResponse = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    if !response.errors.is_empty() {
        return Err(SrpgShopError::InvalidResponse(response.errors.join(" ")));
    }
    Ok(response
        .unlocks
        .into_iter()
        .filter(|row| row.url.contains(".zip"))
        .map(|row| ParsedDownload {
            item_id: value_text(&row.id),
            name: clean_cell(&row.song),
            details: clean_cell(&row.data),
            url: absolutize_url(&row.url),
            site_downloaded: row.dled != 0,
        })
        .collect())
}

fn merge_downloads(items: &mut Vec<SrpgShopItem>, downloads: Vec<ParsedDownload>) {
    for download in downloads {
        if let Some(item) = items
            .iter_mut()
            .find(|item| item.item_id == download.item_id)
        {
            item.owned = true;
            item.site_downloaded = download.site_downloaded;
            item.download_url = Some(download.url);
            continue;
        }
        let (difficulty, bpm) = {
            let mut numbers = download
                .details
                .split_whitespace()
                .filter_map(|part| part.parse::<u32>().ok());
            (numbers.next(), numbers.next())
        };
        items.push(SrpgShopItem {
            item_id: download.item_id,
            kind: SrpgShopItemKind::Song,
            name: download.name,
            description: "Purchased song unlock".to_string(),
            effect: download.details,
            cost: None,
            difficulty,
            bpm,
            type_id: 1,
            owned: true,
            site_downloaded: download.site_downloaded,
            download_url: Some(download.url),
        });
    }
}

fn parse_catalog(body: &str) -> Result<Vec<SrpgShopItem>, SrpgShopError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    let rows = match &value {
        Value::Object(map) => object_array(map, &["data", "aaData", "rows", "items"]),
        Value::Array(rows) => Some(rows),
        _ => None,
    }
    .ok_or_else(|| SrpgShopError::InvalidResponse("SRPG10 catalog has no rows".to_string()))?;
    Ok(rows.iter().filter_map(catalog_item).collect())
}

fn catalog_item(row: &Value) -> Option<SrpgShopItem> {
    let cells = row.as_array()?;
    let cell = |index: usize| cells.get(index).map(value_text).unwrap_or_default();
    let type_id = cell(11).parse().unwrap_or(0);
    let kind = if type_id == 1 {
        SrpgShopItemKind::Song
    } else {
        SrpgShopItemKind::Relic
    };
    Some(SrpgShopItem {
        item_id: cell(0),
        kind,
        name: clean_cell(&cell(2)),
        description: clean_cell(&cell(3)),
        effect: clean_cell(&cell(4)).replace('|', "  •  "),
        cost: cell(7).replace(',', "").parse().ok(),
        difficulty: (kind == SrpgShopItemKind::Song)
            .then(|| cell(12).parse().ok())
            .flatten(),
        bpm: (kind == SrpgShopItemKind::Song)
            .then(|| cell(13).parse().ok())
            .flatten(),
        type_id,
        owned: false,
        site_downloaded: false,
        download_url: None,
    })
}

fn download_from_object(map: &Map<String, Value>) -> Option<ParsedDownload> {
    let url = object_text(map, &["url", "href", "download_url"])?;
    if !url.contains(".zip") {
        return None;
    }
    Some(ParsedDownload {
        item_id: object_text(map, &["id", "cid", "itemid"]).unwrap_or_default(),
        name: object_text(map, &["song", "title", "name"])
            .map(|name| clean_cell(&name))
            .unwrap_or_else(|| "SRPG10 unlock".to_string()),
        details: String::new(),
        url: absolutize_url(&url),
        site_downloaded: false,
    })
}

fn get_text(agent: &HttpAgent, url: &str) -> Result<String, SrpgShopError> {
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8")
        .call()
        .map_err(|error| network_error(network::error_from_ureq(error)))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| SrpgShopError::Request(error.to_string()))
}

fn post_form(
    agent: &HttpAgent,
    url: &str,
    params: &[(&str, String)],
    referer: &str,
    ajax: bool,
) -> Result<String, SrpgShopError> {
    let pairs: Vec<(&str, &str)> = params
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let mut request = agent
        .post(url)
        .header("User-Agent", USER_AGENT)
        .header(
            "Accept",
            "application/json,text/javascript,text/html,*/*;q=0.8",
        )
        .header("Origin", BASE_ORIGIN)
        .header("Referer", referer);
    if ajax {
        request = request.header("X-Requested-With", "XMLHttpRequest");
    }
    let mut response = request
        .send_form(pairs)
        .map_err(|error| network_error(network::error_from_ureq(error)))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| SrpgShopError::Request(error.to_string()))
}

fn get_form(
    agent: &HttpAgent,
    url: &str,
    params: &[(&str, String)],
    referer: &str,
    ajax: bool,
) -> Result<String, SrpgShopError> {
    let mut request = agent
        .get(url)
        .query_pairs(params.iter().map(|(key, value)| (*key, value.as_str())))
        .header("User-Agent", USER_AGENT)
        .header(
            "Accept",
            "application/json,text/javascript,text/html,*/*;q=0.8",
        )
        .header("Referer", referer);
    if ajax {
        request = request.header("X-Requested-With", "XMLHttpRequest");
    }
    let mut response = request
        .call()
        .map_err(|error| network_error(network::error_from_ureq(error)))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| SrpgShopError::Request(error.to_string()))
}

fn network_error(error: network::NetworkError) -> SrpgShopError {
    match error {
        network::NetworkError::Timeout => SrpgShopError::Timeout,
        network::NetworkError::HttpStatus(status) => SrpgShopError::HttpStatus(status),
        network::NetworkError::Request(message) | network::NetworkError::Decode(message) => {
            SrpgShopError::Request(message)
        }
    }
}

fn shop_url(shop_id: u32) -> String {
    format!("{BASE_ORIGIN}/index.php?page=genshop&shopid={shop_id}")
}

fn looks_logged_out(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("you need to be logged in")
        || lower.contains("please log in")
        || (lower.contains("username:")
            && lower.contains("password:")
            && lower.contains("log in")
            && !lower.contains("log out")
            && !lower.contains("logout"))
}

fn find_number_after_key(haystack: &str, key: &str) -> Option<String> {
    let lower = haystack.to_ascii_lowercase();
    let start = lower.find(&key.to_ascii_lowercase())? + key.len();
    let mut digits = String::new();
    let mut found = false;
    for ch in haystack[start..].chars().take(200) {
        if ch.is_ascii_digit() {
            digits.push(ch);
            found = true;
        } else if found {
            break;
        }
    }
    (!digits.is_empty()).then_some(digits)
}

fn object_array<'a>(map: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Vec<Value>> {
    keys.iter().find_map(|wanted| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .and_then(|(_, value)| value.as_array())
    })
}

fn object_text(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|wanted| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value_text(value))
            .filter(|value| !value.is_empty())
    })
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

fn clean_cell(text: &str) -> String {
    let text = text
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn absolutize_url(url: &str) -> String {
    let url = url.replace("\\/", "/");
    if url.starts_with("https://") || url.starts_with("http://") {
        url
    } else if url.starts_with('/') {
        format!("{BASE_ORIGIN}{url}")
    } else {
        format!("{BASE_ORIGIN}/{}", url.trim_start_matches("./"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_catalog_song_and_relic_rows() {
        let body = r#"{"data":[["7","chart.png","Fast Song","Purchase to unlock","Difficulty: 14|Speed Tier: 180 BPM","2","0","1234","0","0","0","1","14","180","0"],["2","axe.png","Stone Axe","Useful","Lv. 1 EP","0","0","294","0","0","0","0","---","---","---"]]}"#;
        let items = parse_catalog(body).expect("catalog");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, SrpgShopItemKind::Song);
        assert_eq!(items[0].difficulty, Some(14));
        assert_eq!(items[0].bpm, Some(180));
        assert_eq!(items[1].kind, SrpgShopItemKind::Relic);
        assert_eq!(items[1].cost, Some(294));
    }

    #[test]
    fn merges_owned_downloads_into_catalog() {
        let mut items = vec![SrpgShopItem {
            item_id: "7".to_string(),
            kind: SrpgShopItemKind::Song,
            name: "Fast Song".to_string(),
            description: String::new(),
            effect: String::new(),
            cost: Some(1234),
            difficulty: Some(14),
            bpm: Some(180),
            type_id: 1,
            owned: false,
            site_downloaded: false,
            download_url: None,
        }];
        merge_downloads(
            &mut items,
            vec![ParsedDownload {
                item_id: "7".to_string(),
                name: "Fast Song".to_string(),
                details: "14 180".to_string(),
                url: "https://example.test/song.zip".to_string(),
                site_downloaded: true,
            }],
        );
        assert!(items[0].owned);
        assert!(items[0].site_downloaded);
        assert_eq!(
            items[0].download_url.as_deref(),
            Some("https://example.test/song.zip")
        );
    }

    #[test]
    fn finds_page_numbers_after_javascript_keys() {
        let html = "var entrantid = 24; var currentcurrency = 20737;";
        assert_eq!(
            find_number_after_key(html, "entrantid").as_deref(),
            Some("24")
        );
        assert_eq!(
            find_number_after_key(html, "var currentcurrency").as_deref(),
            Some("20737")
        );
    }

    #[test]
    fn parses_purchase_unlock_and_errors() {
        let result = parse_purchase(
            r#"{"unlocks":{"song":"Fast Song","url":"\/downloads\/unlocks\/7.zip"},"errors":[]}"#,
        )
        .expect("purchase response");
        assert!(result.errors.is_empty());
        let download = result.download.expect("download");
        assert_eq!(download.name, "Fast Song");
        assert_eq!(
            download.url,
            "https://srpg10.groovestats.com/downloads/unlocks/7.zip"
        );

        let result =
            parse_purchase(r#"{"errors":["Not enough Gold"]}"#).expect("purchase error response");
        assert_eq!(result.errors, ["Not enough Gold"]);
        assert!(result.download.is_none());
    }
}
