//! Completed gameplay results, independent of screen construction and rendering.
use crate::GameplayProfile;
use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::course_run::CourseRun;
use deadsync_gameplay::{GameplayRuntimeState, build_crossover_rows};
use deadsync_profile as profile_data;
use deadsync_rules::course::CourseType;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::{judgment, timing as timing_stats};
use deadsync_score::stage_stats::{self, PlayerStageSummary, StageSummary};
use deadsync_score::{self as score_data, GrooveStatsEvalState, ItlEvalState};
use std::collections::HashMap;
use std::sync::Arc;

/// Played chart identities and resolved settings retained for Evaluation.
#[derive(Clone)]
pub struct EvaluationStage {
    pub song: Arc<SongData>,
    charts: [Option<(profile_data::PlayerSide, Arc<ChartData>)>; MAX_PLAYERS],
    pub music_rate: f32,
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
}

impl EvaluationStage {
    pub fn from_gameplay_result<A, C, D>(
        game: &GameplayRuntimeState<GameplayProfile, A, C, D>,
        result: &StageSummary,
    ) -> Option<Self> {
        let players = std::array::from_fn(|player| {
            (player < game.num_players()).then(|| {
                let side = crate::profile_side_from_gameplay(
                    game.setup.session.runtime_player_side(player),
                );
                (side, game.scroll_speed_for_player(player))
            })
        });
        Self::from_stage_result(result, players)
    }

    /// Players are in result slot order. Restart settings use physical-side order.
    pub fn from_stage_result(
        result: &StageSummary,
        players: [Option<(profile_data::PlayerSide, ScrollSpeedSetting)>; MAX_PLAYERS],
    ) -> Option<Self> {
        let mut scroll_speed = [ScrollSpeedSetting::default(); MAX_PLAYERS];
        let charts = players.map(|player| {
            let (side, speed) = player?;
            let index = profile_data::player_side_index(side);
            let result = result.players[index].as_ref()?;
            scroll_speed[index] = speed;
            Some((side, Arc::clone(&result.chart)))
        });
        charts.iter().any(Option::is_some).then(|| Self {
            song: Arc::clone(&result.song),
            charts,
            music_rate: if result.music_rate.is_finite() && result.music_rate > 0.0 {
                result.music_rate
            } else {
                1.0
            },
            scroll_speed,
        })
    }

    pub fn chart_hashes(&self) -> [&str; MAX_PLAYERS] {
        let mut hashes = [""; MAX_PLAYERS];
        for (side, chart_hash) in self.player_chart_queries().into_iter().flatten() {
            hashes[profile_data::player_side_index(side)] = chart_hash;
        }
        hashes
    }

    pub fn player_chart_queries(&self) -> [Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS] {
        self.charts.each_ref().map(|entry| {
            entry.as_ref().map(|(side, chart)| (*side, chart.short_hash.as_str()))
        })
    }
}

fn course_summary_totals(run: &CourseRun) -> [stage_stats::CourseSummaryTotals; MAX_PLAYERS] {
    if run.course_type == CourseType::Endless {
        [stage_stats::CourseSummaryTotals::default(); MAX_PLAYERS]
    } else {
        run.course_display_totals
            .map(|total| stage_stats::CourseSummaryTotals {
                possible_grade_points: total.possible_grade_points,
                total_steps: total.total_steps,
                holds_total: total.holds_total,
                rolls_total: total.rolls_total,
                mines_total: total.mines_total,
            })
    }
}

pub fn course_summary_stage(
    run: &CourseRun,
    stage_summaries: &[StageSummary],
    song_stub: &SongData,
) -> Option<StageSummary> {
    stage_stats::build_course_summary_stage(stage_stats::CourseSummaryInput {
        path: run.path.as_path(),
        name: &song_stub.title,
        banner_path: song_stub.banner_path.as_deref(),
        score_hash: run.score_hash.as_str(),
        difficulty_name: run.course_difficulty_name.as_str(),
        meter: run.course_meter,
        song_stub,
        course_total_seconds: run.total_seconds(),
        totals: course_summary_totals(run),
        stage_summaries,
    })
}

/// The shared result saved for the master participant in a Routine course.
pub fn routine_shared_course_result(
    run: &CourseRun,
    summary: &StageSummary,
    play_style: profile_data::PlayStyle,
) -> Option<PlayerStageSummary> {
    if !play_style.is_routine() {
        return None;
    }
    let [Some(first), Some(second)] = &summary.players else {
        return None;
    };
    Some(stage_stats::routine_shared_course_player(
        first,
        second,
        course_summary_totals(run),
    ))
}

/// The result each side saves for a course: StageStats::CommitScores saves
/// one shared Routine result, for the master player.
pub fn course_summary_records<'a>(
    summary: &'a StageSummary,
    routine_shared: Option<&'a PlayerStageSummary>,
) -> [Option<&'a PlayerStageSummary>; MAX_PLAYERS] {
    match routine_shared {
        Some(shared) => std::array::from_fn(|idx| (idx == 0).then_some(shared)),
        None => summary.players.each_ref().map(Option::as_ref),
    }
}

/// Capture each completed player's original chart result before the runtime is
/// discarded. Online eligibility is supplied by the shell after score saving.
/// This load/transition-time work never reads theme or global session state.
// The assembly deliberately keeps the per-player result calculations together.
pub fn stage_result<A, C, D>(
    gs: &GameplayRuntimeState<GameplayProfile, A, C, D>,
    online: [(GrooveStatsEvalState, ItlEvalState); MAX_PLAYERS],
) -> StageSummary {
    let cols_per_player = gs.cols_per_player();
    let mut players = std::array::from_fn(|_| None);
    for (player_idx, (groovestats, itl)) in online
        .into_iter()
        .enumerate()
        .take(gs.num_players().min(MAX_PLAYERS))
    {
        let (start, end) = gs.note_range_for_player(player_idx);
        let notes = &gs.notes()[start..end];
        let column_judgment_eligible = &gs.column_judgment_eligible()[start..end];
        let note_times = &gs.note_time_cache_ns()[start..end];
        let p = &gs.players()[player_idx];
        let prof = &gs.profiles()[player_idx];
        let col_offset = player_idx.saturating_mul(cols_per_player);
        let fail_stream_progress = p.fail_time.and_then(|fail_time| {
            let timing = gs.timing_for_player(player_idx)?;
            let chart = gs.gameplay_chart(player_idx)?;
            deadsync_gameplay::zmod_fail_stream_progress_for_note_data(
                &chart.notes,
                cols_per_player,
                timing.get_beat_for_time(fail_time),
            )
        });

        // Compute timing statistics across all non-miss tap judgments.
        let (foot_by_row, foot_by_note) = foot_parity_for_results(gs, player_idx);
        let stats = timing_stats::compute_note_timing_stats(notes);
        let arrow_timing = timing_stats::compute_arrow_timing_stats(
            notes,
            col_offset,
            cols_per_player,
            (!foot_by_note.is_empty()).then_some(&foot_by_note),
        );
        // Prepare scatter points and histogram bins
        let scatter = timing_stats::build_scatter_points(
            notes,
            note_times,
            col_offset,
            cols_per_player,
            (!foot_by_row.is_empty()).then_some(foot_by_row.as_slice()),
        );
        let histogram = timing_stats::build_histogram_ms(notes);
        let scatter_worst_window_ms = {
            let tw = timing_stats::effective_windows_ms();
            let observed = histogram.worst_observed_ms.max(0.0);
            let mut idx: usize = if observed <= tw[0] {
                1
            } else if observed <= tw[1] {
                2
            } else if observed <= tw[2] {
                3
            } else if observed <= tw[3] {
                4
            } else {
                5
            };
            // `scatterplot_max_window` takes precedence over the older
            // `scale_scatterplot` toggle. When set, the plot's worst
            // window is min(observed worst tier, selected tier) so the
            // scale is forced to clamp at the chosen judgment window
            // (matching Chris's Simply-Love-SM5-8ms `ScaleGraph`
            // semantics, generalized per tier).
            let cap_idx: Option<usize> = match prof.scatterplot_max_window {
                profile_data::ScatterplotMaxWindow::Off => None,
                profile_data::ScatterplotMaxWindow::Fantastic => Some(1),
                profile_data::ScatterplotMaxWindow::Excellent => Some(2),
                profile_data::ScatterplotMaxWindow::Great => Some(3),
            };
            if let Some(cap) = cap_idx {
                idx = idx.min(cap);
                tw[idx - 1]
            } else if prof.scale_scatterplot {
                // zmod-style `ScaleGraph`: cap at Great so a single
                // Decent/Way Off doesn't squash the plot, and floor
                // at Fantastic so quad/quint runs can zoom past
                // Excellent. Snap-to-max-error and padding stay as
                // internal-only knobs for future use.
                const MAX_WINDOW: ScatterWindow = ScatterWindow::Great;
                const MIN_WINDOW: ScatterWindow = ScatterWindow::Fantastic;
                const SNAP_MAX_ERROR: bool = true;
                const PADDING_PCT: u8 = 5;

                let max_w = MAX_WINDOW.ms();
                let min_w = MIN_WINDOW.ms();
                let lo = min_w.min(max_w);
                let hi = min_w.max(max_w);
                let candidate = if SNAP_MAX_ERROR {
                    observed * (1.0 + f32::from(PADDING_PCT) / 100.0)
                } else {
                    if idx == 1 {
                        idx = 2;
                    }
                    let tier = tw[idx - 1];
                    // FA+ W0 is layered on top of W1..W5 in deadsync,
                    // so the tier-edge ladder above never lands on it.
                    // Honor it explicitly when it's the configured
                    // lower bound and the data fits.
                    if MIN_WINDOW == ScatterWindow::FantasticPlus
                        && observed <= ScatterWindow::FantasticPlus.ms()
                    {
                        ScatterWindow::FantasticPlus.ms().min(tier)
                    } else {
                        tier
                    }
                };
                candidate.clamp(lo, hi)
            } else {
                // Original deadsync behavior: Excellent floor with
                // no upper cap.
                idx = idx.max(2);
                tw[idx - 1]
            }
        };
        let graph_first_second = 0.0_f32.min(gs.timing().get_time_for_beat(0.0));
        let graph_last_second = gs
            .song()
            .precise_last_second()
            .max(graph_first_second + 0.001);
        let totals = gs.stage_totals_for_player(player_idx);

        let score_percent = judgment::calculate_itg_score_percent_from_counts(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            totals.possible_grade_points,
        );
        let score_valid = gs.score_valid_for_player(player_idx) && !gs.autoplay_used();
        let disqualified = gs.autoplay_used();
        let outcome = gs.individual_song_outcome(player_idx);
        let mut grade = stage_grade(
            outcome.is_failing,
            outcome.song_completed_naturally,
            disqualified,
            score_percent,
        );
        // Per-window counts for the FA+ pane should reflect tracked
        // gameplay counts. These continue after failure but skip live
        // autoplay, matching Simply Love's JudgmentMessage guards.
        let window_counts = gs.live_window_counts(player_idx);
        let window_counts_10ms = gs.live_window_counts_10ms(player_idx);
        let ex_data = gs.stage_scored_ex_score_data(player_idx);
        let ex_score_percent = judgment::ex_score_percent(&ex_data);
        let hard_ex_score_percent = judgment::hard_ex_score_percent(&ex_data);

        // Quint comes from the achieved result, not whether FA+ is displayed.
        grade = score_data::promote_quint_grade(grade, ex_score_percent);

        let column_judgments = score_data::compute_column_judgments(
            notes,
            column_judgment_eligible,
            cols_per_player,
            col_offset,
            prof.show_fa_plus_window,
        );
        let notes_hit = column_judgments.iter().fold(0u32, |total, column| {
            total
                .saturating_add(column.w0)
                .saturating_add(column.w1)
                .saturating_add(column.w2)
                .saturating_add(column.w3)
                .saturating_add(column.w4)
                .saturating_add(column.w5)
        });
        let side = gs.setup.session.runtime_player_side(player_idx);
        let golf_par = deadsync_chart::golf::par(
            &gs.song().simfile_path,
            &gs.charts()[player_idx].difficulty,
        );
        players[deadsync_gameplay::gameplay_player_side_index(side)] = Some(PlayerStageSummary {
            profile_name: prof.display_name.clone(),
            chart: gs.charts()[player_idx].clone(),
            judgment_counts: p.judgment_counts,
            column_judgments,
            fail_stream_progress,
            score_valid,
            disqualified,
            groovestats,
            itl,
            grade,
            score_percent,
            earned_grade_points: p.earned_grade_points,
            possible_grade_points: totals.possible_grade_points,
            ex_score_percent,
            hard_ex_score_percent,
            hands_achieved: p.hands_achieved,
            hands_total: gs.hands_total_for_player(player_idx),
            holds_held: p.holds_held,
            holds_held_for_score: p.holds_held_for_score,
            holds_total: totals.holds_total,
            rolls_held: p.rolls_held,
            rolls_held_for_score: p.rolls_held_for_score,
            rolls_total: totals.rolls_total,
            mines_hit_for_score: p.mines_hit_for_score,
            mines_avoided: p.mines_avoided,
            mines_total: totals.mines_total,
            notes_hit,
            calories_burned: p.calories_burned,
            window_counts,
            window_counts_10ms,
            timing: stats,
            arrow_timing,
            scatter,
            scatter_worst_window_ms,
            histogram,
            graph_first_second,
            graph_last_second,
            life_history: p.life_history.clone(),
            fail_time: p.fail_time,
            show_w0: (prof.show_fa_plus_window && prof.show_fa_plus_pane) || prof.show_ex_score,
            show_fa_plus_pane: prof.show_fa_plus_pane,
            show_ex_score: prof.show_ex_score,
            show_hard_ex_score: prof.show_hard_ex_score,
            track_early_judgments: prof.track_early_judgments,
            dim_post_fail_scatter: prof.dim_post_fail_scatter,
            golf_score: golf_results_shown(prof.show_golf_results, prof.golf_all_charts, golf_par)
                .then(|| p.golf_score()),
            golf_par,
        });
    }
    let music_rate = gs.music_rate();
    StageSummary {
        song: gs.song_arc(),
        music_rate: if music_rate.is_finite() && music_rate > 0.0 {
            music_rate
        } else {
            1.0
        },
        duration_seconds: gs.total_elapsed_in_screen(),
        players,
    }
}

/// Whether the stage summary carries a golf score for the results pane. It follows
/// the same Golf Display choice as the gameplay counter: charts without an authored
/// par stay untouched unless the player opted into golf on all charts.
fn golf_results_shown(show_golf_results: bool, golf_all_charts: bool, par: Option<f64>) -> bool {
    show_golf_results && (golf_all_charts || par.is_some())
}

fn collect_foot_parity<const LANES: usize>(
    notes: &[deadsync_rules::note::Note],
    note_range: (usize, usize),
    timing_segments: &deadsync_rules::timing::TimingSegments,
    col_start: usize,
) -> (
    Vec<(usize, timing_stats::ScatterFoot)>,
    HashMap<(usize, usize), timing_stats::ScatterFoot>,
) {
    use timing_stats::ScatterFoot;

    let (rows, row_to_beat, row_indices) =
        build_crossover_rows::<LANES>(notes, note_range, col_start);
    let annotations = deadsync_simfile::timing::crossover_annotations::<LANES>(
        &rows,
        &row_to_beat,
        timing_segments,
    );
    let note_count = annotations
        .iter()
        .map(|annotation| {
            (annotation.left_foot_mask | annotation.right_foot_mask).count_ones() as usize
        })
        .sum();
    let mut row_feet = Vec::with_capacity(annotations.len());
    let mut note_feet = HashMap::with_capacity(note_count);
    for (annotation, row_index) in annotations.iter().zip(row_indices) {
        let row_foot = match (
            annotation.left_foot_mask != 0,
            annotation.right_foot_mask != 0,
        ) {
            (true, true) => ScatterFoot::Both,
            (true, false) => ScatterFoot::Left,
            (false, true) => ScatterFoot::Right,
            (false, false) => ScatterFoot::Unknown,
        };
        if row_foot != ScatterFoot::Unknown {
            row_feet.push((row_index, row_foot));
        }
        for lane in 0..LANES {
            let bit = 1u8 << lane;
            let foot = if annotation.left_foot_mask & bit != 0 {
                ScatterFoot::Left
            } else if annotation.right_foot_mask & bit != 0 {
                ScatterFoot::Right
            } else {
                continue;
            };
            note_feet.insert((row_index, col_start + lane), foot);
        }
    }
    (row_feet, note_feet)
}

fn foot_parity_for_results<A, C, D>(
    gs: &GameplayRuntimeState<GameplayProfile, A, C, D>,
    player_idx: usize,
) -> (
    Vec<(usize, timing_stats::ScatterFoot)>,
    HashMap<(usize, usize), timing_stats::ScatterFoot>,
) {
    if player_idx >= gs.num_players() {
        return Default::default();
    }
    let note_range = gs.note_range_for_player(player_idx);
    let Some(chart) = gs.gameplay_chart(player_idx) else {
        return Default::default();
    };
    let cols_per_player = gs.cols_per_player();
    let col_start = player_idx.saturating_mul(cols_per_player);
    match cols_per_player {
        4 => collect_foot_parity::<4>(gs.notes(), note_range, &chart.timing_segments, col_start),
        8 => collect_foot_parity::<8>(gs.notes(), note_range, &chart.timing_segments, col_start),
        _ => Default::default(),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScatterWindow {
    FantasticPlus,
    Fantastic,
    Great,
}

impl ScatterWindow {
    #[inline]
    fn ms(self) -> f32 {
        let tw = timing_stats::effective_windows_ms();
        match self {
            Self::FantasticPlus => timing_stats::FA_PLUS_W0_MS,
            Self::Fantastic => tw[0],
            Self::Great => tw[2],
        }
    }
}

#[inline(always)]
fn stage_grade(
    is_failing: bool,
    song_completed_naturally: bool,
    disqualified: bool,
    score_percent: f64,
) -> score_data::Grade {
    if is_failing || !song_completed_naturally || disqualified {
        score_data::Grade::Failed
    } else {
        score_data::score_to_grade(score_percent * 10000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golf_results_follow_the_golf_display_choice() {
        assert!(golf_results_shown(true, false, Some(3.0)));
        assert!(!golf_results_shown(true, false, None));
        assert!(golf_results_shown(true, true, None));
        assert!(!golf_results_shown(false, true, Some(3.0)));
    }

    #[test]
    fn disqualified_grade_fails() {
        assert_eq!(
            stage_grade(false, true, true, 1.0),
            score_data::Grade::Failed
        );
    }
}
