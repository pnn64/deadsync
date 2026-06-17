use deadsync_gameplay::{
    capture_failed_ex_score_inputs as gameplay_capture_failed_ex_score_inputs,
    effective_ex_score_inputs as gameplay_effective_ex_score_inputs,
};
use deadsync_profile::ScoreDisplayMode;
use deadsync_rules::judgment::{self, JudgeGrade, Judgment};
use deadsync_rules::timing::{self, WindowCounts};

use super::{
    CourseDisplayCarry, CourseDisplayTotals, DisplayWindowCountsMode, DisplayWindowCountsSources,
    ExScoreInputs, GameplayScoreDisplayMode, ItgScoreInputs, ItgScoreStage, MAX_PLAYERS,
    PlayerRuntime, State, display_ex_score_percent_for_mode,
    display_hard_ex_score_percent_for_mode, display_itg_score_percent_for_mode,
    display_judgment_count_for_grade, display_window_counts_current, display_window_counts_mode,
    display_window_counts_with_carry, ex_score_data_from_display_inputs,
    itg_score_inputs_from_display, itg_score_percent_from_inputs, player_blue_window_ms,
    predictive_itg_score_percent_from_inputs,
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
    ex_score_data_from_display_inputs(inputs, carry, totals)
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
    judgment::add_judgment_to_window_counts(
        &mut state.live_window_counts[player_idx],
        judgment,
        timing::FA_PLUS_W0_MS,
    );
    judgment::add_judgment_to_window_counts(
        &mut state.live_window_counts_10ms_blue[player_idx],
        judgment,
        timing::FA_PLUS_W010_MS,
    );
    judgment::add_judgment_to_window_counts(
        &mut state.live_window_counts_display_blue[player_idx],
        judgment,
        display_window_ms,
    );
}

#[inline(always)]
pub(super) fn record_current_combo_window_count(player: &mut PlayerRuntime, judgment: &Judgment) {
    judgment::add_judgment_to_window_counts(
        &mut player.current_combo_window_counts,
        judgment,
        timing::FA_PLUS_W0_MS,
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
    let mode = display_window_counts_mode(blue_window_ms, player_blue_window_ms(state, player_idx));
    let sources = DisplayWindowCountsSources {
        canonical: state.live_window_counts[player_idx],
        ten_ms_blue: state.live_window_counts_10ms_blue[player_idx],
        display_blue: state.live_window_counts_display_blue[player_idx],
    };

    let current = match display_window_counts_current(sources, mode) {
        Some(counts) => counts,
        None => {
            let split_ms = match mode {
                DisplayWindowCountsMode::CustomBlue { split_ms } => split_ms,
                _ => return WindowCounts::default(),
            };
            let (start, end) = state.note_ranges[player_idx];
            timing::compute_window_counts_blue_ms(&state.notes[start..end], split_ms)
        }
    };
    display_window_counts_with_carry(current, display_carry_for_player(state, player_idx), mode)
}

pub fn display_itg_score_percent(state: &State, player_idx: usize) -> f64 {
    display_itg_score_inputs(state, player_idx).map_or(0.0, itg_score_percent_from_inputs)
}

fn display_itg_score_inputs(state: &State, player_idx: usize) -> Option<ItgScoreInputs> {
    if player_idx >= state.num_players {
        return None;
    }
    let carry = display_carry_for_player(state, player_idx);
    let player = &state.players[player_idx];
    Some(itg_score_inputs_from_display(
        ItgScoreStage {
            scoring_counts: player.scoring_counts,
            holds_held_for_score: player.holds_held_for_score,
            holds_let_go_for_score: player.holds_let_go_for_score,
            rolls_held_for_score: player.rolls_held_for_score,
            rolls_let_go_for_score: player.rolls_let_go_for_score,
            mines_hit_for_score: player.mines_hit_for_score,
        },
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
pub(super) fn effective_ex_score_inputs(
    player: &PlayerRuntime,
    live: ExScoreInputs,
) -> ExScoreInputs {
    gameplay_effective_ex_score_inputs(live, player.failed_ex_score_inputs)
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
    ex_score_data_from_inputs(state, player_idx, effective_ex_score_inputs(player, live))
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
