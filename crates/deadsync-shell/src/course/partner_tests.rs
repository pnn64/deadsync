use super::*;
use deadsync_simfile::cache::{
    SerializableSongData, build_requested_gameplay_charts, build_song_meta,
};
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

fn plan(song: &Arc<SongData>, count: usize) -> SelectedCoursePlan {
    SelectedCoursePlan {
        path: "tests/partner-course.crs".into(),
        name: "Partner course".into(),
        banner_path: None,
        score_hash: "partner-course".into(),
        course_difficulty_name: "Hard".into(),
        course_meter: Some(8),
        course_stepchart_label: "Hard 8".into(),
        course_type: CourseTypeView::Endless,
        lives: 4,
        song_stub: Arc::clone(song),
        stages: (0..count)
            .map(|i| CourseStagePlan {
                song: Arc::clone(song),
                chart_hash: song.charts[0].short_hash.clone(),
                modifiers: format!("stage-{i}"),
                gain_seconds: 10.0,
                gain_lives: 1,
            })
            .collect(),
    }
}

fn empty_course(song: &Arc<SongData>) -> CourseRunState {
    let p = plan(song, 0);
    CourseRunState {
        path: p.path,
        name: p.name,
        banner_path: p.banner_path,
        score_hash: p.score_hash,
        course_difficulty_name: p.course_difficulty_name,
        course_meter: p.course_meter,
        course_stepchart_label: p.course_stepchart_label,
        course_type: p.course_type,
        lives: p.lives,
        song_stub: p.song_stub,
        stages: Vec::new(),
        course_display_totals: [CourseDisplayTotals::default(); MAX_PLAYERS],
        next_stage_index: 0,
        stage_summaries: Vec::new(),
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
            append_endless_cycle_with_loader(
                &mut course,
                plan(&song, 2),
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
        append_endless_cycle_with_loader(&mut course, plan(&song, 1), kind, &mut load).unwrap();
        course.next_stage_index = 1;
        let original_song = Arc::clone(&course.stages[0].song);
        let original_modifier = course.stages[0].modifiers.as_ptr();
        let mut calls = 0;
        let error = append_endless_cycle_with_loader(
            &mut course,
            plan(&song, 3),
            kind,
            &mut |song, indices, offset| {
                calls += 1;
                if calls == 2 {
                    Err("simfile removed during reroll".into())
                } else {
                    load(song, indices, offset)
                }
            },
        )
        .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("tests/partner-course.sm") && error.contains(kind));
        assert!(error.contains("simfile removed during reroll"));
        assert_eq!(course.stages.len(), 1);
        assert_eq!(course.next_stage_index, 1);
        assert!(Arc::ptr_eq(&course.stages[0].song, &original_song));
        assert_eq!(course.stages[0].modifiers.as_ptr(), original_modifier);
        assert_eq!(course.next_stage().unwrap().modifiers, "stage-0");
        assert_totals(&course.course_display_totals, 1);
        assert!(
            append_endless_cycle_with_loader(&mut course, plan(&song, 3), kind, &mut load).unwrap()
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
        let error = append_endless_cycle_with_loader(
            &mut course,
            plan(&song, 1),
            "dance-routine",
            &mut |_, indices, offset| {
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
fn second_participant_missing_chart_does_not_commit_first_participants_totals() {
    let (song, _) = partner_song("dance-routine");
    let mut stage =
        course_stage_runtime_from_plan(plan(&song, 1).stages.pop().unwrap(), "dance-routine")
            .unwrap();
    stage.steps_index[1] = usize::MAX;
    let mut totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
    let (_, parsed) = partner_song("dance-routine");
    let error = add_course_stage_totals_with_loader(
        &mut totals,
        &stage,
        "dance-routine",
        &mut |_, indices, offset| build_requested_gameplay_charts(&parsed, indices, offset),
    )
    .unwrap_err();
    assert!(error.contains("is missing"));
    for total in totals {
        assert_eq!(total.total_steps, 0);
        assert_eq!(total.possible_grade_points, 0);
    }
}
