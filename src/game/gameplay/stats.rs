use deadsync_chart::ChartData;
use deadsync_gameplay::{
    GameplayTargetScoreSetting, ScoreValidityOptions, score_invalid_reason_lines_for_options,
    stream_segments_for_note_data,
};
use deadsync_profile::{MeasureCounter, MiniIndicator, TargetScoreSetting};
use deadsync_rules::stream::StreamSegment;

use super::{
    CourseDisplayCarry, CourseDisplayStage, MAX_PLAYERS, ScrollSpeedSetting, State,
    course_display_carry_for_stage,
};

#[inline(always)]
pub(crate) fn mini_indicator_mode(profile: &deadsync_profile::Profile) -> MiniIndicator {
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
pub(crate) fn needs_stream_data(profile: &deadsync_profile::Profile) -> bool {
    profile.measure_counter != MeasureCounter::None
        || mini_indicator_mode(profile) != MiniIndicator::None
}

pub fn stream_segments_for_results(state: &State, player: usize) -> Vec<StreamSegment> {
    if player >= state.num_players {
        return Vec::new();
    }
    if !state.mini_indicator_stream_segments[player].is_empty() {
        return state.mini_indicator_stream_segments[player].clone();
    }
    let constant_bpm = !state.timing_players[player].has_bpm_changes();
    let (segments, _, _) = stream_segments_for_note_data(
        &state.gameplay_charts[player].notes,
        state.cols_per_player,
        constant_bpm,
    );
    segments
}

pub fn score_invalid_reason_lines_for_chart(
    chart: &ChartData,
    profile: &deadsync_profile::Profile,
    _scroll_speed: ScrollSpeedSetting,
    music_rate: f32,
) -> Vec<&'static str> {
    score_invalid_reason_lines_for_options(
        chart,
        ScoreValidityOptions {
            chart_effects: super::chart_effects_from_profile(profile),
            attack_mode: super::gameplay_attack_mode(profile.attack_mode),
            music_rate,
        },
    )
}

#[inline(always)]
pub(crate) fn gameplay_target_score_setting(
    setting: TargetScoreSetting,
) -> GameplayTargetScoreSetting {
    match setting {
        TargetScoreSetting::CMinus => GameplayTargetScoreSetting::CMinus,
        TargetScoreSetting::C => GameplayTargetScoreSetting::C,
        TargetScoreSetting::CPlus => GameplayTargetScoreSetting::CPlus,
        TargetScoreSetting::BMinus => GameplayTargetScoreSetting::BMinus,
        TargetScoreSetting::B => GameplayTargetScoreSetting::B,
        TargetScoreSetting::BPlus => GameplayTargetScoreSetting::BPlus,
        TargetScoreSetting::AMinus => GameplayTargetScoreSetting::AMinus,
        TargetScoreSetting::A => GameplayTargetScoreSetting::A,
        TargetScoreSetting::APlus => GameplayTargetScoreSetting::APlus,
        TargetScoreSetting::SMinus => GameplayTargetScoreSetting::SMinus,
        TargetScoreSetting::S => GameplayTargetScoreSetting::S,
        TargetScoreSetting::SPlus => GameplayTargetScoreSetting::SPlus,
        TargetScoreSetting::MachineBest => GameplayTargetScoreSetting::MachineBest,
        TargetScoreSetting::PersonalBest => GameplayTargetScoreSetting::PersonalBest,
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
        carry[player] = course_display_carry_for_stage(
            previous,
            CourseDisplayStage {
                life: p.life,
                judgment_counts: p.judgment_counts,
                scoring_counts: p.scoring_counts,
                full_combo_grade: p.full_combo_grade,
                current_combo_grade: p.current_combo_grade,
                current_combo_window_counts: p.current_combo_window_counts,
                combo: p.combo,
                first_fc_attempt_broken: p.first_fc_attempt_broken,
                window_counts: state.live_window_counts[player],
                window_counts_10ms_blue: state.live_window_counts_10ms_blue[player],
                window_counts_display_blue: state.live_window_counts_display_blue[player],
                holds_held: p.holds_held,
                rolls_held: p.rolls_held,
                mines_avoided: p.mines_avoided,
                holds_held_for_score: p.holds_held_for_score,
                holds_let_go_for_score: p.holds_let_go_for_score,
                rolls_held_for_score: p.rolls_held_for_score,
                rolls_let_go_for_score: p.rolls_let_go_for_score,
                mines_hit_for_score: p.mines_hit_for_score,
            },
        );
    }
    if state.num_players == 1 {
        carry[1] = carry[0];
    }
    carry
}
