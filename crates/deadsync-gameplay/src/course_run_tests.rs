use super::*;
use crate::runtime_regression_tests::{regression_payload_with_segments, regression_song};

#[test]
fn reload_borrows_the_active_stage_and_retains_the_canonical_cursor() {
    let song = Arc::new(regression_song());
    let chart = &song.charts[0];
    let mut run = CourseRun::prepare(
        CourseSessionPlan {
            path: "tests/course.crs".into(),
            score_hash: "test-hash".into(),
            course_difficulty_name: "Hard".into(),
            course_meter: Some(8),
            course_type: CourseType::Oni,
            lives: 4,
            stages: [1, 2]
                .map(|gain_lives| CourseStagePlan {
                    song: Arc::clone(&song),
                    chart_hash: chart.short_hash.clone(),
                    modifiers: format!("stage-{gain_lives}"),
                    gain_seconds: 0.0,
                    gain_lives,
                })
                .into(),
        },
        "dance-single",
        |_, _, _| panic!("a single chart only needs its metadata"),
    )
    .unwrap()
    .unwrap();
    run.progress.finish_entry(false);

    let reload = run.entry(true);
    let next = run.entry(false);
    assert!(std::ptr::eq(reload.next_stage().unwrap(), &run.stages[0]));
    assert!(std::ptr::eq(next.next_stage().unwrap(), &run.stages[1]));
    assert_eq!(reload.stage_number(), 1);
    assert_eq!(next.stage_number(), 2);
    assert_eq!(reload.finite_stage_count(), Some(2));
    assert!(reload.lead_in_timing().is_none());
    let lead_in = next.lead_in_timing().unwrap();
    assert_eq!(lead_in.min_seconds_to_step, 4.0);
    assert_eq!(lead_in.min_seconds_to_music, 0.0);
    assert_eq!(
        reload.life_config(&chart.chart_type),
        [CourseLifeConfig::Battery {
            total_lives: 4,
            reward_lives: 1,
        }; MAX_PLAYERS],
    );
    assert_eq!(
        next.life_config(&chart.chart_type),
        [CourseLifeConfig::Battery {
            total_lives: 4,
            reward_lives: 2,
        }; MAX_PLAYERS],
    );
    assert_eq!(run.progress.next_stage_index(), 1);
}

#[test]
fn second_participant_missing_chart_does_not_commit_first_participants_totals() {
    let mut song = regression_song();
    song.charts[0].chart_type = "dance-routine".into();
    let song = Arc::new(song);
    let plan = CourseStagePlan {
        chart_hash: song.charts[0].short_hash.clone(),
        song,
        modifiers: String::new(),
        gain_seconds: 0.0,
        gain_lives: 0,
    };
    let mut stage = course_stage_runtime_from_plan(plan, "dance-routine").unwrap();
    stage.steps_index[1] = usize::MAX;
    let mut totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
    let error = add_course_stage_totals_with_loader(
        &mut totals,
        &mut stage,
        "dance-routine",
        &mut |_, _, _| {
            Ok(vec![regression_payload_with_segments(
                Default::default(),
                96,
            )])
        },
    )
    .unwrap_err();
    assert!(error.contains("is missing"));
    for total in totals {
        assert_eq!(total.total_steps, 0);
        assert_eq!(total.possible_grade_points, 0);
    }
}
