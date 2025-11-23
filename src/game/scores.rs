use crate::core::network;
use crate::game::profile::Profile;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

const API_URL: &str = "https://api.groovestats.com/player-leaderboards.php";

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
    GRADE_CACHE.lock().unwrap().get(chart_hash).copied()
}

pub fn set_cached_score(chart_hash: String, score: CachedScore) {
    info!("Caching score {:?} for chart hash {}", score, chart_hash);
    GRADE_CACHE.lock().unwrap().insert(chart_hash, score);
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
        // For now, assign a deterministic "random" lamp index based on the chart hash.
        // This keeps behavior stable across runs without pulling in RNG dependencies.
        let lamp_index = {
            let mut h: u32 = 0;
            for b in chart_hash.as_bytes() {
                h = h.wrapping_mul(31).wrapping_add(*b as u32);
            }
            Some((h & 0xFF) as u8)
        };
        let cached_score = CachedScore {
            grade,
            score_percent: score_data.score / 10000.0,
            lamp_index,
        };
        set_cached_score(chart_hash, cached_score);
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
