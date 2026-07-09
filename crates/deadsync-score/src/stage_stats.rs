use std::{path::Path, sync::Arc};

use deadsync_chart::{ChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_rules::judgment;
use deadsync_rules::timing::{
    ArrowTimingStats, HistogramMs, ScatterPoint, TimingStats, WindowCounts,
};

use crate::{Grade, GrooveStatsEvalState, ItlEvalState, promote_quint_grade, score_to_grade};

#[derive(Clone, Debug)]
pub struct StageSummary {
    pub song: Arc<SongData>,
    pub music_rate: f32,
    pub duration_seconds: f32,
    pub players: [Option<PlayerStageSummary>; MAX_PLAYERS],
}

#[derive(Clone, Debug)]
pub struct PlayerStageSummary {
    pub profile_name: String,
    pub chart: Arc<ChartData>,
    pub score_valid: bool,
    pub disqualified: bool,
    pub groovestats: GrooveStatsEvalState,
    pub itl: ItlEvalState,
    pub grade: Grade,
    pub score_percent: f64,
    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub hands_achieved: u32,
    pub hands_total: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_total: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    /// Total hit tapnotes this stage (counts jumps/hands as >1).
    pub notes_hit: u32,
    pub calories_burned: f32,
    pub window_counts: WindowCounts,
    pub window_counts_10ms: WindowCounts,
    pub timing: TimingStats,
    pub arrow_timing: ArrowTimingStats,
    pub scatter: Vec<ScatterPoint>,
    pub scatter_worst_window_ms: f32,
    pub histogram: HistogramMs,
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub life_history: Vec<(f32, f32)>,
    pub fail_time: Option<f32>,
    pub show_w0: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub track_early_judgments: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseSummaryTotals {
    pub possible_grade_points: i32,
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
}

pub struct CourseSummaryInput<'a> {
    pub path: &'a Path,
    pub name: &'a str,
    pub banner_path: Option<&'a Path>,
    pub score_hash: &'a str,
    pub difficulty_name: &'a str,
    pub meter: Option<u32>,
    pub song_stub: &'a SongData,
    pub course_total_seconds: f32,
    pub totals: [CourseSummaryTotals; MAX_PLAYERS],
    pub stage_summaries: &'a [StageSummary],
}

#[inline(always)]
const fn merge_window_counts(mut total: WindowCounts, add: WindowCounts) -> WindowCounts {
    total.w0 = total.w0.saturating_add(add.w0);
    total.w1 = total.w1.saturating_add(add.w1);
    total.w2 = total.w2.saturating_add(add.w2);
    total.w3 = total.w3.saturating_add(add.w3);
    total.w4 = total.w4.saturating_add(add.w4);
    total.w5 = total.w5.saturating_add(add.w5);
    total.miss = total.miss.saturating_add(add.miss);
    total
}

#[inline(always)]
const fn window_counts_total(counts: WindowCounts) -> u32 {
    counts
        .w0
        .saturating_add(counts.w1)
        .saturating_add(counts.w2)
        .saturating_add(counts.w3)
        .saturating_add(counts.w4)
        .saturating_add(counts.w5)
        .saturating_add(counts.miss)
}

pub fn build_course_summary_stage(input: CourseSummaryInput<'_>) -> Option<StageSummary> {
    if input.stage_summaries.is_empty() {
        return None;
    }
    let mut summary_song = input.song_stub.clone();
    summary_song.simfile_path = input.path.to_path_buf();
    summary_song.title = input.name.to_string();
    summary_song.translit_title = input.name.to_string();
    summary_song.banner_path = input.banner_path.map(Path::to_path_buf);
    let played_duration_seconds: f32 = input
        .stage_summaries
        .iter()
        .map(|stage| stage.duration_seconds.max(0.0))
        .sum();
    let duration_seconds = input.course_total_seconds.max(played_duration_seconds);
    summary_song.music_length_seconds = duration_seconds;
    summary_song.total_length_seconds = duration_seconds.round() as i32;
    let summary_song = Arc::new(summary_song);

    let mut players: [Option<PlayerStageSummary>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    for idx in 0..MAX_PLAYERS {
        let course_totals = input.totals[idx];
        let mut earned_grade_points = 0i32;
        let mut possible_grade_points = course_totals.possible_grade_points;
        let mut notes_hit: u32 = 0;
        let mut played_total_steps: u32 = 0;
        let mut played_possible_grade_points = 0i32;
        let mut calories_burned = 0.0_f32;
        let mut meter_sum = 0u32;
        let mut meter_count = 0u32;
        let mut any_failed = false;
        let mut score_valid = true;
        let mut disqualified = false;
        let mut show_w0 = false;
        let mut show_fa_plus_pane = false;
        let mut show_ex = false;
        let mut show_hard_ex = false;
        let mut track_early_judgments = false;
        let mut counts = WindowCounts::default();
        let mut counts_10ms = WindowCounts::default();
        let mut hands_achieved = 0u32;
        let mut hands_total = 0u32;
        let mut holds_held = 0u32;
        let mut holds_held_for_score = 0u32;
        let mut played_holds_total = 0u32;
        let mut rolls_held = 0u32;
        let mut rolls_held_for_score = 0u32;
        let mut played_rolls_total = 0u32;
        let mut mines_hit_for_score = 0u32;
        let mut mines_avoided = 0u32;
        let mut played_mines_total = 0u32;
        let mut timing_offsets_ms = Vec::new();
        let mut scatter = Vec::new();
        let mut scatter_worst_window_ms = 0.0_f32;
        let mut histograms = Vec::new();
        let mut graph_first_second = 0.0_f32;
        let mut graph_last_second = duration_seconds.max(0.001);
        let mut life_history = Vec::new();
        let mut fail_time = None;
        let mut elapsed = 0.0_f32;
        let mut first_player: Option<&PlayerStageSummary> = None;
        for stage in input.stage_summaries {
            let stage_offset = elapsed;
            let Some(player) = stage.players[idx].as_ref() else {
                elapsed += stage.duration_seconds.max(0.0);
                continue;
            };
            first_player.get_or_insert(player);
            earned_grade_points = earned_grade_points.saturating_add(player.earned_grade_points);
            played_possible_grade_points =
                played_possible_grade_points.saturating_add(player.possible_grade_points);
            notes_hit = notes_hit.saturating_add(player.notes_hit);
            played_total_steps =
                played_total_steps.saturating_add(window_counts_total(player.window_counts));
            calories_burned += player.calories_burned.max(0.0);
            meter_sum = meter_sum.saturating_add(player.chart.meter);
            meter_count = meter_count.saturating_add(1);
            any_failed |= player.grade == Grade::Failed;
            score_valid &= player.score_valid;
            disqualified |= player.disqualified;
            show_w0 |= player.show_w0;
            show_fa_plus_pane |= player.show_fa_plus_pane;
            show_ex |= player.show_ex_score;
            show_hard_ex |= player.show_hard_ex_score;
            track_early_judgments |= player.track_early_judgments;
            counts = merge_window_counts(counts, player.window_counts);
            counts_10ms = merge_window_counts(counts_10ms, player.window_counts_10ms);
            hands_achieved = hands_achieved.saturating_add(player.hands_achieved);
            hands_total = hands_total.saturating_add(player.hands_total);
            holds_held = holds_held.saturating_add(player.holds_held);
            holds_held_for_score = holds_held_for_score.saturating_add(player.holds_held_for_score);
            played_holds_total = played_holds_total.saturating_add(player.holds_total);
            rolls_held = rolls_held.saturating_add(player.rolls_held);
            rolls_held_for_score = rolls_held_for_score.saturating_add(player.rolls_held_for_score);
            played_rolls_total = played_rolls_total.saturating_add(player.rolls_total);
            mines_hit_for_score = mines_hit_for_score.saturating_add(player.mines_hit_for_score);
            mines_avoided = mines_avoided.saturating_add(player.mines_avoided);
            played_mines_total = played_mines_total.saturating_add(player.mines_total);
            scatter.reserve(player.scatter.len());
            for point in &player.scatter {
                let mut shifted = *point;
                shifted.time_sec += stage_offset;
                if let Some(offset_ms) = shifted.offset_ms {
                    timing_offsets_ms.push(offset_ms);
                }
                scatter.push(shifted);
            }
            scatter_worst_window_ms = scatter_worst_window_ms.max(player.scatter_worst_window_ms);
            histograms.push(player.histogram.clone());
            graph_first_second = graph_first_second.min(player.graph_first_second + stage_offset);
            graph_last_second = graph_last_second.max(player.graph_last_second + stage_offset);
            life_history.reserve(player.life_history.len());
            for &(time, life) in &player.life_history {
                life_history.push((time + stage_offset, life));
            }
            if fail_time.is_none() {
                fail_time = player.fail_time.map(|time| time + stage_offset);
            }
            elapsed += stage.duration_seconds.max(0.0);
        }
        let Some(first_player) = first_player else {
            continue;
        };
        if possible_grade_points <= 0 {
            possible_grade_points = played_possible_grade_points;
        }
        let total_steps = course_totals.total_steps.max(played_total_steps);
        let holds_total = if course_totals.holds_total > 0 {
            course_totals.holds_total
        } else {
            played_holds_total
        };
        let rolls_total = if course_totals.rolls_total > 0 {
            course_totals.rolls_total
        } else {
            played_rolls_total
        };
        let mines_total = if course_totals.mines_total > 0 {
            course_totals.mines_total
        } else {
            played_mines_total
        };
        let score_percent = judgment::calculate_itg_score_percent_from_points(
            earned_grade_points,
            possible_grade_points,
        );
        let ex_data = judgment::ExScoreData {
            counts,
            counts_10ms,
            holds_held: holds_held_for_score,
            holds_resolved: holds_held_for_score,
            rolls_held: rolls_held_for_score,
            rolls_resolved: rolls_held_for_score,
            mines_hit: mines_hit_for_score,
            total_steps,
            holds_total,
            rolls_total,
            mines_total,
        };
        let ex_score_percent = judgment::ex_score_percent(&ex_data);
        let hard_ex_score_percent = judgment::hard_ex_score_percent(&ex_data);
        let mut grade = if any_failed {
            Grade::Failed
        } else {
            score_to_grade(score_percent * 10000.0)
        };
        grade = promote_quint_grade(grade, ex_score_percent);
        let mut summary_chart = (*first_player.chart).clone();
        summary_chart.short_hash = input.score_hash.to_string();
        summary_chart.difficulty = input.difficulty_name.to_string();
        summary_chart.step_artist.clear();
        summary_chart.description.clear();
        summary_chart.chart_name.clear();
        summary_chart.meter = input.meter.unwrap_or_else(|| {
            if meter_count > 0 {
                (meter_sum as f32 / meter_count as f32).round() as u32
            } else {
                summary_chart.meter
            }
        });
        players[idx] = Some(PlayerStageSummary {
            profile_name: first_player.profile_name.clone(),
            chart: Arc::new(summary_chart),
            score_valid,
            disqualified,
            groovestats: GrooveStatsEvalState {
                valid: false,
                reason_lines: vec!["GrooveStats QR is unavailable in course mode.".to_string()],
                manual_qr_url: None,
            },
            itl: ItlEvalState::default(),
            grade,
            score_percent,
            earned_grade_points,
            possible_grade_points,
            ex_score_percent,
            hard_ex_score_percent,
            hands_achieved,
            hands_total,
            holds_held,
            holds_held_for_score,
            holds_total,
            rolls_held,
            rolls_held_for_score,
            rolls_total,
            mines_hit_for_score,
            mines_avoided,
            mines_total,
            notes_hit,
            calories_burned,
            window_counts: counts,
            window_counts_10ms: counts_10ms,
            timing: deadsync_rules::timing::timing_stats_from_offsets(timing_offsets_ms),
            arrow_timing: ArrowTimingStats::default(),
            scatter,
            scatter_worst_window_ms: scatter_worst_window_ms.max(45.0),
            histogram: deadsync_rules::timing::merge_histograms_ms(histograms.as_slice()),
            graph_first_second,
            graph_last_second,
            life_history,
            fail_time,
            show_w0,
            show_fa_plus_pane,
            show_ex_score: show_ex,
            show_hard_ex_score: show_hard_ex,
            track_early_judgments,
        });
    }

    let music_rate = input
        .stage_summaries
        .last()
        .map(|s| s.music_rate)
        .unwrap_or(1.0);
    Some(StageSummary {
        song: summary_song,
        music_rate,
        duration_seconds,
        players,
    })
}

#[inline(always)]
pub const fn course_eval_is_final(
    next_stage_index: usize,
    stage_count: usize,
    failed: bool,
) -> bool {
    failed || next_stage_index >= stage_count
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};

    use super::*;

    fn test_chart(hash: &str) -> Arc<ChartData> {
        Arc::new(ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Hard".to_string(),
            description: "Stage Description".to_string(),
            chart_name: "Stage Chart Name".to_string(),
            meter: 9,
            step_artist: "Stage Artist".to_string(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats {
                total_steps: 20,
                ..ArrowStats::default()
            },
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 500,
            holds_total: 2,
            rolls_total: 1,
            mines_total: 3,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        })
    }

    fn test_song(path: &str, title: &str, seconds: f32) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(path),
            title: title.to_string(),
            subtitle: String::new(),
            translit_title: title.to_string(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: seconds,
            first_second: 0.0,
            total_length_seconds: seconds.round() as i32,
            precise_last_second_seconds: seconds,
            charts: Vec::new(),
        })
    }

    fn test_player_summary(
        chart: Arc<ChartData>,
        grade: Grade,
        earned_grade_points: i32,
        possible_grade_points: i32,
    ) -> PlayerStageSummary {
        PlayerStageSummary {
            profile_name: "P1".to_string(),
            chart,
            score_valid: true,
            disqualified: false,
            groovestats: GrooveStatsEvalState::default(),
            itl: ItlEvalState::default(),
            grade,
            score_percent: 1.0,
            earned_grade_points,
            possible_grade_points,
            ex_score_percent: 100.0,
            hard_ex_score_percent: 100.0,
            hands_achieved: 0,
            hands_total: 0,
            holds_held: 2,
            holds_held_for_score: 2,
            holds_total: 2,
            rolls_held: 1,
            rolls_held_for_score: 1,
            rolls_total: 1,
            mines_hit_for_score: 0,
            mines_avoided: 3,
            mines_total: 3,
            notes_hit: 20,
            calories_burned: 1.25,
            window_counts: WindowCounts {
                w1: 20,
                ..WindowCounts::default()
            },
            window_counts_10ms: WindowCounts {
                w1: 20,
                ..WindowCounts::default()
            },
            timing: TimingStats {
                mean_ms: 10.0,
                mean_abs_ms: 10.0,
                max_abs_ms: 10.0,
                ..TimingStats::default()
            },
            arrow_timing: ArrowTimingStats::default(),
            scatter: vec![ScatterPoint {
                time_sec: 5.0,
                offset_ms: Some(10.0),
                direction_code: 1,
                is_stream: false,
                is_left_foot: false,
                miss_because_held: false,
            }],
            scatter_worst_window_ms: 30.0,
            histogram: HistogramMs {
                bins: vec![(10, 1)],
                smoothed: vec![(10, 1.0)],
                max_count: 1,
                worst_observed_ms: 10.0,
                worst_window_ms: 30.0,
            },
            graph_first_second: 0.0,
            graph_last_second: 60.0,
            life_history: vec![(1.0, 1.0)],
            fail_time: Some(40.0),
            show_w0: true,
            show_ex_score: true,
            show_hard_ex_score: true,
            show_fa_plus_pane: true,
            track_early_judgments: true,
        }
    }

    #[test]
    fn course_summary_uses_trail_totals_and_keeps_timing_graphs() {
        let song_a = test_song("Songs/Test/a.ssc", "a", 60.0);
        let song_b = test_song("Songs/Test/b.ssc", "b", 90.0);
        let mut players: [Option<PlayerStageSummary>; MAX_PLAYERS] = std::array::from_fn(|_| None);
        players[0] = Some(test_player_summary(
            test_chart("stage-a"),
            Grade::Failed,
            500,
            500,
        ));
        let stages = vec![StageSummary {
            song: song_a.clone(),
            music_rate: 1.0,
            duration_seconds: 60.0,
            players,
        }];

        let summary = build_course_summary_stage(CourseSummaryInput {
            path: Path::new("Courses/Test.crs"),
            name: "Test Course",
            banner_path: None,
            score_hash: "course-hash",
            difficulty_name: "Hard",
            meter: Some(12),
            song_stub: song_a.as_ref(),
            course_total_seconds: song_a.precise_last_second() + song_b.precise_last_second(),
            totals: [
                CourseSummaryTotals {
                    possible_grade_points: 1000,
                    total_steps: 40,
                    holds_total: 4,
                    rolls_total: 2,
                    mines_total: 6,
                },
                CourseSummaryTotals::default(),
            ],
            stage_summaries: stages.as_slice(),
        })
        .expect("course summary");

        let player = summary.players[0].as_ref().expect("P1 summary");
        assert!((summary.duration_seconds - 150.0).abs() <= f32::EPSILON);
        assert!((player.score_percent - 0.5).abs() <= f64::EPSILON);
        assert_eq!(player.earned_grade_points, 500);
        assert_eq!(player.possible_grade_points, 1000);
        assert_eq!(player.holds_total, 4);
        assert_eq!(player.rolls_total, 2);
        assert_eq!(player.mines_total, 6);
        assert_eq!(player.grade, Grade::Failed);
        assert_eq!(player.chart.short_hash, "course-hash");
        assert_eq!(player.chart.difficulty, "Hard");
        assert_eq!(player.chart.meter, 12);
        assert!(player.chart.step_artist.is_empty());
        assert!(player.chart.description.is_empty());
        assert!(player.chart.chart_name.is_empty());
        assert_eq!(player.scatter.len(), 1);
        assert!(!player.histogram.bins.is_empty());
        assert!((player.timing.mean_ms - 10.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn course_eval_final_on_completion_or_failure() {
        assert!(!course_eval_is_final(1, 3, false));
        assert!(course_eval_is_final(1, 3, true));
        assert!(course_eval_is_final(3, 3, false));
    }
}
