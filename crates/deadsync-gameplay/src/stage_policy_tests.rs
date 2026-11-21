fn stage_policy_chart_totals(state: &mut State, player: usize) -> CourseDisplayTotals {
    // The shared regression chart deliberately leaves grade points at zero.
    // These two-tap policy fixtures need real score metadata to reach the end
    // of the ITG scoring denominator, just as their EX step totals already do.
    let steps = state.stage_totals_for_player(player).total_steps;
    assert_eq!(steps, 2);
    state.progress.chart_totals.possible_grade_points[player] =
        i32::try_from(steps).unwrap() * judgment::TAP_SCORE_MAX;
    state.display_totals_for_player(player)
}

fn miss_stage_target(state: &mut State, player: usize) {
    let totals = stage_policy_chart_totals(state, player);
    let runtime = &mut state.players_runtime.players[player];
    runtime.current_possible_points = totals.possible_grade_points;
    runtime.earned_grade_points = 0;
    runtime.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Miss)] = totals.total_steps;
    let counts = WindowCounts {
        miss: totals.total_steps,
        ..WindowCounts::default()
    };
    state.set_live_window_counts(player, counts, counts, counts);
}

fn target_policy_profile(
    kind: GameplayTargetScoreKind,
    policy: GameplayTargetScoreMissPolicy,
) -> TestProfile {
    TestProfile {
        target_score: GameplayTargetScoreSetting::Star2,
        target_score_kind: kind,
        target_score_miss_policy: policy,
        ..TestProfile::default()
    }
}

#[test]
fn stage_policy_target_failure_and_restart_are_game_owned_and_once_per_player() {
    for kind in [
        GameplayTargetScoreKind::Itg,
        GameplayTargetScoreKind::Ex,
        GameplayTargetScoreKind::HardEx,
    ] {
        for policy in [
            GameplayTargetScoreMissPolicy::Fail,
            GameplayTargetScoreMissPolicy::RestartSong,
        ] {
            let mut state = regression_state_with_profiles([
                target_policy_profile(kind, policy),
                TestProfile::default(),
            ]);
            miss_stage_target(&mut state, 0);
            let life_before = state.players()[0].life;
            let expected = if policy == GameplayTargetScoreMissPolicy::Fail {
                GameplayAction::NavigateNoFade(GameplayExit::Complete)
            } else {
                GameplayAction::Restart
            };
            assert_eq!(
                state.resolve_frame_action(GameplayAction::None),
                expected,
                "{kind:?} {policy:?}"
            );
            if policy == GameplayTargetScoreMissPolicy::Fail {
                assert!(state.players()[0].is_failing);
                assert_eq!(state.players()[0].life, 0.0);
                assert_eq!(
                    state.players()[0].fail_time,
                    Some(state.current_music_time_seconds())
                );
            } else {
                assert_eq!(state.players()[0].life, life_before);
                assert!(!state.players()[0].is_failing);
                assert_eq!(state.players()[0].fail_time, None);
            }
            assert_eq!(
                state.resolve_frame_action(GameplayAction::None),
                GameplayAction::None
            );
        }
    }
}

#[test]
fn stage_policy_untouched_and_display_only_targets_do_not_change_gameplay() {
    for policy in [
        GameplayTargetScoreMissPolicy::Nothing,
        GameplayTargetScoreMissPolicy::DimMiniIndicator,
    ] {
        let mut state = regression_state_with_profiles([
            target_policy_profile(GameplayTargetScoreKind::Itg, policy),
            TestProfile::default(),
        ]);
        miss_stage_target(&mut state, 0);
        assert_eq!(
            state.resolve_frame_action(GameplayAction::None),
            GameplayAction::None
        );
        assert!(!state.players()[0].is_failing);
        state.profiles_runtime.profiles[0].target_score_miss_policy =
            GameplayTargetScoreMissPolicy::RestartSong;
        assert_eq!(
            state.resolve_frame_action(GameplayAction::None),
            GameplayAction::Restart,
            "a display-only policy must not consume the player's one-shot action"
        );
    }
    let mut state = regression_state_with_profiles([
        target_policy_profile(
            GameplayTargetScoreKind::Itg,
            GameplayTargetScoreMissPolicy::Fail,
        ),
        TestProfile::default(),
    ]);
    assert_eq!(
        state.resolve_frame_action(GameplayAction::None),
        GameplayAction::None
    );
    assert!(!state.players()[0].is_failing);
}

#[test]
fn stage_policy_existing_exit_has_priority_without_consuming_target_action() {
    for exit in [
        GameplayAction::Navigate(GameplayExit::Complete),
        GameplayAction::NavigateNoFade(GameplayExit::Complete),
        GameplayAction::Navigate(GameplayExit::Cancel),
        GameplayAction::NavigateNoFade(GameplayExit::Cancel),
        GameplayAction::Restart,
    ] {
        let mut state = regression_state_with_profiles([
            target_policy_profile(
                GameplayTargetScoreKind::Itg,
                GameplayTargetScoreMissPolicy::Fail,
            ),
            TestProfile::default(),
        ]);
        miss_stage_target(&mut state, 0);
        assert_eq!(state.resolve_frame_action(exit), exit);
        assert!(!state.players()[0].is_failing);
        assert_eq!(
            state.resolve_frame_action(GameplayAction::None),
            GameplayAction::NavigateNoFade(GameplayExit::Complete)
        );
    }
}

#[test]
fn stage_policy_uses_selected_score_kind_and_ignores_indicator_visibility() {
    for mode in [
        GameplayMiniIndicatorMode::None,
        GameplayMiniIndicatorMode::Pacemaker,
    ] {
        for kind in [
            GameplayTargetScoreKind::Itg,
            GameplayTargetScoreKind::Ex,
            GameplayTargetScoreKind::HardEx,
        ] {
            let mut profile =
                target_policy_profile(kind, GameplayTargetScoreMissPolicy::RestartSong);
            profile.mini_indicator_options.requested_mode = mode;
            let mut state = regression_state_with_profiles([profile, TestProfile::default()]);
            let totals = stage_policy_chart_totals(&mut state, 0);
            let player = &mut state.players_runtime.players[0];
            player.earned_grade_points = totals.possible_grade_points;
            player.current_possible_points = totals.possible_grade_points;
            player.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Fantastic)] =
                totals.total_steps;
            let counts = WindowCounts {
                w1: totals.total_steps,
                ..WindowCounts::default()
            };
            state.set_live_window_counts(0, counts, counts, counts);
            let progress = state.target_score_progress(0, kind).unwrap();
            assert_eq!(progress.current_possible_ratio, 1.0);
            assert_eq!(
                progress.current_score_percent,
                if kind == GameplayTargetScoreKind::Itg {
                    100.0
                } else {
                    85.71
                }
            );
            let expected = if kind == GameplayTargetScoreKind::Itg {
                GameplayAction::None
            } else {
                GameplayAction::Restart
            };
            assert_eq!(
                state.resolve_frame_action(GameplayAction::None),
                expected,
                "{kind:?} {mode:?}"
            );
        }
    }
}

#[test]
fn stage_policy_preserves_player_priority_and_skips_inactive_slots() {
    let profiles = std::array::from_fn(|_| {
        target_policy_profile(
            GameplayTargetScoreKind::Itg,
            GameplayTargetScoreMissPolicy::RestartSong,
        )
    });
    let mut state = regression_state_with_session_profiles_and_scroll(
        GameplaySession {
            play_style: GameplayInputPlayStyle::Versus,
            ..GameplaySession::default()
        },
        GameplayConfig::default(),
        profiles,
        [ScrollSpeedSetting::default(); MAX_PLAYERS],
    );
    miss_stage_target(&mut state, 0);
    miss_stage_target(&mut state, 1);
    state.setup.num_players = 1;
    assert_eq!(
        state.resolve_frame_action(GameplayAction::None),
        GameplayAction::Restart
    );
    assert_eq!(state.control.missed_target_handled, [true, false]);
    assert_eq!(
        state.resolve_frame_action(GameplayAction::None),
        GameplayAction::None
    );
    state.setup.num_players = 2;
    assert_eq!(
        state.resolve_frame_action(GameplayAction::None),
        GameplayAction::Restart
    );
    assert_eq!(state.control.missed_target_handled, [true, true]);
    assert_eq!(
        state.resolve_frame_action(GameplayAction::None),
        GameplayAction::None
    );
}

#[test]
fn stage_policy_target_progress_preserves_truncation_and_non_tap_judgments() {
    let mut state = regression_state();
    state.progress.chart_totals.possible_grade_points[0] = 3;
    state.players_runtime.players[0].earned_grade_points = 1;
    state.players_runtime.players[0].current_possible_points = 1;
    let progress = state
        .target_score_progress(0, GameplayTargetScoreKind::Itg)
        .unwrap();
    assert_eq!(progress.current_score_percent, 33.33);
    assert_eq!(progress.current_possible_ratio, 1.0 / 3.0);
    assert!(!progress.judged_any);
    for reason in 0..5 {
        let player = &mut state.players_runtime.players[0];
        player.holds_let_go_for_score = u32::from(reason == 0);
        player.rolls_let_go_for_score = u32::from(reason == 1);
        player.mines_hit_for_score = u32::from(reason == 2);
        player.is_failing = reason == 3;
        player.life = if reason == 4 { 0.0 } else { 0.5 };
        assert!(
            state
                .target_score_progress(0, GameplayTargetScoreKind::Itg)
                .unwrap()
                .judged_any
        );
    }
    assert!(
        state
            .target_score_progress(MAX_PLAYERS, GameplayTargetScoreKind::Itg)
            .is_none()
    );
}

#[test]
fn stage_policy_autoplay_off_preserves_life_failure_clock_and_player_identity() {
    let mut state = regression_state_with_session(GameplaySession {
        play_style: GameplayInputPlayStyle::Versus,
        ..GameplaySession::default()
    });
    state.players_runtime.players[0].autoplay_judged = true;
    state.mark_autoplay_stage_failures();
    assert!(state.players()[0].stage_failed);
    assert!(!state.players()[1].stage_failed);
    for player in state.players() {
        assert_eq!(player.life, 0.5);
        assert!(!player.is_failing);
        assert_eq!(player.fail_time, None);
    }
    state.players_runtime.players[1].autoplay_judged = true;
    state.setup.num_players = 1;
    state.mark_autoplay_stage_failures();
    assert!(
        !state.players()[1].stage_failed,
        "disabled players do not receive Off"
    );
    state.players_runtime.players[0].autoplay_judged = false;
    state.setup.num_players = 2;
    state.mark_autoplay_stage_failures();
    assert!(
        state.players()[0].stage_failed,
        "an existing failure stays failed"
    );
    assert!(state.players()[1].stage_failed);
}

#[test]
fn stage_policy_finalizes_mines_only_for_natural_completion_and_before_autoplay_failure() {
    for completed in [false, true] {
        let mut state = regression_state();
        set_regression_mine(&mut state, 1, 1, 96, 2_000_000_000);
        state.progress.stage.song_completed_naturally = completed;
        state.players_runtime.players[0].autoplay_judged = true;
        state.finalize_completed_stage();
        assert_eq!(state.players()[0].mines_avoided, u32::from(completed));
        assert!(state.players()[0].stage_failed);
        assert_eq!(state.players()[0].life, 0.5);
        assert_eq!(state.players()[0].fail_time, None);
        state.finalize_completed_stage();
        assert_eq!(state.players()[0].mines_avoided, u32::from(completed));
    }
}
