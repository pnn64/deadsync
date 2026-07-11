use crate::GameplayCoreState as State;
use deadsync_gameplay::PlayerRuntime;
use deadsync_notefield::{
    MiniIndicatorColorStyle, MiniIndicatorMode, MiniIndicatorProgress, MiniIndicatorScoreType,
    MiniIndicatorSize, MiniIndicatorSubtractiveDisplay, ZmodComboColorParams, ZmodComboColorStyle,
    ZmodMiniIndicatorParams, zmod_combo_quint_active as crate_zmod_combo_quint_active,
    zmod_mini_indicator_output, zmod_mini_indicator_zoom as crate_zmod_mini_indicator_zoom,
    zmod_percent_from_points, zmod_resolved_combo_color as crate_zmod_resolved_combo_color,
    zmod_resolved_mini_indicator_mode, zmod_static_combo_color as crate_zmod_static_combo_color,
    zmod_stream_prog_completion_for_beat,
};
use deadsync_profile as profile_data;
use deadsync_rules::judgment::{self, HOLD_SCORE_HELD, JudgeGrade};
use std::sync::Arc;

use super::player_blue_window_ms;
use super::text::cached_zmod_mini_indicator_text;

#[inline(always)]
pub(super) fn zmod_small_combo_font(combo_font: profile_data::ComboFont) -> &'static str {
    match combo_font {
        profile_data::ComboFont::Wendy | profile_data::ComboFont::WendyCursed => "wendy",
        profile_data::ComboFont::ArialRounded => "combo_arial_rounded",
        profile_data::ComboFont::Asap => "combo_asap",
        profile_data::ComboFont::BebasNeue => "combo_bebas_neue",
        profile_data::ComboFont::SourceCode => "combo_source_code",
        profile_data::ComboFont::Work => "combo_work",
        profile_data::ComboFont::Mega => "combo_mega",
        profile_data::ComboFont::None => "wendy",
    }
}

#[inline(always)]
pub(super) fn zmod_combo_font_name(combo_font: profile_data::ComboFont) -> Option<&'static str> {
    match combo_font {
        profile_data::ComboFont::Wendy => Some("wendy_combo"),
        profile_data::ComboFont::ArialRounded => Some("combo_arial_rounded"),
        profile_data::ComboFont::Asap => Some("combo_asap"),
        profile_data::ComboFont::BebasNeue => Some("combo_bebas_neue"),
        profile_data::ComboFont::SourceCode => Some("combo_source_code"),
        profile_data::ComboFont::Work => Some("combo_work"),
        profile_data::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
        profile_data::ComboFont::Mega => Some("combo_mega"),
        profile_data::ComboFont::None => None,
    }
}

#[inline(always)]
fn zmod_combo_quint_active(
    state: &State,
    player_idx: usize,
    profile: &profile_data::Profile,
) -> bool {
    if player_idx >= state.num_players() {
        return false;
    }
    let counts = if profile.combo_mode == profile_data::ComboMode::FullCombo {
        let blue_window_ms = player_blue_window_ms(state, player_idx);
        state.display_window_counts(player_idx, None, blue_window_ms)
    } else {
        state.players()[player_idx].current_combo_window_counts
    };
    crate_zmod_combo_quint_active(profile.show_fa_plus_window, counts)
}

#[inline(always)]
fn zmod_combo_color_style(colors: profile_data::ComboColors) -> ZmodComboColorStyle {
    match colors {
        profile_data::ComboColors::None => ZmodComboColorStyle::None,
        profile_data::ComboColors::Rainbow => ZmodComboColorStyle::Rainbow,
        profile_data::ComboColors::RainbowScroll => ZmodComboColorStyle::RainbowScroll,
        profile_data::ComboColors::Glow => ZmodComboColorStyle::Glow,
        profile_data::ComboColors::Solid => ZmodComboColorStyle::Solid,
    }
}

fn zmod_combo_color_params(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> ZmodComboColorParams {
    ZmodComboColorParams {
        style: zmod_combo_color_style(profile.combo_colors),
        full_combo_mode: profile.combo_mode == profile_data::ComboMode::FullCombo,
        combo: p.combo,
        full_combo_grade: p.full_combo_grade,
        current_combo_grade: p.current_combo_grade,
        quint_active: zmod_combo_quint_active(state, player_idx, profile),
        elapsed_s: state.total_elapsed_in_screen(),
    }
}

pub(super) fn zmod_resolved_combo_color(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> [f32; 4] {
    crate_zmod_resolved_combo_color(zmod_combo_color_params(state, p, profile, player_idx))
}

fn zmod_static_combo_color(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> [f32; 4] {
    crate_zmod_static_combo_color(zmod_combo_color_params(state, p, profile, player_idx))
}

fn zmod_mini_indicator_progress(
    state: &State,
    p: &PlayerRuntime,
    player_idx: usize,
    score_type: profile_data::MiniIndicatorScoreType,
) -> MiniIndicatorProgress {
    let w1 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Fantastic)];
    let w2 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)];
    let w3 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Great)];
    let w4 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Decent)];
    let w5 = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::WayOff)];
    let miss = p.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Miss)];

    let let_go = p
        .holds_let_go_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let mines_hit = p.mines_hit_for_score;
    let tap_rows = w1
        .saturating_add(w2)
        .saturating_add(w3)
        .saturating_add(w4)
        .saturating_add(w5)
        .saturating_add(miss);
    let resolved_holds = p
        .holds_held_for_score
        .saturating_add(p.holds_let_go_for_score);
    let resolved_rolls = p
        .rolls_held_for_score
        .saturating_add(p.rolls_let_go_for_score);
    let current_possible_dp = (tap_rows
        .saturating_add(resolved_holds)
        .saturating_add(resolved_rolls) as i32)
        .saturating_mul(HOLD_SCORE_HELD);

    let possible_dp = state
        .display_totals_for_player(player_idx)
        .possible_grade_points
        .max(1);
    let actual_dp = p.earned_grade_points;

    let (
        kept_percent,
        lost_percent,
        pace_percent,
        current_score_percent,
        current_possible_ratio,
        white_count,
        white_10ms_count,
    ) = match score_type {
        profile_data::MiniIndicatorScoreType::Itg => {
            let (kept, lost, pace) = judgment::predictive_itg_score_percents(
                current_possible_dp,
                possible_dp,
                actual_dp,
            );
            let current_score = zmod_percent_from_points(actual_dp, possible_dp);
            let current_possible_ratio =
                (f64::from(current_possible_dp.max(0)) / f64::from(possible_dp)).clamp(0.0, 1.0);
            (
                kept,
                lost,
                pace,
                current_score,
                current_possible_ratio,
                0,
                0,
            )
        }
        profile_data::MiniIndicatorScoreType::Ex | profile_data::MiniIndicatorScoreType::HardEx => {
            let blue_window_ms = player_blue_window_ms(state, player_idx);
            let score = state.display_scored_ex_score_data(player_idx, blue_window_ms);
            let white_count = score.counts.w1;
            let fantastic_total = score.counts.w0.saturating_add(score.counts.w1);
            let white_10ms_count = fantastic_total.saturating_sub(score.counts_10ms.w0);
            let current_possible_ratio = judgment::ex_current_possible_ratio(&score);
            if score_type == profile_data::MiniIndicatorScoreType::Ex {
                let (kept, lost, pace) = judgment::predictive_ex_score_percents(&score);
                (
                    kept,
                    lost,
                    pace,
                    judgment::ex_score_percent(&score),
                    current_possible_ratio,
                    white_count,
                    white_10ms_count,
                )
            } else {
                let (kept, lost, pace) = judgment::predictive_hard_ex_score_percents(&score);
                (
                    kept,
                    lost,
                    pace,
                    judgment::hard_ex_score_percent(&score),
                    current_possible_ratio,
                    white_count,
                    white_10ms_count,
                )
            }
        }
    };

    let judged_any = tap_rows > 0 || let_go > 0 || mines_hit > 0 || p.is_failing || p.life <= 0.0;
    MiniIndicatorProgress {
        kept_percent,
        lost_percent,
        pace_percent,
        current_score_percent,
        current_possible_ratio,
        current_possible_dp,
        actual_dp,
        white_count,
        white_10ms_count,
        w2,
        w3,
        w4,
        w5,
        miss,
        let_go,
        mines_hit,
        judged_any,
    }
}

#[inline(always)]
fn mini_indicator_score_type(
    score_type: profile_data::MiniIndicatorScoreType,
) -> MiniIndicatorScoreType {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => MiniIndicatorScoreType::Itg,
        profile_data::MiniIndicatorScoreType::Ex => MiniIndicatorScoreType::Ex,
        profile_data::MiniIndicatorScoreType::HardEx => MiniIndicatorScoreType::HardEx,
    }
}

#[inline(always)]
fn mini_indicator_mode(mode: profile_data::MiniIndicator) -> MiniIndicatorMode {
    match mode {
        profile_data::MiniIndicator::None => MiniIndicatorMode::None,
        profile_data::MiniIndicator::SubtractiveScoring => MiniIndicatorMode::SubtractiveScoring,
        profile_data::MiniIndicator::PredictiveScoring => MiniIndicatorMode::PredictiveScoring,
        profile_data::MiniIndicator::PaceScoring => MiniIndicatorMode::PaceScoring,
        profile_data::MiniIndicator::RivalScoring => MiniIndicatorMode::RivalScoring,
        profile_data::MiniIndicator::Pacemaker => MiniIndicatorMode::Pacemaker,
        profile_data::MiniIndicator::StreamProg => MiniIndicatorMode::StreamProg,
    }
}

#[inline(always)]
pub(super) fn zmod_indicator_mode(profile: &profile_data::Profile) -> MiniIndicatorMode {
    zmod_resolved_mini_indicator_mode(
        mini_indicator_mode(profile.mini_indicator),
        profile.subtractive_scoring,
        profile.pacemaker,
    )
}

#[inline(always)]
fn mini_indicator_color_style(style: profile_data::MiniIndicatorColor) -> MiniIndicatorColorStyle {
    match style {
        profile_data::MiniIndicatorColor::Default => MiniIndicatorColorStyle::Default,
        profile_data::MiniIndicatorColor::Detailed => MiniIndicatorColorStyle::Detailed,
        profile_data::MiniIndicatorColor::Combo => MiniIndicatorColorStyle::Combo,
    }
}

#[inline(always)]
fn mini_indicator_subtractive_display(
    display: profile_data::MiniIndicatorSubtractiveDisplay,
) -> MiniIndicatorSubtractiveDisplay {
    match display {
        profile_data::MiniIndicatorSubtractiveDisplay::Percent => {
            MiniIndicatorSubtractiveDisplay::CountThenPercent
        }
        profile_data::MiniIndicatorSubtractiveDisplay::Points => {
            MiniIndicatorSubtractiveDisplay::Points
        }
    }
}

#[inline(always)]
pub(super) fn zmod_mini_indicator_zoom(size: profile_data::MiniIndicatorSize) -> f32 {
    let size = match size {
        profile_data::MiniIndicatorSize::Default => MiniIndicatorSize::Default,
        profile_data::MiniIndicatorSize::Large => MiniIndicatorSize::Large,
    };
    crate_zmod_mini_indicator_zoom(size)
}

fn zmod_stream_prog_completion(state: &State, player_idx: usize) -> Option<f64> {
    let total_stream = state.mini_indicator_total_stream_measures(player_idx) as f64;
    let segs = state.mini_indicator_stream_segments(player_idx);
    let beat_floor = state.visible_beat(player_idx).floor();
    zmod_stream_prog_completion_for_beat(total_stream, segs, beat_floor)
}

pub(super) fn zmod_mini_indicator_text(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
) -> Option<(Arc<str>, [f32; 4])> {
    let mode = zmod_indicator_mode(profile);
    let progress =
        zmod_mini_indicator_progress(state, p, player_idx, profile.mini_indicator_score_type);
    let output = zmod_mini_indicator_output(
        &progress,
        ZmodMiniIndicatorParams {
            mode,
            color_style: mini_indicator_color_style(profile.mini_indicator_color),
            subtractive_display: mini_indicator_subtractive_display(
                profile.mini_indicator_subtractive_display,
            ),
            score_type: mini_indicator_score_type(profile.mini_indicator_score_type),
            combo_color: zmod_static_combo_color(state, p, profile, player_idx),
            is_failing: p.is_failing,
            life: p.life,
            rival_score_percent: state.mini_indicator_rival_score_percent(player_idx),
            target_score_percent: state.mini_indicator_target_score_percent(player_idx),
            stream_completion: zmod_stream_prog_completion(state, player_idx),
        },
    )?;
    Some((cached_zmod_mini_indicator_text(output.text), output.color))
}
