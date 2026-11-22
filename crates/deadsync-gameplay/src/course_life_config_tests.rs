use super::*;
use crate::runtime_regression_tests::regression_song;
use deadsync_chart::song::STANDARD_DIFFICULTY_COUNT;
use deadsync_rules::course::CourseType;

fn selection_song() -> SongData {
    let mut song = regression_song();
    let template = song.charts[0].clone();
    // Deliberately differ from difficulty-slot order and mix chart types.
    song.charts = [
        ("dance-double", "Easy", 9),
        ("dance-single", "Challenge", 8),
        ("dance-double", "Challenge", 7),
        ("dance-single", "Easy", 7),
    ]
    .into_iter()
    .map(|(chart_type, difficulty, meter)| ChartData {
        chart_type: chart_type.to_string(),
        difficulty: difficulty.to_string(),
        meter,
        ..template.clone()
    })
    .collect();
    song
}

fn stage(
    song: &SongData,
    steps_index: [usize; MAX_PLAYERS],
    gain_lives: i32,
    gain_seconds: f32,
) -> CourseLifeStage<'_> {
    CourseLifeStage {
        song,
        steps_index,
        gain_lives,
        gain_seconds,
    }
}

fn battery(total_lives: u32, reward_lives: u32) -> CourseLifeConfig {
    CourseLifeConfig::Battery {
        total_lives,
        reward_lives,
    }
}

#[test]
fn missing_stage_uses_bar_for_every_mode() {
    for mode in [
        CourseType::Nonstop,
        CourseType::Oni,
        CourseType::Endless,
        CourseType::Survival,
    ] {
        assert_eq!(
            course_life_config(mode, i32::MIN, None, "dance-single"),
            [CourseLifeConfig::Bar; MAX_PLAYERS],
        );
    }
}

#[test]
fn nonstop_and_endless_use_bar_with_a_stage() {
    let song = selection_song();
    for mode in [CourseType::Nonstop, CourseType::Endless] {
        assert_eq!(
            course_life_config(
                mode,
                i32::MAX,
                Some(stage(&song, [usize::MAX; MAX_PLAYERS], -1, f32::NAN)),
                "dance-single",
            ),
            [CourseLifeConfig::Bar; MAX_PLAYERS],
        );
    }
}

#[test]
fn oni_clamps_total_lives_to_at_least_one() {
    let song = selection_song();
    for (lives, expected) in [
        (i32::MIN, 1),
        (-1, 1),
        (0, 1),
        (1, 1),
        (4, 4),
        (i32::MAX, 2_147_483_647),
    ] {
        assert_eq!(
            course_life_config(
                CourseType::Oni,
                lives,
                Some(stage(&song, [1, 4], 1, 0.0)),
                "dance-single",
            ),
            [battery(expected, 1); MAX_PLAYERS],
        );
    }
}

#[test]
fn oni_preserves_explicit_nonnegative_rewards_for_all_players() {
    let song = selection_song();
    for (gain_lives, expected) in [(0, 0), (3, 3), (i32::MAX, 2_147_483_647)] {
        assert_eq!(
            course_life_config(
                CourseType::Oni,
                4,
                Some(stage(&song, [4, usize::MAX], gain_lives, 0.0)),
                "dance-single",
            ),
            [battery(4, expected); MAX_PLAYERS],
        );
    }
}

#[test]
fn oni_automatic_rewards_use_each_players_standard_and_edit_slots() {
    let mut song = selection_song();
    let template = song.charts[1].clone();
    // Edit slots are sorted by meter, independently of their source positions.
    song.charts.extend([8, 7].map(|meter| ChartData {
        difficulty: "Edit".to_string(),
        meter,
        ..template.clone()
    }));
    for steps_index in [
        [1, 4],
        [STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_COUNT + 1],
    ] {
        for gain_lives in [-1, i32::MIN] {
            assert_eq!(
                course_life_config(
                    CourseType::Oni,
                    4,
                    Some(stage(&song, steps_index, gain_lives, 0.0)),
                    "dance-single",
                ),
                [battery(4, 1), battery(4, 2)],
            );
        }
    }
}

#[test]
fn oni_automatic_rewards_match_chart_type_and_allow_missing_charts() {
    let song = selection_song();
    for (chart_type, steps_index, expected) in [
        ("dance-double", [1, 4], [battery(4, 2), battery(4, 1)]),
        ("dance-single", [4, 0], [battery(4, 2), battery(4, 1)]),
        ("pump-single", [1, 4], [battery(4, 1), battery(4, 1)]),
    ] {
        assert_eq!(
            course_life_config(
                CourseType::Oni,
                4,
                Some(stage(&song, steps_index, -1, 0.0)),
                chart_type,
            ),
            expected,
        );
    }
}

#[test]
fn survival_preserves_raw_gains_until_initialization() {
    let song = selection_song();
    for (gain_seconds, expected_seconds) in [
        (-30.0, 15.0),
        (-0.0, 15.0),
        (0.0, 15.0),
        (12.5, 15.0),
        (45.0, 45.0),
        (f32::from_bits(0xffc1_2345), 15.0),
        (f32::INFINITY, 15.0),
        (f32::NEG_INFINITY, 15.0),
    ] {
        let configs = course_life_config(
            CourseType::Survival,
            i32::MIN,
            Some(stage(&song, [usize::MAX; MAX_PLAYERS], -1, gain_seconds)),
            "missing-chart-type",
        );
        for config in configs {
            let CourseLifeConfig::Survival { gain_seconds: raw } = config else {
                panic!("expected survival configuration");
            };
            assert_eq!(raw.to_bits(), gain_seconds.to_bits());
        }
        let (state, life) = init_course_life(configs[0], None);
        assert_eq!(
            state,
            CourseLifeState::Survival {
                remaining_seconds: expected_seconds,
            },
        );
        assert_eq!(life, expected_seconds / 90.0);
    }
}

#[test]
fn selected_battery_rewards_apply_on_carry_and_explicit_zero_stays_zero() {
    let song = selection_song();
    let automatic = course_life_config(
        CourseType::Oni,
        4,
        Some(stage(&song, [1, 4], -1, 0.0)),
        "dance-single",
    );
    let no_reward = course_life_config(
        CourseType::Oni,
        4,
        Some(stage(&song, [1, 4], 0, 0.0)),
        "dance-single",
    );
    for (player, (reward_lives, carried_lives)) in [(1, 2), (2, 3)].into_iter().enumerate() {
        let (state, life) = init_course_life(
            automatic[player],
            Some(CourseLifeState::Battery {
                lives: 1,
                total_lives: 8,
                reward_lives: 7,
            }),
        );
        assert_eq!(
            state,
            CourseLifeState::Battery {
                lives: 1,
                total_lives: 4,
                reward_lives,
            },
        );
        assert_eq!(life, 0.25);

        let carry = course_life_carry(state);
        assert_eq!(
            carry,
            CourseLifeState::Battery {
                lives: carried_lives,
                total_lives: 4,
                reward_lives: 0,
            },
        );
        let (next, next_life) = init_course_life(no_reward[player], Some(carry));
        assert_eq!(next, carry);
        assert_eq!(next_life, carried_lives as f32 / 4.0);
        assert_eq!(course_life_carry(next), next);
    }
}

#[test]
fn selected_survival_gain_adds_to_carried_time() {
    let song = selection_song();
    let initial = course_life_config(
        CourseType::Survival,
        4,
        Some(stage(&song, [1, 4], 0, 5.0)),
        "dance-single",
    );
    let following = course_life_config(
        CourseType::Survival,
        4,
        Some(stage(&song, [1, 4], 0, 30.0)),
        "dance-single",
    );
    let (first, _) = init_course_life(initial[0], None);
    let (next, life) = init_course_life(following[0], Some(course_life_carry(first)));
    assert_eq!(
        next,
        CourseLifeState::Survival {
            remaining_seconds: 45.0,
        },
    );
    assert_eq!(life, 0.5);
}
