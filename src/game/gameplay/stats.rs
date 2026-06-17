use deadsync_chart::ChartData;
use deadsync_gameplay::{
    GameplayMiniIndicatorMode, GameplayMiniIndicatorOptions, GameplayTargetScoreSetting,
    ScoreValidityOptions, mini_indicator_mode_for_options, mini_indicator_needs_stream_data,
    score_invalid_reason_lines_for_options, stream_segments_for_note_data,
};
use deadsync_profile::{MeasureCounter, MiniIndicator, TargetScoreSetting};
use deadsync_rules::stream::StreamSegment;

use super::{
    CourseDisplayCarry, CourseDisplayStage, MAX_PLAYERS, ScrollSpeedSetting, State,
    course_display_carry_for_stage,
};

#[inline(always)]
fn gameplay_mini_indicator_mode(mode: MiniIndicator) -> GameplayMiniIndicatorMode {
    match mode {
        MiniIndicator::None => GameplayMiniIndicatorMode::None,
        MiniIndicator::SubtractiveScoring => GameplayMiniIndicatorMode::SubtractiveScoring,
        MiniIndicator::PredictiveScoring => GameplayMiniIndicatorMode::PredictiveScoring,
        MiniIndicator::PaceScoring => GameplayMiniIndicatorMode::PaceScoring,
        MiniIndicator::RivalScoring => GameplayMiniIndicatorMode::RivalScoring,
        MiniIndicator::Pacemaker => GameplayMiniIndicatorMode::Pacemaker,
        MiniIndicator::StreamProg => GameplayMiniIndicatorMode::StreamProg,
    }
}

#[inline(always)]
fn profile_mini_indicator_mode(mode: GameplayMiniIndicatorMode) -> MiniIndicator {
    match mode {
        GameplayMiniIndicatorMode::None => MiniIndicator::None,
        GameplayMiniIndicatorMode::SubtractiveScoring => MiniIndicator::SubtractiveScoring,
        GameplayMiniIndicatorMode::PredictiveScoring => MiniIndicator::PredictiveScoring,
        GameplayMiniIndicatorMode::PaceScoring => MiniIndicator::PaceScoring,
        GameplayMiniIndicatorMode::RivalScoring => MiniIndicator::RivalScoring,
        GameplayMiniIndicatorMode::Pacemaker => MiniIndicator::Pacemaker,
        GameplayMiniIndicatorMode::StreamProg => MiniIndicator::StreamProg,
    }
}

#[inline(always)]
fn mini_indicator_options(profile: &deadsync_profile::Profile) -> GameplayMiniIndicatorOptions {
    GameplayMiniIndicatorOptions {
        requested_mode: gameplay_mini_indicator_mode(profile.mini_indicator),
        measure_counter_enabled: profile.measure_counter != MeasureCounter::None,
        subtractive_scoring: profile.subtractive_scoring,
        pacemaker: profile.pacemaker,
    }
}

#[inline(always)]
pub(crate) fn mini_indicator_mode(profile: &deadsync_profile::Profile) -> MiniIndicator {
    profile_mini_indicator_mode(mini_indicator_mode_for_options(mini_indicator_options(
        profile,
    )))
}

#[inline(always)]
pub(crate) fn needs_stream_data(profile: &deadsync_profile::Profile) -> bool {
    mini_indicator_needs_stream_data(mini_indicator_options(profile))
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
