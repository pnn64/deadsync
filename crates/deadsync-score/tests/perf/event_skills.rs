use super::*;
use std::hint::black_box;

#[path = "event_skills_baseline.rs"]
mod baseline;

fn fixture(count: usize, long: bool, spare: bool) -> SubmitEventProgressInput {
    let mut skills: Vec<_> = (0..count)
        .map(|i| {
            format!(
                "Skill {i}: 日本😀 +{}",
                if long {
                    "long improvement ".repeat(64)
                } else {
                    "1".to_string()
                }
            )
        })
        .collect();
    if spare {
        skills.reserve_exact(64);
        for skill in &mut skills {
            skill.reserve_exact(1024);
        }
    } else {
        for skill in &mut skills {
            skill.shrink_to_fit();
        }
        skills.shrink_to_fit();
    }
    let progress = SubmitProgress {
        stat_improvements: vec![
            SubmitStatImprovement {
                name: "StaminaLevel".into(),
                gained: 2,
                current: 42,
            },
            SubmitStatImprovement {
                name: "clearType".into(),
                gained: 1,
                current: 4,
            },
        ],
        skill_improvements: skills,
        quests_completed: vec![SubmitQuest {
            title: "Quest 日本".into(),
            rewards: vec![SubmitQuestReward {
                reward_type: "Points".into(),
                description: "+10".into(),
            }],
        }],
        achievements_completed: vec![SubmitAchievement {
            title: "Achievement 😀".into(),
            rewards: vec![SubmitAchievementReward {
                tier: "Gold".into(),
                requirements: vec!["Clear the chart".into()],
                title_unlocked: "Title".into(),
            }],
        }],
    };
    let srpg = SubmitEventProgressData {
        name: " SRPG 日本 \n".into(),
        is_doubles: true,
        score_delta: -100,
        rate_delta: -5,
        top_score_points: 100,
        prev_top_score_points: 120,
        current_ranking_point_total: 200,
        previous_ranking_point_total: 180,
        current_song_point_total: 20,
        previous_song_point_total: 30,
        current_ex_point_total: 40,
        previous_ex_point_total: 10,
        current_point_total: u32::MAX,
        previous_point_total: 0,
        total_passes: 7,
        leaderboard: vec![LeaderboardEntry {
            rank: 1,
            name: "Player".into(),
            machine_tag: Some("Machine".into()),
            score: -0.0,
            date: "2026-10-03".into(),
            is_rival: true,
            is_self: false,
            is_fail: false,
        }],
        progress: Some(progress),
    };
    SubmitEventProgressInput {
        result: "score-added".into(),
        score_10000: 9876,
        rate_hundredths: 110,
        itl_score_hundredths: Some(9950),
        srpg: Some(srpg),
        itl: None,
    }
}

fn assert_progress_equal(old: &[ItlEventProgress], new: &[ItlEventProgress]) {
    // Derived Debug covers every scalar, string, improvement and page field.
    assert_eq!(format!("{old:#?}"), format!("{new:#?}"));
    for (old, new) in old.iter().zip(new) {
        for (old, new) in old.overlay_pages.iter().zip(&new.overlay_pages) {
            if let (ItlOverlayPage::Leaderboard(old), ItlOverlayPage::Leaderboard(new)) = (old, new)
            {
                assert_eq!(
                    old.iter()
                        .map(|row| row.score.to_bits())
                        .collect::<Vec<_>>(),
                    new.iter()
                        .map(|row| row.score.to_bits())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn owned_skill_transfer_preserves_both_events_pages_filters_and_input() {
    for (count, long, spare) in [
        (0, false, false),
        (1, false, false),
        (8, false, false),
        (64, true, false),
        (8, false, true),
        (0, false, true),
    ] {
        for both in [false, true] {
            let mut input = fixture(count, long, spare);
            if both {
                input.itl = input.srpg.clone();
            }
            for score_added in [false, true] {
                input.result = if score_added {
                    "SCORE-ADDED"
                } else {
                    "score-updated"
                }
                .into();
                let before = format!("{input:#?}");
                let old = baseline::event_progress_from_submit(&input);
                let new = event_progress_from_submit(&input);
                assert_progress_equal(&old, &new);
                assert_eq!(format!("{input:#?}"), before);
                let owned = event_progress_from_submit_owned(input.clone());
                assert_progress_equal(&old, &owned);
            }
        }
        let mut input = fixture(count, long, spare);
        let original = baseline::event_progress_from_submit(&input);
        let skills = &input
            .srpg
            .as_ref()
            .unwrap()
            .progress
            .as_ref()
            .unwrap()
            .skill_improvements;
        let vector_pointer = skills.as_ptr();
        let text_pointers: Vec<_> = skills.iter().map(|text| text.as_ptr()).collect();
        let output = event_progress_from_submit_owned(input.clone());
        assert_progress_equal(&original, &output);
        assert_eq!(output[0].skill_improvements.capacity(), count);
        assert!(
            output[0]
                .skill_improvements
                .iter()
                .all(|text| text.capacity() == text.len())
        );
        // The original exact-capacity allocation is retained by the owned path.
        let output = event_progress_from_submit_owned(std::mem::take(&mut input));
        assert_eq!(output[0].skill_improvements.capacity(), count);
        assert!(
            output[0]
                .skill_improvements
                .iter()
                .all(|text| text.capacity() == text.len())
        );
        if !spare && count > 0 {
            assert_eq!(output[0].skill_improvements.as_ptr(), vector_pointer);
            assert_eq!(
                output[0]
                    .skill_improvements
                    .iter()
                    .map(|text| text.as_ptr())
                    .collect::<Vec<_>>(),
                text_pointers
            );
        }
    }
    for mut input in [
        SubmitEventProgressInput::default(),
        fixture(8, false, false),
    ] {
        if let Some(event) = input.srpg.as_mut() {
            event.progress = None;
        }
        input.itl_score_hundredths = None;
        assert_progress_equal(
            &baseline::event_progress_from_submit_owned(input.clone()),
            &event_progress_from_submit_owned(input),
        );
    }
    let old = fixture(8, false, false);
    let new = fixture(8, false, false);
    crate::perf::assert_reduced_churn(
        || {
            black_box(baseline::event_progress_from_submit_owned(old));
        },
        || {
            black_box(event_progress_from_submit_owned(new));
        },
    );
}

#[test]
#[ignore = "manual original/current complete owned event progress benchmarks; run in release"]
fn event_skills_benchmark() {
    for (count, long, spare) in [
        (0, false, false),
        (1, false, false),
        (8, false, false),
        (64, false, false),
        (64, true, false),
        (8, false, true),
    ] {
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for current in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "event-skills/count={count}/long={long}/spare={spare}/{}",
                    if current { "new" } else { "old" }
                ),
                128,
                1,
                || Some(fixture(count, long, spare)),
                |input| {
                    let input = black_box(input.take().unwrap());
                    black_box(if current {
                        event_progress_from_submit_owned(input)
                    } else {
                        baseline::event_progress_from_submit_owned(input)
                    });
                },
            );
        }
    }
}
