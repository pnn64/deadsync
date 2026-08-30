//! Readers for an `ITGmania` `LocalProfiles/<id>/` directory: the editable profile
//! metadata, online keys, avatar, and the `Stats.xml` high-score database.
//!
//! Nothing here touches `DeadSync` state — these functions only turn files on disk
//! into plain Rust structs. Mapping into `DeadSync` types happens in the
//! root import orchestration layer and in `deadsync_score::import`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

use deadsync_score::ImportedHighScore;
use rustc_hash::FxBuildHasher;

use super::xml::{self, XmlNode};
use crate::ini::SimpleIni;

/// Editable profile metadata from `Editable.ini`.
#[derive(Debug, Clone, Default)]
pub struct ItgEditable {
    pub display_name: String,
    pub weight_pounds: u32,
    pub birth_year: u32,
    pub last_used_high_score_name: String,
    /// `IgnoreStepCountCalories` — disables step-count calorie estimation.
    pub ignore_step_count_calories: bool,
}

/// `GrooveStats` + `ArrowCloud` online keys.
#[derive(Debug, Clone, Default)]
pub struct ItgOnlineKeys {
    pub groovestats_api_key: String,
    pub groovestats_username: String,
    pub groovestats_is_pad_player: bool,
    pub arrowcloud_api_key: String,
}

/// One `<Steps>` block within a `<Song>` and all of its high scores.
#[derive(Debug, Clone, Default)]
pub struct ItgStepsScores {
    pub steps_type: String,
    pub difficulty: String,
    /// `Description` attribute — used to disambiguate Edit charts.
    pub description: String,
    pub high_scores: Vec<ImportedHighScore>,
}

/// One `<Song Dir="...">` block with its per-difficulty score lists.
#[derive(Debug, Clone, Default)]
pub struct ItgSongScores {
    /// Raw `Dir` attribute, e.g. `"Songs/Pack/Song/"`.
    pub dir: String,
    pub steps: Vec<ItgStepsScores>,
}

/// Everything we managed to read from one `ITGmania` local profile directory.
#[derive(Debug, Clone, Default)]
pub struct ItgSource {
    pub source_dir: PathBuf,
    pub editable: ItgEditable,
    pub online: ItgOnlineKeys,
    pub avatar_path: Option<PathBuf>,
    /// Raw `[Simply Love]` settings from `Simply Love UserPrefs.ini`, if present.
    pub simply_love: HashMap<String, String>,
    pub songs: Vec<ItgSongScores>,
    /// Favorited song keys (`Pack/SongFolder`) from `favorites.txt`, with any
    /// Simply Love section headers stripped.
    pub favorites: Vec<String>,
    /// Raw contents of `ITL2026.json` (Simply Love ITL event data), if present.
    pub itl_json: Option<String>,
    /// `Stats.xml` `GeneralData/CurrentCombo` — the running combo carried between
    /// songs. `0` when absent.
    pub current_combo: u32,
    /// `Stats.xml` `GeneralData/Guid` — `ITGmania`'s stable per-profile identifier.
    /// Used to derive the imported `DeadSync` profile's GUID so re-importing the
    /// same profile yields the same identity. Empty when the `Stats.xml` is
    /// missing or has no `Guid`.
    pub guid: String,
}

impl ItgSource {
    /// Total number of high-score records across all songs/steps.
    #[must_use]
    pub fn total_high_scores(&self) -> usize {
        self.songs
            .iter()
            .flat_map(|s| s.steps.iter())
            .map(|st| st.high_scores.len())
            .sum()
    }
}

#[derive(Debug)]
pub enum ItgReadError {
    /// The directory doesn't look like an `ITGmania` profile (no `Editable.ini`).
    NotAProfile(PathBuf),
    Io(std::io::Error),
    Xml(xml::XmlError),
}

impl std::fmt::Display for ItgReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAProfile(p) => {
                write!(
                    f,
                    "{} is not an ITGmania profile (no Editable.ini)",
                    p.display()
                )
            }
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Xml(e) => write!(f, "Stats.xml parse error: {e}"),
        }
    }
}

impl std::error::Error for ItgReadError {}

impl From<std::io::Error> for ItgReadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Returns `true` if `dir` looks like an `ITGmania` local profile directory.
#[must_use]
pub fn is_itg_profile_dir(dir: &Path) -> bool {
    find_case_insensitive(dir, "Editable.ini").is_some()
}

/// Cheaply reads just the `DisplayName` from a profile's `Editable.ini`,
/// without parsing the (potentially large) `Stats.xml`. Used to label profiles
/// in the import picker. Returns `None` when the file is missing or the name is
/// blank.
#[must_use]
pub fn read_display_name(dir: &Path) -> Option<String> {
    let path = find_case_insensitive(dir, "Editable.ini")?;
    let name = read_editable(&path).display_name;
    if name.trim().is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Cheaply reads the `ITGmania` profile `Guid` from `Stats.xml` (or `Stats.xml.gz`)
/// without a full XML parse. The `Guid` lives in `GeneralData` at the very top of
/// the file, so we only scan the head (bounded) instead of the whole — possibly
/// many-megabyte — score database. Used by the import picker to flag profiles
/// that have already been imported. Returns `None` when absent or unreadable.
#[must_use]
pub fn read_source_guid(dir: &Path) -> Option<String> {
    // GeneralData (and thus Guid) sits before SongScores, well within this cap.
    const HEAD_CAP: u64 = 256 * 1024;
    let head = if let Some(path) = find_case_insensitive(dir, "Stats.xml") {
        read_head(&path, HEAD_CAP)?
    } else if let Some(path) = find_case_insensitive(dir, "Stats.xml.gz") {
        read_gz_head(&path, HEAD_CAP)?
    } else {
        return None;
    };
    extract_guid(&head)
}

/// Extracts the text inside the first `<Guid>…</Guid>` element. Returns `None`
/// when the element is absent or empty.
fn extract_guid(s: &str) -> Option<String> {
    let start = s.find("<Guid>")? + "<Guid>".len();
    let rest = &s[start..];
    let end = rest.find("</Guid>")?;
    let guid = rest[..end].trim();
    if guid.is_empty() {
        None
    } else {
        Some(guid.to_string())
    }
}

/// Reads up to `cap` bytes from the head of `path` as lossy UTF-8.
fn read_head(path: &Path, cap: u64) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(cap).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Decompresses up to `cap` bytes from the head of a gzip file as lossy UTF-8.
fn read_gz_head(path: &Path, cap: u64) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut buf = Vec::new();
    decoder.take(cap).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Reads an entire `ITGmania` local profile directory into an [`ItgSource`].
pub fn read_profile_dir(dir: &Path) -> Result<ItgSource, ItgReadError> {
    let editable_path = find_case_insensitive(dir, "Editable.ini")
        .ok_or_else(|| ItgReadError::NotAProfile(dir.to_path_buf()))?;

    let editable = read_editable(&editable_path);
    let online = read_online_keys(dir);
    let avatar_path = find_avatar(dir);
    let simply_love = read_simply_love(dir);
    let stats = read_stats(dir)?;
    let favorites = read_favorites(dir);
    let itl_json = read_itl_json(dir);

    Ok(ItgSource {
        source_dir: dir.to_path_buf(),
        editable,
        online,
        avatar_path,
        simply_love,
        songs: stats.songs,
        favorites,
        itl_json,
        current_combo: stats.current_combo,
        guid: stats.guid,
    })
}

fn read_editable(path: &Path) -> ItgEditable {
    let mut ini = SimpleIni::new();
    if ini.load(path).is_err() {
        return ItgEditable::default();
    }
    let get = |k: &str| ini.get("Editable", k).map(|s| s.trim().to_string());
    ItgEditable {
        display_name: get("DisplayName").unwrap_or_default(),
        weight_pounds: get("WeightPounds")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0),
        birth_year: get("BirthYear")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0),
        last_used_high_score_name: get("LastUsedHighScoreName").unwrap_or_default(),
        ignore_step_count_calories: get("IgnoreStepCountCalories")
            .map(|s| parse_bool(&s))
            .unwrap_or(false),
    }
}

fn read_online_keys(dir: &Path) -> ItgOnlineKeys {
    let mut keys = ItgOnlineKeys::default();

    if let Some(path) = find_case_insensitive(dir, "GrooveStats.ini") {
        let mut ini = SimpleIni::new();
        if ini.load(&path).is_ok() {
            keys.groovestats_api_key = ini
                .get("GrooveStats", "ApiKey")
                .unwrap_or_default()
                .trim()
                .to_string();
            keys.groovestats_username = ini
                .get("GrooveStats", "Username")
                .unwrap_or_default()
                .trim()
                .to_string();
            keys.groovestats_is_pad_player = ini
                .get("GrooveStats", "IsPadPlayer")
                .map(parse_bool)
                .unwrap_or(false);
        }
    }

    if let Some(path) = find_case_insensitive(dir, "ArrowCloud.ini") {
        let mut ini = SimpleIni::new();
        if ini.load(&path).is_ok() {
            keys.arrowcloud_api_key = ini
                .get("ArrowCloud", "ApiKey")
                .unwrap_or_default()
                .trim()
                .to_string();
        }
    }

    keys
}

/// Reads the `[Simply Love]` section of `Simply Love UserPrefs.ini` into a map.
/// Returns an empty map when the file or section is missing (a profile that
/// never ran Simply Love).
fn read_simply_love(dir: &Path) -> HashMap<String, String> {
    let mut ini = SimpleIni::new();
    if let Some(path) = find_case_insensitive(dir, "Simply Love UserPrefs.ini")
        && ini.load(&path).is_ok()
        && let Some(section) = ini.get_section("Simply Love")
    {
        return section
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
    }
    HashMap::new()
}

/// Parses Simply Love `favorites.txt` content into a list of `Pack/SongFolder`
/// song keys. Section header lines (which begin with `---`, e.g.
/// `---My Stamina Playlist`) and blank lines are skipped; remaining lines are
/// the favorited song paths. Order is preserved and duplicates are removed.
#[must_use]
pub fn parse_favorites_text(text: &str) -> Vec<String> {
    parse_favorites_borrowed(text)
}

fn favorite_capacity_hint(text: &str) -> usize {
    const SAMPLE_BYTES: usize = 4 * 1024;

    if text.is_empty() {
        return 0;
    }
    let mut sample_len = text.len().min(SAMPLE_BYTES);
    while !text.is_char_boundary(sample_len) {
        sample_len -= 1;
    }
    let candidates = text[..sample_len]
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("---")
        })
        .count();
    if candidates == 0 {
        return 1;
    }
    text.len()
        .saturating_mul(candidates)
        .div_ceil(sample_len)
        .max(1)
}

#[cfg(any(test, feature = "bench-support"))]
fn parse_favorites_owned_with<S: std::hash::BuildHasher + Default, const RESERVE: bool>(
    text: &str,
) -> Vec<String> {
    let capacity = if RESERVE {
        favorite_capacity_hint(text)
    } else {
        0
    };
    let mut seen = HashSet::with_capacity_and_hasher(capacity, S::default());
    let mut out = Vec::with_capacity(capacity);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("---") {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

#[derive(Clone, Copy)]
struct AsciiCaseless<'a>(&'a str);

impl PartialEq for AsciiCaseless<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(other.0)
    }
}

impl Eq for AsciiCaseless<'_> {}

impl Hash for AsciiCaseless<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        for byte in self.0.bytes() {
            byte.to_ascii_lowercase().hash(state);
        }
    }
}

fn parse_favorites_borrowed(text: &str) -> Vec<String> {
    let capacity = favorite_capacity_hint(text);
    let mut seen = HashSet::with_capacity_and_hasher(capacity, FxBuildHasher);
    let mut out = Vec::with_capacity(capacity);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("---") {
            continue;
        }
        if seen.insert(AsciiCaseless(trimmed)) {
            out.push(trimmed.to_owned());
        }
    }
    out
}

/// Reads `favorites.txt` from a profile directory. Returns an empty list when
/// the file is missing (a profile that never favorited anything).
fn read_favorites(dir: &Path) -> Vec<String> {
    let Some(path) = find_case_insensitive(dir, "favorites.txt") else {
        return Vec::new();
    };
    match fs::read_to_string(&path) {
        Ok(text) => parse_favorites_text(&text),
        Err(_) => Vec::new(),
    }
}

/// Reads the raw `ITL2026.json` (Simply Love ITL event data) from a profile
/// directory, if present. The contents are parsed downstream by `DeadSync`'s ITL
/// module, which uses the same schema. Returns `None` when the file is missing
/// or unreadable.
fn read_itl_json(dir: &Path) -> Option<String> {
    let path = find_case_insensitive(dir, "ITL2026.json")?;
    fs::read_to_string(&path).ok()
}

/// Finds an avatar image in the profile dir. `ITGmania` uses `Avatar.png`; some
/// setups also drop a generic image. We accept common names case-insensitively.
fn find_avatar(dir: &Path) -> Option<PathBuf> {
    const NAMES: [&str; 4] = ["Avatar.png", "avatar.png", "Avatar.jpg", "Avatar.jpeg"];
    for name in NAMES {
        if let Some(p) = find_case_insensitive(dir, name) {
            return Some(p);
        }
    }
    None
}

/// Parsed contents of `Stats.xml` beyond the per-chart song scores.
#[derive(Debug, Clone, Default)]
pub struct ItgStatsData {
    pub songs: Vec<ItgSongScores>,
    /// `GeneralData/CurrentCombo`.
    pub current_combo: u32,
    /// `GeneralData/Guid` — empty when absent.
    pub guid: String,
}

/// Reads `Stats.xml` (or `Stats.xml.gz`) and returns the parsed song scores plus
/// selected `GeneralData`. A missing Stats file is not an error — it yields an
/// empty result.
fn read_stats(dir: &Path) -> Result<ItgStatsData, ItgReadError> {
    let content = if let Some(path) = find_case_insensitive(dir, "Stats.xml") {
        String::from_utf8_lossy(&fs::read(&path)?).into_owned()
    } else if let Some(path) = find_case_insensitive(dir, "Stats.xml.gz") {
        read_gz_to_string(&path)?
    } else {
        return Ok(ItgStatsData::default());
    };

    let root = xml::parse(&content).map_err(ItgReadError::Xml)?;
    let (current_combo, guid) = parse_general_data(&root);
    Ok(ItgStatsData {
        songs: parse_song_scores_owned(root),
        current_combo,
        guid,
    })
}

/// Extracts `(CurrentCombo, Guid)` from a parsed `Stats.xml` root's
/// `GeneralData`. Returns `(0, "")` when the node is absent.
fn parse_general_data(root: &XmlNode) -> (u32, String) {
    let general = if root.tag == "GeneralData" {
        root
    } else {
        match root.child("GeneralData") {
            Some(g) => g,
            None => return (0, String::new()),
        }
    };
    let combo = general.child_parse::<u32>("CurrentCombo").unwrap_or(0);
    let guid = general.child_text("Guid").trim().to_string();
    (combo, guid)
}

fn read_gz_to_string(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    decode_gz_to_string_with::<true, true>(&bytes)
}

fn decode_gz_to_string_with<const RESERVE_OUTPUT: bool, const REUSE_UTF8: bool>(
    bytes: &[u8],
) -> Result<String, std::io::Error> {
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut out = if RESERVE_OUTPUT {
        Vec::with_capacity(gzip_output_capacity(bytes))
    } else {
        Vec::new()
    };
    decoder.read_to_end(&mut out)?;
    if REUSE_UTF8 {
        Ok(match String::from_utf8(out) {
            Ok(text) => text,
            Err(invalid) => String::from_utf8_lossy(invalid.as_bytes()).into_owned(),
        })
    } else {
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}

fn gzip_output_capacity(bytes: &[u8]) -> usize {
    const MAX_PREALLOC_BYTES: usize = 512 * 1024 * 1024;
    const MAX_COMPRESSION_RATIO: usize = 128;

    if bytes.len() < 4 {
        return 0;
    }
    let footer = &bytes[bytes.len() - 4..];
    let reported = u32::from_le_bytes(
        footer
            .try_into()
            .expect("four-byte gzip footer slice must convert to an array"),
    ) as usize;
    reported
        .min(bytes.len().saturating_mul(MAX_COMPRESSION_RATIO))
        .min(MAX_PREALLOC_BYTES)
}

/// Extracts `<SongScores>` from a parsed `Stats.xml` root (`<Stats>`).
pub fn parse_song_scores(root: &XmlNode) -> Vec<ItgSongScores> {
    parse_song_scores_with::<true, true, true>(root)
}

fn output_vec<T, const RESERVE: bool>(source_len: usize) -> Vec<T> {
    if RESERVE {
        Vec::with_capacity(source_len)
    } else {
        Vec::new()
    }
}

fn parse_song_scores_with<
    const RESERVE_SONGS: bool,
    const RESERVE_STEPS: bool,
    const RESERVE_SCORES: bool,
>(
    root: &XmlNode,
) -> Vec<ItgSongScores> {
    // The root is normally <Stats>, with <SongScores> inside. Be tolerant: if we
    // were handed <SongScores> directly, use it.
    let song_scores = if root.tag == "SongScores" {
        root
    } else {
        match root.child("SongScores") {
            Some(s) => s,
            None => return Vec::new(),
        }
    };

    let mut out = output_vec::<_, RESERVE_SONGS>(song_scores.children.len());
    for song in song_scores.children_named("Song") {
        let dir = song.attr("Dir").unwrap_or("").to_string();
        if dir.is_empty() {
            continue;
        }
        let mut steps_list = output_vec::<_, RESERVE_STEPS>(song.children.len());
        for steps in song.children_named("Steps") {
            let steps_type = steps.attr("StepsType").unwrap_or("").to_string();
            let difficulty = steps.attr("Difficulty").unwrap_or("").to_string();
            let description = steps.attr("Description").unwrap_or("").to_string();
            if steps_type.is_empty() || difficulty.is_empty() {
                continue;
            }
            let Some(list) = steps.child("HighScoreList") else {
                continue;
            };
            let mut high_scores = output_vec::<_, RESERVE_SCORES>(list.children.len());
            for high_score in list.children_named("HighScore") {
                high_scores.push(parse_high_score(high_score));
            }
            if high_scores.is_empty() {
                continue;
            }
            steps_list.push(ItgStepsScores {
                steps_type,
                difficulty,
                description,
                high_scores,
            });
        }
        if !steps_list.is_empty() {
            out.push(ItgSongScores {
                dir,
                steps: steps_list,
            });
        }
    }
    out
}

fn parse_high_score(node: &XmlNode) -> ImportedHighScore {
    let tap = node.child("TapNoteScores");
    let hold = node.child("HoldNoteScores");
    let tap_count = |name: &str| {
        tap.and_then(|scores| scores.child_parse::<u32>(name))
            .unwrap_or(0)
    };
    let hold_count = |name: &str| {
        hold.and_then(|scores| scores.child_parse::<u32>(name))
            .unwrap_or(0)
    };

    ImportedHighScore {
        grade: node.child_text("Grade").to_string(),
        percent_dp: node.child_parse::<f64>("PercentDP").unwrap_or(0.0),
        date_time: node.child_text("DateTime").to_string(),
        w1: tap_count("W1"),
        w2: tap_count("W2"),
        w3: tap_count("W3"),
        w4: tap_count("W4"),
        w5: tap_count("W5"),
        miss: tap_count("Miss"),
        hit_mine: tap_count("HitMine"),
        avoid_mine: tap_count("AvoidMine"),
        held: hold_count("Held"),
        let_go: hold_count("LetGo"),
        missed_hold: hold_count("MissedHold"),
        survive_seconds: node.child_parse::<f32>("SurviveSeconds").unwrap_or(0.0),
        modifiers: node.child_text("Modifiers").to_string(),
    }
}

fn parse_tap_judgments_once(node: &XmlNode, score: &mut ImportedHighScore) {
    const W1: u16 = 1 << 0;
    const W2: u16 = 1 << 1;
    const W3: u16 = 1 << 2;
    const W4: u16 = 1 << 3;
    const W5: u16 = 1 << 4;
    const MISS: u16 = 1 << 5;
    const HIT_MINE: u16 = 1 << 6;
    const AVOID_MINE: u16 = 1 << 7;

    let mut seen = 0u16;
    for child in &node.children {
        let value = || child.text.trim().parse().unwrap_or(0);
        match child.tag.as_str() {
            "W1" if seen & W1 == 0 => {
                seen |= W1;
                score.w1 = value();
            }
            "W2" if seen & W2 == 0 => {
                seen |= W2;
                score.w2 = value();
            }
            "W3" if seen & W3 == 0 => {
                seen |= W3;
                score.w3 = value();
            }
            "W4" if seen & W4 == 0 => {
                seen |= W4;
                score.w4 = value();
            }
            "W5" if seen & W5 == 0 => {
                seen |= W5;
                score.w5 = value();
            }
            "Miss" if seen & MISS == 0 => {
                seen |= MISS;
                score.miss = value();
            }
            "HitMine" if seen & HIT_MINE == 0 => {
                seen |= HIT_MINE;
                score.hit_mine = value();
            }
            "AvoidMine" if seen & AVOID_MINE == 0 => {
                seen |= AVOID_MINE;
                score.avoid_mine = value();
            }
            _ => {}
        }
    }
}

fn parse_hold_judgments_once(node: &XmlNode, score: &mut ImportedHighScore) {
    const HELD: u8 = 1 << 0;
    const LET_GO: u8 = 1 << 1;
    const MISSED_HOLD: u8 = 1 << 2;

    let mut seen = 0u8;
    for child in &node.children {
        let value = || child.text.trim().parse().unwrap_or(0);
        match child.tag.as_str() {
            "Held" if seen & HELD == 0 => {
                seen |= HELD;
                score.held = value();
            }
            "LetGo" if seen & LET_GO == 0 => {
                seen |= LET_GO;
                score.let_go = value();
            }
            "MissedHold" if seen & MISSED_HOLD == 0 => {
                seen |= MISSED_HOLD;
                score.missed_hold = value();
            }
            _ => {}
        }
    }
}

fn parse_song_scores_owned(root: XmlNode) -> Vec<ItgSongScores> {
    parse_song_scores_owned_with::<true, true, true>(root)
}

fn parse_song_scores_owned_with<
    const RESERVE_SONGS: bool,
    const RESERVE_STEPS: bool,
    const RESERVE_SCORES: bool,
>(
    root: XmlNode,
) -> Vec<ItgSongScores> {
    let song_scores = if root.tag == "SongScores" {
        root
    } else {
        match root
            .children
            .into_iter()
            .find(|child| child.tag == "SongScores")
        {
            Some(song_scores) => song_scores,
            None => return Vec::new(),
        }
    };

    let mut out = output_vec::<_, RESERVE_SONGS>(song_scores.children.len());
    for song in song_scores.children {
        if song.tag != "Song" {
            continue;
        }
        let XmlNode {
            attrs, children, ..
        } = song;
        let dir = take_first_attr(attrs, "Dir");
        if dir.is_empty() {
            continue;
        }

        let mut steps_list = output_vec::<_, RESERVE_STEPS>(children.len());
        for steps in children {
            if steps.tag != "Steps" {
                continue;
            }
            let XmlNode {
                attrs, children, ..
            } = steps;
            let (steps_type, difficulty, description) = take_steps_attrs(attrs);
            if steps_type.is_empty() || difficulty.is_empty() {
                continue;
            }
            let Some(list) = children
                .into_iter()
                .find(|child| child.tag == "HighScoreList")
            else {
                continue;
            };
            let mut high_scores = output_vec::<_, RESERVE_SCORES>(list.children.len());
            for high_score in list.children {
                if high_score.tag == "HighScore" {
                    high_scores.push(parse_high_score_owned(high_score));
                }
            }
            if high_scores.is_empty() {
                continue;
            }
            steps_list.push(ItgStepsScores {
                steps_type,
                difficulty,
                description,
                high_scores,
            });
        }
        if !steps_list.is_empty() {
            out.push(ItgSongScores {
                dir,
                steps: steps_list,
            });
        }
    }
    out
}

fn take_first_attr(attrs: Vec<(String, String)>, name: &str) -> String {
    attrs
        .into_iter()
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap_or_default()
}

fn take_steps_attrs(attrs: Vec<(String, String)>) -> (String, String, String) {
    let mut steps_type = None;
    let mut difficulty = None;
    let mut description = None;
    for (key, value) in attrs {
        match key.as_str() {
            "StepsType" if steps_type.is_none() => steps_type = Some(value),
            "Difficulty" if difficulty.is_none() => difficulty = Some(value),
            "Description" if description.is_none() => description = Some(value),
            _ => {}
        }
    }
    (
        steps_type.unwrap_or_default(),
        difficulty.unwrap_or_default(),
        description.unwrap_or_default(),
    )
}

fn parse_high_score_owned(node: XmlNode) -> ImportedHighScore {
    const GRADE: u8 = 1 << 0;
    const PERCENT_DP: u8 = 1 << 1;
    const DATE_TIME: u8 = 1 << 2;
    const SURVIVE_SECONDS: u8 = 1 << 3;
    const MODIFIERS: u8 = 1 << 4;
    const TAP: u8 = 1 << 5;
    const HOLD: u8 = 1 << 6;

    let mut score = ImportedHighScore::default();
    let mut seen = 0u8;
    for child in node.children {
        match child.tag.as_str() {
            "Grade" if seen & GRADE == 0 => {
                seen |= GRADE;
                score.grade = child.text;
            }
            "PercentDP" if seen & PERCENT_DP == 0 => {
                seen |= PERCENT_DP;
                score.percent_dp = child.text.trim().parse().unwrap_or(0.0);
            }
            "DateTime" if seen & DATE_TIME == 0 => {
                seen |= DATE_TIME;
                score.date_time = child.text;
            }
            "SurviveSeconds" if seen & SURVIVE_SECONDS == 0 => {
                seen |= SURVIVE_SECONDS;
                score.survive_seconds = child.text.trim().parse().unwrap_or(0.0);
            }
            "Modifiers" if seen & MODIFIERS == 0 => {
                seen |= MODIFIERS;
                score.modifiers = child.text;
            }
            "TapNoteScores" if seen & TAP == 0 => {
                seen |= TAP;
                parse_tap_judgments_once(&child, &mut score);
            }
            "HoldNoteScores" if seen & HOLD == 0 => {
                seen |= HOLD;
                parse_hold_judgments_once(&child, &mut score);
            }
            _ => {}
        }
    }
    score
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod bench_support {
    use super::*;

    fn checksum(songs: &[ItgSongScores]) -> u64 {
        songs.iter().fold(0u64, |sum, song| {
            song.steps
                .iter()
                .fold(sum.wrapping_add(song.dir.len() as u64), |sum, steps| {
                    steps.high_scores.iter().fold(
                        sum.wrapping_add(steps.steps_type.len() as u64)
                            .wrapping_add((steps.difficulty.len() as u64) << 8)
                            .wrapping_add((steps.description.len() as u64) << 16),
                        |sum, score| {
                            sum.wrapping_add(score.grade.len() as u64)
                                .wrapping_add(score.percent_dp.to_bits())
                                .wrapping_add((score.date_time.len() as u64) << 24)
                                .wrapping_add(u64::from(score.w1))
                                .wrapping_add(u64::from(score.w2) << 4)
                                .wrapping_add(u64::from(score.w3) << 8)
                                .wrapping_add(u64::from(score.w4) << 12)
                                .wrapping_add(u64::from(score.w5) << 16)
                                .wrapping_add(u64::from(score.miss) << 20)
                                .wrapping_add(u64::from(score.hit_mine) << 24)
                                .wrapping_add(u64::from(score.avoid_mine) << 28)
                                .wrapping_add(u64::from(score.held) << 32)
                                .wrapping_add(u64::from(score.let_go) << 36)
                                .wrapping_add(u64::from(score.missed_hold) << 40)
                                .wrapping_add(u64::from(score.survive_seconds.to_bits()))
                                .wrapping_add((score.modifiers.len() as u64) << 48)
                        },
                    )
                })
        })
    }

    pub fn borrowed_from_owned(root: XmlNode) -> u64 {
        checksum(&parse_song_scores(&root))
    }

    pub fn consumed(root: XmlNode) -> u64 {
        checksum(&parse_song_scores_owned(root))
    }

    pub fn score_capacity_none(root: &XmlNode) -> u64 {
        checksum(&parse_song_scores_with::<false, false, false>(root))
    }

    pub fn score_capacity_songs(root: &XmlNode) -> u64 {
        checksum(&parse_song_scores_with::<true, false, false>(root))
    }

    pub fn score_capacity_steps(root: &XmlNode) -> u64 {
        checksum(&parse_song_scores_with::<true, true, false>(root))
    }

    pub fn score_capacity_all(root: &XmlNode) -> u64 {
        checksum(&parse_song_scores_with::<true, true, true>(root))
    }

    fn gzip_checksum(text: &str) -> u64 {
        text.as_bytes()
            .iter()
            .take(32)
            .chain(text.as_bytes().iter().rev().take(32))
            .fold(text.len() as u64, |sum, byte| {
                sum.rotate_left(5).wrapping_add(u64::from(*byte))
            })
    }

    fn gzip_stage<const RESERVE_OUTPUT: bool, const REUSE_UTF8: bool>(bytes: &[u8]) -> u64 {
        let text = decode_gz_to_string_with::<RESERVE_OUTPUT, REUSE_UTF8>(bytes)
            .expect("benchmark gzip fixture must decode");
        gzip_checksum(&text)
    }

    pub fn gzip_unreserved_copy(bytes: &[u8]) -> u64 {
        gzip_stage::<false, false>(bytes)
    }

    pub fn gzip_reserved_copy(bytes: &[u8]) -> u64 {
        gzip_stage::<true, false>(bytes)
    }

    pub fn gzip_reserved_reuse(bytes: &[u8]) -> u64 {
        gzip_stage::<true, true>(bytes)
    }

    fn favorite_checksum(favorites: &[String]) -> u64 {
        favorites.iter().fold(0u64, |checksum, favorite| {
            favorite.bytes().fold(
                checksum.rotate_left(7).wrapping_add(favorite.len() as u64),
                |checksum, byte| checksum.rotate_left(5).wrapping_add(u64::from(byte)),
            )
        })
    }

    pub fn favorites_unreserved(text: &str) -> u64 {
        favorite_checksum(&parse_favorites_owned_with::<
            std::collections::hash_map::RandomState,
            false,
        >(text))
    }

    pub fn favorites_reserved(text: &str) -> u64 {
        favorite_checksum(&parse_favorites_owned_with::<
            std::collections::hash_map::RandomState,
            true,
        >(text))
    }

    pub fn favorites_fast_hash(text: &str) -> u64 {
        favorite_checksum(&parse_favorites_owned_with::<FxBuildHasher, true>(text))
    }

    pub fn favorites_borrowed(text: &str) -> u64 {
        favorite_checksum(&parse_favorites_borrowed(text))
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "1" | "true" | "True" | "TRUE")
}

/// Looks up `name` inside `dir`, matching the file name case-insensitively
/// (ITGmania/Windows are case-insensitive; `DeadSync` may run on case-sensitive
/// filesystems). Returns the first matching path.
fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let entries = fs::read_dir(dir).ok()?;
    let lower = name.to_ascii_lowercase();
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(&lower) || n.to_ascii_lowercase() == lower)
        {
            let path = entry.path();
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STATS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Stats>
  <SongScores>
    <Song Dir="Songs/My Pack/Cool Song/">
      <Steps StepsType="dance-single" Difficulty="Hard">
        <HighScoreList>
          <NumTimesPlayed>2</NumTimesPlayed>
          <HighScore>
            <Grade>Tier01</Grade>
            <PercentDP>0.991200</PercentDP>
            <SurviveSeconds>0</SurviveSeconds>
            <DateTime>2023-04-15 21:07:33</DateTime>
            <TapNoteScores>
              <HitMine>1</HitMine>
              <AvoidMine>4</AvoidMine>
              <Miss>0</Miss>
              <W5>0</W5><W4>0</W4><W3>0</W3><W2>12</W2><W1>480</W1>
            </TapNoteScores>
            <HoldNoteScores>
              <LetGo>0</LetGo><Held>20</Held><MissedHold>0</MissedHold>
            </HoldNoteScores>
          </HighScore>
          <HighScore>
            <Grade>Failed</Grade>
            <PercentDP>0.4231</PercentDP>
            <SurviveSeconds>51.5</SurviveSeconds>
            <DateTime>2022-01-02 03:04:05</DateTime>
            <TapNoteScores><Miss>40</Miss><W1>100</W1></TapNoteScores>
            <HoldNoteScores><Held>0</Held></HoldNoteScores>
          </HighScore>
        </HighScoreList>
      </Steps>
      <Steps StepsType="dance-single" Difficulty="Edit" Description="My Edit">
        <HighScoreList>
          <HighScore>
            <Grade>Tier03</Grade>
            <PercentDP>0.95</PercentDP>
            <DateTime>2023-05-01 10:00:00</DateTime>
            <TapNoteScores><W1>200</W1><W3>10</W3></TapNoteScores>
          </HighScore>
        </HighScoreList>
      </Steps>
    </Song>
  </SongScores>
</Stats>"#;

    #[test]
    fn parses_song_scores_tree() {
        let root = xml::parse(SAMPLE_STATS).expect("xml");
        let songs = parse_song_scores(&root);
        assert_eq!(songs.len(), 1);
        let song = &songs[0];
        assert_eq!(song.dir, "Songs/My Pack/Cool Song/");
        assert_eq!(song.steps.len(), 2);

        let hard = &song.steps[0];
        assert_eq!(hard.steps_type, "dance-single");
        assert_eq!(hard.difficulty, "Hard");
        assert_eq!(hard.high_scores.len(), 2);

        let first = &hard.high_scores[0];
        assert_eq!(first.grade, "Tier01");
        assert!((first.percent_dp - 0.9912).abs() < 1e-9);
        assert_eq!(first.w1, 480);
        assert_eq!(first.w2, 12);
        assert_eq!(first.miss, 0);
        assert_eq!(first.hit_mine, 1);
        assert_eq!(first.avoid_mine, 4);
        assert_eq!(first.held, 20);

        let failed = &hard.high_scores[1];
        assert_eq!(failed.grade, "Failed");
        assert_eq!(failed.miss, 40);
        assert!((failed.survive_seconds - 51.5).abs() < 1e-6);

        let edit = &song.steps[1];
        assert_eq!(edit.difficulty, "Edit");
        assert_eq!(edit.description, "My Edit");
    }

    #[test]
    fn optimized_song_score_extraction_matches_reference_semantics() {
        let root = xml::parse(SAMPLE_STATS).expect("xml");
        assert_song_scores_eq(
            &parse_song_scores(&root),
            &parse_song_scores_owned(root.clone()),
        );

        let duplicate_fields = xml::parse(
            r#"<SongScores>
                <Song Dir="Songs/Pack/Song/">
                    <Steps StepsType="dance-single" Difficulty="Hard">
                        <HighScoreList>
                            <HighScore>
                                <Grade>Tier02</Grade><Grade>Failed</Grade>
                                <PercentDP>0.98</PercentDP><PercentDP>invalid</PercentDP>
                                <DateTime>2025-01-02 03:04:05</DateTime>
                                <TapNoteScores><W1>321</W1><W1>999</W1><Miss>4</Miss></TapNoteScores>
                                <HoldNoteScores><Held>12</Held><Held>99</Held><LetGo>2</LetGo></HoldNoteScores>
                                <SurviveSeconds>45.5</SurviveSeconds>
                                <Modifiers>1.2xMusic</Modifiers>
                            </HighScore>
                        </HighScoreList>
                    </Steps>
                </Song>
            </SongScores>"#,
        )
        .expect("xml");
        let reference = parse_song_scores(&duplicate_fields);
        assert_song_scores_eq(
            &reference,
            &parse_song_scores_owned(duplicate_fields.clone()),
        );
        let score = &reference[0].steps[0].high_scores[0];
        assert_eq!(score.grade, "Tier02");
        assert_eq!(score.w1, 321);
        assert_eq!(score.held, 12);
    }

    #[test]
    fn score_capacity_stages_preserve_borrowed_and_owned_results() {
        let root = xml::parse(SAMPLE_STATS).expect("xml");
        let borrowed_reference = parse_song_scores_with::<false, false, false>(&root);
        assert_song_scores_eq(
            &borrowed_reference,
            &parse_song_scores_with::<true, false, false>(&root),
        );
        assert_song_scores_eq(
            &borrowed_reference,
            &parse_song_scores_with::<true, true, false>(&root),
        );
        assert_song_scores_eq(
            &borrowed_reference,
            &parse_song_scores_with::<true, true, true>(&root),
        );

        let owned_reference = parse_song_scores_owned_with::<false, false, false>(root.clone());
        assert_song_scores_eq(
            &owned_reference,
            &parse_song_scores_owned_with::<true, false, false>(root.clone()),
        );
        assert_song_scores_eq(
            &owned_reference,
            &parse_song_scores_owned_with::<true, true, false>(root.clone()),
        );
        assert_song_scores_eq(
            &owned_reference,
            &parse_song_scores_owned_with::<true, true, true>(root),
        );
    }

    #[test]
    fn optimized_gzip_decode_matches_reference_for_valid_and_invalid_utf8() {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        use std::io::Write as _;

        for bytes in [
            &b"<Stats><Guid>valid UTF-8</Guid></Stats>"[..],
            &b"<Stats><Guid>invalid \xF6 byte</Guid></Stats>"[..],
        ] {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(bytes).expect("compress fixture");
            let compressed = encoder.finish().expect("finish fixture");
            let reference =
                decode_gz_to_string_with::<false, false>(&compressed).expect("reference decode");
            assert_eq!(
                decode_gz_to_string_with::<true, false>(&compressed).expect("reserved decode"),
                reference
            );
            assert_eq!(
                decode_gz_to_string_with::<true, true>(&compressed).expect("reused decode"),
                reference
            );
            assert_eq!(gzip_output_capacity(&compressed), bytes.len());
        }
        assert_eq!(gzip_output_capacity(&[1, 2, 3]), 0);
    }

    fn assert_song_scores_eq(expected: &[ItgSongScores], actual: &[ItgSongScores]) {
        assert_eq!(actual.len(), expected.len());
        for (actual_song, expected_song) in actual.iter().zip(expected) {
            assert_eq!(actual_song.dir, expected_song.dir);
            assert_eq!(actual_song.steps.len(), expected_song.steps.len());
            for (actual_steps, expected_steps) in actual_song.steps.iter().zip(&expected_song.steps)
            {
                assert_eq!(actual_steps.steps_type, expected_steps.steps_type);
                assert_eq!(actual_steps.difficulty, expected_steps.difficulty);
                assert_eq!(actual_steps.description, expected_steps.description);
                assert_eq!(actual_steps.high_scores, expected_steps.high_scores);
            }
        }
    }

    #[test]
    fn maps_through_to_local_entries() {
        let root = xml::parse(SAMPLE_STATS).expect("xml");
        let songs = parse_song_scores(&root);
        let hard = &songs[0].steps[0];
        let entry = deadsync_score::local_score_from_itg(&hard.high_scores[0]).expect("entry");
        assert_eq!(entry.judgment_counts, [480, 12, 0, 0, 0, 0]);
        assert_eq!(entry.holds_total, 20);
        assert_eq!(entry.mines_avoided, 4);
    }

    #[test]
    fn parses_favorites_skipping_headers_and_dupes() {
        let text = "---My Stamina Playlist\nPack A/Song One\n\nPack B/Song Two\n---Another Section\npack a/SONG ONE\n  Pack C/Song Three  \nPäck/Über\nPÄCK/ÜBER\n";
        let favs = parse_favorites_text(text);
        assert_eq!(
            favs,
            vec![
                "Pack A/Song One".to_string(),
                "Pack B/Song Two".to_string(),
                "Pack C/Song Three".to_string(),
                "Päck/Über".to_string(),
                "PÄCK/ÜBER".to_string(),
            ]
        );
        assert_eq!(
            favs,
            parse_favorites_owned_with::<std::collections::hash_map::RandomState, false>(text)
        );
    }

    #[test]
    fn parses_general_data_current_combo() {
        let xml_text = r#"<Stats>
  <GeneralData>
    <DisplayName>Test</DisplayName>
    <CurrentCombo>137</CurrentCombo>
    <Guid>99f55b745304ebcf</Guid>
  </GeneralData>
  <SongScores></SongScores>
</Stats>"#;
        let root = xml::parse(xml_text).expect("xml");
        assert_eq!(
            parse_general_data(&root),
            (137, "99f55b745304ebcf".to_string())
        );

        // Absent GeneralData / CurrentCombo / Guid → (0, "").
        let root2 = xml::parse(SAMPLE_STATS).expect("xml");
        assert_eq!(parse_general_data(&root2), (0, String::new()));
    }

    #[test]
    fn extracts_guid_from_stats_head() {
        let head = r#"<Stats><GeneralData>
            <DisplayName>adstep</DisplayName>
            <Guid>99f55b745304ebcf</Guid>
            <CurrentCombo>3</CurrentCombo>
        </GeneralData>"#;
        assert_eq!(extract_guid(head).as_deref(), Some("99f55b745304ebcf"));
        // Missing or empty Guid → None.
        assert_eq!(extract_guid("<GeneralData></GeneralData>"), None);
        assert_eq!(extract_guid("<Guid></Guid>"), None);
        assert_eq!(extract_guid("<Guid>   </Guid>"), None);
    }

    #[test]
    fn read_stats_tolerates_non_utf8_bytes() {
        // ITGmania declares UTF-8 but can write raw filesystem bytes into a song
        // `Dir` — e.g. a Latin-1 `0xF6` ("ö") in "Helt Seriöst".
        let dir = std::env::temp_dir().join(format!(
            "deadsync-itg-nonutf8-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("temp dir");

        let mut bytes = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Stats>\n<SongScores>\n<Song Dir='Songs/Easy As Pie 6/Helt Seri".to_vec();
        bytes.push(0xF6); // raw Latin-1 'ö', invalid as UTF-8
        bytes.extend_from_slice(
            b"st/'>\n<Steps StepsType='dance-single' Difficulty='Hard'>\n\
              <HighScoreList>\n<HighScore>\n<Grade>Tier01</Grade>\n\
              <PercentDP>0.99</PercentDP>\n<DateTime>2024-01-01 00:00:00</DateTime>\n\
              <TapNoteScores><W1>100</W1></TapNoteScores>\n</HighScore>\n\
              </HighScoreList>\n</Steps>\n</Song>\n</SongScores>\n</Stats>\n",
        );
        fs::write(dir.join("Stats.xml"), &bytes).expect("write Stats.xml");

        let stats = read_stats(&dir).expect("read_stats must not fail on invalid UTF-8");
        assert_eq!(stats.songs.len(), 1);
        // The invalid byte is replaced (U+FFFD) rather than aborting the import.
        assert!(
            stats.songs[0]
                .dir
                .starts_with("Songs/Easy As Pie 6/Helt Seri")
        );
        assert_eq!(stats.songs[0].steps[0].high_scores.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
