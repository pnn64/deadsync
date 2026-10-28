use deadsync_chart::{GameplayChartData, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    CourseDisplayTiming, CourseDisplayTotals, CourseLifeConfig, course_display_timing_for_stages,
    course_display_totals_for_chart, course_display_totals_for_chart_side,
};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::{self as profile_data, PlayStyle, PlayerSide};
use deadsync_score::{self as score_data, stage_stats};
use deadsync_theme::views::CourseTypeView;
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
    pub modifiers: String,
    pub gain_seconds: f32,
    pub gain_lives: i32,
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
    pub course_type: CourseTypeView,
    pub lives: i32,
    pub song_stub: Arc<SongData>,
    pub stages: Vec<CourseStageRuntime>,
    pub course_display_totals: [CourseDisplayTotals; MAX_PLAYERS],
    pub next_stage_index: usize,
    pub stage_summaries: Vec<stage_stats::StageSummary>,
}

impl CourseRunState {
    pub fn has_next_stage(&self) -> bool {
        !self.stages.is_empty()
            && (self.course_type == CourseTypeView::Endless
                || self.next_stage_index < self.stages.len())
    }

    pub fn next_stage(&self) -> Option<&CourseStageRuntime> {
        if self.stages.is_empty() {
            return None;
        }
        let index = if self.course_type == CourseTypeView::Endless {
            self.next_stage_index % self.stages.len()
        } else {
            self.next_stage_index
        };
        self.stages.get(index)
    }

    pub fn is_finished(&self, failed: bool) -> bool {
        failed
            || (self.course_type != CourseTypeView::Endless
                && self.next_stage_index >= self.stages.len())
    }
}

pub fn course_life_config_for_stage(
    course: &CourseRunState,
    chart_type: &str,
) -> [CourseLifeConfig; MAX_PLAYERS] {
    let Some(stage) = course.next_stage() else {
        return [CourseLifeConfig::Bar; MAX_PLAYERS];
    };
    match course.course_type {
        CourseTypeView::Nonstop | CourseTypeView::Endless => [CourseLifeConfig::Bar; MAX_PLAYERS],
        CourseTypeView::Oni => {
            let total_lives = u32::try_from(course.lives).unwrap_or(1).max(1);
            std::array::from_fn(|player| {
                let reward_lives = u32::try_from(stage.gain_lives).unwrap_or_else(|_| {
                    stage
                        .song
                        .chart_for_steps_index(chart_type, stage.steps_index[player])
                        .map_or(1, |chart| if chart.meter >= 8 { 2 } else { 1 })
                });
                CourseLifeConfig::Battery {
                    total_lives,
                    reward_lives,
                }
            })
        }
        CourseTypeView::Survival => {
            [CourseLifeConfig::Survival {
                gain_seconds: stage.gain_seconds,
            }; MAX_PLAYERS]
        }
    }
}

fn course_stage_runtime_from_plan(
    plan: CourseStagePlan,
    chart_type: &str,
) -> Option<CourseStageRuntime> {
    let steps_idx = plan
        .song
        .steps_index_for_chart_hash(chart_type, plan.chart_hash.as_str())?;
    Some(CourseStageRuntime {
        song: plan.song,
        steps_index: [steps_idx; MAX_PLAYERS],
        preferred_difficulty_index: [steps_idx; MAX_PLAYERS],
        modifiers: plan.modifiers,
        gain_seconds: plan.gain_seconds,
        gain_lives: plan.gain_lives,
    })
}

/// Loading can fail if a simfile disappears or changes after course selection.
/// Keep the loader explicit so this boundary can be exercised without global
/// scan configuration or external song packs.
fn add_course_stage_totals_with_loader(
    totals: &mut [CourseDisplayTotals; MAX_PLAYERS],
    stage: &CourseStageRuntime,
    chart_type: &str,
    load: &mut impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
) -> Result<(), String> {
    use deadsync_gameplay::GameplayInputPlayStyle as Style;
    let partner_style = [
        ("dance-couple", Style::Couple),
        ("pump-couple", Style::PumpCouple),
        ("dance-routine", Style::Routine),
        ("pump-routine", Style::PumpRoutine),
    ]
    .into_iter()
    .find_map(|(name, style)| name.eq_ignore_ascii_case(chart_type).then_some(style));
    let mut updated = *totals;
    let mut loaded: Option<(usize, GameplayChartData)> = None;
    for (player_idx, total) in updated.iter_mut().enumerate() {
        let steps_index = stage.steps_index[player_idx];
        let context = || {
            format!(
                "{}: {chart_type} chart at steps index {steps_index}",
                stage.song.simfile_path.display()
            )
        };
        let chart = stage
            .song
            .chart_for_steps_index(chart_type, steps_index)
            .ok_or_else(|| format!("{} is missing", context()))?;
        let add = if let Some(style) = partner_style {
            let index = stage
                .song
                .charts
                .iter()
                .position(|entry| std::ptr::eq(entry, chart))
                .ok_or_else(|| format!("{} is not in its song", context()))?;
            if loaded
                .as_ref()
                .is_none_or(|(loaded_index, _)| *loaded_index != index)
            {
                let mut payloads = load(&stage.song, &[index], 0.0)
                    .map_err(|error| format!("Cannot load {}: {error}", context()))?;
                if payloads.len() != 1 {
                    return Err(format!(
                        "Loader returned {} payloads for {} (expected one)",
                        payloads.len(),
                        context()
                    ));
                }
                loaded = payloads.pop().map(|payload| (index, payload));
            }
            let (_, payload) = loaded.as_ref().expect("one validated payload was loaded");
            course_display_totals_for_chart_side(chart, payload, style, player_idx)
        } else {
            course_display_totals_for_chart(chart)
        };
        total.possible_grade_points = total
            .possible_grade_points
            .saturating_add(add.possible_grade_points);
        total.total_steps = total.total_steps.saturating_add(add.total_steps);
        total.holds_total = total.holds_total.saturating_add(add.holds_total);
        total.rolls_total = total.rolls_total.saturating_add(add.rolls_total);
        total.mines_total = total.mines_total.saturating_add(add.mines_total);
    }
    *totals = updated;
    Ok(())
}

fn add_course_stage_totals(
    totals: &mut [CourseDisplayTotals; MAX_PLAYERS],
    stage: &CourseStageRuntime,
    chart_type: &str,
) -> Result<(), String> {
    add_course_stage_totals_with_loader(
        totals,
        stage,
        chart_type,
        &mut deadsync_simfile::app_runtime::load_gameplay_charts,
    )
}

pub fn append_endless_cycle(
    course: &mut CourseRunState,
    selection: SelectedCoursePlan,
    chart_type: &str,
) -> bool {
    match append_endless_cycle_with_loader(
        course,
        selection,
        chart_type,
        &mut deadsync_simfile::app_runtime::load_gameplay_charts,
    ) {
        Ok(appended) => appended,
        Err(error) => {
            log::warn!("Unable to append Endless course cycle: {error}");
            false
        }
    }
}

fn append_endless_cycle_with_loader(
    course: &mut CourseRunState,
    selection: SelectedCoursePlan,
    chart_type: &str,
    load: &mut impl FnMut(&SongData, &[usize], f32) -> Result<Vec<GameplayChartData>, String>,
) -> Result<bool, String> {
    if course.course_type != CourseTypeView::Endless || selection.path != course.path {
        return Ok(false);
    }
    let previous_len = course.stages.len();
    let mut totals = course.course_display_totals;
    for stage in selection
        .stages
        .into_iter()
        .filter_map(|plan| course_stage_runtime_from_plan(plan, chart_type))
    {
        if let Err(error) =
            add_course_stage_totals_with_loader(&mut totals, &stage, chart_type, load)
        {
            // The only mutations so far are appended stages. Drop those on
            // failure, retaining the original stages, cursor and totals.
            course.stages.truncate(previous_len);
            return Err(error);
        }
        course.stages.push(stage);
    }
    course.course_display_totals = totals;
    Ok(course.stages.len() != previous_len)
}

pub fn build_course_run_from_selection(
    selection: SelectedCoursePlan,
    chart_type: &str,
) -> Option<CourseRunState> {
    let mut stages = Vec::with_capacity(selection.stages.len());
    let mut course_display_totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
    for stage in selection.stages {
        if let Some(runtime) = course_stage_runtime_from_plan(stage, chart_type) {
            if let Err(error) =
                add_course_stage_totals(&mut course_display_totals, &runtime, chart_type)
            {
                log::warn!("Unable to build course run: {error}");
                return None;
            }
            stages.push(runtime);
        }
    }
    if stages.is_empty() {
        return None;
    }

    Some(CourseRunState {
        path: selection.path,
        name: selection.name,
        banner_path: selection.banner_path,
        score_hash: selection.score_hash,
        course_difficulty_name: selection.course_difficulty_name,
        course_meter: selection.course_meter,
        course_stepchart_label: selection.course_stepchart_label,
        course_type: selection.course_type,
        lives: selection.lives,
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

fn course_summary_totals(
    course: &CourseRunState,
) -> [stage_stats::CourseSummaryTotals; MAX_PLAYERS] {
    if course.course_type == CourseTypeView::Endless {
        [stage_stats::CourseSummaryTotals::default(); MAX_PLAYERS]
    } else {
        course
            .course_display_totals
            .map(|total| stage_stats::CourseSummaryTotals {
                possible_grade_points: total.possible_grade_points,
                total_steps: total.total_steps,
                holds_total: total.holds_total,
                rolls_total: total.rolls_total,
                mines_total: total.mines_total,
            })
    }
}

pub fn build_course_summary_stage(course: &CourseRunState) -> Option<stage_stats::StageSummary> {
    let totals = course_summary_totals(course);
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

/// The Routine course result both participants share, which the course saves
/// for the master player only.
pub fn routine_shared_course_player(
    course: &CourseRunState,
    summary: &stage_stats::StageSummary,
    play_style: PlayStyle,
) -> Option<stage_stats::PlayerStageSummary> {
    if !play_style.is_routine() {
        return None;
    }
    let [Some(first), Some(second)] = &summary.players else {
        return None;
    };
    Some(stage_stats::routine_shared_course_player(
        first,
        second,
        course_summary_totals(course),
    ))
}

/// The result each side saves for a course: StageStats::CommitScores saves
/// one shared Routine result, for the master player.
pub fn course_summary_records<'a>(
    summary: &'a stage_stats::StageSummary,
    routine_shared: Option<&'a stage_stats::PlayerStageSummary>,
) -> [Option<&'a stage_stats::PlayerStageSummary>; MAX_PLAYERS] {
    match routine_shared {
        Some(shared) => std::array::from_fn(|idx| (idx == 0).then_some(shared)),
        None => summary.players.each_ref().map(Option::as_ref),
    }
}

pub fn score_info_from_stage(
    stage: &stage_stats::StageSummary,
    side: PlayerSide,
) -> Option<ScoreInfo> {
    let idx = profile_data::player_side_index(side);
    score_info_from_stage_with_record(stage, side, stage.players[idx].as_ref())
}

/// `record` is the result saved for `side`, which record highlights rank.
/// None means nothing is saved for `side`, so it highlights no record.
fn score_info_from_stage_with_record(
    stage: &stage_stats::StageSummary,
    side: PlayerSide,
    record: Option<&stage_stats::PlayerStageSummary>,
) -> Option<ScoreInfo> {
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
        judgment_counts: player.judgment_counts,
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

pub fn build_course_summary_score_info(
    stage: &stage_stats::StageSummary,
    routine_shared: Option<&stage_stats::PlayerStageSummary>,
    course_graph_stages: &[Vec<CourseGraphStage>; MAX_PLAYERS],
    play_style: PlayStyle,
    active_side: PlayerSide,
) -> [Option<ScoreInfo>; MAX_PLAYERS] {
    let mut score_info = std::array::from_fn(|_| None);
    let records = course_summary_records(stage, routine_shared);
    match play_style {
        PlayStyle::Versus
        | PlayStyle::PumpVersus
        | PlayStyle::Couple
        | PlayStyle::PumpCouple
        | PlayStyle::Routine
        | PlayStyle::PumpRoutine => {
            for side in [PlayerSide::P1, PlayerSide::P2] {
                let idx = profile_data::player_side_index(side);
                score_info[idx] = score_info_from_stage_with_record(stage, side, records[idx]);
                if let Some(score) = score_info[idx].as_mut() {
                    score
                        .course_graph_stages
                        .clone_from(&course_graph_stages[idx]);
                }
            }
        }
        PlayStyle::Single | PlayStyle::Double | PlayStyle::PumpSingle | PlayStyle::PumpDouble => {
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
    let mut out = std::array::from_fn(|_| Vec::with_capacity(course.stages.len()));
    for stage in &course.stages {
        let build = |index| {
            stage
                .song
                .chart_for_steps_index(chart_type, index)
                .map(|chart| CourseGraphStage {
                    chart: Arc::new(chart.clone()),
                    song_last_second: stage.song.precise_last_second(),
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
#[path = "../../../tests/perf/course_updates.rs"]
mod update_tests;

#[cfg(test)]
mod partner_tests;
