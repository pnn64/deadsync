//! Readers for an ITGmania `LocalProfiles/<id>/` directory: the editable profile
//! metadata, online keys, avatar, and the `Stats.xml` high-score database.
//!
//! Nothing here touches DeadSync state — these functions only turn files on disk
//! into plain Rust structs. Mapping into DeadSync types happens in the
//! orchestration layer (`super::run`) and in `deadsync_score::import`.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use deadsync_score::ImportedHighScore;

use super::xml::{self, XmlNode};
use crate::config::SimpleIni;

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

/// GrooveStats + ArrowCloud online keys.
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

/// Everything we managed to read from one ITGmania local profile directory.
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
    /// `Stats.xml` `GeneralData/Guid` — ITGmania's stable per-profile identifier.
    /// Used to derive the imported DeadSync profile's GUID so re-importing the
    /// same profile yields the same identity. Empty when the `Stats.xml` is
    /// missing or has no `Guid`.
    pub guid: String,
}

impl ItgSource {
    /// Total number of high-score records across all songs/steps.
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
    /// The directory doesn't look like an ITGmania profile (no `Editable.ini`).
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

/// Returns `true` if `dir` looks like an ITGmania local profile directory.
pub fn is_itg_profile_dir(dir: &Path) -> bool {
    find_case_insensitive(dir, "Editable.ini").is_some()
}

/// Cheaply reads just the `DisplayName` from a profile's `Editable.ini`,
/// without parsing the (potentially large) `Stats.xml`. Used to label profiles
/// in the import picker. Returns `None` when the file is missing or the name is
/// blank.
pub fn read_display_name(dir: &Path) -> Option<String> {
    let path = find_case_insensitive(dir, "Editable.ini")?;
    let name = read_editable(&path).display_name;
    if name.trim().is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Cheaply reads the ITGmania profile `Guid` from `Stats.xml` (or `Stats.xml.gz`)
/// without a full XML parse. The `Guid` lives in `GeneralData` at the very top of
/// the file, so we only scan the head (bounded) instead of the whole — possibly
/// many-megabyte — score database. Used by the import picker to flag profiles
/// that have already been imported. Returns `None` when absent or unreadable.
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

/// Reads an entire ITGmania local profile directory into an [`ItgSource`].
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
                .map(|s| parse_bool(&s))
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
    if let Some(path) = find_case_insensitive(dir, "Simply Love UserPrefs.ini") {
        if ini.load(&path).is_ok() {
            if let Some(section) = ini.get_section("Simply Love") {
                return section.clone();
            }
        }
    }
    HashMap::new()
}

/// Parses Simply Love `favorites.txt` content into a list of `Pack/SongFolder`
/// song keys. Section header lines (which begin with `---`, e.g.
/// `---My Stamina Playlist`) and blank lines are skipped; remaining lines are
/// the favorited song paths. Order is preserved and duplicates are removed.
pub fn parse_favorites_text(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
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
/// directory, if present. The contents are parsed downstream by DeadSync's ITL
/// module, which uses the same schema. Returns `None` when the file is missing
/// or unreadable.
fn read_itl_json(dir: &Path) -> Option<String> {
    let path = find_case_insensitive(dir, "ITL2026.json")?;
    fs::read_to_string(&path).ok()
}

/// Finds an avatar image in the profile dir. ITGmania uses `Avatar.png`; some
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
        fs::read_to_string(&path)?
    } else if let Some(path) = find_case_insensitive(dir, "Stats.xml.gz") {
        read_gz_to_string(&path)?
    } else {
        return Ok(ItgStatsData::default());
    };

    let root = xml::parse(&content).map_err(ItgReadError::Xml)?;
    let (current_combo, guid) = parse_general_data(&root);
    Ok(ItgStatsData {
        songs: parse_song_scores(&root),
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
    let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
    let mut out = String::new();
    decoder.read_to_string(&mut out)?;
    Ok(out)
}

/// Extracts `<SongScores>` from a parsed `Stats.xml` root (`<Stats>`).
pub fn parse_song_scores(root: &XmlNode) -> Vec<ItgSongScores> {
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

    let mut out = Vec::new();
    for song in song_scores.children_named("Song") {
        let dir = song.attr("Dir").unwrap_or("").to_string();
        if dir.is_empty() {
            continue;
        }
        let mut steps_list = Vec::new();
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
            let high_scores: Vec<ImportedHighScore> = list
                .children_named("HighScore")
                .map(parse_high_score)
                .collect();
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
    let tap_count = |name: &str| tap.and_then(|t| t.child_parse::<u32>(name)).unwrap_or(0);
    let hold_count = |name: &str| hold.and_then(|h| h.child_parse::<u32>(name)).unwrap_or(0);

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

fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "1" | "true" | "True" | "TRUE")
}

/// Looks up `name` inside `dir`, matching the file name case-insensitively
/// (ITGmania/Windows are case-insensitive; DeadSync may run on case-sensitive
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
        let text = "---My Stamina Playlist\nPack A/Song One\n\nPack B/Song Two\n---Another Section\nPack A/Song One\n  Pack C/Song Three  \n";
        let favs = parse_favorites_text(text);
        assert_eq!(
            favs,
            vec![
                "Pack A/Song One".to_string(),
                "Pack B/Song Two".to_string(),
                "Pack C/Song Three".to_string(),
            ]
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
}
