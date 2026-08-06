use crate::screens::gameplay::GameplayCoreState as State;
use deadlib_present::actors::TextContent;
use deadsync_gameplay::PlayerRuntime;
#[cfg(feature = "bench-support")]
use deadsync_notefield::ZmodMiniIndicatorText;
use deadsync_notefield::{
    MiniIndicatorColorStyle, MiniIndicatorMode, MiniIndicatorProgress, MiniIndicatorScoreType,
    MiniIndicatorSize, MiniIndicatorSubtractiveDisplay, StreamProgressLookup, ZmodComboColorParams,
    ZmodComboColorStyle, ZmodMiniIndicatorParams,
    zmod_combo_quint_active as crate_zmod_combo_quint_active, zmod_mini_indicator_output,
    zmod_mini_indicator_zoom as crate_zmod_mini_indicator_zoom, zmod_percent_from_points,
    zmod_resolved_combo_color as crate_zmod_resolved_combo_color,
    zmod_resolved_mini_indicator_mode, zmod_static_combo_color as crate_zmod_static_combo_color,
    zmod_target_score_missed as crate_zmod_target_score_missed,
};
use deadsync_profile as profile_data;
use deadsync_rules::judgment::{self, HOLD_SCORE_HELD, JudgeGrade};

use super::player_blue_window_ms;
use super::text::zmod_mini_indicator_text_content;

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
    mode: MiniIndicatorMode,
    needs_target_score: bool,
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
    let needs_predictive = matches!(
        mode,
        MiniIndicatorMode::SubtractiveScoring
            | MiniIndicatorMode::PredictiveScoring
            | MiniIndicatorMode::PaceScoring
    );
    let needs_current_score = needs_target_score
        || matches!(
            mode,
            MiniIndicatorMode::RivalScoring | MiniIndicatorMode::Pacemaker
        );
    let needs_subtractive_counts = mode == MiniIndicatorMode::SubtractiveScoring;

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
            let (kept, lost, pace) = if needs_predictive {
                judgment::predictive_itg_score_percents(current_possible_dp, possible_dp, actual_dp)
            } else {
                (0.0, 0.0, 0.0)
            };
            let (current_score, current_possible_ratio) = if needs_current_score {
                (
                    zmod_percent_from_points(actual_dp, possible_dp),
                    (f64::from(current_possible_dp.max(0)) / f64::from(possible_dp))
                        .clamp(0.0, 1.0),
                )
            } else {
                (0.0, 0.0)
            };
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
            if !(needs_predictive || needs_current_score || needs_subtractive_counts) {
                (0.0, 0.0, 0.0, 0.0, 0.0, 0, 0)
            } else {
                let blue_window_ms = player_blue_window_ms(state, player_idx);
                let score = state.display_scored_ex_score_data(player_idx, blue_window_ms);
                let (white_count, white_10ms_count) = if needs_subtractive_counts {
                    let fantastic_total = score.counts.w0.saturating_add(score.counts.w1);
                    (
                        score.counts.w1,
                        fantastic_total.saturating_sub(score.counts_10ms.w0),
                    )
                } else {
                    (0, 0)
                };
                let current_possible_ratio = if needs_current_score {
                    judgment::ex_current_possible_ratio(&score)
                } else {
                    0.0
                };
                let current_score = if !needs_current_score {
                    0.0
                } else if score_type == profile_data::MiniIndicatorScoreType::Ex {
                    judgment::ex_score_percent(&score)
                } else {
                    judgment::hard_ex_score_percent(&score)
                };
                let (kept, lost, pace) = if !needs_predictive {
                    (0.0, 0.0, 0.0)
                } else if score_type == profile_data::MiniIndicatorScoreType::Ex {
                    judgment::predictive_ex_score_percents(&score)
                } else {
                    judgment::predictive_hard_ex_score_percents(&score)
                };
                (
                    kept,
                    lost,
                    pace,
                    current_score,
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

pub(crate) fn zmod_target_score_missed(
    state: &State,
    player_idx: usize,
    score_type: profile_data::MiniIndicatorScoreType,
    target_score_percent: f64,
) -> bool {
    let Some(player) = state.player(player_idx) else {
        return false;
    };
    crate_zmod_target_score_missed(
        &zmod_mini_indicator_progress(
            state,
            player,
            player_idx,
            score_type,
            MiniIndicatorMode::None,
            true,
        ),
        target_score_percent,
    )
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
fn mini_indicator_output_if_enabled<T>(
    mode: MiniIndicatorMode,
    build: impl FnOnce() -> Option<T>,
) -> Option<T> {
    if mode == MiniIndicatorMode::None {
        None
    } else {
        build()
    }
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

fn zmod_stream_prog_completion(
    state: &State,
    player_idx: usize,
    lookup: &StreamProgressLookup,
) -> Option<f64> {
    let total_stream = state.mini_indicator_total_stream_measures(player_idx) as f64;
    let beat_floor = state.visible_beat(player_idx).floor();
    lookup.completion_for_beat(total_stream, beat_floor)
}

pub(super) fn zmod_mini_indicator_text(
    state: &State,
    p: &PlayerRuntime,
    profile: &profile_data::Profile,
    player_idx: usize,
    stream_progress_lookup: &StreamProgressLookup,
) -> Option<(TextContent, [f32; 4])> {
    let mode = zmod_indicator_mode(profile);
    mini_indicator_output_if_enabled(mode, || {
        let dims_missed_target = profile.target_score_miss_policy
            == profile_data::TargetScoreMissPolicy::DimMiniIndicator;
        let progress = zmod_mini_indicator_progress(
            state,
            p,
            player_idx,
            profile.mini_indicator_score_type,
            mode,
            dims_missed_target,
        );
        let color_style = mini_indicator_color_style(profile.mini_indicator_color);
        let target_score_percent = if mode == MiniIndicatorMode::Pacemaker || dims_missed_target {
            state.mini_indicator_target_score_percent(player_idx)
        } else {
            0.0
        };
        let output = zmod_mini_indicator_output(
            &progress,
            ZmodMiniIndicatorParams {
                mode,
                color_style,
                subtractive_display: mini_indicator_subtractive_display(
                    profile.mini_indicator_subtractive_display,
                ),
                score_type: mini_indicator_score_type(profile.mini_indicator_score_type),
                combo_color: if color_style == MiniIndicatorColorStyle::Combo
                    && mode != MiniIndicatorMode::StreamProg
                {
                    zmod_static_combo_color(state, p, profile, player_idx)
                } else {
                    [1.0; 4]
                },
                is_failing: p.is_failing,
                life: p.life,
                rival_score_percent: if mode == MiniIndicatorMode::RivalScoring {
                    state.mini_indicator_rival_score_percent(player_idx)
                } else {
                    0.0
                },
                target_score_percent,
                stream_completion: if mode == MiniIndicatorMode::StreamProg {
                    zmod_stream_prog_completion(state, player_idx, stream_progress_lookup)
                } else {
                    None
                },
            },
        )?;
        let mut color = output.color;
        if dims_missed_target && crate_zmod_target_score_missed(&progress, target_score_percent) {
            color[3] *= 0.65;
        }
        Some((zmod_mini_indicator_text_content(output.text), color))
    })
}

#[cfg(feature = "bench-support")]
fn disabled_mini_indicator_bench_work(frame: usize) -> Option<u64> {
    let current_possible_dp = 2_000_i32.saturating_add(frame as i32 & 1_023);
    let possible_dp = 20_000;
    let actual_dp = current_possible_dp.saturating_sub((frame as i32 & 255) * 2);
    let (kept_percent, lost_percent, pace_percent) =
        judgment::predictive_itg_score_percents(current_possible_dp, possible_dp, actual_dp);
    let progress = MiniIndicatorProgress {
        kept_percent,
        lost_percent,
        pace_percent,
        current_score_percent: zmod_percent_from_points(actual_dp, possible_dp),
        current_possible_ratio: f64::from(current_possible_dp) / f64::from(possible_dp),
        current_possible_dp,
        actual_dp,
        judged_any: true,
        ..MiniIndicatorProgress::default()
    };
    let output = zmod_mini_indicator_output(
        &progress,
        ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::None,
            color_style: MiniIndicatorColorStyle::Detailed,
            subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
            score_type: MiniIndicatorScoreType::Itg,
            combo_color: [1.0; 4],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 95.0,
            target_score_percent: 90.0,
            stream_completion: Some(0.5),
        },
    );
    std::hint::black_box(output.map(|value| u64::from(value.color[3].to_bits())))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct PacemakerFrameBench {
    stream_lookup: StreamProgressLookup,
}

#[cfg(feature = "bench-support")]
impl Default for PacemakerFrameBench {
    fn default() -> Self {
        let segments = (0..256)
            .map(|index| deadsync_rules::stream::StreamSegment {
                start: index * 2,
                end: index * 2 + 2,
                is_break: index % 3 == 2,
            })
            .collect::<Vec<_>>();
        Self {
            stream_lookup: StreamProgressLookup::new(&segments),
        }
    }
}

#[cfg(feature = "bench-support")]
impl PacemakerFrameBench {
    fn score_values(frame: usize) -> (i32, i32, i32) {
        let possible_dp = 20_000;
        let current_possible_dp = 2_000_i32.saturating_add((frame % 16_001) as i32);
        let actual_dp = current_possible_dp.saturating_sub(((frame * 17) % 256) as i32 * 2);
        (current_possible_dp, possible_dp, actual_dp)
    }

    fn output_checksum(
        output: deadsync_notefield::ZmodMiniIndicatorOutput,
        text: TextContent,
    ) -> usize {
        let color = output.color.into_iter().fold(0usize, |checksum, value| {
            checksum.rotate_left(7) ^ value.to_bits() as usize
        });
        text.as_str().bytes().fold(color, |checksum, byte| {
            checksum.rotate_left(5) ^ byte as usize
        })
    }

    pub fn legacy_frame(&self, frame: usize) -> usize {
        let (current_possible_dp, possible_dp, actual_dp) = Self::score_values(frame);
        let (kept_percent, lost_percent, pace_percent) =
            judgment::predictive_itg_score_percents(current_possible_dp, possible_dp, actual_dp);
        let progress = MiniIndicatorProgress {
            kept_percent,
            lost_percent,
            pace_percent,
            current_score_percent: zmod_percent_from_points(actual_dp, possible_dp),
            current_possible_ratio: f64::from(current_possible_dp) / f64::from(possible_dp),
            current_possible_dp,
            actual_dp,
            judged_any: true,
            ..MiniIndicatorProgress::default()
        };
        let combo_color = crate_zmod_static_combo_color(ZmodComboColorParams {
            style: ZmodComboColorStyle::Solid,
            full_combo_mode: true,
            combo: frame as u32,
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            quint_active: frame & 1 == 0,
            elapsed_s: frame as f32 / 60.0,
        });
        let stream_completion = self
            .stream_lookup
            .completion_for_beat(342.0, (frame % 2_048) as f32);
        let output = zmod_mini_indicator_output(
            &progress,
            ZmodMiniIndicatorParams {
                mode: MiniIndicatorMode::Pacemaker,
                color_style: MiniIndicatorColorStyle::Default,
                subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
                score_type: MiniIndicatorScoreType::Itg,
                combo_color,
                is_failing: false,
                life: 1.0,
                rival_score_percent: 95.0,
                target_score_percent: 92.0,
                stream_completion,
            },
        )
        .expect("the benchmark progress has a judged row");
        let ZmodMiniIndicatorText::SignedPercent { value, negative } = output.text else {
            unreachable!("pacemaker always emits a signed percent")
        };
        Self::output_checksum(
            output,
            super::text::benchmark_pacemaker_text_legacy(value, negative),
        )
    }

    pub fn optimized_frame(&self, frame: usize) -> usize {
        let (current_possible_dp, possible_dp, actual_dp) = Self::score_values(frame);
        let progress = MiniIndicatorProgress {
            current_score_percent: zmod_percent_from_points(actual_dp, possible_dp),
            current_possible_ratio: f64::from(current_possible_dp) / f64::from(possible_dp),
            current_possible_dp,
            actual_dp,
            judged_any: true,
            ..MiniIndicatorProgress::default()
        };
        let output = zmod_mini_indicator_output(
            &progress,
            ZmodMiniIndicatorParams {
                mode: MiniIndicatorMode::Pacemaker,
                color_style: MiniIndicatorColorStyle::Default,
                subtractive_display: MiniIndicatorSubtractiveDisplay::CountThenPercent,
                score_type: MiniIndicatorScoreType::Itg,
                combo_color: [1.0; 4],
                is_failing: false,
                life: 1.0,
                rival_score_percent: 0.0,
                target_score_percent: 92.0,
                stream_completion: None,
            },
        )
        .expect("the benchmark progress has a judged row");
        let ZmodMiniIndicatorText::SignedPercent { value, negative } = output.text else {
            unreachable!("pacemaker always emits a signed percent")
        };
        Self::output_checksum(
            output,
            super::text::benchmark_pacemaker_text(value, negative),
        )
    }
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_disabled_mini_indicator_legacy(frame: usize) -> usize {
    usize::from(disabled_mini_indicator_bench_work(frame).is_some())
}

#[cfg(feature = "bench-support")]
pub(super) fn benchmark_disabled_mini_indicator(frame: usize) -> usize {
    usize::from(
        mini_indicator_output_if_enabled(MiniIndicatorMode::None, || {
            disabled_mini_indicator_bench_work(frame)
        })
        .is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn disabled_mini_indicator_does_not_build_frame_inputs() {
        let calls = Cell::new(0);
        let output = mini_indicator_output_if_enabled(MiniIndicatorMode::None, || {
            calls.set(calls.get() + 1);
            Some(7)
        });

        assert_eq!(output, None);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn enabled_mini_indicator_preserves_frame_output() {
        let calls = Cell::new(0);
        let output = mini_indicator_output_if_enabled(MiniIndicatorMode::PredictiveScoring, || {
            calls.set(calls.get() + 1);
            Some(7)
        });

        assert_eq!(output, Some(7));
        assert_eq!(calls.get(), 1);
    }
}
