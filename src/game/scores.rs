use crate::core::network;
use crate::game::profile::Profile;
use crate::game::song::get_song_cache;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bincode::{Decode, Encode};

const API_URL: &str = "https://api.groovestats.com/player-leaderboards.php";
const GS_SCORES_DIR: &str = "save/profiles/00000000/scores/gs";

// --- Grade Definitions ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)] // Quint will be used eventually for W0 tracking
pub enum Grade {
    Quint, Tier01, Tier02, Tier03, Tier04, Tier05, Tier06, Tier07, Tier08,
    Tier09, Tier10, Tier11, Tier12, Tier13, Tier14, Tier15, Tier16, Tier17, Failed,
}

impl Grade {
    /// Converts a grade to the corresponding frame index on the "grades 1x19.png" spritesheet.
    pub fn to_sprite_state(&self) -> u32 {
        match self {
            Grade::Quint => 0,
            Grade::Tier01 => 1, Grade::Tier02 => 2, Grade::Tier03 => 3, Grade::Tier04 => 4,
            Grade::Tier05 => 5, Grade::Tier06 => 6, Grade::Tier07 => 7, Grade::Tier08 => 8,
            Grade::Tier09 => 9, Grade::Tier10 => 10, Grade::Tier11 => 11, Grade::Tier12 => 12,
            Grade::Tier13 => 13, Grade::Tier14 => 14, Grade::Tier15 => 15, Grade::Tier16 => 16,
            Grade::Tier17 => 17, Grade::Failed => 18,
        }
    }
}

/// A struct to hold both the calculated grade and the precise score percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CachedScore {
    pub grade: Grade,
    pub score_percent: f64, // Stored as 0.0 to 1.0
    /// Optional lamp index for UI (e.g., Select Music wheel).
    /// This is intentionally UI-agnostic: the meaning of the index is left
    /// to the presentation layer (colors, effects, etc.).
    pub lamp_index: Option<u8>,
}

// --- Global Grade Cache ---

static GRADE_CACHE: Lazy<Mutex<HashMap<String, CachedScore>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn get_cached_score(chart_hash: &str) -> Option<CachedScore> {
    if let Some(score) = GRADE_CACHE.lock().unwrap().get(chart_hash).copied() {
        return Some(score);
    }
    if let Some(from_disk) = load_best_score_from_disk(chart_hash) {
        GRADE_CACHE
            .lock()
            .unwrap()
            .insert(chart_hash.to_string(), from_disk);
        return Some(from_disk);
    }
    None
}

pub fn set_cached_score(chart_hash: String, score: CachedScore) {
    info!("Caching score {:?} for chart hash {}", score, chart_hash);
    GRADE_CACHE.lock().unwrap().insert(chart_hash, score);
}

// --- On-disk GrooveStats score storage ---

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntry {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

fn grade_to_code(g: Grade) -> u8 {
    match g {
        Grade::Quint => 0,
        Grade::Tier01 => 1,
        Grade::Tier02 => 2,
        Grade::Tier03 => 3,
        Grade::Tier04 => 4,
        Grade::Tier05 => 5,
        Grade::Tier06 => 6,
        Grade::Tier07 => 7,
        Grade::Tier08 => 8,
        Grade::Tier09 => 9,
        Grade::Tier10 => 10,
        Grade::Tier11 => 11,
        Grade::Tier12 => 12,
        Grade::Tier13 => 13,
        Grade::Tier14 => 14,
        Grade::Tier15 => 15,
        Grade::Tier16 => 16,
        Grade::Tier17 => 17,
        Grade::Failed => 18,
    }
}

fn grade_from_code(code: u8) -> Grade {
    match code {
        0 => Grade::Quint,
        1 => Grade::Tier01,
        2 => Grade::Tier02,
        3 => Grade::Tier03,
        4 => Grade::Tier04,
        5 => Grade::Tier05,
        6 => Grade::Tier06,
        7 => Grade::Tier07,
        8 => Grade::Tier08,
        9 => Grade::Tier09,
        10 => Grade::Tier10,
        11 => Grade::Tier11,
        12 => Grade::Tier12,
        13 => Grade::Tier13,
        14 => Grade::Tier14,
        15 => Grade::Tier15,
        16 => Grade::Tier16,
        17 => Grade::Tier17,
        _ => Grade::Failed,
    }
}

fn gs_scores_dir() -> PathBuf {
    PathBuf::from(GS_SCORES_DIR)
}

fn entry_from_cached(score: CachedScore, username: &str, fetched_at_ms: i64) -> GsScoreEntry {
    GsScoreEntry {
        score_percent: score.score_percent,
        grade_code: grade_to_code(score.grade),
        lamp_index: score.lamp_index,
        username: username.to_string(),
        fetched_at_ms,
    }
}

fn cached_from_entry(entry: &GsScoreEntry) -> CachedScore {
    CachedScore {
        grade: grade_from_code(entry.grade_code),
        score_percent: entry.score_percent,
        lamp_index: entry.lamp_index,
    }
}

fn load_all_entries_for_chart(chart_hash: &str) -> Vec<GsScoreEntry> {
    let dir = gs_scores_dir();
    if !dir.is_dir() {
        return Vec::new();
    }
    let prefix = format!("{}-", chart_hash);
    let Ok(read_dir) = fs::read_dir(&dir) else { return Vec::new(); };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".bin") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue; };
        if let Ok((entry, _)) = bincode::decode_from_slice::<GsScoreEntry, _>(&bytes, bincode::config::standard()) {
            entries.push(entry);
        }
    }
    entries
}

fn load_best_score_from_disk(chart_hash: &str) -> Option<CachedScore> {
    let entries = load_all_entries_for_chart(chart_hash);
    if entries.is_empty() {
        return None;
    }
    let mut best: Option<&GsScoreEntry> = None;
    for entry in &entries {
        if let Some(current) = best {
            if entry.score_percent > current.score_percent {
                best = Some(entry);
            }
        } else {
            best = Some(entry);
        }
    }
    best.map(cached_from_entry)
}

fn append_gs_score_on_disk(chart_hash: &str, score: CachedScore, username: &str) {
    if username.trim().is_empty() {
        return;
    }
    let mut entries = load_all_entries_for_chart(chart_hash);
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let new_entry = entry_from_cached(score, username, fetched_at_ms);

    let epsilon = 1e-9_f64;
    for existing in &entries {
        if existing.username.eq_ignore_ascii_case(username)
            && (existing.score_percent - new_entry.score_percent).abs() <= epsilon
            && existing.lamp_index == new_entry.lamp_index
            && existing.grade_code == new_entry.grade_code
        {
            return;
        }
    }

    entries.push(new_entry.clone());

    let dir = gs_scores_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create GrooveStats scores dir {:?}: {}", dir, e);
        return;
    }

    let file_name = format!("{}-{}.bin", chart_hash, fetched_at_ms);
    let mut path = dir;
    path.push(file_name);

    match bincode::encode_to_vec(&new_entry, bincode::config::standard()) {
        Ok(buf) => {
            if let Err(e) = fs::write(&path, buf) {
                warn!("Failed to write GrooveStats score file {:?}: {}", path, e);
            } else {
                info!(
                    "Stored GrooveStats score on disk for chart {} at {:?}",
                    chart_hash, path
                );
            }
        }
        Err(e) => {
            warn!(
                "Failed to encode GrooveStats score for chart {}: {}",
                chart_hash, e
            );
        }
    }
}

// --- API Response Structs ---

#[derive(Deserialize, Debug)]
struct ApiResponse {
    player1: Option<Player1>,
}

#[derive(Deserialize, Debug)]
struct Player1 {
    #[serde(rename = "gsLeaderboard")]
    gs_leaderboard: Option<Vec<GrooveScore>>,
}

#[derive(Deserialize, Debug)]
struct GrooveScore {
    name: String,
    score: f64, // 0..10000
    /// Optional human-readable comment string (e.g., "189w, 33e, 2g, 1d, 3m, C690").
    /// This is generated by Simply Love as part of GrooveStats score submission
    /// and exposed via the `comments` field in GrooveStats' JSON.
    #[serde(default)]
    comments: Option<String>,
}

// --- ITG PercentScore weights (mirror Simply Love SL_Init.lua, ITG mode) ---
const DP_W1: i32 = 5;
const DP_W2: i32 = 4;
const DP_W3: i32 = 2;
const DP_W4: i32 = 0;
const DP_W5: i32 = -6;
const DP_MISS: i32 = -12;
const DP_HELD: i32 = 5;

#[derive(Debug, Default, Clone, Copy)]
struct ParsedCommentCounts {
    w: u32,
    e: u32,
    g: u32,
    d: u32,
    wo: u32,
    m: u32,
}

fn parse_comment_counts(comment: &str) -> ParsedCommentCounts {
    let mut counts = ParsedCommentCounts::default();
    for part in comment.split(',') {
        let s = part.trim();
        if s.is_empty() { continue; }

        let mut value: u32 = 0;
        let mut idx = 0usize;
        for (i, ch) in s.char_indices() {
            if let Some(d) = ch.to_digit(10) {
                value = value.saturating_mul(10).saturating_add(d);
                idx = i + ch.len_utf8();
            } else {
                break;
            }
        }
        if value == 0 { continue; }

        let suffix = s[idx..].trim().to_ascii_lowercase();
        match suffix.as_str() {
            "w"  => counts.w  = value,
            "e"  => counts.e  = value,
            "g"  => counts.g  = value,
            "d"  => counts.d  = value,
            "wo" => counts.wo = value,
            "m"  => counts.m  = value,
            _ => {}
        }
    }
    counts
}

fn find_chart_stats_for_hash(chart_hash: &str) -> Option<rssp::stats::ArrowStats> {
    let cache = get_song_cache();
    for pack in cache.iter() {
        for song in &pack.songs {
            for chart in &song.charts {
                if chart.short_hash == chart_hash {
                    return Some(chart.stats.clone());
                }
            }
        }
    }
    None
}

fn compute_lamp_index(score: f64, comment: Option<&str>, chart_hash: &str) -> Option<u8> {
    let score_percent = score / 10000.0;

    // Perfect 100%: always at least a W1 full combo lamp.
    // Use a very small epsilon so only true 100.00% (score == 10000) hits this,
    // not 99.95% (score == 9995) or similar edge cases.
    if (score_percent - 1.0).abs() <= 1e-9 {
        info!(
            "GrooveStats lamp: hash={} score={:.4}% -> Quad lamp (W1 FC, no DP check needed)",
            chart_hash,
            score_percent * 100.0
        );
        return Some(1);
    }

    let comment = match comment {
        Some(c) => c,
        None => {
            info!(
                "GrooveStats lamp: hash={} score={:.4}% -> no lamp (no GrooveStats comment available)",
                chart_hash,
                score_percent * 100.0
            );
            return None;
        }
    };
    let counts = parse_comment_counts(comment);

    // Any explicit Miss or Way Off disqualifies lamps immediately.
    if counts.m > 0 || counts.wo > 0 {
        return None;
    }

    let stats = match find_chart_stats_for_hash(chart_hash) {
        Some(s) => s,
        None => {
            info!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (chart stats not found for hash)",
                chart_hash,
                score_percent * 100.0,
                comment
            );
            return None;
        }
    };
    let taps_rows = stats.total_steps as i32;
    let holds = stats.holds as i32;
    let rolls = stats.rolls as i32;

    if taps_rows <= 0 {
        info!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (taps_rows <= 0, taps_rows={})",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows
        );
        return None;
    }

    // Reconstruct W1 count as "everything not explicitly listed".
    let non_w1_from_suffixes =
        counts.e + counts.g + counts.d + counts.wo + counts.m + counts.w;
    let inferred_w1 = if (non_w1_from_suffixes as i32) > taps_rows {
        0
    } else {
        (taps_rows as u32).saturating_sub(
            counts.e + counts.g + counts.d + counts.wo + counts.m,
        )
    };
    let w1_total = counts.w.max(inferred_w1);

    // Dance Points from tap judgments (rows) only, per ITG PercentScoreWeight.
    let dp_taps: i32 =
        (w1_total as i32) * DP_W1 +
        (counts.e as i32) * DP_W2 +
        (counts.g as i32) * DP_W3 +
        (counts.d as i32) * DP_W4 +
        (counts.wo as i32) * DP_W5 +
        (counts.m as i32) * DP_MISS;

    // Holds + rolls assumed fully held for the "no hidden errors" hypothesis.
    let dp_hold_roll: i32 = (holds + rolls) * DP_HELD;

    // Maximum possible DP if every tap was W1 and all holds/rolls fully held.
    let dp_possible_max: i32 = (taps_rows * DP_W1 + dp_hold_roll).max(1);
    let dp_expect_no_hidden_errors: i32 = dp_taps + dp_hold_roll;

    let dp_expect_frac = dp_expect_no_hidden_errors as f64 / dp_possible_max as f64;
    let dp_diff = (score_percent - dp_expect_frac).abs();
    let dp_consistent = dp_diff <= 0.0005;

    if !dp_consistent {
        // There must have been extra DP loss (e.g., dropped holds or hit mines).
        info!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP mismatch: score%={:.5} vs no-hidden-errors%={:.5} (Δ={:.6}); \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> no lamp",
            chart_hash,
            score_percent * 100.0,
            comment,
            score_percent * 100.0,
            dp_expect_frac * 100.0,
            dp_diff * 100.0,
            taps_rows,
            holds,
            rolls,
            counts.w,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return None;
    }

    // At this point, we know there were no hidden hold/mine mistakes.
    // Classify the lamp tier, mirroring Simply Love's StageAward semantics.
    if counts.g == 0 && counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // Only W1/W2 present (and W1 reconstructed) => W2 full combo (FEC).
        if counts.e > 0 || w1_total > 0 {
            info!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=FEC (index=2)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(2);
        }
    }

    if counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // At least one Great, but no Decents/WayOff/Miss => W3 full combo.
        if counts.g > 0 {
            info!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W3 FC (index=3)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(3);
        }
    }

    // No WayOff/Miss and DP-consistent => at worst a W4 full combo.
    if counts.wo == 0 && counts.m == 0 {
        info!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W4 FC (index=4)",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows,
            holds,
            rolls,
            w1_total,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return Some(4);
    }

    None
}

// --- Grade Calculation ---

pub fn score_to_grade(score: f64) -> Grade {
    let percent = score / 10000.0;
    if percent >= 1.00 { Grade::Tier01 }    // Note: We don't have enough info to detect Quints (W0) yet.
    else if percent >= 0.99 { Grade::Tier02 } // three-stars
    else if percent >= 0.98 { Grade::Tier03 } // two-stars
    else if percent >= 0.96 { Grade::Tier04 } // one-star
    else if percent >= 0.94 { Grade::Tier05 } // s-plus
    else if percent >= 0.92 { Grade::Tier06 } // s
    else if percent >= 0.89 { Grade::Tier07 } // s-minus
    else if percent >= 0.86 { Grade::Tier08 } // a-plus
    else if percent >= 0.83 { Grade::Tier09 } // a
    else if percent >= 0.80 { Grade::Tier10 } // a-minus
    else if percent >= 0.76 { Grade::Tier11 } // b-plus
    else if percent >= 0.72 { Grade::Tier12 } // b
    else if percent >= 0.68 { Grade::Tier13 } // b-minus
    else if percent >= 0.64 { Grade::Tier14 } // c-plus
    else if percent >= 0.60 { Grade::Tier15 } // c
    else if percent >= 0.55 { Grade::Tier16 } // c-minus
    else { Grade::Tier17 } // d
    // Grade::Failed is not score-based; it's determined by gameplay failure (e.g., lifebar empty),
    // which is not yet implemented. This function will never return Grade::Failed.
}

// --- Public Fetch Function ---

pub fn fetch_and_store_grade(profile: Profile, chart_hash: String) -> Result<(), Box<dyn Error + Send + Sync>> {
    if profile.groovestats_api_key.is_empty() || profile.groovestats_username.is_empty() {
        return Err("GrooveStats API key or username is not set in profile.ini.".into());
    }

    info!(
        "Requesting scores for '{}' on chart '{}'...",
        profile.groovestats_username, chart_hash
    );

    let agent = network::get_agent();
    let response = agent
        .get(API_URL)
        .header("x-api-key-player-1", &profile.groovestats_api_key)
        .query("chartHashP1", &chart_hash)
        .call()?;

    if response.status() != 200 {
        return Err(format!("API returned status {}", response.status()).into());
    }

    let api_response: ApiResponse = response.into_body().read_json()?;

    let player_score = api_response
        .player1
        .and_then(|p1| p1.gs_leaderboard)
        .and_then(|scores| {
            scores.into_iter().find(|s| s.name.eq_ignore_ascii_case(&profile.groovestats_username))
        });

    if let Some(score_data) = player_score {
        let grade = score_to_grade(score_data.score);
        let lamp_index = compute_lamp_index(
            score_data.score,
            score_data.comments.as_deref(),
            &chart_hash,
        );
        let cached_score = CachedScore {
            grade,
            score_percent: score_data.score / 10000.0,
            lamp_index,
        };
        set_cached_score(chart_hash.clone(), cached_score);
        append_gs_score_on_disk(&chart_hash, cached_score, &profile.groovestats_username);
    } else {
        warn!(
            "No score found for player '{}' on chart '{}'. Caching as Failed.",
            profile.groovestats_username, chart_hash
        );
        let cached_score = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.0,
            // No lamp when there is no score for this chart.
            lamp_index: None,
        };
        set_cached_score(chart_hash, cached_score);
    }

    Ok(())
}
