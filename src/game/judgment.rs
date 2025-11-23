use std::collections::HashMap;

use crate::game::timing::WindowCounts;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimingWindow {
    // FA+ inner Fantastic (W0) lives strictly inside the normal Fantastic window.
    W0,
    // ITG-style tap windows, mapped 1:1 to JudgeGrade semantics.
    W1,
    W2,
    W3,
    W4,
    W5,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JudgeGrade {
    Fantastic, // W1 (plus FA+ W0 when enabled)
    Excellent, // W2
    Great,     // W3
    Decent,    // W4
    WayOff,    // W5
    Miss,
}

#[derive(Clone, Debug)]
pub struct Judgment {
    pub time_error_ms: f32,
    pub grade: JudgeGrade,          // The grade of this specific note
    pub window: Option<TimingWindow>, // Optional detailed window (W0-W5) for FA+/EX-style features
}

pub const HOLD_SCORE_HELD: i32 = 5;
pub const MINE_SCORE_HIT: i32 = -6;

pub fn grade_points_for(grade: JudgeGrade) -> i32 {
    match grade {
        JudgeGrade::Fantastic => 5,
        JudgeGrade::Excellent => 4,
        JudgeGrade::Great => 2,
        JudgeGrade::Decent => 0,
        JudgeGrade::WayOff => -6,
        JudgeGrade::Miss => -12,
    }
}

pub fn calculate_itg_grade_points(
    scoring_counts: &HashMap<JudgeGrade, u32>,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
) -> i32 {
    let mut total = 0i32;
    for (grade, count) in scoring_counts {
        total += grade_points_for(*grade) * (*count as i32);
    }

    total += holds_held_for_score as i32 * HOLD_SCORE_HELD;
    total += rolls_held_for_score as i32 * HOLD_SCORE_HELD;
    total += mines_hit_for_score as i32 * MINE_SCORE_HIT;
    total
}

pub fn calculate_itg_score_percent(
    scoring_counts: &HashMap<JudgeGrade, u32>,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
    possible_grade_points: i32,
) -> f64 {
    if possible_grade_points <= 0 {
        return 0.0;
    }

    let total_points = calculate_itg_grade_points(
        scoring_counts,
        holds_held_for_score,
        rolls_held_for_score,
        mines_hit_for_score,
    );

    (total_points as f64 / possible_grade_points as f64).max(0.0)
}

// ----------------------------- FA+ EX Scoring -----------------------------
// Mirrors Simply Love's SL.ExWeights table and CalculateExScore() helper:
//   W0=3.5, W1=3, W2=2, W3=1, W4=0, W5=0, Miss=0, LetGo=0, Held=1, HitMine=-1.

const EX_WEIGHT_W0: f64 = 3.5;
const EX_WEIGHT_W1: f64 = 3.0;
const EX_WEIGHT_W2: f64 = 2.0;
const EX_WEIGHT_W3: f64 = 1.0;
const EX_WEIGHT_W4: f64 = 0.0;
const EX_WEIGHT_W5: f64 = 0.0;
const EX_WEIGHT_MISS: f64 = 0.0;
const EX_WEIGHT_LET_GO: f64 = 0.0;
const EX_WEIGHT_HELD: f64 = 1.0;
const EX_WEIGHT_HIT_MINE: f64 = -1.0;

/// Calculates FA+ style EX score percentage (0.00–100.00) given:
/// - detailed tap window counts (including W0),
/// - hold/roll results (Held/LetGo),
/// - mine hits,
/// - and total step/hold/roll/mine counts from chart radar data.
///
/// The formula is:
///   total_possible = total_steps * W0_weight + (total_holds + total_rolls) * Held_weight
///   total_points   = sum(count_i * weight_i)
///   ex_percent     = floor(total_points / total_possible * 10000) / 100
///
/// `mines_disabled` implements the “NoMines still affect EX score” rule:
/// when true we pretend all mines were hit by adding total_mines * HitMine_weight
/// to the numerator before applying the actual hit_mine count.
pub fn calculate_ex_score_fa_plus(
    windows: &WindowCounts,
    held: u32,
    let_go: u32,
    hit_mine: u32,
    total_steps: u32,
    total_holds: u32,
    total_rolls: u32,
    total_mines: u32,
    mines_disabled: bool,
) -> f64 {
    let total_steps_f = total_steps as f64;
    let total_holds_f = total_holds as f64;
    let total_rolls_f = total_rolls as f64;

    let total_possible =
        total_steps_f * EX_WEIGHT_W0 + (total_holds_f + total_rolls_f) * EX_WEIGHT_HELD;
    if total_possible <= 0.0 {
        return 0.0;
    }

    let mut total_points = 0.0_f64;

    // Mines disabled: still account for them as if they could have been hit.
    if mines_disabled && total_mines > 0 {
        total_points += (total_mines as f64) * EX_WEIGHT_HIT_MINE;
    }

    total_points += (windows.w0 as f64) * EX_WEIGHT_W0;
    total_points += (windows.w1 as f64) * EX_WEIGHT_W1;
    total_points += (windows.w2 as f64) * EX_WEIGHT_W2;
    total_points += (windows.w3 as f64) * EX_WEIGHT_W3;
    total_points += (windows.w4 as f64) * EX_WEIGHT_W4;
    total_points += (windows.w5 as f64) * EX_WEIGHT_W5;
    total_points += (windows.miss as f64) * EX_WEIGHT_MISS;

    total_points += (held as f64) * EX_WEIGHT_HELD;
    total_points += (let_go as f64) * EX_WEIGHT_LET_GO;
    total_points += (hit_mine as f64) * EX_WEIGHT_HIT_MINE;

    let ratio = (total_points / total_possible).max(0.0);
    // Match Simply Love: floor(total_points/total_possible * 10000) / 100
    ((ratio * 10000.0).floor()) / 100.0
}
