use crate::game::profile;
use deadsync_chart::{ChartData, GameplayChartData};
use deadsync_profile::{AttackMode, MeasureCounter, MiniIndicator, TargetScoreSetting};
use deadsync_rules::judgment;
use deadsync_rules::stream::{StreamSegment, zmod_stream_totals_full_measures};
use deadsync_rules::timing::WindowCounts;

use super::{
    CourseDisplayCarry, HOLDS_MASK_BIT_FLOORED, HOLDS_MASK_BIT_NO_ROLLS, HOLDS_MASK_BIT_PLANTED,
    HOLDS_MASK_BIT_TWISTER, INSERT_MASK_BIT_ECHO, MAX_PLAYERS, REMOVE_MASK_BIT_LITTLE,
    REMOVE_MASK_BIT_NO_FAKES, REMOVE_MASK_BIT_NO_HANDS, REMOVE_MASK_BIT_NO_HOLDS,
    REMOVE_MASK_BIT_NO_JUMPS, REMOVE_MASK_BIT_NO_LIFTS, REMOVE_MASK_BIT_NO_MINES,
    REMOVE_MASK_BIT_NO_QUADS, ScrollSpeedSetting, State,
};

#[inline(always)]
fn chart_has_attacks(chart: &ChartData) -> bool {
    chart.has_chart_attacks
}

#[inline(always)]
pub(crate) fn mini_indicator_mode(profile: &profile::Profile) -> MiniIndicator {
    if profile.mini_indicator != MiniIndicator::None {
        profile.mini_indicator
    } else if profile.subtractive_scoring {
        MiniIndicator::SubtractiveScoring
    } else if profile.pacemaker {
        MiniIndicator::Pacemaker
    } else {
        MiniIndicator::None
    }
}

#[inline(always)]
pub(crate) fn needs_stream_data(profile: &profile::Profile) -> bool {
    profile.measure_counter != MeasureCounter::None
        || mini_indicator_mode(profile) != MiniIndicator::None
}

#[inline(always)]
fn chart_stream_segments(
    gameplay_chart: &GameplayChartData,
    lanes: usize,
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let measure_densities = rssp::stats::measure_densities(&gameplay_chart.notes, lanes);
    zmod_stream_totals_full_measures(&measure_densities, constant_bpm)
}

pub fn stream_segments_for_results(state: &State, player: usize) -> Vec<StreamSegment> {
    if player >= state.num_players {
        return Vec::new();
    }
    if !state.mini_indicator_stream_segments[player].is_empty() {
        return state.mini_indicator_stream_segments[player].clone();
    }
    let constant_bpm = !state.timing_players[player].has_bpm_changes();
    let (segments, _, _) = chart_stream_segments(
        &state.gameplay_charts[player],
        state.cols_per_player,
        constant_bpm,
    );
    segments
}

pub fn score_invalid_reason_lines_for_chart(
    chart: &ChartData,
    profile: &profile::Profile,
    _scroll_speed: ScrollSpeedSetting,
    music_rate: f32,
) -> Vec<&'static str> {
    let mut reasons = Vec::with_capacity(6);
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    if rate < 1.0 {
        reasons.push("music rate is below 1.0x");
    }

    let remove_mask = profile.remove_active_mask.bits();
    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 && chart.stats.holds > 0 {
        reasons.push("No Holds is enabled on a chart with holds");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 && chart.mines_nonfake > 0 {
        reasons.push("No Mines is enabled on a chart with mines");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 && chart.stats.jumps > 0 {
        reasons.push("No Jumps is enabled on a chart with jumps");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Hands is enabled on a chart with hands");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Quads is enabled on a chart with quads");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 && chart.stats.lifts > 0 {
        reasons.push("No Lifts is enabled on a chart with lifts");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 && chart.stats.fakes > 0 {
        reasons.push("No Fakes is enabled on a chart with fakes");
    }

    let holds_mask = profile.holds_active_mask.bits();
    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 && chart.stats.rolls > 0 {
        reasons.push("No Rolls is enabled on a chart with rolls");
    }

    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        reasons.push("Little is enabled");
    }

    let insert_mask = profile.insert_active_mask.bits();
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        reasons.push("Echo is enabled");
    }

    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        reasons.push("Planted is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        reasons.push("Floored is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        reasons.push("Twister is enabled");
    }

    match profile.attack_mode {
        AttackMode::Off => {
            if chart_has_attacks(chart) {
                reasons.push("AttackMode=Off is enabled on a chart with attacks");
            }
        }
        AttackMode::On => {}
        AttackMode::Random => reasons.push("AttackMode=Random is enabled"),
    }

    reasons
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTotals {
    pub possible_grade_points: i32,
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
}

pub fn course_display_totals_for_chart(chart: &ChartData) -> CourseDisplayTotals {
    CourseDisplayTotals {
        possible_grade_points: chart.possible_grade_points,
        total_steps: chart.stats.total_steps,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
    }
}

#[inline(always)]
pub(crate) fn target_score_setting_percent(setting: TargetScoreSetting) -> Option<f64> {
    match setting {
        TargetScoreSetting::CMinus => Some(50.0),
        TargetScoreSetting::C => Some(55.0),
        TargetScoreSetting::CPlus => Some(60.0),
        TargetScoreSetting::BMinus => Some(64.0),
        TargetScoreSetting::B => Some(68.0),
        TargetScoreSetting::BPlus => Some(72.0),
        TargetScoreSetting::AMinus => Some(76.0),
        TargetScoreSetting::A => Some(80.0),
        TargetScoreSetting::APlus => Some(83.0),
        TargetScoreSetting::SMinus => Some(86.0),
        TargetScoreSetting::S => Some(89.0),
        TargetScoreSetting::SPlus => Some(92.0),
        TargetScoreSetting::MachineBest | TargetScoreSetting::PersonalBest => None,
    }
}

pub fn course_display_carry_from_state(state: &State) -> [CourseDisplayCarry; MAX_PLAYERS] {
    let mut carry = [CourseDisplayCarry::default(); MAX_PLAYERS];
    for player in 0..state.num_players.min(MAX_PLAYERS) {
        let p = &state.players[player];
        let previous = state
            .course_display_carry
            .as_ref()
            .map_or(CourseDisplayCarry::default(), |old| old[player]);
        let life = p.life.clamp(0.0, 1.0);
        let mut judgment_counts = [0u32; 6];
        let mut scoring_counts = [0u32; 6];
        for grade in judgment::DISPLAY_JUDGE_ORDER {
            let ix = judgment::display_judge_ix(grade);
            let stage_judgment = p.judgment_counts[ix];
            let stage_scoring = p.scoring_counts[ix];
            judgment_counts[ix] = previous.judgment_counts[ix].saturating_add(stage_judgment);
            scoring_counts[ix] = previous.scoring_counts[ix].saturating_add(stage_scoring);
        }
        let stage_window_counts = state.live_window_counts[player];
        let stage_window_counts_10ms = state.live_window_counts_10ms_blue[player];
        let stage_window_counts_display_blue = state.live_window_counts_display_blue[player];
        let first_fc_attempt_broken = previous.first_fc_attempt_broken || p.first_fc_attempt_broken;
        let full_combo_grade = if first_fc_attempt_broken {
            None
        } else {
            match (previous.full_combo_grade, p.full_combo_grade) {
                (Some(prev), Some(current)) => Some(prev.max(current)),
                (Some(prev), None) => Some(prev),
                (None, current) => current,
            }
        };
        let window_counts = WindowCounts {
            w0: previous
                .window_counts
                .w0
                .saturating_add(stage_window_counts.w0),
            w1: previous
                .window_counts
                .w1
                .saturating_add(stage_window_counts.w1),
            w2: previous
                .window_counts
                .w2
                .saturating_add(stage_window_counts.w2),
            w3: previous
                .window_counts
                .w3
                .saturating_add(stage_window_counts.w3),
            w4: previous
                .window_counts
                .w4
                .saturating_add(stage_window_counts.w4),
            w5: previous
                .window_counts
                .w5
                .saturating_add(stage_window_counts.w5),
            miss: previous
                .window_counts
                .miss
                .saturating_add(stage_window_counts.miss),
        };
        let window_counts_10ms_blue = WindowCounts {
            w0: previous
                .window_counts_10ms_blue
                .w0
                .saturating_add(stage_window_counts_10ms.w0),
            w1: previous
                .window_counts_10ms_blue
                .w1
                .saturating_add(stage_window_counts_10ms.w1),
            w2: previous
                .window_counts_10ms_blue
                .w2
                .saturating_add(stage_window_counts_10ms.w2),
            w3: previous
                .window_counts_10ms_blue
                .w3
                .saturating_add(stage_window_counts_10ms.w3),
            w4: previous
                .window_counts_10ms_blue
                .w4
                .saturating_add(stage_window_counts_10ms.w4),
            w5: previous
                .window_counts_10ms_blue
                .w5
                .saturating_add(stage_window_counts_10ms.w5),
            miss: previous
                .window_counts_10ms_blue
                .miss
                .saturating_add(stage_window_counts_10ms.miss),
        };
        let window_counts_display_blue = WindowCounts {
            w0: previous
                .window_counts_display_blue
                .w0
                .saturating_add(stage_window_counts_display_blue.w0),
            w1: previous
                .window_counts_display_blue
                .w1
                .saturating_add(stage_window_counts_display_blue.w1),
            w2: previous
                .window_counts_display_blue
                .w2
                .saturating_add(stage_window_counts_display_blue.w2),
            w3: previous
                .window_counts_display_blue
                .w3
                .saturating_add(stage_window_counts_display_blue.w3),
            w4: previous
                .window_counts_display_blue
                .w4
                .saturating_add(stage_window_counts_display_blue.w4),
            w5: previous
                .window_counts_display_blue
                .w5
                .saturating_add(stage_window_counts_display_blue.w5),
            miss: previous
                .window_counts_display_blue
                .miss
                .saturating_add(stage_window_counts_display_blue.miss),
        };
        carry[player] = CourseDisplayCarry {
            life,
            judgment_counts,
            scoring_counts,
            full_combo_grade,
            current_combo_grade: p.current_combo_grade,
            current_combo_window_counts: if p.combo > 0 {
                p.current_combo_window_counts
            } else {
                WindowCounts::default()
            },
            first_fc_attempt_broken,
            window_counts,
            window_counts_10ms_blue,
            window_counts_display_blue,
            holds_held: previous.holds_held.saturating_add(p.holds_held),
            rolls_held: previous.rolls_held.saturating_add(p.rolls_held),
            mines_avoided: previous.mines_avoided.saturating_add(p.mines_avoided),
            holds_held_for_score: previous
                .holds_held_for_score
                .saturating_add(p.holds_held_for_score),
            holds_let_go_for_score: previous
                .holds_let_go_for_score
                .saturating_add(p.holds_let_go_for_score),
            rolls_held_for_score: previous
                .rolls_held_for_score
                .saturating_add(p.rolls_held_for_score),
            rolls_let_go_for_score: previous
                .rolls_let_go_for_score
                .saturating_add(p.rolls_let_go_for_score),
            mines_hit_for_score: previous
                .mines_hit_for_score
                .saturating_add(p.mines_hit_for_score),
        };
    }
    if state.num_players == 1 {
        carry[1] = carry[0];
    }
    carry
}
