pub(super) fn course_stage_runtime_from_plan(
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
        modifiers: plan.modifiers.clone(),
        gain_seconds: plan.gain_seconds,
        gain_lives: plan.gain_lives,
    })
}

pub(super) fn append_endless_cycle(
    course: &mut CourseRunState,
    selection: SelectedCoursePlan,
    chart_type: &str,
) -> bool {
    if course.course_type != CourseTypeView::Endless || selection.path != course.path {
        return false;
    }
    let stages: Vec<_> = selection
        .stages
        .iter()
        .filter_map(|stage| course_stage_runtime_from_plan(stage, chart_type))
        .collect();
    if stages.is_empty() {
        return false;
    }
    for stage in &stages {
        add_course_stage_totals(&mut course.course_display_totals, stage, chart_type);
    }
    course.stages.extend(stages);
    true
}

pub(super) fn build_course_run_from_selection(
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
        add_course_stage_totals(&mut course_display_totals, stage, chart_type);
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

pub(super) fn build_course_graph_stages(
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
