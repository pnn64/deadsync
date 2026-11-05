//! Frozen scoring functions from 83c3161f9.
use deadsync_core::{note::NoteType, song_time::song_time_ns_invalid};
use deadsync_rules::judgment::{
    ExScoreData, ExScoreTotals, JudgeGrade, Judgment, select_row_final_judgment,
};
use deadsync_rules::note::{HoldResult, MineResult, Note};
use deadsync_rules::timing::{FA_PLUS_W0_MS, FA_PLUS_W010_MS, WindowCounts};

#[inline(always)]
#[must_use]
pub fn normalized_blue_window_ms(ms: f32) -> f32 {
    if ms.is_finite() && ms > 0.0 {
        ms
    } else {
        FA_PLUS_W010_MS
    }
}

#[inline(always)]
pub fn add_judgment_to_window_counts(
    counts: &mut WindowCounts,
    judgment: &Judgment,
    blue_window_ms: f32,
) {
    let split_ms = normalized_blue_window_ms(blue_window_ms);
    match judgment.grade {
        JudgeGrade::Fantastic => {
            if judgment.time_error_ms.abs() <= split_ms {
                counts.w0 = counts.w0.saturating_add(1);
            } else {
                counts.w1 = counts.w1.saturating_add(1);
            }
        }
        JudgeGrade::Excellent => counts.w2 = counts.w2.saturating_add(1),
        JudgeGrade::Great => counts.w3 = counts.w3.saturating_add(1),
        JudgeGrade::Decent => counts.w4 = counts.w4.saturating_add(1),
        JudgeGrade::WayOff => counts.w5 = counts.w5.saturating_add(1),
        JudgeGrade::Miss => counts.miss = counts.miss.saturating_add(1),
    }
}

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

#[derive(Default)]
struct ExScoreCounts {
    windows: WindowCounts,
    w010: u32,
    holds_held: u32,
    rolls_held: u32,
    mines_hit: u32,
}

#[inline(always)]
fn resolved_before_fail(
    note: &Note,
    index: usize,
    note_times_ns: &[i64],
    hold_end_times_ns: &[i64],
    fail_time_ns: Option<i64>,
) -> bool {
    let Some(fail_time_ns) = fail_time_ns else {
        return true;
    };
    let relevant_time_ns = if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
        hold_end_times_ns
            .get(index)
            .copied()
            .filter(|&time_ns| !song_time_ns_invalid(time_ns))
            .unwrap_or(0)
    } else {
        note_times_ns.get(index).copied().unwrap_or(0)
    };
    relevant_time_ns <= fail_time_ns
}

#[inline(always)]
fn add_resolved_note(counts: &mut ExScoreCounts, note: &Note) {
    match note.note_type {
        NoteType::Hold
            if note
                .hold
                .as_ref()
                .is_some_and(|h| h.result == Some(HoldResult::Held)) =>
        {
            counts.holds_held = counts.holds_held.saturating_add(1);
        }
        NoteType::Roll
            if note
                .hold
                .as_ref()
                .is_some_and(|h| h.result == Some(HoldResult::Held)) =>
        {
            counts.rolls_held = counts.rolls_held.saturating_add(1);
        }
        NoteType::Mine if note.mine_result == Some(MineResult::Hit) => {
            counts.mines_hit = counts.mines_hit.saturating_add(1);
        }
        _ => {}
    }
}

#[inline(always)]
fn add_ex_row(counts: &mut ExScoreCounts, judgment: &Judgment) {
    add_judgment_to_window_counts(&mut counts.windows, judgment, FA_PLUS_W0_MS);
    if judgment.grade == JudgeGrade::Fantastic && judgment.time_error_ms.abs() <= FA_PLUS_W010_MS {
        counts.w010 = counts.w010.saturating_add(1);
    }
}

#[inline(always)]
fn compute_ex_score_counts(
    notes: &[Note],
    note_times_ns: &[i64],
    hold_end_times_ns: &[i64],
    fail_time_ns: Option<i64>,
) -> ExScoreCounts {
    let mut counts = ExScoreCounts::default();
    let mut idx: usize = 0;
    while idx < notes.len() {
        let row_index = notes[idx].row_index;
        let row_time_ns = note_times_ns.get(idx).copied().unwrap_or(0);
        let row_is_playable = fail_time_ns.is_none_or(|time_ns| row_time_ns <= time_ns);
        let mut row_judgment = None;
        while idx < notes.len() && notes[idx].row_index == row_index {
            let note = &notes[idx];
            if row_is_playable
                && !note.is_fake
                && note.can_be_judged
                && !matches!(note.note_type, NoteType::Mine)
                && let Some(j) = note.result.as_ref()
            {
                select_row_final_judgment(&mut row_judgment, j);
            }
            if !note.is_fake
                && note.can_be_judged
                && matches!(
                    note.note_type,
                    NoteType::Hold | NoteType::Roll | NoteType::Mine
                )
                && resolved_before_fail(note, idx, note_times_ns, hold_end_times_ns, fail_time_ns)
            {
                add_resolved_note(&mut counts, note);
            }
            idx += 1;
        }
        if let Some(judgment) = row_judgment {
            add_ex_row(&mut counts, judgment);
        }
    }
    counts
}

#[inline(always)]
const fn window_counts_total_taps(counts: WindowCounts) -> u32 {
    counts
        .w0
        .saturating_add(counts.w1)
        .saturating_add(counts.w2)
        .saturating_add(counts.w3)
        .saturating_add(counts.w4)
        .saturating_add(counts.w5)
        .saturating_add(counts.miss)
}

#[inline(always)]
fn ex_total_possible(data: &ExScoreData) -> f64 {
    if data.total_steps == 0 {
        return 0.0;
    }
    f64::from(data.total_steps).mul_add(
        EX_WEIGHT_W0,
        f64::from(data.holds_total.saturating_add(data.rolls_total)),
    )
}

#[inline(always)]
/// Earned EX points before percentage rounding, including mine penalties.
pub fn ex_score_points(data: &ExScoreData) -> f64 {
    let mines_effective = data.mines_hit.min(data.mines_total);
    f64::from(mines_effective).mul_add(
        EX_WEIGHT_HIT_MINE,
        f64::from(data.rolls_held).mul_add(
            EX_WEIGHT_HELD,
            f64::from(data.holds_held).mul_add(
                EX_WEIGHT_HELD,
                f64::from(data.counts.w3).mul_add(
                    EX_WEIGHT_W3,
                    f64::from(data.counts.w2).mul_add(
                        EX_WEIGHT_W2,
                        f64::from(data.counts.w1)
                            .mul_add(EX_WEIGHT_W1, f64::from(data.counts.w0) * EX_WEIGHT_W0),
                    ),
                ),
            ),
        ),
    )
}

#[inline(always)]
/// Earned Hard EX points before percentage rounding, including mine penalties.
pub fn hard_ex_score_points(data: &ExScoreData) -> f64 {
    let w010 = data.counts_10ms.w0;
    let fantastic_total = data.counts.w0.saturating_add(data.counts.w1);
    let w110 = fantastic_total.saturating_sub(w010);
    let mines_effective = data.mines_hit.min(data.mines_total);
    f64::from(mines_effective).mul_add(
        HARD_EX_WEIGHT_HIT_MINE,
        f64::from(data.rolls_held).mul_add(
            HARD_EX_WEIGHT_HELD,
            f64::from(data.holds_held).mul_add(
                HARD_EX_WEIGHT_HELD,
                f64::from(data.counts.w2).mul_add(
                    HARD_EX_WEIGHT_W2,
                    f64::from(w110)
                        .mul_add(HARD_EX_WEIGHT_W110, f64::from(w010) * HARD_EX_WEIGHT_W010),
                ),
            ),
        ),
    )
}

#[inline(always)]
fn ex_current_possible_points(data: &ExScoreData) -> f64 {
    let resolved = data.holds_resolved.saturating_add(data.rolls_resolved);
    f64::from(window_counts_total_taps(data.counts)).mul_add(EX_WEIGHT_W0, f64::from(resolved))
}

#[inline(always)]
#[must_use]
pub fn ex_current_possible_ratio(data: &ExScoreData) -> f64 {
    let total = ex_total_possible(data);
    if total <= 0.0 {
        return 0.0;
    }
    (ex_current_possible_points(data) / total).clamp(0.0, 1.0)
}

#[inline(always)]
fn floor_percent(points: f64, total: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    ((points / total).max(0.0) * 10000.0).floor() / 100.0
}

#[inline(always)]
fn predictive_score_percents(
    total_possible: f64,
    current_possible: f64,
    actual: f64,
) -> (f64, f64, f64) {
    if total_possible <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let lost = (current_possible - actual).max(0.0);
    let kept = (total_possible - lost).max(0.0);
    let kept_pct = floor_percent(kept, total_possible);
    let lost_pct = (100.0 - kept_pct).max(0.0);
    let pace_pct = if current_possible > 0.0 {
        floor_percent(actual, current_possible)
    } else {
        0.0
    };
    (kept_pct, lost_pct, pace_pct)
}

#[inline(always)]
#[must_use]
pub fn ex_score_percent(data: &ExScoreData) -> f64 {
    floor_percent(ex_score_points(data), ex_total_possible(data))
}

#[inline(always)]
#[must_use]
pub fn hard_ex_score_percent(data: &ExScoreData) -> f64 {
    floor_percent(hard_ex_score_points(data), ex_total_possible(data))
}

#[inline(always)]
#[must_use]
pub fn predictive_ex_score_percents(data: &ExScoreData) -> (f64, f64, f64) {
    predictive_score_percents(
        ex_total_possible(data),
        ex_current_possible_points(data),
        ex_score_points(data),
    )
}

#[inline(always)]
#[must_use]
pub fn predictive_hard_ex_score_percents(data: &ExScoreData) -> (f64, f64, f64) {
    predictive_score_percents(
        ex_total_possible(data),
        ex_current_possible_points(data),
        hard_ex_score_points(data),
    )
}

#[inline(always)]
fn ex_score_data(counts: ExScoreCounts, totals: ExScoreTotals) -> ExScoreData {
    ExScoreData {
        counts: counts.windows,
        counts_10ms: WindowCounts {
            w0: counts.w010,
            ..WindowCounts::default()
        },
        holds_held: counts.holds_held,
        holds_resolved: counts.holds_held,
        rolls_held: counts.rolls_held,
        rolls_resolved: counts.rolls_held,
        mines_hit: counts.mines_hit,
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
    }
}

/// Reconstructs EX and Hard EX together in one allocation-free note scan.
#[inline(always)]
#[must_use]
pub fn calculate_ex_score_percents_from_notes(
    notes: &[Note],
    note_times_ns: &[i64],
    hold_end_times_ns: &[i64],
    totals: ExScoreTotals,
    fail_time_ns: Option<i64>,
) -> (f64, f64) {
    if notes.is_empty() {
        return (0.0, 0.0);
    }
    let data = ex_score_data(
        compute_ex_score_counts(notes, note_times_ns, hold_end_times_ns, fail_time_ns),
        totals,
    );
    (ex_score_percent(&data), hard_ex_score_percent(&data))
}
