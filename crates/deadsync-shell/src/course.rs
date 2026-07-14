use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    CourseDisplayTiming, CourseDisplayTotals, course_display_timing_for_stages,
    course_display_totals_for_chart,
};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::{self as profile_data, PlayStyle, PlayerSide};
use deadsync_score::{self as score_data, ColumnJudgments, stage_stats};
use deadsync_theme_simply_love::views::{
    CourseGraphStage, CourseStagePlan, ScoreInfo, SelectedCoursePlan,
};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct CourseStageRuntime {
    pub song: Arc<SongData>,
    pub steps_index: [usize; MAX_PLAYERS],
    pub preferred_difficulty_index: [usize; MAX_PLAYERS],
}

#[derive(Clone)]
pub struct CourseRunState {
    pub path: PathBuf,
    pub name: String,
    pub banner_path: Option<PathBuf>,
    pub score_hash: String,
    pub course_difficulty_name: String,
    pub course_meter: Option<u32>,
    pub course_stepchart_label: String,
    pub song_stub: Arc<SongData>,
    pub stages: Vec<CourseStageRuntime>,
    pub course_display_totals: [CourseDisplayTotals; MAX_PLAYERS],
    pub next_stage_index: usize,
    pub stage_summaries: Vec<stage_stats::StageSummary>,
}

fn course_stage_runtime_from_plan(
    plan: &CourseStagePlan,
    chart_type: &str,
) -> Option<CourseStageRuntime> {
    let steps_idx = plan
        .song
        .steps_index_for_chart_hash(chart_type, plan.chart_hash.as_str())?;
    Some(CourseStageRuntime {
        song: plan.song.clone(),
        steps_index: [steps_idx; MAX_PLAYERS],
        preferred_difficulty_index: [steps_idx; MAX_PLAYERS],
    })
}

pub fn build_course_run_from_selection(
    selection: SelectedCoursePlan,
    chart_type: &str,
) -> Option<CourseRunState> {
    let mut stages = Vec::with_capacity(selection.stages.len());
    for stage in &selection.stages {
        if let Some(runtime) = course_stage_runtime_from_plan(stage, chart_type) {
            stages.push(runtime);
        }
    }
    if stages.is_empty() {
        return None;
    }

    let mut course_display_totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
    for stage in &stages {
        for (player_idx, total) in course_display_totals.iter_mut().enumerate() {
            let Some(chart) = stage
                .song
                .chart_for_steps_index(chart_type, stage.steps_index[player_idx])
            else {
                continue;
            };
            let add = course_display_totals_for_chart(chart);
            total.possible_grade_points = total
                .possible_grade_points
                .saturating_add(add.possible_grade_points);
            total.total_steps = total.total_steps.saturating_add(add.total_steps);
            total.holds_total = total.holds_total.saturating_add(add.holds_total);
            total.rolls_total = total.rolls_total.saturating_add(add.rolls_total);
            total.mines_total = total.mines_total.saturating_add(add.mines_total);
        }
    }

    Some(CourseRunState {
        path: selection.path,
        name: selection.name,
        banner_path: selection.banner_path,
        score_hash: selection.score_hash,
        course_difficulty_name: selection.course_difficulty_name,
        course_meter: selection.course_meter,
        course_stepchart_label: selection.course_stepchart_label,
        song_stub: selection.song_stub,
        stages,
        course_display_totals,
        next_stage_index: 0,
        stage_summaries: Vec::new(),
    })
}

fn course_stage_seconds(stage: &CourseStageRuntime) -> f32 {
    let seconds = stage.song.precise_last_second();
    if seconds.is_finite() {
        seconds.max(0.0)
    } else {
        0.0
    }
}

pub fn course_total_seconds(course: &CourseRunState) -> f32 {
    course.stages.iter().map(course_stage_seconds).sum()
}

pub fn course_display_timing_for_run(course: &CourseRunState) -> CourseDisplayTiming {
    course_display_timing_for_stages(&course.stages, course.next_stage_index, |stage| {
        stage.song.music_length_seconds
    })
}

pub fn build_course_summary_stage(course: &CourseRunState) -> Option<stage_stats::StageSummary> {
    let totals = course
        .course_display_totals
        .map(|total| stage_stats::CourseSummaryTotals {
            possible_grade_points: total.possible_grade_points,
            total_steps: total.total_steps,
            holds_total: total.holds_total,
            rolls_total: total.rolls_total,
            mines_total: total.mines_total,
        });
    stage_stats::build_course_summary_stage(stage_stats::CourseSummaryInput {
        path: course.path.as_path(),
        name: course.name.as_str(),
        banner_path: course.banner_path.as_deref(),
        score_hash: course.score_hash.as_str(),
        difficulty_name: course.course_difficulty_name.as_str(),
        meter: course.course_meter,
        song_stub: course.song_stub.as_ref(),
        course_total_seconds: course_total_seconds(course),
        totals,
        stage_summaries: course.stage_summaries.as_slice(),
    })
}

pub fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: PlayerSide,
) -> Option<ScoreInfo> {
    let idx = profile_data::player_side_index(side);
    let player = stage.players[idx].as_ref()?;
    let judgment_counts = [
        player
            .window_counts
            .w0
            .saturating_add(player.window_counts.w1),
        player.window_counts.w2,
        player.window_counts.w3,
        player.window_counts.w4,
        player.window_counts.w5,
        player.window_counts.miss,
    ];

    let chart_hash = player.chart.short_hash.as_str();
    let machine_records = scores::get_machine_leaderboard_local(chart_hash, usize::MAX);
    let personal_records =
        scores::get_personal_leaderboard_local_for_side(chart_hash, side, usize::MAX);
    let machine_record_highlight_rank =
        score_data::leaderboard_rank_for_score(machine_records.as_slice(), player.score_percent);
    let personal_record_highlight_rank =
        score_data::leaderboard_rank_for_score(personal_records.as_slice(), player.score_percent);
    let local_score_valid = player.score_valid && !player.disqualified;
    let earned_machine_record =
        local_score_valid && machine_record_highlight_rank.is_some_and(|rank| rank <= 10);
    let earned_top2_personal =
        local_score_valid && personal_record_highlight_rank.is_some_and(|rank| rank <= 2);
    let machine_record_highlight_rank = local_score_valid
        .then_some(machine_record_highlight_rank)
        .flatten();
    let personal_record_highlight_rank = local_score_valid
        .then_some(personal_record_highlight_rank)
        .flatten();
    let profile = profile::get_for_side(side);
    let speed_mod = profile.scroll_speed;
    let mods_text = profile_data::evaluation_mods_text(&profile, speed_mod);
    let disabled_timing_windows = profile.timing_windows.disabled_windows();

    Some(ScoreInfo {
        song: stage.song.clone(),
        chart: player.chart.clone(),
        course_graph_stages: Vec::new(),
        side,
        profile_name: player.profile_name.clone(),
        score_valid: player.score_valid,
        disqualified: player.disqualified,
        expected_groovestats_submit: false,
        expected_arrowcloud_submit: false,
        groovestats: player.groovestats.clone(),
        itl: player.itl.clone(),
        judgment_counts,
        score_percent: player.score_percent,
        earned_grade_points: player.earned_grade_points,
        possible_grade_points: player.possible_grade_points,
        grade: player.grade,
        speed_mod,
        mods_text,
        hands_achieved: player.hands_achieved,
        hands_total: player.hands_total,
        holds_held: player.holds_held,
        holds_held_for_score: player.holds_held_for_score,
        holds_total: player.holds_total,
        rolls_held: player.rolls_held,
        rolls_held_for_score: player.rolls_held_for_score,
        rolls_total: player.rolls_total,
        mines_hit_for_score: player.mines_hit_for_score,
        mines_avoided: player.mines_avoided,
        mines_total: player.mines_total,
        timing: player.timing,
        arrow_timing: player.arrow_timing.clone(),
        scatter: player.scatter.clone(),
        scatter_worst_window_ms: player.scatter_worst_window_ms,
        histogram: player.histogram.clone(),
        graph_first_second: player.graph_first_second,
        graph_last_second: player.graph_last_second,
        music_rate: if stage.music_rate.is_finite() && stage.music_rate > 0.0 {
            stage.music_rate
        } else {
            1.0
        },
        life_history: player.life_history.clone(),
        fail_time: player.fail_time.or_else(|| {
            (player.grade == score_data::Grade::Failed).then_some(stage.duration_seconds)
        }),
        window_counts: player.window_counts,
        window_counts_10ms: player.window_counts_10ms,
        ex_score_percent: player.ex_score_percent,
        hard_ex_score_percent: player.hard_ex_score_percent,
        calories_burned: player.calories_burned,
        column_judgments: Vec::new(),
        noteskin: None,
        show_fa_plus_window: player.show_w0,
        show_ex_score: player.show_ex_score,
        show_hard_ex_score: player.show_hard_ex_score,
        show_fa_plus_pane: player.show_fa_plus_pane,
        track_early_judgments: player.track_early_judgments,
        disabled_timing_windows,
        machine_records,
        machine_record_highlight_rank,
        personal_records,
        personal_record_highlight_rank,
        show_machine_personal_split: !earned_machine_record && earned_top2_personal,
    })
}

pub fn build_course_summary_score_info(
    stage: &stage_stats::StageSummary,
    course_graph_stages: &[Vec<CourseGraphStage>; MAX_PLAYERS],
    play_style: PlayStyle,
    active_side: PlayerSide,
) -> [Option<ScoreInfo>; MAX_PLAYERS] {
    let mut score_info = std::array::from_fn(|_| None);
    match play_style {
        PlayStyle::Versus => {
            for side in [PlayerSide::P1, PlayerSide::P2] {
                let idx = profile_data::player_side_index(side);
                score_info[idx] = score_info_from_stage(stage, side);
                if let Some(score) = score_info[idx].as_mut() {
                    score
                        .course_graph_stages
                        .clone_from(&course_graph_stages[idx]);
                }
            }
        }
        PlayStyle::Single | PlayStyle::Double => {
            let idx = profile_data::player_side_index(active_side);
            score_info[0] = score_info_from_stage(stage, active_side);
            if let Some(score) = score_info[0].as_mut() {
                score
                    .course_graph_stages
                    .clone_from(&course_graph_stages[idx]);
            }
        }
    }
    score_info
}

pub fn build_course_graph_stages(
    course: &CourseRunState,
    chart_type: &str,
) -> [Vec<CourseGraphStage>; MAX_PLAYERS] {
    std::array::from_fn(|player_idx| {
        let mut out = Vec::with_capacity(course.stages.len());
        for stage in &course.stages {
            let Some(chart) = stage
                .song
                .chart_for_steps_index(chart_type, stage.steps_index[player_idx])
            else {
                continue;
            };
            out.push(CourseGraphStage {
                chart: Arc::new(chart.clone()),
                song_last_second: stage.song.precise_last_second(),
            });
        }
        out
    })
}

#[inline(always)]
fn add_column_judgments(dst: &mut ColumnJudgments, src: ColumnJudgments) {
    dst.w0 = dst.w0.saturating_add(src.w0);
    dst.w1 = dst.w1.saturating_add(src.w1);
    dst.w2 = dst.w2.saturating_add(src.w2);
    dst.w3 = dst.w3.saturating_add(src.w3);
    dst.w4 = dst.w4.saturating_add(src.w4);
    dst.w5 = dst.w5.saturating_add(src.w5);
    dst.miss = dst.miss.saturating_add(src.miss);
    dst.early_w1 = dst.early_w1.saturating_add(src.early_w1);
    dst.early_w2 = dst.early_w2.saturating_add(src.early_w2);
    dst.early_w3 = dst.early_w3.saturating_add(src.early_w3);
    dst.early_w4 = dst.early_w4.saturating_add(src.early_w4);
    dst.early_w5 = dst.early_w5.saturating_add(src.early_w5);
    dst.early_total_w0 = dst.early_total_w0.saturating_add(src.early_total_w0);
    dst.early_total_w1 = dst.early_total_w1.saturating_add(src.early_total_w1);
    dst.early_total_w2 = dst.early_total_w2.saturating_add(src.early_total_w2);
    dst.early_total_w3 = dst.early_total_w3.saturating_add(src.early_total_w3);
    dst.early_total_w4 = dst.early_total_w4.saturating_add(src.early_total_w4);
    dst.early_total_w5 = dst.early_total_w5.saturating_add(src.early_total_w5);
    dst.held_miss = dst.held_miss.saturating_add(src.held_miss);
}

fn merge_column_judgments(dst: &mut Vec<ColumnJudgments>, src: &[ColumnJudgments]) {
    if dst.len() < src.len() {
        dst.resize(src.len(), ColumnJudgments::default());
    }
    for (dst, src) in dst.iter_mut().zip(src.iter().copied()) {
        add_column_judgments(dst, src);
    }
}

pub fn merge_course_score_columns<'a>(
    summary: &mut ScoreInfo,
    song_scores: impl IntoIterator<Item = &'a ScoreInfo>,
) {
    let mut columns = Vec::new();
    let mut noteskin = None;
    for song in song_scores {
        if song.side != summary.side {
            continue;
        }
        merge_column_judgments(&mut columns, &song.column_judgments);
        if noteskin.is_none() && song.noteskin.is_some() {
            noteskin.clone_from(&song.noteskin);
        }
    }
    summary.column_judgments = columns;
    if summary.noteskin.is_none() {
        summary.noteskin = noteskin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_judgments_resize_and_saturate() {
        let mut columns = vec![ColumnJudgments {
            w0: u32::MAX,
            held_miss: 1,
            ..Default::default()
        }];
        merge_column_judgments(
            &mut columns,
            &[
                ColumnJudgments {
                    w0: 1,
                    held_miss: 2,
                    ..Default::default()
                },
                ColumnJudgments {
                    miss: 3,
                    ..Default::default()
                },
            ],
        );

        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].w0, u32::MAX);
        assert_eq!(columns[0].held_miss, 3);
        assert_eq!(columns[1].miss, 3);
    }
}
