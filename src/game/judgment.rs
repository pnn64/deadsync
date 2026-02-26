use std::cmp::Ordering;

use crate::game::note::{HoldResult, MineResult, Note, NoteType};
use crate::game::timing::{FA_PLUS_W0_MS, FA_PLUS_W010_MS, WindowCounts};

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

pub const JUDGE_GRADE_COUNT: usize = 6;
pub type JudgeCounts = [u32; JUDGE_GRADE_COUNT];

#[inline(always)]
pub const fn judge_grade_ix(grade: JudgeGrade) -> usize {
    match grade {
        JudgeGrade::Fantastic => 0,
        JudgeGrade::Excellent => 1,
        JudgeGrade::Great => 2,
        JudgeGrade::Decent => 3,
        JudgeGrade::WayOff => 4,
        JudgeGrade::Miss => 5,
    }
}

#[derive(Clone, Debug)]
pub struct Judgment {
    pub time_error_ms: f32,
    pub grade: JudgeGrade,            // The grade of this specific note
    pub window: Option<TimingWindow>, // Optional detailed window (W0-W5) for FA+/EX-style features
    // ITGmania parity: tap notes that are missed while the corresponding input is still held.
    // This is not a distinct tap note score in ITGmania; it is tracked as a separate flag.
    pub miss_because_held: bool,
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

const GRADE_POINTS_BY_IX: [i32; JUDGE_GRADE_COUNT] = [5, 4, 2, 0, -6, -12];

pub fn calculate_itg_grade_points_from_counts(
    scoring_counts: &JudgeCounts,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
) -> i32 {
    let mut total = 0i32;
    let mut i = 0usize;
    while i < JUDGE_GRADE_COUNT {
        total += GRADE_POINTS_BY_IX[i] * scoring_counts[i] as i32;
        i += 1;
    }
    total += holds_held_for_score as i32 * HOLD_SCORE_HELD;
    total += rolls_held_for_score as i32 * HOLD_SCORE_HELD;
    total += mines_hit_for_score as i32 * MINE_SCORE_HIT;
    total
}

pub fn calculate_itg_score_percent_from_counts(
    scoring_counts: &JudgeCounts,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
    possible_grade_points: i32,
) -> f64 {
    if possible_grade_points <= 0 {
        return 0.0;
    }

    let total_points = calculate_itg_grade_points_from_counts(
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
    let mut percent = f64::from(total_points) / f64::from(possible_grade_points);
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
const EX_WEIGHT_HELD: f64 = 1.0;
const EX_WEIGHT_HIT_MINE: f64 = -1.0;

const HARD_EX_WEIGHT_W010: f64 = 3.5;
const HARD_EX_WEIGHT_W110: f64 = 3.0;
const HARD_EX_WEIGHT_W2: f64 = 1.0;
const HARD_EX_WEIGHT_HELD: f64 = 1.0;
const HARD_EX_WEIGHT_HIT_MINE: f64 = -1.0;

struct ExScoreCounts {
    windows: WindowCounts,
    w010: u32,
    holds_held: u32,
    mines_hit: u32,
}

fn compute_ex_score_counts(
    notes: &[Note],
    note_times: &[f32],
    hold_end_times: &[Option<f32>],
    fail_time: Option<f32>,
) -> ExScoreCounts {
    let mut windows = WindowCounts::default();
    let mut w010: u32 = 0;

    let mut idx: usize = 0;
    let len = notes.len();
    while idx < len {
        let row_index = notes[idx].row_index;

        let row_time = note_times.get(idx).copied().unwrap_or(0.0);
        let row_is_playable = match fail_time {
            Some(t) => row_time <= t,
            None => true,
        };

        let mut row_judgments: Vec<&Judgment> = Vec::new();
        while idx < len && notes[idx].row_index == row_index {
            let note = &notes[idx];
            if row_is_playable
                && !note.is_fake
                && note.can_be_judged
                && !matches!(note.note_type, NoteType::Mine)
                && let Some(j) = note.result.as_ref()
            {
                row_judgments.push(j);
            }
            idx += 1;
        }

        if row_judgments.is_empty() {
            continue;
        }

        if let Some(j) = aggregate_row_final_judgment(row_judgments.iter().copied()) {
            match j.grade {
                JudgeGrade::Fantastic => {
                    if j.time_error_ms.abs() <= FA_PLUS_W0_MS {
                        windows.w0 = windows.w0.saturating_add(1);
                    } else {
                        windows.w1 = windows.w1.saturating_add(1);
                    }
                    if j.time_error_ms.abs() <= FA_PLUS_W010_MS {
                        w010 = w010.saturating_add(1);
                    }
                }
                JudgeGrade::Excellent => windows.w2 = windows.w2.saturating_add(1),
                JudgeGrade::Great => windows.w3 = windows.w3.saturating_add(1),
                JudgeGrade::Decent => windows.w4 = windows.w4.saturating_add(1),
                JudgeGrade::WayOff => windows.w5 = windows.w5.saturating_add(1),
                JudgeGrade::Miss => windows.miss = windows.miss.saturating_add(1),
            }
        }
    }

    let mut holds_held: u32 = 0;
    let mut mines_hit: u32 = 0;

    for (i, note) in notes.iter().enumerate() {
        if note.is_fake || !note.can_be_judged {
            continue;
        }

        if let Some(ft) = fail_time {
            let relevant_time = if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                hold_end_times.get(i).and_then(|t| *t).unwrap_or(0.0)
            } else {
                note_times.get(i).copied().unwrap_or(0.0)
            };
            if relevant_time > ft {
                continue;
            }
        }

        match note.note_type {
            NoteType::Hold => {
                if let Some(h) = note.hold.as_ref()
                    && h.result == Some(HoldResult::Held)
                {
                    holds_held = holds_held.saturating_add(1);
                }
            }
            NoteType::Mine => {
                if note.mine_result == Some(MineResult::Hit) {
                    mines_hit = mines_hit.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    ExScoreCounts {
        windows,
        w010,
        holds_held,
        mines_hit,
    }
}

/// Calculates FA+ EX score using the same algebra as SL:
///
///   `total_possible` = `total_steps` * 3.5 + (`total_holds` + `total_rolls`)
///   `total_points`   = W0*3.5 + W1*3 + W2*2 + W3 + `holds_held` - `mines_hit`
///   `ex_percent`     = `floor(total_points` / `total_possible` * 10000) / 100
///
/// where W0..W3 are taken from the final per-row window counts used by the FA+
/// pane, `holds_held` counts successful holds only (rolls are reported
/// separately), and `mines_hit` is the number of mines actually hit.
///
/// This version respects `fail_time` to stop accumulating points if the player
/// has failed the song.
pub fn calculate_ex_score_from_notes(
    notes: &[Note],
    note_times: &[f32],
    hold_end_times: &[Option<f32>],
    total_steps: u32,
    holds_total: u32,
    rolls_total: u32,
    mines_total: u32,
    fail_time: Option<f32>,
    _mines_disabled: bool,
) -> f64 {
    if total_steps == 0 {
        return 0.0;
    }

    let counts = compute_ex_score_counts(notes, note_times, hold_end_times, fail_time);

    let total_steps_f = f64::from(total_steps);
    let total_holds_f = f64::from(holds_total);
    let total_rolls_f = f64::from(rolls_total);

    let total_possible = total_steps_f.mul_add(
        EX_WEIGHT_W0,
        (total_holds_f + total_rolls_f) * EX_WEIGHT_HELD,
    );
    if total_possible <= 0.0 {
        return 0.0;
    }

    // Spreadsheet-style EX points, ignoring rolls in the numerator:
    let mut total_points = 0.0_f64;
    total_points += f64::from(counts.windows.w0) * EX_WEIGHT_W0;
    total_points += f64::from(counts.windows.w1) * EX_WEIGHT_W1;
    total_points += f64::from(counts.windows.w2) * EX_WEIGHT_W2;
    total_points += f64::from(counts.windows.w3) * EX_WEIGHT_W3;

    total_points += f64::from(counts.holds_held) * EX_WEIGHT_HELD;

    // Mines subtract if hit while alive.
    let mines_effective = counts.mines_hit.min(mines_total);
    total_points += f64::from(mines_effective) * EX_WEIGHT_HIT_MINE;

    let ratio = (total_points / total_possible).max(0.0);
    ((ratio * 10000.0).floor()) / 100.0
}

pub fn calculate_hard_ex_score_from_notes(
    notes: &[Note],
    note_times: &[f32],
    hold_end_times: &[Option<f32>],
    total_steps: u32,
    holds_total: u32,
    rolls_total: u32,
    mines_total: u32,
    fail_time: Option<f32>,
    _mines_disabled: bool,
) -> f64 {
    if total_steps == 0 {
        return 0.0;
    }

    let counts = compute_ex_score_counts(notes, note_times, hold_end_times, fail_time);

    let total_steps_f = f64::from(total_steps);
    let total_holds_f = f64::from(holds_total);
    let total_rolls_f = f64::from(rolls_total);

    let total_possible = total_steps_f.mul_add(
        HARD_EX_WEIGHT_W010,
        (total_holds_f + total_rolls_f) * HARD_EX_WEIGHT_HELD,
    );
    if total_possible <= 0.0 {
        return 0.0;
    }

    let fantastic_total = counts.windows.w0.saturating_add(counts.windows.w1);
    let w110 = fantastic_total.saturating_sub(counts.w010);

    let mut total_points = 0.0_f64;
    total_points += f64::from(counts.w010) * HARD_EX_WEIGHT_W010;
    total_points += f64::from(w110) * HARD_EX_WEIGHT_W110;
    total_points += f64::from(counts.windows.w2) * HARD_EX_WEIGHT_W2;
    total_points += f64::from(counts.holds_held) * HARD_EX_WEIGHT_HELD;

    let mines_effective = counts.mines_hit.min(mines_total);
    total_points += f64::from(mines_effective) * HARD_EX_WEIGHT_HIT_MINE;

    let ratio = (total_points / total_possible).max(0.0);
    ((ratio * 10000.0).floor()) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::game::note::{HoldData, HoldResult, MineResult, Note, NoteType};

    const BLUE_FANTASTIC: u32 = 713;
    const WHITE_FANTASTIC: u32 = 204;
    const EXCELLENT: u32 = 307;
    const GREAT: u32 = 115;
    const DECENT: u32 = 8;
    const WAY_OFF: u32 = 4;
    const MISS: u32 = 13;
    const HOLDS_HELD: u32 = 60;
    const HOLDS_TOTAL: u32 = 76;
    const ROLLS_HELD: u32 = 3;
    const ROLLS_TOTAL: u32 = 4;
    const MINES_HIT: u32 = 9;

    #[inline(always)]
    fn make_tap(row_index: usize, grade: JudgeGrade, time_error_ms: f32) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index,
            result: Some(Judgment {
                time_error_ms,
                grade,
                window: None,
                miss_because_held: false,
            }),
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[inline(always)]
    fn make_hold(row_index: usize, held: bool) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Hold,
            row_index,
            result: None,
            early_result: None,
            hold: Some(HoldData {
                end_row_index: row_index.saturating_add(1),
                end_beat: row_index.saturating_add(1) as f32,
                result: Some(if held {
                    HoldResult::Held
                } else {
                    HoldResult::LetGo
                }),
                life: if held { 1.0 } else { 0.0 },
                let_go_started_at: None,
                let_go_starting_life: 1.0,
                last_held_row_index: row_index,
                last_held_beat: row_index as f32,
            }),
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[inline(always)]
    fn make_mine(row_index: usize) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Mine,
            row_index,
            result: None,
            early_result: None,
            hold: None,
            mine_result: Some(MineResult::Hit),
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[inline(always)]
    fn push_taps(
        notes: &mut Vec<Note>,
        row_index: &mut usize,
        count: u32,
        grade: JudgeGrade,
        time_error_ms: f32,
    ) {
        for _ in 0..count {
            notes.push(make_tap(*row_index, grade, time_error_ms));
            *row_index = row_index.saturating_add(1);
        }
    }

    #[test]
    fn itg_percent_matches_known_reference_counts() {
        let scoring_counts: JudgeCounts = [
            BLUE_FANTASTIC.saturating_add(WHITE_FANTASTIC),
            EXCELLENT,
            GREAT,
            DECENT,
            WAY_OFF,
            MISS,
        ];
        let total_steps = BLUE_FANTASTIC
            .saturating_add(WHITE_FANTASTIC)
            .saturating_add(EXCELLENT)
            .saturating_add(GREAT)
            .saturating_add(DECENT)
            .saturating_add(WAY_OFF)
            .saturating_add(MISS);
        let possible_grade_points = i32::try_from(
            total_steps
                .saturating_add(HOLDS_TOTAL)
                .saturating_add(ROLLS_TOTAL),
        )
        .unwrap_or(i32::MAX)
        .saturating_mul(HOLD_SCORE_HELD);

        let percent = calculate_itg_score_percent_from_counts(
            &scoring_counts,
            HOLDS_HELD,
            ROLLS_HELD,
            MINES_HIT,
            possible_grade_points,
        );
        let total_points = calculate_itg_grade_points_from_counts(
            &scoring_counts,
            HOLDS_HELD,
            ROLLS_HELD,
            MINES_HIT,
        );
        let plain_floor_percent =
            ((f64::from(total_points) / f64::from(possible_grade_points)) * 10000.0).floor()
                / 100.0;
        assert!((plain_floor_percent - 84.81).abs() <= 1e-9);

        // MakePercentScore-style epsilon (+0.000001 before truncation) pushes
        // this boundary case to 84.82 instead of 84.81.
        assert!((percent - 0.8482).abs() <= 1e-9);
        assert!((percent * 100.0 - 84.82).abs() <= 1e-9);
    }

    #[test]
    fn ex_percent_matches_known_reference_counts() {
        let total_steps = BLUE_FANTASTIC
            .saturating_add(WHITE_FANTASTIC)
            .saturating_add(EXCELLENT)
            .saturating_add(GREAT)
            .saturating_add(DECENT)
            .saturating_add(WAY_OFF)
            .saturating_add(MISS);

        let mut notes: Vec<Note> =
            Vec::with_capacity((total_steps + HOLDS_TOTAL + MINES_HIT) as usize);
        let mut row_index = 0usize;

        push_taps(
            &mut notes,
            &mut row_index,
            BLUE_FANTASTIC,
            JudgeGrade::Fantastic,
            0.0,
        );
        push_taps(
            &mut notes,
            &mut row_index,
            WHITE_FANTASTIC,
            JudgeGrade::Fantastic,
            FA_PLUS_W0_MS + 0.001,
        );
        push_taps(
            &mut notes,
            &mut row_index,
            EXCELLENT,
            JudgeGrade::Excellent,
            0.0,
        );
        push_taps(&mut notes, &mut row_index, GREAT, JudgeGrade::Great, 0.0);
        push_taps(&mut notes, &mut row_index, DECENT, JudgeGrade::Decent, 0.0);
        push_taps(&mut notes, &mut row_index, WAY_OFF, JudgeGrade::WayOff, 0.0);
        push_taps(&mut notes, &mut row_index, MISS, JudgeGrade::Miss, 0.0);

        for i in 0..HOLDS_TOTAL {
            notes.push(make_hold(
                row_index.saturating_add(i as usize),
                i < HOLDS_HELD,
            ));
        }
        row_index = row_index.saturating_add(HOLDS_TOTAL as usize);
        for i in 0..MINES_HIT {
            notes.push(make_mine(row_index.saturating_add(i as usize)));
        }

        let note_times = vec![0.0_f32; notes.len()];
        let hold_end_times = vec![None; notes.len()];
        let ex = calculate_ex_score_from_notes(
            &notes,
            &note_times,
            &hold_end_times,
            total_steps,
            HOLDS_TOTAL,
            ROLLS_TOTAL,
            MINES_HIT,
            None,
            false,
        );

        assert!((ex - 80.08).abs() <= 1e-9);
    }

    #[test]
    fn ex_percent_matches_second_known_reference_counts() {
        const BLUE_FANTASTIC_2: u32 = 54;
        const WHITE_FANTASTIC_2: u32 = 204;
        const EXCELLENT_2: u32 = 561;
        const GREAT_2: u32 = 117;
        const DECENT_2: u32 = 15;
        const WAY_OFF_2: u32 = 4;
        const MISS_2: u32 = 13;
        const HOLDS_HELD_2: u32 = 73;
        const HOLDS_TOTAL_2: u32 = 76;
        const ROLLS_TOTAL_2: u32 = 4;
        const MINES_HIT_2: u32 = 27;

        let total_steps = BLUE_FANTASTIC_2
            .saturating_add(WHITE_FANTASTIC_2)
            .saturating_add(EXCELLENT_2)
            .saturating_add(GREAT_2)
            .saturating_add(DECENT_2)
            .saturating_add(WAY_OFF_2)
            .saturating_add(MISS_2);

        let mut notes: Vec<Note> =
            Vec::with_capacity((total_steps + HOLDS_TOTAL_2 + MINES_HIT_2) as usize);
        let mut row_index = 0usize;

        push_taps(
            &mut notes,
            &mut row_index,
            BLUE_FANTASTIC_2,
            JudgeGrade::Fantastic,
            0.0,
        );
        push_taps(
            &mut notes,
            &mut row_index,
            WHITE_FANTASTIC_2,
            JudgeGrade::Fantastic,
            FA_PLUS_W0_MS + 0.001,
        );
        push_taps(
            &mut notes,
            &mut row_index,
            EXCELLENT_2,
            JudgeGrade::Excellent,
            0.0,
        );
        push_taps(&mut notes, &mut row_index, GREAT_2, JudgeGrade::Great, 0.0);
        push_taps(
            &mut notes,
            &mut row_index,
            DECENT_2,
            JudgeGrade::Decent,
            0.0,
        );
        push_taps(
            &mut notes,
            &mut row_index,
            WAY_OFF_2,
            JudgeGrade::WayOff,
            0.0,
        );
        push_taps(&mut notes, &mut row_index, MISS_2, JudgeGrade::Miss, 0.0);

        for i in 0..HOLDS_TOTAL_2 {
            notes.push(make_hold(
                row_index.saturating_add(i as usize),
                i < HOLDS_HELD_2,
            ));
        }
        row_index = row_index.saturating_add(HOLDS_TOTAL_2 as usize);
        for i in 0..MINES_HIT_2 {
            notes.push(make_mine(row_index.saturating_add(i as usize)));
        }

        let note_times = vec![0.0_f32; notes.len()];
        let hold_end_times = vec![None; notes.len()];
        let ex = calculate_ex_score_from_notes(
            &notes,
            &note_times,
            &hold_end_times,
            total_steps,
            HOLDS_TOTAL_2,
            ROLLS_TOTAL_2,
            MINES_HIT_2,
            None,
            false,
        );

        assert!((ex - 60.14).abs() <= 1e-9);
    }
}
