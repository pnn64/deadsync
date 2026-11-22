use super::*;
use deadsync_chart::course::{CourseCyclePlan, CourseStagePlan};
use deadsync_gameplay::CourseDisplayTotals;
use deadsync_gameplay::CourseLifeConfig;
use deadsync_gameplay::course_progress::{CourseProgress, CourseRenewal};
use deadsync_rules::course::CourseType;
use deadsync_simfile::cache::{SerializableSongData, build_requested_gameplay_charts, build_song_meta};
use deadsync_simfile::song::{ParseSongOptions, parse_song_bytes};
use std::path::Path;

/// Authored SM fixture: P1 has two rows including a hold and one mine;
/// P2 has three rows including a roll. The first row is simultaneous.
fn partner_song(kind: &str) -> (Arc<SongData>, SerializableSongData) {
    let width = if kind.starts_with("pump") { 10 } else { 8 };
    let line = |column, symbol: char| {
        (0..width)
            .map(|col| if col == column { symbol } else { '0' })
            .collect::<String>()
    };
    let p1 = ['1', '2', '3', 'M'].map(|s| line(0, s));
    let p2 = ['1', '4', '3', '1'].map(|s| line(width - 1, s));
    let notes = if kind.ends_with("routine") {
        format!("{}\n&\n{}", p1.join("\n"), p2.join("\n"))
    } else {
        p1.iter()
            .zip(p2.iter())
            .map(|(a, b)| {
                a.chars()
                    .zip(b.chars())
                    .map(|(x, y)| if y != '0' { y } else { x })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let source = format!(
        "#TITLE:Partner course;\n#BPMS:0=120;\n#NOTES:{kind}::Hard:8:0,0,0,0,0:\n{notes}\n;"
    );
    let parsed = parse_song_bytes(
        source.as_bytes(),
        "sm",
        Path::new("tests/partner-course.sm"),
        &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        |_| 10.0,
    )
    .unwrap();
    (Arc::new(build_song_meta(parsed.clone(), 0.0)), parsed)
}

fn stage_plans(song: &Arc<SongData>, count: usize) -> Vec<CourseStagePlan> {
    (0..count)
        .map(|i| CourseStagePlan {
            song: Arc::clone(song),
            chart_hash: song.charts[0].short_hash.clone(),
            modifiers: format!("stage-{i}"),
            gain_seconds: 10.0,
            gain_lives: 1,
        })
        .collect()
}

fn plan(song: &Arc<SongData>, count: usize) -> CourseSessionPlan {
    CourseSessionPlan {
        path: "tests/partner-course.crs".into(),
        score_hash: "partner-course".into(),
        course_difficulty_name: "Hard".into(),
        course_meter: Some(8),
        course_type: CourseType::Endless,
        lives: 4,
        stages: stage_plans(song, count),
    }
}

fn cycle_plan(song: &Arc<SongData>, count: usize) -> CourseCyclePlan {
    CourseCyclePlan {
        path: "tests/partner-course.crs".into(),
        stages: stage_plans(song, count),
    }
}

fn assert_totals(totals: &[CourseDisplayTotals; MAX_PLAYERS], multiplier: u32) {
    for (side, total) in totals.iter().enumerate() {
        assert_eq!(total.total_steps, (2 + side as u32) * multiplier);
        assert_eq!(total.holds_total, u32::from(side == 0) * multiplier);
        assert_eq!(total.rolls_total, u32::from(side == 1) * multiplier);
        assert_eq!(total.mines_total, u32::from(side == 0) * multiplier);
        let expected = deadsync_rules::judgment::max_grade_points(
            total.total_steps,
            total.holds_total,
            total.rolls_total,
            0,
        );
        assert_eq!(total.possible_grade_points, expected);
    }
}

fn presentation(song: &Arc<SongData>) -> CoursePresentationView {
    CoursePresentationView {
        name: "Partner course".into(),
        banner_path: None,
        course_stepchart_label: "Hard 8".into(),
        song_stub: Arc::clone(song),
    }
}

#[test]
fn startup_uses_game_stages_and_rules_independently_of_presentation() {
    let kind = "dance-routine";
    let (song, parsed) = partner_song(kind);
    let mut unrelated_stub = (*song).clone();
    unrelated_stub.simfile_path = "presentation/unrelated.ssc".into();
    unrelated_stub.charts.clear();
    unrelated_stub.music_length_seconds = 5000.0;
    unrelated_stub.total_length_seconds = 5000;
    unrelated_stub.precise_last_second_seconds = 5000.0;
    let variants = [
        presentation(&song),
        CoursePresentationView {
            name: "Unrelated display name".into(),
            banner_path: Some("presentation/other-banner.png".into()),
            course_stepchart_label: "Edit 999".into(),
            song_stub: Arc::new(unrelated_stub),
        },
    ];
    for display in variants {
        let expected_display = display.clone();
        let name_pointer = display.name.as_ptr();
        let label_pointer = display.course_stepchart_label.as_ptr();
        let mut selection = plan(&song, 2);
        selection.course_type = CourseType::Oni;
        selection.lives = 7;
        selection.stages[0].gain_lives = -1;
        selection.stages[0].gain_seconds = f32::from_bits(0x7fc0_1234);
        let modifier_pointer = selection.stages[0].modifiers.as_ptr();
        let score_pointer = selection.score_hash.as_ptr();
        let difficulty_pointer = selection.course_difficulty_name.as_ptr();
        // Missing hashes remain skippable without changing surviving payloads.
        selection.stages.insert(
            0,
            CourseStagePlan {
                song: song.clone(),
                chart_hash: "missing".into(),
                modifiers: "unresolved".into(),
                gain_seconds: 1.0,
                gain_lives: 99,
            },
        );
        let mut loads = 0;
        let course = build_course_run_from_selection_with_loader(
            selection,
            display,
            None,
            kind,
            |loaded_song, indices, offset| {
                loads += 1;
                assert!(std::ptr::eq(loaded_song, song.as_ref()));
                assert_eq!(indices, &[0]);
                assert_eq!(offset, 0.0);
                build_requested_gameplay_charts(&parsed, indices, offset)
            },
        )
        .unwrap()
        .expect("the two valid stages start a course");

        assert_eq!(loads, 2);
        assert_eq!(course.run.path, Path::new("tests/partner-course.crs"));
        assert_eq!(course.run.score_hash, "partner-course");
        assert_eq!(course.run.score_hash.as_ptr(), score_pointer);
        assert_eq!(course.run.course_difficulty_name, "Hard");
        assert_eq!(course.run.course_difficulty_name.as_ptr(), difficulty_pointer);
        assert_eq!(course.run.course_meter, Some(8));
        assert_eq!(course.run.course_type, CourseType::Oni);
        assert_eq!(course.run.lives, 7);
        assert_eq!(course.run.stages.len(), 2);
        for (index, stage) in course.run.stages.iter().enumerate() {
            assert!(Arc::ptr_eq(&stage.song, &song));
            // Hard is runtime slot 3; the loader uses source chart index 0.
            assert_eq!(stage.steps_index, [3; MAX_PLAYERS]);
            assert_eq!(stage.preferred_difficulty_index, [3; MAX_PLAYERS]);
            assert_eq!(stage.modifiers, format!("stage-{index}"));
        }
        assert_eq!(course.run.stages[0].modifiers.as_ptr(), modifier_pointer);
        assert_eq!(course.run.stages[0].gain_seconds.to_bits(), 0x7fc0_1234);
        assert_eq!(course.run.stages[0].gain_lives, -1);
        assert_totals(&course.run.course_display_totals, 2);
        assert_eq!(
            course.run.entry(false).life_config(kind),
            [CourseLifeConfig::Battery {
                total_lives: 7,
                reward_lives: 2,
            }; MAX_PLAYERS],
        );
        assert_eq!(course.run.progress, CourseProgress::default());
        assert!(course.run.has_next_stage());
        assert!(!course.run.is_finished(false));
        assert_eq!(course.run.next_stage().unwrap().modifiers, "stage-0");
        assert_eq!(course.run.renewal(), CourseRenewal::NotRequired);
        assert!(course.stage_summaries.is_empty());
        assert_eq!(
            course.run.total_seconds(),
            2.0 * song.precise_last_second()
        );
        assert_eq!(course.presentation.name, expected_display.name);
        assert_eq!(course.presentation.name.as_ptr(), name_pointer);
        assert_eq!(
            course.presentation.banner_path,
            expected_display.banner_path
        );
        assert_eq!(
            course.presentation.course_stepchart_label,
            expected_display.course_stepchart_label,
        );
        assert_eq!(
            course.presentation.course_stepchart_label.as_ptr(),
            label_pointer
        );
        assert!(Arc::ptr_eq(
            &course.presentation.song_stub,
            &expected_display.song_stub,
        ));
    }
}

fn empty_course(song: &Arc<SongData>) -> CourseRun {
    let p = plan(song, 0);
    CourseRun {
        path: p.path,
        score_hash: p.score_hash,
        course_difficulty_name: p.course_difficulty_name,
        course_meter: p.course_meter,
        course_type: p.course_type,
        lives: p.lives,
        stages: Vec::new(),
        course_display_totals: [CourseDisplayTotals::default(); MAX_PLAYERS],
        progress: CourseProgress::default(),
    }
}

#[test]
fn startup_skips_unresolved_stages_and_stops_at_the_first_load_failure() {
    let kind = "dance-routine";
    let (song, parsed) = partner_song(kind);
    let mut unresolved = plan(&song, 1);
    unresolved.stages[0].chart_hash = "missing".into();
    for selection in [plan(&song, 0), unresolved] {
        assert!(
            CourseRun::prepare(selection, kind, |_, _, _| panic!(
                "unresolved stages must not load payloads"
            ),)
            .unwrap()
            .is_none()
        );
    }
    let mut loads = 0;
    let result = CourseRun::prepare(plan(&song, 3), kind, |_, indices, offset| {
        loads += 1;
        if loads == 2 {
            Err("simfile removed during startup".into())
        } else {
            build_requested_gameplay_charts(&parsed, indices, offset)
        }
    });
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("a failed stage load must reject the startup"),
    };
    assert_eq!(loads, 2);
    assert!(error.contains("tests/partner-course.sm") && error.contains(kind));
    assert!(error.contains("simfile removed during startup"));
}

#[test]
fn cycle_rejection_does_not_load_or_mutate_the_existing_run() {
    let kind = "dance-routine";
    let (song, parsed) = partner_song(kind);
    let original = CourseRun::prepare(plan(&song, 1), kind, |_, indices, offset| {
        build_requested_gameplay_charts(&parsed, indices, offset)
    })
    .unwrap()
    .unwrap();
    for reason in 0..3 {
        let mut course = original.clone();
        let mut cycle = cycle_plan(&song, 1);
        match reason {
            0 => course.course_type = CourseType::Nonstop,
            1 => course.progress.interrupt(),
            _ => cycle.path = "tests/other-course.crs".into(),
        }
        let progress = course.progress;
        let modifier_pointer = course.stages[0].modifiers.as_ptr();
        assert!(
            !course
                .append_endless_cycle(cycle, kind, |_, _, _| {
                    panic!("rejected cycles must not load payloads")
                })
                .unwrap()
        );
        assert_eq!(course.stages.len(), 1);
        assert!(Arc::ptr_eq(&course.stages[0].song, &song));
        assert_eq!(course.stages[0].modifiers.as_ptr(), modifier_pointer);
        assert_eq!(course.progress, progress);
        assert_totals(&course.course_display_totals, 1);
    }
}

#[test]
fn partner_course_totals_split_authored_ownership_and_load_each_chart_once() {
    for kind in [
        "dance-routine",
        "pump-routine",
        "dance-couple",
        "pump-couple",
    ] {
        let (song, parsed) = partner_song(kind);
        let mut course = empty_course(&song);
        let mut calls = 0;
        let mut loader = |_: &SongData, indices: &[usize], offset: f32| {
            calls += 1;
            build_requested_gameplay_charts(&parsed, indices, offset)
        };
        assert!(
            course
                .append_endless_cycle(
                    cycle_plan(&song, 2),
                    &kind.to_ascii_uppercase(),
                    &mut loader
                )
                .unwrap()
        );
        assert_eq!(
            calls, 2,
            "one payload load per stage, shared by both participants"
        );
        assert_eq!(course.stages.len(), 2);
        assert_totals(&course.course_display_totals, 2);
    }
}

#[test]
fn endless_cycle_load_failure_rolls_back_stages_totals_and_cursor_then_can_retry() {
    for kind in [
        "dance-routine",
        "pump-routine",
        "dance-couple",
        "pump-couple",
    ] {
        let (song, parsed) = partner_song(kind);
        let mut course = empty_course(&song);
        let mut load = |_: &SongData, indices: &[usize], offset: f32| {
            build_requested_gameplay_charts(&parsed, indices, offset)
        };
        course
            .append_endless_cycle(cycle_plan(&song, 1), kind, &mut load)
            .unwrap();
        course.progress = CourseProgress::new(1);
        let original_progress = course.progress;
        let original_song = Arc::clone(&course.stages[0].song);
        let original_modifier = course.stages[0].modifiers.as_ptr();
        let mut calls = 0;
        let error = course
            .append_endless_cycle(cycle_plan(&song, 3), kind, |song, indices, offset| {
                calls += 1;
                if calls == 2 {
                    Err("simfile removed during reroll".into())
                } else {
                    load(song, indices, offset)
                }
            })
            .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("tests/partner-course.sm") && error.contains(kind));
        assert!(error.contains("simfile removed during reroll"));
        assert_eq!(course.stages.len(), 1);
        assert_eq!(course.progress, original_progress);
        assert!(Arc::ptr_eq(&course.stages[0].song, &original_song));
        assert_eq!(course.stages[0].modifiers.as_ptr(), original_modifier);
        assert_eq!(course.next_stage().unwrap().modifiers, "stage-0");
        assert_totals(&course.course_display_totals, 1);
        assert!(
            course
                .append_endless_cycle(cycle_plan(&song, 3), kind, &mut load)
                .unwrap()
        );
        assert_eq!(course.stages.len(), 4);
        assert_totals(&course.course_display_totals, 4);
    }
}

#[test]
fn partner_course_rejects_missing_or_extra_payloads_without_panicking_or_mutating() {
    let (song, parsed) = partner_song("dance-routine");
    for count in [0, 2] {
        let mut course = empty_course(&song);
        let error = course
            .append_endless_cycle(
                cycle_plan(&song, 1),
                "dance-routine",
                |_, indices, offset| {
                    let payload = build_requested_gameplay_charts(&parsed, indices, offset)?
                        .pop()
                        .unwrap();
                    Ok(vec![payload; count])
                },
            )
            .unwrap_err();
        assert!(error.contains(&format!("{count} payloads")));
        assert!(course.stages.is_empty());
        assert_eq!(course.course_display_totals[0].total_steps, 0);
    }
}

#[test]
fn interrupted_endless_is_final_without_player_failure_and_cannot_resume() {
    let (song, parsed) = partner_song("dance-routine");
    let mut course = empty_course(&song);
    course
        .append_endless_cycle(
            cycle_plan(&song, 1),
            "dance-routine",
            |_, indices, offset| build_requested_gameplay_charts(&parsed, indices, offset),
        )
        .unwrap();
    course.progress = CourseProgress::new(1);
    course.progress.interrupt();
    assert!(course.is_finished(false));
    assert!(!course.has_next_stage());
    assert!(course.next_stage().is_none());
    let totals = course.course_display_totals;
    assert!(
        !course
            .append_endless_cycle(cycle_plan(&song, 1), "dance-routine", |_, _, _| panic!(
                "an interrupted course must not load another chart"
            ),)
            .unwrap()
    );
    assert_eq!(course.stages.len(), 1);
    let counters = |total: CourseDisplayTotals| {
        (
            total.possible_grade_points,
            total.total_steps,
            total.holds_total,
            total.rolls_total,
            total.mines_total,
        )
    };
    assert_eq!(
        course.course_display_totals.map(counters),
        totals.map(counters)
    );
}
