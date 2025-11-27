use std::cmp::Ordering;
use std::collections::HashMap;

use crate::game::note::{HoldResult, MineResult, Note, NoteType};
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

/// Aggregates per-note judgments on a single row into the final row judgment,
/// mirroring the logic used by gameplay scoring:
/// - Any Miss on the row yields a Miss row judgment.
/// - Otherwise, the note with the largest absolute timing error determines
///   the row's grade and timing window.
#[inline(always)]
pub fn aggregate_row_final_judgment<'a, I>(judgments: I) -> Option<&'a Judgment>
where
    I: IntoIterator<Item = &'a Judgment>,
{
    let mut has_miss = false;
    let mut chosen: Option<&'a Judgment> = None;

    for j in judgments {
        if j.grade == JudgeGrade::Miss {
            if !has_miss {
                has_miss = true;
                chosen = Some(j);
            }
            continue;
        }

        if has_miss {
            continue;
        }

        match chosen {
            None => chosen = Some(j),
            Some(current) => {
                let a = j.time_error_ms.abs();
                let b = current.time_error_ms.abs();
                let ord = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
                if ord == Ordering::Greater {
                    chosen = Some(j);
                }
            }
        }
    }

    chosen
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

    if total_points <= 0 {
        return 0.0;
    }
    if total_points >= possible_grade_points {
        // Correct for rounding error at the top end, mirroring
        // PlayerStageStats::MakePercentScore when actual == possible.
        return 1.0;
    }

    // Base ITG percent as a 0.0–1.0 ratio.
    let mut percent = total_points as f64 / possible_grade_points as f64;
    if percent < 0.0 {
        percent = 0.0;
    }

    // Mirror ITGmania's MakePercentScore truncation semantics so that the
    // displayed percent never rounds up beyond what the underlying grade
    // thresholds would allow.
    //
    // CommonMetrics::PercentScoreDecimalPlaces is 2 in ITGmania, which yields:
    //   iPercentTotalDigits = 3 + 2 = 5 ("100.00")
    //   fTruncInterval      = 10^-(5-1) = 0.0001
    //
    // We hard-code the same behavior here.
    const DECIMAL_PLACES: i32 = 2;
    let percent_total_digits = 3 + DECIMAL_PLACES;
    let trunc_interval = 10_f64.powi(-(percent_total_digits - 1));

    // Small boost to avoid ftruncf-style underflow when very close to 1.0.
    percent += 0.000001_f64;

    let scaled = (percent / trunc_interval).floor() * trunc_interval;
    scaled.max(0.0)
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

/// Calculates FA+ EX score from a full gameplay note stream, applying failure
/// semantics so that score no longer increases after the moment of failure.
///
/// - `notes`:           Full note list for the chart.
/// - `note_times`:      Cached absolute times (seconds) for each note.
/// - `hold_end_times`:  Optional cached end times (seconds) for holds/rolls.
/// - `holds_total`:     Total chart holds (for denominator).
/// - `rolls_total`:     Total chart rolls (for denominator).
/// - `mines_total`:     Total chart mines (for denominator).
/// - `fail_time`:       First music time (seconds) when life reached 0, if any.
/// - `mines_disabled`:  Whether NoMines is active (EX still counts mines).
///
/// This mirrors Simply Love's CalculateExScore semantics and additionally
/// enforces that:
/// - taps and tap windows only count for notes scheduled at or before fail,
/// - hold/roll Held/LetGo results only count if their end time is at or before fail,
/// - mine hits only count if their scheduled time is at or before fail,
/// while the denominator (total_steps/holds/rolls/mines) remains chart-wide.
pub fn calculate_ex_score_from_notes(
    notes: &[Note],
    note_times: &[f32],
    hold_end_times: &[Option<f32>],
    holds_total: u32,
    rolls_total: u32,
    mines_total: u32,
    fail_time: Option<f32>,
    mines_disabled: bool,
) -> f64 {
    let mut total_steps: u32 = 0;
    let mut held: u32 = 0;
    let mut let_go: u32 = 0;
    let mut hit_mine: u32 = 0;

    // Pass 1: aggregate step, hold/roll, and mine results using the same
    // failure gating semantics as Simply Love's TrackExScoreJudgments.
    for (i, note) in notes.iter().enumerate() {
        if note.is_fake || !note.can_be_judged {
            continue;
        }

        match note.note_type {
            NoteType::Tap => {
                total_steps = total_steps.saturating_add(1);
            }
            NoteType::Hold | NoteType::Roll => {
                total_steps = total_steps.saturating_add(1);

                let note_time = *note_times.get(i).unwrap_or(&0.0);
                let end_time = hold_end_times
                    .get(i)
                    .and_then(|t| *t)
                    .unwrap_or(note_time);
                let include_hold = match fail_time {
                    None => true,
                    Some(ft) => end_time <= ft,
                };

                if include_hold {
                    if let Some(h) = note.hold.as_ref() {
                        match h.result {
                            Some(HoldResult::Held) => {
                                held = held.saturating_add(1);
                            }
                            Some(HoldResult::LetGo) => {
                                let_go = let_go.saturating_add(1);
                            }
                            None => {}
                        }
                    }
                }
            }
            NoteType::Mine => {
                let note_time = *note_times.get(i).unwrap_or(&0.0);
                let include_mine = match fail_time {
                    None => true,
                    Some(ft) => note_time <= ft,
                };
                if include_mine && note.mine_result == Some(MineResult::Hit) {
                    hit_mine = hit_mine.saturating_add(1);
                }
            }
            NoteType::Fake => {}
        }
    }

    // Pass 2: aggregate tap window counts row-by-row, mirroring ITGmania's
    // row-based TapNoteScore semantics and Simply Love's EX tracking
    // (TrackExScoreJudgments) with failure gating.
    let mut windows = WindowCounts::default();

    if !notes.is_empty() {
        let mut idx: usize = 0;
        let len = notes.len();

        while idx < len {
            let row_index = notes[idx].row_index;
            let mut row_time = f32::INFINITY;
            let mut row_judgments: Vec<&Judgment> = Vec::new();

            while idx < len && notes[idx].row_index == row_index {
                let note = &notes[idx];
                if !note.is_fake && note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                    let note_time = *note_times.get(idx).unwrap_or(&0.0);
                    if note_time < row_time {
                        row_time = note_time;
                    }
                    if let Some(j) = note.result.as_ref() {
                        row_judgments.push(j);
                    }
                }
                idx += 1;
            }

            if row_judgments.is_empty() {
                continue;
            }

            let active_at_fail = match fail_time {
                None => true,
                Some(ft) => row_time <= ft,
            };
            if !active_at_fail {
                continue;
            }

            if let Some(j) = aggregate_row_final_judgment(row_judgments.iter().copied()) {
                match j.grade {
                    JudgeGrade::Fantastic => match j.window {
                        Some(TimingWindow::W0) => {
                            windows.w0 = windows.w0.saturating_add(1);
                        }
                        _ => {
                            windows.w1 = windows.w1.saturating_add(1);
                        }
                    },
                    JudgeGrade::Excellent => {
                        windows.w2 = windows.w2.saturating_add(1);
                    }
                    JudgeGrade::Great => {
                        windows.w3 = windows.w3.saturating_add(1);
                    }
                    JudgeGrade::Decent => {
                        windows.w4 = windows.w4.saturating_add(1);
                    }
                    JudgeGrade::WayOff => {
                        windows.w5 = windows.w5.saturating_add(1);
                    }
                    JudgeGrade::Miss => {
                        windows.miss = windows.miss.saturating_add(1);
                    }
                }
            }
        }
    }

    calculate_ex_score_fa_plus(
        &windows,
        held,
        let_go,
        hit_mine,
        total_steps,
        holds_total,
        rolls_total,
        mines_total,
        mines_disabled,
    )
}
