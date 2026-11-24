use crate::session_results::EvaluationScoreView;
use deadsync_chart::{GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::course_run::CourseRun;
use deadsync_profile_gameplay::results::{
    course_summary_records, course_summary_stage, routine_shared_course_result,
};
use deadsync_online::score_compat as scores;
use deadsync_profile::{self as profile_data, PlayStyle, PlayerSide};
use deadsync_score::{self as score_data, stage_stats};
use deadsync_simfile::course_selection::{EndlessCourseSource, PreparedCourseStart};
use deadsync_theme::views::CourseGraphStageView;
use std::sync::Arc;

#[derive(Clone)]
pub struct CourseRunState {
    pub run: CourseRun,
    pub endless_source: Option<Arc<EndlessCourseSource>>,
    pub song: Arc<SongData>,
    pub stage_summaries: Vec<stage_stats::StageSummary>,
}

pub fn renew_endless_cycle(course: &mut CourseRunState, chart_type: &str) -> bool {
    let Some(source) = &course.endless_source else {
        return false;
    };
    match course.run.append_endless_cycle(
        source.next_cycle(),
        chart_type,
        deadsync_simfile::app_runtime::load_gameplay_charts,
    ) {
        Ok(appended) => appended,
        Err(error) => {
            log::warn!("Unable to append Endless course cycle: {error}");
            false
        }
    }
}

pub fn build_course_run_from_selection(
    start: PreparedCourseStart,
    chart_type: &str,
) -> Option<CourseRunState> {
    match build_course_run_from_selection_with_loader(
        start,
        chart_type,
        deadsync_simfile::app_runtime::load_gameplay_charts,
    ) {
        Ok(course) => course,
        Err(error) => {
            log::warn!("Unable to build course run: {error}");
            None
        }
    }
}

fn build_course_run_from_selection_with_loader(
    start: PreparedCourseStart,
    chart_type: &str,
    load: impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
) -> Result<Option<CourseRunState>, String> {
    let Some(run) = CourseRun::prepare(start.plan, chart_type, load)? else {
        return Ok(None);
    };
    Ok(Some(CourseRunState {
        run,
        endless_source: start.endless_source.map(Arc::new),
        song: start.song,
        stage_summaries: Vec::new(),
    }))
}

pub fn build_course_summary_stage(course: &CourseRunState) -> Option<stage_stats::StageSummary> {
    course_summary_stage(&course.run, &course.stage_summaries, &course.song)
}

pub fn routine_shared_course_player(
    course: &CourseRunState,
    summary: &stage_stats::StageSummary,
    play_style: PlayStyle,
) -> Option<stage_stats::PlayerStageSummary> {
    routine_shared_course_result(&course.run, summary, play_style)
}

#[cfg(test)]
pub fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: PlayerSide,
) -> Option<EvaluationScoreView> {
    let idx = profile_data::player_side_index(side);
    score_info_from_stage_with_record(
        stage, side, stage.players[idx].as_ref(), &profile_data::runtime_profile_for_side(side),
    )
}

/// `record` is the result saved for `side`, which record highlights rank.
/// None means nothing is saved for `side`, so it highlights no record.
fn score_info_from_stage_with_record(
    stage: &stage_stats::StageSummary,
    side: PlayerSide,
    record: Option<&stage_stats::PlayerStageSummary>,
    profile: &profile_data::Profile,
) -> Option<EvaluationScoreView> {
    let idx = profile_data::player_side_index(side);
    let player = stage.players[idx].as_ref()?;
    let chart_hash = player.chart.short_hash.as_str();
    let machine_records = scores::get_machine_leaderboard_local(chart_hash, usize::MAX);
    let personal_records =
        scores::get_personal_leaderboard_local_for_side(chart_hash, side, usize::MAX);
    let record_percent = record.map_or(player.score_percent, |record| record.score_percent);
    let machine_record_highlight_rank =
        score_data::leaderboard_rank_for_score(machine_records.as_slice(), record_percent);
    let personal_record_highlight_rank =
        score_data::leaderboard_rank_for_score(personal_records.as_slice(), record_percent);
    let local_score_valid = record.is_some_and(|record| record.score_valid && !record.disqualified);
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
    let speed_mod = profile.scroll_speed;
    let mods_text = profile_data::evaluation_mods_text(profile, speed_mod);
    let disabled_timing_windows = profile.timing_windows.disabled_windows();

    Some(EvaluationScoreView {
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
        judgment_counts: player.judgment_counts,
        score_percent: player.score_percent,
        earned_grade_points: player.earned_grade_points,
        possible_grade_points: player.possible_grade_points,
        grade: player.grade,
        speed_mod,
        mods_text,
        max_combo: player.max_combo,
        combo_runs: player.combo_runs.clone(),
        jumps: player.jumps,
        hands_achieved: player.hands_achieved,
        hands_total: player.hands_total,
        holds_held: player.holds_held,
        holds_held_for_score: player.holds_held_for_score,
        dropped_hold_or_roll: player.dropped_hold_or_roll,
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
        steps_life: player.steps_life.clone(),
        fail_time: player.fail_time.or_else(|| {
            (player.grade == score_data::Grade::Failed).then_some(stage.duration_seconds)
        }),
        window_counts: player.window_counts,
        window_counts_10ms: player.window_counts_10ms,
        ex_score_percent: player.ex_score_percent,
        hard_ex_score_percent: player.hard_ex_score_percent,
        calories_burned: player.calories_burned,
        column_judgments: player.column_judgments.clone(),
        noteskin: None,
        show_fa_plus_window: player.show_w0,
        show_ex_score: player.show_ex_score,
        show_hard_ex_score: player.show_hard_ex_score,
        show_fa_plus_pane: player.show_fa_plus_pane,
        track_early_judgments: player.track_early_judgments,
        dim_post_fail_scatter: player.dim_post_fail_scatter,
        golf_score: player.golf_score,
        golf_par: player.golf_par,
        disabled_timing_windows,
        machine_records,
        machine_record_highlight_rank,
        personal_records,
        personal_record_highlight_rank,
        show_machine_personal_split: !earned_machine_record && earned_top2_personal,
    })
}

pub fn course_summary_profiles(
    stage: &stage_stats::StageSummary,
    play_style: PlayStyle,
    active_side: PlayerSide,
) -> [Option<(PlayerSide, profile_data::Profile)>; MAX_PLAYERS] {
    let profiles = profile_data::runtime_lock_profiles();
    std::array::from_fn(|slot| {
        if slot >= play_style.player_count() {
            return None;
        }
        let side = if play_style.player_count() > 1 {
            profile_data::player_side_for_index(slot)
        } else {
            active_side
        };
        let idx = profile_data::player_side_index(side);
        stage.players[idx].as_ref().map(|_| (side, profiles[idx].clone()))
    })
}

pub fn build_course_summary_score_info(
    stage: &stage_stats::StageSummary,
    routine_shared: Option<&stage_stats::PlayerStageSummary>,
    course_graph_stages: &[Vec<CourseGraphStageView>; MAX_PLAYERS],
    play_style: PlayStyle,
    profiles: &[Option<(PlayerSide, profile_data::Profile)>; MAX_PLAYERS],
) -> [Option<EvaluationScoreView>; MAX_PLAYERS] {
    let mut score_info = std::array::from_fn(|_| None);
    let records = course_summary_records(stage, routine_shared);
    let solo = play_style.player_count() == 1;
    for (slot, profile) in profiles.iter().enumerate() {
        let Some((side, profile)) = profile else { continue };
        let idx = profile_data::player_side_index(*side);
        let record = if solo { stage.players[idx].as_ref() } else { records[idx] };
        score_info[slot] = score_info_from_stage_with_record(stage, *side, record, profile);
        if let Some(score) = score_info[slot].as_mut() {
            score.course_graph_stages.clone_from(&course_graph_stages[idx]);
        }
    }
    score_info
}

pub fn build_course_graph_stages(
    course: &CourseRunState,
    chart_type: &str,
) -> [Vec<CourseGraphStageView>; MAX_PLAYERS] {
    let mut out = std::array::from_fn(|_| Vec::with_capacity(course.run.stages.len()));
    for stage in &course.run.stages {
        let build = |index| {
            stage
                .song
                .chart_for_steps_index(chart_type, index)
                .map(|chart| CourseGraphStageView {
                    chart: Arc::new(chart.clone()),
                    song_last_second: stage.song.precise_last_second(),
                    step_seconds: stage_stats::song_step_seconds(&stage.song),
                })
        };
        let first = build(stage.steps_index[0]);
        let second = if stage.steps_index[0] == stage.steps_index[1] {
            let Some(first) = &first else {
                continue;
            };
            Some(first.clone())
        } else {
            build(stage.steps_index[1])
        };
        let [p1, p2] = &mut out;
        p1.extend(first);
        p2.extend(second);
    }
    out
}


#[cfg(test)]
mod partner_tests;
