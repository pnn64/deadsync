use deadsync_gameplay::{
    capture_failed_ex_score_inputs as gameplay_capture_failed_ex_score_inputs,
    effective_ex_score_inputs as gameplay_effective_ex_score_inputs,
    record_display_window_counts_for_judgment,
};
use deadsync_profile::ScoreDisplayMode;
use deadsync_rules::judgment::{self, JudgeGrade, Judgment};
use deadsync_rules::timing::{self, WindowCounts};

use super::{
    CourseDisplayCarry, CourseDisplayTotals, DisplayWindowCountsMode, DisplayWindowCountsSources,
    ExScoreInputs, GameplayScoreDisplayMode, ItgScoreInputs, ItgScoreStage, MAX_PLAYERS, State,
    course_display_carry_for_player, course_display_totals_for_player,
    display_ex_score_percent_for_mode, display_hard_ex_score_percent_for_mode,
    display_itg_score_percent_for_mode, display_judgment_count_for_grade,
    display_window_counts_for_notes, display_window_counts_with_carry,
    ex_score_data_from_display_inputs, ex_score_inputs_from_display, itg_score_inputs_from_display,
    itg_score_percent_from_inputs, player_blue_window_ms, predictive_itg_score_percent_from_inputs,
};

#[inline(always)]
fn gameplay_score_display_mode(mode: ScoreDisplayMode) -> GameplayScoreDisplayMode {
    match mode {
        ScoreDisplayMode::Normal => GameplayScoreDisplayMode::Normal,
        ScoreDisplayMode::Predictive => GameplayScoreDisplayMode::Predictive,
    }
}

#[inline(always)]
fn display_window_counts_10ms(state: &State, player_idx: usize) -> WindowCounts {
    if player_idx >= state.num_players {
        return WindowCounts::default();
    }
    let current = state.live_window_counts_10ms_blue[player_idx];
    let carry = display_carry_for_player(state, player_idx);
    display_window_counts_with_carry(current, carry, DisplayWindowCountsMode::TenMsBlue)
}

#[inline(always)]
fn display_score_stage(state: &State, player_idx: usize) -> ItgScoreStage {
    let player = &state.players[player_idx];
    ItgScoreStage {
        scoring_counts: player.scoring_counts,
        holds_held_for_score: player.holds_held_for_score,
        holds_let_go_for_score: player.holds_let_go_for_score,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_let_go_for_score: player.rolls_let_go_for_score,
        mines_hit_for_score: player.mines_hit_for_score,
    }
}

#[inline(always)]
fn live_ex_score_inputs(state: &State, player_idx: usize) -> ExScoreInputs {
    ex_score_inputs_from_display(
        display_window_counts(state, player_idx, None),
        display_window_counts_10ms(state, player_idx),
        display_score_stage(state, player_idx),
    )
}

#[inline(always)]
fn ex_score_data_from_inputs(
    state: &State,
    player_idx: usize,
    inputs: ExScoreInputs,
) -> judgment::ExScoreData {
    let carry = display_carry_for_player(state, player_idx);
    let totals = display_totals_for_player(state, player_idx);
    ex_score_data_from_display_inputs(inputs, carry, totals)
}

#[inline(always)]
pub fn display_carry_for_player(state: &State, player_idx: usize) -> CourseDisplayCarry {
    course_display_carry_for_player(state.course_display_carry.as_ref(), player_idx)
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
    record_display_window_counts_for_judgment(
        &mut state.live_window_counts[player_idx],
        &mut state.live_window_counts_10ms_blue[player_idx],
        &mut state.live_window_counts_display_blue[player_idx],
        judgment,
        display_window_ms,
    );
}

#[inline(always)]
pub fn display_totals_for_player(state: &State, player_idx: usize) -> CourseDisplayTotals {
    course_display_totals_for_player(
        state.course_display_totals.as_ref(),
        &state.possible_grade_points,
        &state.total_steps,
        &state.holds_total,
        &state.rolls_total,
        &state.mines_total,
        player_idx,
    )
}

pub fn display_judgment_count(state: &State, player_idx: usize, grade: JudgeGrade) -> u32 {
    if player_idx >= state.num_players {
        return 0;
    }
    let carry = display_carry_for_player(state, player_idx);
    display_judgment_count_for_grade(state.players[player_idx].judgment_counts, carry, grade)
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
    let sources = DisplayWindowCountsSources {
        canonical: state.live_window_counts[player_idx],
        ten_ms_blue: state.live_window_counts_10ms_blue[player_idx],
        display_blue: state.live_window_counts_display_blue[player_idx],
    };
    let (start, end) = state.note_ranges[player_idx];
    let end = end.min(state.notes.len());
    let notes = if start < end {
        &state.notes[start..end]
    } else {
        &[]
    };
    display_window_counts_for_notes(
        sources,
        display_carry_for_player(state, player_idx),
        notes,
        blue_window_ms,
        player_blue_window_ms(state, player_idx),
    )
}

pub fn display_itg_score_percent(state: &State, player_idx: usize) -> f64 {
    display_itg_score_inputs(state, player_idx).map_or(0.0, itg_score_percent_from_inputs)
}

fn display_itg_score_inputs(state: &State, player_idx: usize) -> Option<ItgScoreInputs> {
    if player_idx >= state.num_players {
        return None;
    }
    let carry = display_carry_for_player(state, player_idx);
    Some(itg_score_inputs_from_display(
        display_score_stage(state, player_idx),
        carry,
        display_totals_for_player(state, player_idx),
    ))
}

pub fn display_predictive_itg_score_percent(state: &State, player_idx: usize) -> f64 {
    display_itg_score_inputs(state, player_idx)
        .map_or(0.0, predictive_itg_score_percent_from_inputs)
}

pub fn display_gameplay_itg_score_percent(
    state: &State,
    player_idx: usize,
    mode: ScoreDisplayMode,
) -> f64 {
    display_itg_score_inputs(state, player_idx).map_or(0.0, |inputs| {
        display_itg_score_percent_for_mode(inputs, gameplay_score_display_mode(mode))
    })
}

#[inline(always)]
pub(super) fn capture_failed_ex_score_inputs(state: &mut State, player_idx: usize) {
    if player_idx >= state.num_players || player_idx >= MAX_PLAYERS {
        return;
    }
    let live = live_ex_score_inputs(state, player_idx);
    let player = &mut state.players[player_idx];
    gameplay_capture_failed_ex_score_inputs(
        &mut player.failed_ex_score_inputs,
        player.fail_time,
        live,
    );
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
    let inputs = gameplay_effective_ex_score_inputs(live, player.failed_ex_score_inputs);
    ex_score_data_from_inputs(state, player_idx, inputs)
}

pub fn display_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::ex_score_percent(&display_scored_ex_score_data(state, player_idx))
}

pub fn display_gameplay_ex_score_percent(
    state: &State,
    player_idx: usize,
    mode: ScoreDisplayMode,
) -> f64 {
    let score = display_scored_ex_score_data(state, player_idx);
    display_ex_score_percent_for_mode(&score, gameplay_score_display_mode(mode))
}

pub fn display_hard_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::hard_ex_score_percent(&display_scored_ex_score_data(state, player_idx))
}

pub fn display_gameplay_hard_ex_score_percent(
    state: &State,
    player_idx: usize,
    mode: ScoreDisplayMode,
) -> f64 {
    let score = display_scored_ex_score_data(state, player_idx);
    display_hard_ex_score_percent_for_mode(&score, gameplay_score_display_mode(mode))
}
