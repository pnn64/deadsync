use std::collections::HashMap;

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
