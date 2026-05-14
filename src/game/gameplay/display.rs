use crate::game::judgment::{self, JudgeGrade, Judgment};
use crate::game::timing::{self, WindowCounts};

use super::{
    CourseDisplayCarry, CourseDisplayTotals, ExScoreInputs, MAX_PLAYERS, PlayerRuntime, State,
    display_judge_ix, player_blue_window_ms,
};

#[inline(always)]
fn add_window_counts(lhs: WindowCounts, rhs: WindowCounts) -> WindowCounts {
    WindowCounts {
        w0: lhs.w0.saturating_add(rhs.w0),
        w1: lhs.w1.saturating_add(rhs.w1),
        w2: lhs.w2.saturating_add(rhs.w2),
        w3: lhs.w3.saturating_add(rhs.w3),
        w4: lhs.w4.saturating_add(rhs.w4),
        w5: lhs.w5.saturating_add(rhs.w5),
        miss: lhs.miss.saturating_add(rhs.miss),
    }
}

#[inline(always)]
fn normalized_blue_window_ms(ms: f32) -> f32 {
    if ms.is_finite() && ms > 0.0 {
        ms
    } else {
        timing::FA_PLUS_W010_MS
    }
}

#[inline(always)]
fn add_judgment_to_window_counts(
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

#[inline(always)]
fn float_match(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.000_1
}

#[inline(always)]
fn display_window_counts_10ms(state: &State, player_idx: usize) -> WindowCounts {
    if player_idx >= state.num_players {
        return WindowCounts::default();
    }
    let current = state.live_window_counts_10ms_blue[player_idx];
    let carry = display_carry_for_player(state, player_idx);
    add_window_counts(current, carry.window_counts_10ms_blue)
}

#[inline(always)]
pub(super) fn scored_hold_totals_with_carry(
    held: u32,
    let_go: u32,
    carry_held: u32,
    carry_let_go: u32,
) -> (u32, u32) {
    let held_total = held.saturating_add(carry_held);
    let resolved_total = held_total
        .saturating_add(let_go)
        .saturating_add(carry_let_go);
    (held_total, resolved_total)
}

#[inline(always)]
fn live_ex_score_inputs(state: &State, player_idx: usize) -> ExScoreInputs {
    let player = &state.players[player_idx];
    ExScoreInputs {
        counts: display_window_counts(state, player_idx, None),
        counts_10ms: display_window_counts_10ms(state, player_idx),
        holds_held_for_score: player.holds_held_for_score,
        holds_let_go_for_score: player.holds_let_go_for_score,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_let_go_for_score: player.rolls_let_go_for_score,
        mines_hit_for_score: player.mines_hit_for_score,
    }
}

#[inline(always)]
fn ex_score_data_from_inputs(
    state: &State,
    player_idx: usize,
    inputs: ExScoreInputs,
) -> judgment::ExScoreData {
    let carry = display_carry_for_player(state, player_idx);
    let totals = display_totals_for_player(state, player_idx);
    let (holds_held, holds_resolved) = scored_hold_totals_with_carry(
        inputs.holds_held_for_score,
        inputs.holds_let_go_for_score,
        carry.holds_held_for_score,
        carry.holds_let_go_for_score,
    );
    let (rolls_held, rolls_resolved) = scored_hold_totals_with_carry(
        inputs.rolls_held_for_score,
        inputs.rolls_let_go_for_score,
        carry.rolls_held_for_score,
        carry.rolls_let_go_for_score,
    );
    judgment::ExScoreData {
        counts: inputs.counts,
        counts_10ms: inputs.counts_10ms,
        holds_held,
        holds_resolved,
        rolls_held,
        rolls_resolved,
        mines_hit: inputs
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
    }
}

#[inline(always)]
pub fn display_carry_for_player(state: &State, player_idx: usize) -> CourseDisplayCarry {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayCarry::default();
    }
    state
        .course_display_carry
        .as_ref()
        .map_or(CourseDisplayCarry::default(), |carry| carry[player_idx])
}

#[inline(always)]
pub(super) fn record_display_window_counts(
    state: &mut State,
    player_idx: usize,
    judgment: &Judgment,
) {
    if player_idx >= state.num_players || player_idx >= MAX_PLAYERS {
        return;
    }
    let display_window_ms = player_blue_window_ms(state, player_idx);
    add_judgment_to_window_counts(
        &mut state.live_window_counts[player_idx],
        judgment,
        timing::FA_PLUS_W0_MS,
    );
    add_judgment_to_window_counts(
        &mut state.live_window_counts_10ms_blue[player_idx],
        judgment,
        timing::FA_PLUS_W010_MS,
    );
    add_judgment_to_window_counts(
        &mut state.live_window_counts_display_blue[player_idx],
        judgment,
        display_window_ms,
    );
}

#[inline(always)]
pub fn display_totals_for_player(state: &State, player_idx: usize) -> CourseDisplayTotals {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayTotals::default();
    }
    if let Some(totals) = state.course_display_totals.as_ref() {
        return totals[player_idx];
    }
    CourseDisplayTotals {
        possible_grade_points: state.possible_grade_points[player_idx],
        total_steps: state.total_steps[player_idx],
        holds_total: state.holds_total[player_idx],
        rolls_total: state.rolls_total[player_idx],
        mines_total: state.mines_total[player_idx],
    }
}

pub fn display_judgment_count(state: &State, player_idx: usize, grade: JudgeGrade) -> u32 {
    if player_idx >= state.num_players {
        return 0;
    }
    let base = state.players[player_idx].judgment_counts[display_judge_ix(grade)];
    let carry = display_carry_for_player(state, player_idx);
    base.saturating_add(carry.judgment_counts[display_judge_ix(grade)])
}

pub fn display_live_timing_stats(state: &State, player_idx: usize) -> timing::LiveTimingSnapshot {
    if player_idx >= state.num_players {
        return timing::LiveTimingSnapshot::default();
    }
    timing::live_timing_stats_snapshot(&state.players[player_idx].live_timing_stats)
}

pub fn display_window_counts(
    state: &State,
    player_idx: usize,
    blue_window_ms: Option<f32>,
) -> WindowCounts {
    if player_idx >= state.num_players {
        return WindowCounts::default();
    }
    let current = if let Some(ms) = blue_window_ms {
        let split_ms = normalized_blue_window_ms(ms);
        let display_split_ms = normalized_blue_window_ms(player_blue_window_ms(state, player_idx));
        if float_match(split_ms, timing::FA_PLUS_W0_MS) {
            state.live_window_counts[player_idx]
        } else if float_match(split_ms, timing::FA_PLUS_W010_MS) {
            state.live_window_counts_10ms_blue[player_idx]
        } else if float_match(split_ms, display_split_ms) {
            state.live_window_counts_display_blue[player_idx]
        } else {
            let (start, end) = state.note_ranges[player_idx];
            timing::compute_window_counts_blue_ms(&state.notes[start..end], split_ms)
        }
    } else {
        state.live_window_counts[player_idx]
    };
    let carry = display_carry_for_player(state, player_idx);
    let carry_counts = if let Some(ms) = blue_window_ms {
        let split_ms = normalized_blue_window_ms(ms);
        if float_match(split_ms, timing::FA_PLUS_W0_MS) {
            carry.window_counts
        } else if float_match(split_ms, timing::FA_PLUS_W010_MS) {
            carry.window_counts_10ms_blue
        } else {
            carry.window_counts_display_blue
        }
    } else {
        carry.window_counts
    };
    add_window_counts(current, carry_counts)
}

pub fn display_itg_score_percent(state: &State, player_idx: usize) -> f64 {
    if player_idx >= state.num_players {
        return 0.0;
    }
    let carry = display_carry_for_player(state, player_idx);
    let mut scoring_counts = state.players[player_idx].scoring_counts;
    for (ix, total) in scoring_counts.iter_mut().enumerate() {
        *total = total.saturating_add(carry.scoring_counts[ix]);
    }
    let holds = state.players[player_idx]
        .holds_held_for_score
        .saturating_add(carry.holds_held_for_score);
    let rolls = state.players[player_idx]
        .rolls_held_for_score
        .saturating_add(carry.rolls_held_for_score);
    let mines = state.players[player_idx]
        .mines_hit_for_score
        .saturating_add(carry.mines_hit_for_score);
    let possible = display_totals_for_player(state, player_idx).possible_grade_points;
    judgment::calculate_itg_score_percent_from_counts(
        &scoring_counts,
        holds,
        rolls,
        mines,
        possible,
    )
}

#[inline(always)]
pub(super) fn effective_ex_score_inputs(
    player: &PlayerRuntime,
    live: ExScoreInputs,
) -> ExScoreInputs {
    player.failed_ex_score_inputs.unwrap_or(live)
}

#[inline(always)]
pub(super) fn capture_failed_ex_score_inputs(state: &mut State, player_idx: usize) {
    if player_idx >= state.num_players || player_idx >= MAX_PLAYERS {
        return;
    }
    let live = live_ex_score_inputs(state, player_idx);
    let player = &mut state.players[player_idx];
    if player.fail_time.is_none() || player.failed_ex_score_inputs.is_some() {
        return;
    }
    player.failed_ex_score_inputs = Some(live);
}

pub(crate) fn display_ex_score_data(state: &State, player_idx: usize) -> judgment::ExScoreData {
    if player_idx >= state.num_players {
        return judgment::ExScoreData::default();
    }
    ex_score_data_from_inputs(state, player_idx, live_ex_score_inputs(state, player_idx))
}

pub(crate) fn display_scored_ex_score_data(
    state: &State,
    player_idx: usize,
) -> judgment::ExScoreData {
    if player_idx >= state.num_players {
        return judgment::ExScoreData::default();
    }
    let live = live_ex_score_inputs(state, player_idx);
    let player = &state.players[player_idx];
    ex_score_data_from_inputs(state, player_idx, effective_ex_score_inputs(player, live))
}

pub fn display_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::ex_score_percent(&display_scored_ex_score_data(state, player_idx))
}

pub fn display_hard_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::hard_ex_score_percent(&display_scored_ex_score_data(state, player_idx))
}
