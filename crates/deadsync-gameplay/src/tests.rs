#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use deadsync_core::song_time::{
        INVALID_SONG_TIME_NS, song_time_ns_from_seconds, song_time_ns_to_seconds,
    };
    use deadsync_core::timing::ROWS_PER_BEAT;
    use deadsync_rules::note::{HoldData, Note};
    use deadsync_rules::timing::{
        DelaySegment, FakeSegment, StopSegment, TimingSegments, WarpSegment,
    };
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.000_001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn player_life_dead_policy_uses_failing_or_empty_life() {
        assert!(player_life_is_dead(1.0, true));
        assert!(player_life_is_dead(0.0, false));
        assert!(player_life_is_dead(-0.1, false));
        assert!(!player_life_is_dead(0.1, false));
    }

    #[test]
    fn joined_players_failed_requires_active_dead_players() {
        let players = [
            PlayerLifeStatus {
                life: 0.0,
                is_failing: false,
            },
            PlayerLifeStatus {
                life: 1.0,
                is_failing: true,
            },
        ];

        assert!(!all_joined_players_failed(&players, 0));
        assert!(all_joined_players_failed(&players, 1));
        assert!(all_joined_players_failed(&players, 2));
    }

    #[test]
    fn joined_players_failed_rejects_any_alive_player() {
        let players = [
            PlayerLifeStatus {
                life: 0.0,
                is_failing: false,
            },
            PlayerLifeStatus {
                life: 0.25,
                is_failing: false,
            },
        ];

        assert!(!all_joined_players_failed(&players, 2));
    }

    #[test]
    fn player_runtime_defaults_match_new_song_state() {
        let player = init_player_runtime();

        assert_eq!(player.combo, 0);
        assert_eq!(player.miss_combo, 0);
        assert_eq!(player.life, 0.5);
        assert_eq!(player.judgment_counts, [0; judgment::JUDGE_GRADE_COUNT]);
        assert_eq!(player.scoring_counts, [0; judgment::JUDGE_GRADE_COUNT]);
        assert!(player.combo_milestones.is_empty());
        assert_eq!(player.hands_holding_count_for_stats, 0);
        assert!(player.failed_ex_score_inputs.is_none());
        assert!(player.course_submit_life.is_none());
        assert_eq!(player.life_history.capacity(), 10000);
        assert_eq!(player.error_bar_avg_samples.capacity(), 64);
        assert_eq!(player.error_bar_long_avg_samples.capacity(), 64);
    }

    #[test]
    fn practice_player_runtime_records_start_life() {
        let player = init_player_runtime_for_practice(42.0);

        assert_eq!(player.life, 0.5);
        assert_eq!(player_life(&player), 0.5);
        assert_eq!(player.life_history, vec![(42.0, 0.5)]);
    }

    #[test]
    fn song_player_runtime_applies_course_and_combo_carry() {
        let carry = CourseDisplayCarry {
            life: 0.75,
            full_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_grade: Some(JudgeGrade::Great),
            current_combo_window_counts: WindowCounts {
                w2: 3,
                ..WindowCounts::default()
            },
            first_fc_attempt_broken: true,
            ..CourseDisplayCarry::default()
        };

        let player = init_player_runtime_for_song(12.5, true, Some(carry), true, false, 42);

        assert_eq!(player.life, 0.75);
        assert_eq!(player.life_history, vec![(12.5, 0.75)]);
        assert!(player.course_submit_life.is_some());
        assert_eq!(player.combo, 42);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_window_counts.w2, 3);
        assert!(player.first_fc_attempt_broken);
    }

    #[test]
    fn player_runtime_life_delta_updates_runtime_fields() {
        let mut player = init_player_runtime();
        player.life = 1.0;
        player.life_history.push((0.0, 1.0));
        player.course_submit_life = Some(deadsync_rules::life::LifeMeter::course_submit_start());

        let update = apply_player_runtime_life_delta(&mut player, 12.0, -0.6);

        assert_eq!(update, GameplayLifeDeltaUpdate::default());
        assert_near(player.life, 0.4);
        assert_eq!(player.life_history.len(), 2);
        assert_eq!(player.life_history[0], (0.0, 1.0));
        assert_eq!(player.life_history[1].0, 12.0);
        assert_near(player.life_history[1].1, 0.4);
        let course_life = player
            .course_submit_life
            .as_ref()
            .expect("course submit life should remain attached");
        assert_eq!(course_life.life, 0.0);
        assert!(course_life.is_failing);
        assert_eq!(course_life.fail_time, Some(12.0));
    }

    #[test]
    fn player_runtime_dead_policy_uses_runtime_life_fields() {
        let mut player = init_player_runtime();
        assert!(!player_runtime_is_dead(&player));

        player.life = 0.0;
        assert!(player_runtime_is_dead(&player));

        player.life = 1.0;
        player.is_failing = true;
        assert!(player_runtime_is_dead(&player));
    }

    #[test]
    fn player_runtime_combo_state_round_trips_runtime_fields() {
        let mut player = init_player_runtime();
        player.combo = 12;
        player.miss_combo = 3;
        player.full_combo_grade = Some(JudgeGrade::Excellent);
        player.current_combo_grade = Some(JudgeGrade::Great);
        player.first_fc_attempt_broken = true;

        let state = player_combo_state(&player);
        assert_eq!(state.combo, 12);
        assert_eq!(state.miss_combo, 3);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(state.first_fc_attempt_broken);

        write_player_combo_state(
            &mut player,
            ComboState {
                combo: 100,
                miss_combo: 0,
                full_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                first_fc_attempt_broken: false,
            },
        );

        assert_eq!(player.combo, 100);
        assert_eq!(player.miss_combo, 0);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Fantastic));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Fantastic));
        assert!(!player.first_fc_attempt_broken);
    }

    #[test]
    fn player_runtime_combo_update_applies_feedback_state() {
        let mut player = init_player_runtime();
        player.current_combo_window_counts.w1 = 7;

        apply_combo_update(
            &mut player,
            ComboUpdate {
                combo_broken: true,
                hit_hundred_milestone: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(player.current_combo_window_counts.w1, 0);
        assert_eq!(player.combo_milestones.len(), 1);
        assert_eq!(player.combo_milestones[0].kind, ComboMilestoneKind::Hundred);
    }

    #[test]
    fn player_runtime_feedback_adapters_write_runtime_fields() {
        let mut player = init_player_runtime();
        add_player_step_calories(&mut player, 12.5);
        add_player_step_calories(&mut player, 0.25);
        assert_near(player.calories_burned, 12.75);

        add_player_mines_avoided(&mut player, 2);
        add_player_mines_avoided(&mut player, u32::MAX);
        assert_eq!(player.mines_avoided, u32::MAX);
        set_player_mines_avoided(&mut player, 9);
        assert_eq!(player.mines_avoided, 9);
        player.mines_hit = 4;
        assert_eq!(player_mines_hit(&player), 4);

        player.combo_milestones.push(ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.0,
        });
        tick_player_combo_milestones(&mut player, 0.25);
        assert_near(player.combo_milestones[0].elapsed, 0.25);

        let judgment = Judgment {
            time_error_ms: 3.0,
            time_error_music_ns: 3_000_000,
            grade: JudgeGrade::Excellent,
            window: Some(TimingWindow::W2),
            miss_because_held: false,
        };
        record_player_live_timing_stats(&mut player, &judgment);
        assert_near(player_live_timing_snapshot(&player).all.mean_ms, 3.0);

        set_player_last_judgment(&mut player, judgment, 5.0);
        assert_eq!(
            player
                .last_judgment
                .as_ref()
                .map(|info| info.judgment.grade),
            Some(JudgeGrade::Excellent)
        );
        assert_eq!(
            player
                .last_judgment
                .as_ref()
                .map(|info| info.started_at_screen_s),
            Some(5.0)
        );

        set_player_last_mine_judgment(&mut player, MineResult::Hit, 3, 6.5);
        assert_eq!(
            player.last_mine_judgment.as_ref().map(|info| info.result),
            Some(MineResult::Hit)
        );
        assert_eq!(
            player.last_mine_judgment.as_ref().map(|info| info.column),
            Some(3)
        );
        assert_eq!(
            player
                .last_mine_judgment
                .as_ref()
                .map(|info| info.started_at_screen_s),
            Some(6.5)
        );
    }

    #[test]
    fn update_itg_grade_totals_writes_runtime_score_points() {
        let mut player = init_player_runtime();
        player.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Fantastic)] = 2;
        player.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Great)] = 1;
        player.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Miss)] = 1;
        player.holds_held_for_score = 3;
        player.rolls_held_for_score = 2;
        player.mines_hit_for_score = 1;

        update_itg_grade_totals(&mut player);

        assert_eq!(player.earned_grade_points, 19);
    }

    #[test]
    fn apply_course_combo_carry_writes_runtime_combo_fields() {
        let mut player = init_player_runtime();
        player.combo = 4;
        let carry = CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_grade: Some(JudgeGrade::Great),
            current_combo_window_counts: WindowCounts {
                w2: 3,
                ..WindowCounts::default()
            },
            first_fc_attempt_broken: true,
            ..CourseDisplayCarry::default()
        };

        apply_course_combo_carry(&mut player, true, false, 37, Some(carry));

        assert_eq!(player.combo, 37);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_window_counts.w2, 3);
        assert!(player.first_fc_attempt_broken);
    }

    #[test]
    fn player_display_adapters_read_runtime_fields() {
        let mut player = init_player_runtime();
        player.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)] = 2;
        player.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Great)] = 4;
        player.holds_held_for_score = 3;
        player.holds_let_go_for_score = 4;
        player.rolls_held_for_score = 5;
        player.rolls_let_go_for_score = 6;
        player.mines_hit_for_score = 7;

        let stage = player_score_stage(&player);
        assert_eq!(
            stage.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)],
            2
        );
        assert_eq!(stage.holds_held_for_score, 3);
        assert_eq!(stage.holds_let_go_for_score, 4);
        assert_eq!(stage.rolls_held_for_score, 5);
        assert_eq!(stage.rolls_let_go_for_score, 6);
        assert_eq!(stage.mines_hit_for_score, 7);
        assert_eq!(
            player_display_judgment_count(
                &player,
                CourseDisplayCarry {
                    judgment_counts: [1; judgment::JUDGE_GRADE_COUNT],
                    ..CourseDisplayCarry::default()
                },
                JudgeGrade::Great,
            ),
            5
        );

        let carry_stage = player_course_display_stage(
            &player,
            WindowCounts {
                w1: 8,
                ..WindowCounts::default()
            },
            WindowCounts {
                w0: 1,
                ..WindowCounts::default()
            },
            WindowCounts {
                w2: 2,
                ..WindowCounts::default()
            },
        );
        assert_eq!(carry_stage.life, 0.5);
        assert_eq!(
            carry_stage.scoring_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)],
            2
        );
        assert_eq!(carry_stage.window_counts.w1, 8);
        assert_eq!(carry_stage.window_counts_10ms_blue.w0, 1);
        assert_eq!(carry_stage.window_counts_display_blue.w2, 2);
        assert_eq!(carry_stage.holds_held_for_score, 3);
        assert_eq!(carry_stage.mines_hit_for_score, 7);

        let mut players = std::array::from_fn(|_| init_player_runtime());
        players[0].life = 0.0;
        assert!(all_joined_player_runtimes_failed(&players, 1));
        assert!(!all_joined_player_runtimes_failed(&players, 2));
        assert_eq!(player_health_state(&players[0]), HealthState::Dead);
        assert_eq!(player_health_state(&players[1]), HealthState::Alive);

        assert!(player_course_submit_life_eligible(&player));

        player.course_submit_life = Some(deadsync_rules::life::LifeMeter {
            life: 0.0,
            ..deadsync_rules::life::LifeMeter::course_submit_start()
        });
        assert!(!player_course_submit_life_eligible(&player));

        deadsync_rules::timing::record_live_timing_stats(
            &mut player.live_timing_stats,
            &Judgment {
                time_error_ms: -12.0,
                time_error_music_ns: -12_000_000,
                grade: JudgeGrade::Great,
                window: Some(TimingWindow::W3),
                miss_because_held: false,
            },
        );
        let timing = player_live_timing_snapshot(&player);
        assert_near(timing.all.mean_ms, -12.0);
        assert_near(timing.recent.mean_abs_ms, 12.0);

        let live = ExScoreInputs {
            holds_held_for_score: 9,
            ..ExScoreInputs::default()
        };
        assert_eq!(
            player_effective_ex_score_inputs(&player, live).holds_held_for_score,
            9
        );

        player.fail_time = Some(33.0);
        assert!(capture_player_failed_ex_score_inputs(&mut player, live));
        let updated_live = ExScoreInputs {
            holds_held_for_score: 12,
            ..ExScoreInputs::default()
        };
        assert_eq!(
            player_effective_ex_score_inputs(&player, updated_live).holds_held_for_score,
            9
        );
        assert!(!capture_player_failed_ex_score_inputs(
            &mut player,
            updated_live
        ));
    }

    #[test]
    fn hold_resolution_player_state_round_trips_runtime_fields() {
        let mut player = init_player_runtime();
        player.hands_holding_count_for_stats = 2;
        player.holds_held = 3;
        player.holds_held_for_score = 4;
        player.holds_let_go_for_score = 5;
        player.rolls_held = 6;
        player.rolls_held_for_score = 7;
        player.rolls_let_go_for_score = 8;
        player.combo = 99;
        player.miss_combo = 1;

        let mut state = hold_resolution_player_state(&player);
        assert_eq!(state.stats.hands_holding_count_for_stats, 2);
        assert_eq!(state.stats.holds_held, 3);
        assert_eq!(state.stats.holds_held_for_score, 4);
        assert_eq!(state.stats.holds_let_go_for_score, 5);
        assert_eq!(state.stats.rolls_held, 6);
        assert_eq!(state.stats.rolls_held_for_score, 7);
        assert_eq!(state.stats.rolls_let_go_for_score, 8);
        assert_eq!(state.combo.combo, 99);
        assert_eq!(state.combo.miss_combo, 1);

        state.stats.hands_holding_count_for_stats = 0;
        state.stats.holds_held = 13;
        state.stats.rolls_let_go_for_score = 21;
        state.combo.combo = 100;
        state.combo.miss_combo = 0;
        apply_hold_resolution_player_state(&mut player, state);

        assert_eq!(player.hands_holding_count_for_stats, 0);
        assert_eq!(player.holds_held, 13);
        assert_eq!(player.rolls_let_go_for_score, 21);
        assert_eq!(player.combo, 100);
        assert_eq!(player.miss_combo, 0);
    }

    #[test]
    fn row_finalization_player_state_round_trips_runtime_fields() {
        let mut player = init_player_runtime();
        player.combo = 32;
        player.current_combo_window_counts.w2 = 4;
        player.judgment_counts[0] = 1;
        player.scoring_counts[1] = 2;
        player.hands_achieved = 3;

        let mut state = row_finalization_player_state(&player);
        assert_eq!(state.combo.combo, 32);
        assert_eq!(state.current_combo_window_counts.w2, 4);
        assert_eq!(state.judgment_counts[0], 1);
        assert_eq!(state.scoring_counts[1], 2);
        assert_eq!(state.hands_achieved, 3);

        state.combo.combo = 64;
        state.current_combo_window_counts.w3 = 5;
        state.judgment_counts[2] = 7;
        state.scoring_counts[3] = 8;
        state.hands_achieved = 9;
        set_row_finalization_player_state(&mut player, state);

        assert_eq!(player.combo, 64);
        assert_eq!(player.current_combo_window_counts.w3, 5);
        assert_eq!(player.judgment_counts[2], 7);
        assert_eq!(player.scoring_counts[3], 8);
        assert_eq!(player.hands_achieved, 9);
    }

    #[test]
    fn mine_hit_player_state_round_trips_runtime_fields() {
        let mut player = init_player_runtime();
        player.mines_hit = 2;
        player.mines_hit_for_score = 1;
        player.combo = 100;
        player.current_combo_window_counts.w1 = 3;

        let mut state = mine_hit_player_state(&player);
        assert_eq!(state.mines_hit, 2);
        assert_eq!(state.mines_hit_for_score, 1);
        assert_eq!(state.combo.combo, 100);

        state.mines_hit = 3;
        state.mines_hit_for_score = 2;
        state.combo.combo = 0;
        apply_mine_hit_player_update(
            &mut player,
            state,
            MineHitPlayerUpdate {
                combo_update: ComboUpdate {
                    combo_broken: true,
                    ..ComboUpdate::default()
                },
                ..MineHitPlayerUpdate::default()
            },
        );

        assert_eq!(player.mines_hit, 3);
        assert_eq!(player.mines_hit_for_score, 2);
        assert_eq!(player.combo, 0);
        assert_eq!(player.current_combo_window_counts.w1, 0);
    }

    #[test]
    fn course_submit_life_eligibility_uses_optional_meter_state() {
        assert!(course_submit_life_eligible(None));
        assert!(course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter::course_submit_start()
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                is_failing: true,
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                fail_time: Some(12.0),
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                life: 0.0,
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
    }

    #[test]
    fn gameplay_life_delta_records_history_for_life_changes() {
        let mut meter = deadsync_rules::life::LifeMeter::new(0.5);
        let mut history = vec![(0.0, 0.5)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            12.0,
            deadsync_rules::life::LIFE_GREAT,
        );

        assert_eq!(update, GameplayLifeDeltaUpdate::default());
        assert_near(meter.life, 0.504);
        assert_eq!(history, vec![(0.0, 0.5), (12.0, 0.504)]);
    }

    #[test]
    fn gameplay_life_delta_reports_failure_and_updates_course_submit_life() {
        let mut meter = deadsync_rules::life::LifeMeter::new(1.0);
        let mut course_submit_life = deadsync_rules::life::LifeMeter::course_submit_start();
        let mut history = vec![(0.0, 1.0)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            Some(&mut course_submit_life),
            12.0,
            -0.6,
        );

        assert!(!update.failed_now);
        assert!(!update.was_dead);
        assert_near(meter.life, 0.4);
        assert!(!meter.is_failing);
        assert_eq!(meter.fail_time, None);
        assert_eq!(course_submit_life.life, 0.0);
        assert!(course_submit_life.is_failing);
        assert_eq!(course_submit_life.fail_time, Some(12.0));
    }

    #[test]
    fn gameplay_life_delta_reports_active_meter_failure() {
        let mut meter = deadsync_rules::life::LifeMeter::new(0.05);
        let mut history = vec![(0.0, 0.05)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            8.0,
            deadsync_rules::life::LIFE_MISS,
        );

        assert!(update.failed_now);
        assert!(!update.was_dead);
        assert_eq!(meter.life, 0.0);
        assert!(meter.is_failing);
        assert_eq!(meter.fail_time, Some(8.0));
        assert_eq!(history, vec![(0.0, 0.05), (8.0, 0.0)]);
    }

    #[test]
    fn gameplay_life_delta_clamps_already_dead_meter() {
        let mut meter = deadsync_rules::life::LifeMeter {
            life: -0.25,
            combo_after_miss: 3,
            is_failing: false,
            fail_time: None,
        };
        let mut history = vec![(0.0, 0.0)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            8.0,
            deadsync_rules::life::LIFE_GREAT,
        );

        assert!(!update.failed_now);
        assert!(update.was_dead);
        assert_eq!(meter.life, 0.0);
        assert!(meter.is_failing);
        assert_eq!(meter.fail_time, None);
        assert_eq!(history, vec![(0.0, 0.0)]);
    }

    fn song_lua_ease_mask_window(
        target: SongLuaEaseMaskTarget,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
        from: f32,
        to: f32,
    ) -> SongLuaEaseMaskWindow {
        SongLuaEaseMaskWindow {
            start_second,
            end_second,
            sustain_end_second,
            target,
            from,
            to,
            easing: None,
            opt1: None,
            opt2: None,
        }
    }

    fn song_lua_column_offset_window(
        column: usize,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
    ) -> SongLuaColumnOffsetWindowRuntime {
        SongLuaColumnOffsetWindowRuntime {
            column,
            start_second,
            end_second,
            sustain_end_second,
            from_y: 0.0,
            to_y: 64.0,
            easing: None,
            opt1: None,
            opt2: None,
        }
    }

    fn song_lua_overlay_ease_window(
        overlay_index: usize,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
        cutoff_second: Option<f32>,
    ) -> SongLuaOverlayEaseWindowRuntime<u8> {
        build_song_lua_overlay_ease_window_runtime(
            overlay_index,
            start_second,
            end_second,
            sustain_end_second,
            cutoff_second,
            1,
            2,
            None,
            None,
            None,
        )
    }

    fn attack_mask_window(
        start_second: f32,
        end_second: f32,
        mods: ParsedAttackMods,
    ) -> AttackMaskWindow {
        attack_mask_window_from_parts(
            &ChartAttackWindow {
                start_second,
                len_seconds: end_second - start_second,
                mods: String::new(),
            },
            mods,
        )
        .expect("test attack mask window must have an effect")
    }

    #[test]
    fn song_lua_runtime_visuals_are_parser_free_containers() {
        let layer = SongLuaVisualLayerRuntime {
            start_second: 2.0,
            screen_width: 640.0,
            screen_height: 480.0,
            overlays: vec![1_u8],
            overlay_eases: vec![song_lua_overlay_ease_window(0, 2.0, 3.0, 4.0, None)],
            overlay_ease_ranges: vec![0..1],
            overlay_events: vec![vec![build_song_lua_overlay_message_runtime(2.5, 3)]],
            song_foreground: 9_u16,
            song_foreground_events: Vec::new(),
        };
        assert_eq!(layer.overlays, [1]);
        assert_eq!(layer.song_foreground, 9);
        assert_eq!(layer.overlay_eases[0].to, 2);

        let visuals = SongLuaRuntimeVisuals {
            overlays: vec![3_u8],
            overlay_eases: layer.overlay_eases.clone(),
            overlay_ease_ranges: vec![0..1],
            overlay_events: layer.overlay_events.clone(),
            background_visual_layers: vec![layer.clone()],
            foreground_visual_layers: Vec::new(),
            player_actors: [9_u16; MAX_PLAYERS],
            player_events: std::array::from_fn(|_| Vec::new()),
            song_foreground: 11_u16,
            song_foreground_events: vec![build_song_lua_overlay_message_runtime(5.0, 4)],
            hidden_players: [false; MAX_PLAYERS],
            note_hides: std::array::from_fn(|_| Vec::new()),
            column_offsets: std::array::from_fn(|_| Vec::new()),
            screen_width: 800.0,
            screen_height: 600.0,
        };
        assert_eq!(visuals.overlays, [3]);
        assert_eq!(visuals.background_visual_layers[0].song_foreground, 9);
        assert_eq!(visuals.player_actors[0], 9);
        assert_eq!(visuals.song_foreground, 11);
    }

    #[test]
    fn exit_timing_matches_screen_policy() {
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Start), 0.33);
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Back), 1.0);

        assert_eq!(exit_total_seconds(ExitTransitionKind::Out), 1.5);
        assert_eq!(exit_total_seconds(ExitTransitionKind::Cancel), 0.5);

        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Out),
            GameplayExit::Complete
        );
        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Cancel),
            GameplayExit::Cancel
        );
    }

    #[test]
    fn gameplay_menu_input_plan_handles_start_and_back_holds() {
        assert_eq!(
            gameplay_menu_input_plan(GameplayMenuInput::P1Start, true, true, false, true, None,),
            GameplayMenuInputPlan::ArmHold(HoldToExitKey::Start)
        );
        assert_eq!(
            gameplay_menu_input_plan(
                GameplayMenuInput::P1Start,
                false,
                true,
                false,
                true,
                Some(HoldToExitKey::Start),
            ),
            GameplayMenuInputPlan::AbortHold(HoldToExitKey::Start)
        );
        assert_eq!(
            gameplay_menu_input_plan(GameplayMenuInput::P1Back, true, true, false, true, None),
            GameplayMenuInputPlan::ArmHold(HoldToExitKey::Back)
        );
        assert_eq!(
            gameplay_menu_input_plan(GameplayMenuInput::P1Back, true, true, false, false, None),
            GameplayMenuInputPlan::BeginExit(ExitTransitionKind::Cancel)
        );
        assert_eq!(
            gameplay_menu_input_plan(
                GameplayMenuInput::P2Back,
                false,
                false,
                true,
                true,
                Some(HoldToExitKey::Back),
            ),
            GameplayMenuInputPlan::AbortHold(HoldToExitKey::Back)
        );
    }

    #[test]
    fn gameplay_menu_input_plan_ignores_inactive_or_mismatched_inputs() {
        assert_eq!(
            gameplay_menu_input_plan(GameplayMenuInput::P2Start, true, true, false, true, None,),
            GameplayMenuInputPlan::None
        );
        assert_eq!(
            gameplay_menu_input_plan(
                GameplayMenuInput::P1Back,
                false,
                true,
                false,
                true,
                Some(HoldToExitKey::Start),
            ),
            GameplayMenuInputPlan::None
        );
        assert_eq!(
            gameplay_menu_input_plan(GameplayMenuInput::P1Start, false, true, false, true, None,),
            GameplayMenuInputPlan::None
        );
    }

    #[test]
    fn gameplay_offset_prompt_ignores_pad_lr_in_dedicated_menu_mode() {
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_left, true),
            None
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_right, true),
            None
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p2_left, true),
            None
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p2_right, true),
            None
        );
    }

    #[test]
    fn gameplay_offset_prompt_keeps_menu_lr_in_dedicated_menu_mode() {
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_menu_left, true),
            Some(-1)
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_menu_right, true),
            Some(1)
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p2_menu_left, true),
            Some(-1)
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p2_menu_right, true),
            Some(1)
        );
    }

    #[test]
    fn gameplay_offset_prompt_allows_pad_lr_when_fallback_enabled() {
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_left, false),
            Some(-1)
        );
        assert_eq!(
            gameplay_offset_prompt_choice_delta(VirtualAction::p1_right, false),
            Some(1)
        );
    }

    #[test]
    fn gameplay_exit_input_state_tracks_hold_exit_and_modifiers() {
        let mut state = GameplayExitInputState {
            shift_held: true,
            ctrl_held: true,
            ..GameplayExitInputState::default()
        };
        let started = Instant::now();

        state.arm_hold(HoldToExitKey::Back, started);
        assert_eq!(state.hold_to_exit_key, Some(HoldToExitKey::Back));
        assert_eq!(state.hold_to_exit_start, Some(started));
        assert_eq!(state.hold_to_exit_aborted_at, None);

        state.abort_hold(started);
        assert_eq!(state.hold_to_exit_key, None);
        assert_eq!(state.hold_to_exit_start, None);
        assert_eq!(state.hold_to_exit_aborted_at, Some(started));
        state.clear_aborted_hold();
        assert_eq!(state.hold_to_exit_aborted_at, None);

        assert!(state.begin_exit(ExitTransitionKind::Cancel, started));
        assert_eq!(
            state.exit_transition.map(|exit| exit.kind),
            Some(ExitTransitionKind::Cancel)
        );
        assert!(!state.begin_exit(ExitTransitionKind::Out, started));
        state.clear_exit();
        assert_eq!(state.exit_transition.map(|exit| exit.kind), None);

        state.arm_hold(HoldToExitKey::Start, started);
        state.reset();
        assert_eq!(state.hold_to_exit_key, None);
        assert_eq!(state.hold_to_exit_start, None);
        assert_eq!(state.exit_transition.map(|exit| exit.kind), None);
        assert!(!state.shift_held);
        assert!(!state.ctrl_held);
    }

    #[test]
    fn exit_alpha_respects_delay_and_fade() {
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 0.5),
            0.0,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 1.0),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 0.3),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 9.0),
            1.0,
        );
    }

    #[test]
    fn notefield_viewport_policy_matches_runtime_layout() {
        assert_near(RECEPTOR_Y_OFFSET_FROM_CENTER, -125.0);
        assert_near(RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, 145.0);

        assert_near(scroll_receptor_y(0.0, 0.0, 100.0, 500.0, 300.0), 100.0);
        assert_near(scroll_receptor_y(1.0, 0.0, 100.0, 500.0, 300.0), 500.0);
        assert_near(scroll_receptor_y(0.5, 0.0, 100.0, 500.0, 300.0), 300.0);
        assert_near(scroll_receptor_y(0.0, 1.0, 100.0, 500.0, 300.0), 300.0);
        assert_near(scroll_receptor_y(0.0, 2.0, 100.0, 500.0, 300.0), 500.0);
    }

    #[test]
    fn draw_distances_scale_by_viewport_and_centered_scroll() {
        assert_near(
            draw_distance_before_targets(480.0, 1.0),
            480.0 * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER,
        );
        assert_near(draw_distance_before_targets(480.0, 1.5), 1080.0);
        assert_near(
            draw_distance_after_targets(480.0, 1.0, 0.0),
            DRAW_DISTANCE_AFTER_TARGETS,
        );
        assert_near(draw_distance_after_targets(480.0, 1.0, 1.0), 288.0);
        assert_near(draw_distance_after_targets(480.0, 1.0, 0.5), 209.0);
        assert_near(draw_distance_after_targets(480.0, 1.0, 2.0), 288.0);
    }

    #[test]
    fn step_stats_notefield_width_matches_sl_style_widths() {
        assert_eq!(step_stats_notefield_width(4), Some(256.0));
        assert_eq!(step_stats_notefield_width(8), Some(512.0));
        assert_eq!(step_stats_notefield_width(0), None);
    }

    #[test]
    fn step_stats_upper_density_width_matches_zmod_policy() {
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Single),
            226.0,
        );
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Versus),
            226.0,
        );
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Double),
            226.0,
        );
    }

    #[test]
    fn step_stats_density_graph_width_matches_sl_double() {
        let width = step_stats_density_graph_width(
            StepStatsPlayStyle::Double,
            8,
            1,
            854.0,
            480.0,
            true,
            false,
        );
        let expected = ((854.0 - 512.0) * 0.5) * 0.95;
        assert_near(width, expected);
    }

    #[test]
    fn step_stats_density_graph_width_handles_centered_and_ultrawide() {
        assert_near(
            step_stats_density_graph_width(
                StepStatsPlayStyle::Single,
                4,
                1,
                854.0,
                480.0,
                true,
                true,
            ),
            299.0,
        );
        assert_near(
            step_stats_density_graph_width(
                StepStatsPlayStyle::Versus,
                4,
                2,
                2560.0,
                1080.0,
                true,
                false,
            ),
            512.0,
        );
    }

    #[test]
    fn density_life_points_skip_duplicate_and_update_same_time() {
        let mut points = vec![[1.0, 0.25]];

        assert!(!push_density_life_point(&mut points, 1.0, 0.25));
        assert_eq!(points, vec![[1.0, 0.25]]);

        assert!(push_density_life_point(&mut points, 1.0, 0.5));
        assert_eq!(points, vec![[1.0, 0.5]]);
    }

    #[test]
    fn density_life_points_replace_nearly_straight_segments() {
        let mut points = vec![[0.0, 0.0], [1.0, 0.1]];

        assert!(push_density_life_point(&mut points, 2.0, 0.2));

        assert_eq!(points.len(), 2);
        assert_near(points[1][0], 2.0);
        assert_near(points[1][1], 0.2);
    }

    #[test]
    fn density_life_points_keep_sharp_turns() {
        let mut points = vec![[0.0, 0.0], [1.0, 0.0]];

        assert!(push_density_life_point(&mut points, 2.0, 1.0));

        assert_eq!(points, vec![[0.0, 0.0], [1.0, 0.0], [2.0, 1.0]]);
    }

    #[test]
    fn density_graph_u0_tracks_visible_window() {
        let window = DensityGraphWindow {
            first_second: 0.0,
            last_second: 100.0,
            duration: 100.0,
            graph_w: 200.0,
            graph_h: 40.0,
            scaled_width: 400.0,
            u_window: 0.5,
        };

        assert_near(density_graph_u0_for_time(window, 10.0), 0.0);
        assert_near(density_graph_u0_for_time(window, 50.0), 0.375);
        assert_near(density_graph_u0_for_time(window, 70.0), 0.5);
        assert_near(
            density_graph_u0_for_time(
                DensityGraphWindow {
                    graph_w: 0.0,
                    ..window
                },
                70.0,
            ),
            0.0,
        );
    }

    #[test]
    fn density_graph_life_sampling_policy_is_bounded() {
        assert_eq!(density_graph_life_catch_up_steps(0.9, 1.0, 0.25), 0);
        assert_eq!(density_graph_life_catch_up_steps(1.0, 1.0, 0.25), 1);
        assert_eq!(density_graph_life_catch_up_steps(20.0, 1.0, 0.25), 64);
        assert_eq!(density_graph_life_catch_up_steps(1.0, 1.0, 0.0), 0);

        assert_eq!(
            density_graph_life_sample_x(0.0, 0.0, 10.0, 10.0, 100.0),
            None
        );
        assert_eq!(
            density_graph_life_sample_x(11.0, 0.0, 10.0, 10.0, 100.0),
            None
        );
        assert_near(
            density_graph_life_sample_x(5.0, 0.0, 10.0, 10.0, 100.0).unwrap(),
            50.0,
        );
        assert_near(
            density_graph_life_sample_x(5.0, -5.0, 10.0, 10.0, 100.0).unwrap(),
            100.0,
        );
    }

    #[test]
    fn gameplay_density_graph_state_defaults_to_disabled_graphs() {
        let state = GameplayDensityGraphState::default();

        assert_eq!(state.graph_w, 0.0);
        assert_eq!(state.graph_h, 0.0);
        assert_eq!(state.u_window, 1.0);
        assert_eq!(state.life_update_rate, 0.25);
        assert!(state.life_points.iter().all(|points| points.is_empty()));
        assert_eq!(state.life_dirty, [false; MAX_PLAYERS]);
        assert_eq!(state.top_scale_y, [1.0; MAX_PLAYERS]);
    }

    #[test]
    fn density_graph_view_top_mesh_height_clamps_scale() {
        let view = GameplayDensityGraphView {
            first_second: 0.0,
            last_second: 60.0,
            duration: 60.0,
            graph_w: 100.0,
            graph_h: 40.0,
            scaled_width: 100.0,
            u0: 0.0,
            u_window: 1.0,
            top_h: 24.0,
            top_w: [100.0, 80.0],
            top_scale_y: [1.25, -0.5],
        };

        assert_eq!(view.top_mesh_h(0), 24.0);
        assert_eq!(view.top_mesh_h(1), 0.0);
    }

    #[test]
    fn reference_bpm_prefers_chart_display_max() {
        let display_bpm = ChartDisplayBpm::Specified {
            min: 100.0,
            max: 180.0,
        };

        assert_eq!(
            reference_bpm_from_display_tag(Some(&display_bpm), "120"),
            Some(180.0)
        );
    }

    #[test]
    fn reference_bpm_random_chart_display_suppresses_fallback() {
        assert_eq!(
            reference_bpm_from_display_tag(Some(&ChartDisplayBpm::Random), "100:160"),
            None
        );
    }

    #[test]
    fn reference_bpm_uses_song_range_max_and_single_value() {
        assert_eq!(reference_bpm_from_display_tag(None, "100:160"), Some(160.0));
        assert_eq!(reference_bpm_from_display_tag(None, " 145 "), Some(145.0));
    }

    #[test]
    fn reference_bpm_ignores_empty_star_and_invalid_chart_max() {
        let invalid_chart = ChartDisplayBpm::Specified {
            min: 100.0,
            max: f64::NAN,
        };

        assert_eq!(reference_bpm_from_display_tag(None, ""), None);
        assert_eq!(reference_bpm_from_display_tag(None, "*"), None);
        assert_eq!(
            reference_bpm_from_display_tag(Some(&invalid_chart), "125"),
            Some(125.0)
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_places_two_players() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                2,
                0,
                viewport,
                SongLuaCompilePlayStyle::Versus,
                false,
                20.0,
                false,
            ),
            193.5,
        );
        assert_near(
            song_lua_compile_player_screen_x(
                2,
                1,
                viewport,
                SongLuaCompilePlayStyle::Versus,
                false,
                20.0,
                false,
            ),
            660.5,
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_centers_single_and_double() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                false,
                50.0,
                true,
            ),
            viewport.center_x(),
        );
        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Double,
                true,
                50.0,
                false,
            ),
            viewport.center_x(),
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_uses_side_and_offset_policy() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                false,
                10.0,
                false,
            ),
            203.5,
        );
        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                true,
                999.0,
                false,
            ),
            690.5,
        );
    }

    #[test]
    fn mini_value_uses_fallback_big_adjustment_and_clamps() {
        assert_near(mini_value_for_percent(50.0, 0.0, false), 0.5);
        assert_near(mini_value_for_percent(f32::NAN, 25.0, false), 0.25);
        assert_near(mini_value_for_percent(50.0, 0.0, true), -0.5);
        assert_near(mini_value_for_percent(-250.0, 0.0, false), -1.0);
        assert_near(mini_value_for_percent(250.0, 0.0, false), 1.5);
    }

    #[test]
    fn mini_value_and_draw_scale_use_visual_mask_big() {
        assert_near(mini_value_for_visual_mask(50.0, 0.0, 0), 0.5);
        assert_near(
            mini_value_for_visual_mask(50.0, 0.0, VISUAL_MASK_BIT_BIG),
            -0.5,
        );
        assert_near(mini_value_for_visual_mask(f32::NAN, 25.0, 0), 0.25);
        assert_near(player_draw_scale_for_visual_mask(1.0, 50.0, 0.0, 0), 2.25);
        assert_near(
            player_draw_scale_for_visual_mask(1.0, 50.0, 0.0, VISUAL_MASK_BIT_BIG),
            2.25,
        );
    }

    #[test]
    fn effective_mini_percent_uses_active_fallback_and_clear_all() {
        assert_eq!(MINI_PERCENT_MIN, -100.0);
        assert_eq!(MINI_PERCENT_MAX, 150.0);
        assert_near(effective_mini_percent(Some(25.0), 50.0, false), 25.0);
        assert_near(effective_mini_percent(Some(f32::NAN), 50.0, false), 50.0);
        assert_near(effective_mini_percent(None, 50.0, true), 0.0);
        assert_near(effective_mini_percent(None, 50.0, false), 50.0);
        assert_near(effective_mini_percent(Some(250.0), 0.0, false), 150.0);
        assert_near(effective_mini_percent(Some(-250.0), 0.0, false), -100.0);
    }

    #[test]
    fn mini_attack_target_supports_absolute_and_delta_modes() {
        assert_near(
            attack_mini_target_percent(25.0, MiniAttackMode::Absolute, 50.0),
            25.0,
        );
        assert_near(
            attack_mini_target_percent(25.0, MiniAttackMode::Delta, 50.0),
            75.0,
        );
    }

    #[test]
    fn attack_value_approaches_or_snaps_to_target() {
        let mut current = Some(10.0);
        approach_attack_value(&mut current, Some(50.0), 0.0, Some(2.0), 0.5, 10.0);
        assert_near(current.unwrap(), 20.0);

        approach_attack_value(&mut current, Some(50.0), 0.0, None, 1.0, 10.0);
        assert_near(current.unwrap(), 50.0);

        approach_attack_value(&mut current, None, 0.0, Some(1.0), 1.0, 10.0);
        assert_eq!(current, None);
    }

    #[test]
    fn attack_value_merge_uses_finite_override_or_base() {
        assert_near(merge_attack_value(0.25, Some(0.75)), 0.75);
        assert_near(merge_attack_value(0.25, Some(f32::NAN)), 0.25);
        assert_near(merge_attack_value(0.25, None), 0.25);
    }

    #[test]
    fn attack_effect_merges_apply_scalar_overrides() {
        let accel = merge_attack_accel_effects(
            AccelEffects {
                boost: 0.25,
                wave: 0.5,
                ..AccelEffects::default()
            },
            AccelOverrides {
                boost: Some(1.0),
                wave: Some(f32::NAN),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 1.0);
        assert_near(accel.wave, 0.5);

        let visibility = merge_attack_visibility_effects(
            VisibilityEffects {
                dark: 0.1,
                blind: 0.2,
                cover: 0.3,
            },
            VisibilityOverrides {
                dark: Some(1.0),
                blind: Some(f32::NAN),
                cover: None,
            },
        );
        assert_near(visibility.dark, 1.0);
        assert_near(visibility.blind, 0.2);
        assert_near(visibility.cover, 0.3);
    }

    #[test]
    fn attack_visual_merge_preserves_big_and_overrides_columns() {
        let mut base = VisualEffects {
            drunk: 0.25,
            big: 1.0,
            bumpy: 0.5,
            ..VisualEffects::default()
        };
        base.bumpy_cols[1] = 0.25;
        base.tiny_cols[2] = 0.5;

        let mut attack = VisualOverrides {
            drunk: Some(1.0),
            bumpy: Some(f32::NAN),
            ..VisualOverrides::default()
        };
        attack.bumpy_cols[1] = Some(0.75);
        attack.tiny_cols[2] = Some(f32::NAN);

        let visual = merge_attack_visual_effects(base, attack);

        assert_near(visual.drunk, 1.0);
        assert_near(visual.big, 1.0);
        assert_near(visual.bumpy, 0.5);
        assert_near(visual.bumpy_cols[1], 0.75);
        assert_near(visual.tiny_cols[2], 0.5);
    }

    #[test]
    fn attack_scroll_and_perspective_merges_use_base_for_invalid_overrides() {
        let scroll = merge_attack_scroll_effects(
            ScrollEffects {
                reverse: 0.25,
                split: 0.5,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                reverse: Some(1.0),
                split: Some(f32::NAN),
                centered: Some(0.75),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 1.0);
        assert_near(scroll.split, 0.5);
        assert_near(scroll.centered, 0.75);

        let perspective = merge_attack_perspective_effects(
            PerspectiveEffects {
                tilt: -0.5,
                skew: 0.25,
            },
            PerspectiveOverrides {
                tilt: Some(f32::NAN),
                skew: Some(1.0),
            },
        );
        assert_near(perspective.tilt, -0.5);
        assert_near(perspective.skew, 1.0);
    }

    #[test]
    fn effective_attack_outputs_use_profile_base_and_active_overrides() {
        let accel = effective_attack_accel_effects(
            false,
            ACCEL_MASK_BIT_BOOST,
            AccelOverrides {
                brake: Some(0.5),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 1.0);
        assert_near(accel.brake, 0.5);

        let visual = effective_attack_visual_effects(
            false,
            VISUAL_MASK_BIT_BIG,
            VisualOverrides {
                drunk: Some(0.75),
                ..VisualOverrides::default()
            },
        );
        assert_near(visual.big, 1.0);
        assert_near(visual.drunk, 0.75);

        let visibility = effective_attack_visibility_effects(VisibilityOverrides {
            dark: Some(1.0),
            ..VisibilityOverrides::default()
        });
        assert_near(visibility.dark, 1.0);

        let scroll = effective_attack_scroll_effects(
            false,
            ScrollEffects {
                reverse: 0.25,
                split: 0.5,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                reverse: Some(f32::NAN),
                centered: Some(0.75),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 0.25);
        assert_near(scroll.split, 0.5);
        assert_near(scroll.centered, 0.75);

        let perspective = effective_attack_perspective_effects(
            false,
            PerspectiveEffects {
                tilt: -0.5,
                skew: 0.25,
            },
            PerspectiveOverrides {
                tilt: Some(1.0),
                ..PerspectiveOverrides::default()
            },
        );
        assert_near(perspective.tilt, 1.0);
        assert_near(perspective.skew, 0.25);
    }

    #[test]
    fn effective_attack_outputs_clear_base_but_keep_active_overrides() {
        let accel = effective_attack_accel_effects(
            true,
            ACCEL_MASK_BIT_BOOST,
            AccelOverrides {
                wave: Some(0.5),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 0.0);
        assert_near(accel.wave, 0.5);

        let visual = effective_attack_visual_effects(
            true,
            VISUAL_MASK_BIT_BIG,
            VisualOverrides {
                drunk: Some(0.75),
                ..VisualOverrides::default()
            },
        );
        assert_near(visual.big, 0.0);
        assert_near(visual.drunk, 0.75);

        let scroll = effective_attack_scroll_effects(
            true,
            ScrollEffects {
                reverse: 1.0,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                centered: Some(0.5),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 0.0);
        assert_near(scroll.centered, 0.5);
    }

    #[test]
    fn effective_attack_scroll_speed_uses_active_or_base_clear_policy() {
        assert!(matches!(
            effective_attack_scroll_speed(
                false,
                Some(ScrollSpeedSetting::CMod(650.0)),
                ScrollSpeedSetting::XMod(2.0),
            ),
            ScrollSpeedSetting::CMod(v) if (v - 650.0).abs() <= 0.000_001
        ));
        assert!(matches!(
            effective_attack_scroll_speed(false, None, ScrollSpeedSetting::XMod(2.0)),
            ScrollSpeedSetting::XMod(v) if (v - 2.0).abs() <= 0.000_001
        ));
        assert_eq!(
            effective_attack_scroll_speed(true, None, ScrollSpeedSetting::XMod(2.0)),
            ScrollSpeedSetting::default()
        );
    }

    #[test]
    fn attack_mini_approach_uses_base_and_clamps() {
        let mut current = None;
        approach_attack_mini_percent_to_target(&mut current, Some(100.0), 0.0, Some(1.0), 0.5);
        assert_near(current.unwrap(), 50.0);

        let mut invalid_current = Some(f32::NAN);
        approach_attack_mini_percent_to_target(
            &mut invalid_current,
            Some(75.0),
            25.0,
            Some(0.25),
            1.0,
        );
        assert_near(invalid_current.unwrap(), 50.0);

        let mut high = None;
        approach_attack_mini_percent_to_target(&mut high, Some(250.0), 0.0, None, 1.0);
        assert_near(high.unwrap(), 150.0);

        let mut low = None;
        approach_attack_mini_percent_to_target(&mut low, Some(-250.0), 0.0, None, 1.0);
        assert_near(low.unwrap(), -100.0);
    }

    #[test]
    fn player_draw_scale_uses_tilt_and_absolute_mini() {
        assert_near(player_draw_scale_for_mini(0.0, 0.0), 1.0);
        assert_near(player_draw_scale_for_mini(-1.0, 0.0), 1.5);
        assert_near(player_draw_scale_for_mini(0.0, -0.5), 1.5);
        assert_near(player_draw_scale_for_mini(1.0, 0.5), 2.25);
    }

    #[test]
    fn accel_effects_decode_profile_mask_bits() {
        let effects = AccelEffects::from_mask_bits(
            ACCEL_MASK_BIT_BOOST
                | ACCEL_MASK_BIT_BRAKE
                | ACCEL_MASK_BIT_WAVE
                | ACCEL_MASK_BIT_EXPAND
                | ACCEL_MASK_BIT_BOOMERANG,
        );

        assert_near(effects.boost, 1.0);
        assert_near(effects.brake, 1.0);
        assert_near(effects.wave, 1.0);
        assert_near(effects.expand, 1.0);
        assert_near(effects.boomerang, 1.0);
        assert_eq!(AccelEffects::from_mask_bits(0).boost, 0.0);
    }

    #[test]
    fn visual_effects_decode_and_reencode_mask_bits() {
        let mask = VISUAL_MASK_BIT_DRUNK
            | VISUAL_MASK_BIT_BIG
            | VISUAL_MASK_BIT_FLIP
            | VISUAL_MASK_BIT_BUMPY
            | VISUAL_MASK_BIT_BEAT;
        let effects = VisualEffects::from_mask_bits(mask);

        assert_near(effects.drunk, 1.0);
        assert_near(effects.big, 1.0);
        assert_near(effects.flip, 1.0);
        assert_near(effects.bumpy, 1.0);
        assert_near(effects.beat, 1.0);
        assert_eq!(effects.to_mask_bits() & mask, mask);

        let mut column_bumpy = VisualEffects::default();
        column_bumpy.bumpy_cols[2] = 0.5;
        assert_near(column_bumpy.bumpy, 0.0);
        assert_ne!(column_bumpy.to_mask_bits() & VISUAL_MASK_BIT_BUMPY, 0);

        let signed_effects = VisualEffects {
            drunk: -1.0,
            dizzy: -1.0,
            confusion: -1.0,
            big: -1.0,
            flip: -1.0,
            invert: -1.0,
            tornado: -1.0,
            tipsy: -1.0,
            bumpy: -1.0,
            beat: -1.0,
            ..VisualEffects::default()
        };
        let signed_mask = signed_effects.to_mask_bits();
        assert_ne!(signed_mask & VISUAL_MASK_BIT_DRUNK, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_DIZZY, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_CONFUSION, 0);
        assert_eq!(signed_mask & VISUAL_MASK_BIT_BIG, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_FLIP, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_INVERT, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_TORNADO, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_TIPSY, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_BUMPY, 0);
        assert_ne!(signed_mask & VISUAL_MASK_BIT_BEAT, 0);
    }

    #[test]
    fn visual_overrides_approach_base_and_clear_when_reached() {
        let mut visual = VisualOverrides {
            drunk: Some(1.0),
            tipsy: None,
            ..VisualOverrides::default()
        };
        visual.bumpy_cols[1] = Some(1.0);

        let mut base = VisualEffects::default();
        base.bumpy_cols[1] = 0.25;

        approach_visual_overrides_to_base(&mut visual, base, 0.5);

        assert_near(visual.drunk.unwrap(), 0.5);
        assert_eq!(visual.tipsy, None);
        assert_near(visual.bumpy_cols[1].unwrap(), 0.5);

        approach_visual_overrides_to_base(&mut visual, base, 1.0);

        assert_eq!(visual.drunk, None);
        assert_eq!(visual.bumpy_cols[1], None);
    }

    #[test]
    fn visual_overrides_approach_target_scalars_and_columns() {
        let mut current = VisualOverrides {
            flip: Some(1.0),
            ..VisualOverrides::default()
        };

        let mut target = VisualOverrides {
            drunk: Some(1.0),
            flip: None,
            ..VisualOverrides::default()
        };
        target.bumpy_cols[2] = Some(-1.0);

        let mut speed = VisualOverrides {
            drunk: Some(2.0),
            ..VisualOverrides::default()
        };
        speed.bumpy_cols[2] = Some(4.0);

        let mut base = VisualEffects {
            drunk: 0.25,
            ..VisualEffects::default()
        };
        base.bumpy_cols[2] = 0.0;

        approach_visual_overrides_to_target(&mut current, target, speed, base, 0.25);

        assert_near(current.drunk.unwrap(), 0.75);
        assert_eq!(current.flip, None);
        assert_near(current.bumpy_cols[2].unwrap(), -1.0);
    }

    #[test]
    fn appearance_effects_decode_mask_bits_and_default_speeds() {
        let effects = AppearanceEffects::from_mask_bits(
            APPEARANCE_MASK_BIT_HIDDEN
                | APPEARANCE_MASK_BIT_SUDDEN
                | APPEARANCE_MASK_BIT_STEALTH
                | APPEARANCE_MASK_BIT_BLINK
                | APPEARANCE_MASK_BIT_RANDOM_VANISH,
        );

        assert_near(effects.hidden, 1.0);
        assert_near(effects.sudden, 1.0);
        assert_near(effects.stealth, 1.0);
        assert_near(effects.blink, 1.0);
        assert_near(effects.random_vanish, 1.0);

        let speeds = AppearanceEffects::approach_speeds();
        assert_near(speeds.hidden, 1.0);
        assert_near(speeds.hidden_offset, 1.0);
        assert_near(speeds.random_vanish, 1.0);
    }

    #[test]
    fn appearance_target_applies_overrides_and_speeds() {
        let mut target = AppearanceEffects {
            hidden: 0.2,
            sudden: 0.3,
            blink: 0.4,
            ..AppearanceEffects::default()
        };
        let mut speed = AppearanceEffects::approach_speeds();

        apply_appearance_target(
            &mut target,
            &mut speed,
            AppearanceOverrides {
                hidden: Some(0.75),
                sudden: Some(0.25),
                random_vanish: Some(1.0),
                ..AppearanceOverrides::default()
            },
            AppearanceOverrides {
                hidden: Some(2.0),
                sudden: Some(-1.0),
                ..AppearanceOverrides::default()
            },
        );

        assert_near(target.hidden, 0.75);
        assert_near(speed.hidden, 2.0);
        assert_near(target.sudden, 0.25);
        assert_near(speed.sudden, 0.0);
        assert_near(target.blink, 0.4);
        assert_near(speed.blink, 1.0);
        assert_near(target.random_vanish, 1.0);
        assert_near(speed.random_vanish, 1.0);
    }

    #[test]
    fn appearance_effects_approach_targets_by_speed() {
        let mut current = AppearanceEffects {
            hidden: 0.0,
            sudden: 1.0,
            random_vanish: 0.25,
            ..AppearanceEffects::default()
        };

        approach_appearance_effects(
            &mut current,
            AppearanceEffects {
                hidden: 1.0,
                sudden: 0.0,
                random_vanish: 1.0,
                ..AppearanceEffects::default()
            },
            AppearanceEffects {
                hidden: 2.0,
                sudden: 4.0,
                random_vanish: 100.0,
                ..AppearanceEffects::default()
            },
            0.25,
        );

        assert_near(current.hidden, 0.5);
        assert_near(current.sudden, 0.0);
        assert_near(current.random_vanish, 1.0);

        approach_appearance_effects(
            &mut current,
            AppearanceEffects::default(),
            AppearanceEffects::approach_speeds(),
            -1.0,
        );

        assert_near(current.hidden, 0.5);
        assert_near(current.random_vanish, 1.0);
    }

    #[test]
    fn chart_attack_windows_parse_time_len_and_mods_chunks() {
        let windows = parse_chart_attack_windows(
            "TIME=1.25:LEN=2.5:MODS=*2 50% drunk, TIME=5:END=8:MODS=clearall",
        );

        assert_eq!(windows.len(), 2);
        assert_near(windows[0].start_second, 1.25);
        assert_near(windows[0].len_seconds, 2.5);
        assert_eq!(windows[0].mods, "*2 50% drunk");
        assert_near(windows[1].start_second, 5.0);
        assert_near(windows[1].len_seconds, 3.0);
        assert_eq!(windows[1].mods, "clearall");
    }

    #[test]
    fn chart_attack_windows_skip_bad_chunks_and_clamp_lengths() {
        let windows = parse_chart_attack_windows(
            "garbage TIME=nan:LEN=2:MODS=drunk TIME=4:END=2:MODS=tipsy \
             TIME=6:LEN=abc:MODS=wave TIME=9:LEN=1:MODS=,",
        );

        assert_eq!(windows.len(), 2);
        assert_near(windows[0].start_second, 4.0);
        assert_near(windows[0].len_seconds, 0.0);
        assert_eq!(windows[0].mods, "tipsy");
        assert_near(windows[1].start_second, 6.0);
        assert_near(windows[1].len_seconds, 0.0);
        assert_eq!(windows[1].mods, "wave");
        assert!(parse_chart_attack_windows("").is_empty());
        assert!(parse_chart_attack_windows("LEN=1:MODS=drunk").is_empty());
    }

    #[test]
    fn random_attack_windows_use_fixed_timing_policy() {
        let windows = build_random_attack_windows(18.0, 0, 12345);

        assert_eq!(windows.len(), 3);
        assert_near(windows[0].start_second, 4.5);
        assert_near(windows[1].start_second, 10.0);
        assert_near(windows[2].start_second, 15.5);
        for window in &windows {
            assert_near(window.len_seconds, RANDOM_ATTACK_RUN_TIME_SECONDS);
            assert!(RANDOM_ATTACK_MOD_POOL.contains(&window.mods.as_str()));
        }
    }

    #[test]
    fn random_attack_windows_are_seeded_by_player_and_count() {
        let player_one = build_random_attack_windows(18.0, 0, 99);
        let player_one_again = build_random_attack_windows(18.0, 0, 99);
        let player_two = build_random_attack_windows(18.0, 1, 99);
        let longer_song = build_random_attack_windows(24.0, 0, 99);

        assert_eq!(player_one, player_one_again);
        assert_ne!(player_one, player_two);
        assert_ne!(player_one, longer_song);
        assert_ne!(
            random_attack_seed(99, 0, player_one.len()),
            random_attack_seed(99, 1, player_one.len()),
        );
    }

    #[test]
    fn random_attack_windows_skip_invalid_or_too_short_songs() {
        assert!(build_random_attack_windows(f32::NAN, 0, 1).is_empty());
        assert!(build_random_attack_windows(0.0, 0, 1).is_empty());
        assert!(build_random_attack_windows(4.5, 0, 1).is_empty());
        assert_eq!(build_random_attack_windows(4.6, 0, 1).len(), 1);
    }

    #[test]
    fn attack_windows_for_mode_select_chart_random_or_off() {
        let chart = "TIME=1:LEN=2:MODS=drunk";

        assert!(
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::Off, 0, 99, 18.0)
                .is_empty()
        );

        let parsed =
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::On, 0, 99, 18.0);
        assert_eq!(parsed.len(), 1);
        assert_near(parsed[0].start_second, 1.0);
        assert_near(parsed[0].len_seconds, 2.0);
        assert_eq!(parsed[0].mods, "drunk");

        let random =
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::Random, 0, 99, 18.0);
        assert_eq!(random, build_random_attack_windows(18.0, 0, 99));
    }

    #[test]
    fn attack_windows_for_mode_handles_missing_chart_attacks() {
        assert!(
            build_attack_windows_for_mode(None, GameplayAttackMode::On, 0, 99, 18.0).is_empty()
        );
        assert!(
            !build_attack_windows_for_mode(None, GameplayAttackMode::Random, 0, 99, 18.0)
                .is_empty()
        );
    }

    #[test]
    fn chart_attacks_enabled_for_mode_matches_profile_policy() {
        assert!(!chart_attacks_enabled_for_mode(
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::Off,
        ));
        assert!(!chart_attacks_enabled_for_mode(
            Some("   "),
            GameplayAttackMode::On,
        ));
        assert!(chart_attacks_enabled_for_mode(
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::On,
        ));
        assert!(chart_attacks_enabled_for_mode(
            None,
            GameplayAttackMode::Random,
        ));
    }

    #[test]
    fn player_chart_changes_for_options_tracks_chart_mutation_sources() {
        assert!(!player_chart_changes_for_options(
            false,
            GameplayTurnOption::None,
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            true,
            GameplayTurnOption::None,
            None,
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            false,
            GameplayTurnOption::Mirror,
            None,
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            false,
            GameplayTurnOption::None,
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::On,
        ));
    }

    #[test]
    fn gameplay_attack_runtime_state_defaults_to_empty_inactive_state() {
        let state = GameplayAttackRuntimeState::default();

        assert!(state.mask_windows.iter().all(|windows| windows.is_empty()));
        assert!(
            state
                .song_lua_ease_windows
                .iter()
                .all(|windows| windows.is_empty())
        );
        assert!(!state.cleared_for_outro);
        assert_eq!(state.clear_all, [false; MAX_PLAYERS]);
        assert_eq!(state.scroll_speed, [None; MAX_PLAYERS]);
        assert_eq!(state.mini_percent, [None; MAX_PLAYERS]);
    }

    #[test]
    fn outro_attack_visual_clear_snapshots_active_visual_once() {
        let mut cleared = false;
        let mut active = [VisualOverrides::default(); MAX_PLAYERS];
        let mut outro = [VisualOverrides::default(); MAX_PLAYERS];
        active[0].drunk = Some(0.25);
        active[1].tipsy = Some(0.75);

        begin_outro_attack_visual_clear(&mut cleared, 2, &active, &mut outro);

        assert!(cleared);
        assert_eq!(outro[0].drunk, Some(0.25));
        assert_eq!(outro[1].tipsy, Some(0.75));

        active[0].drunk = Some(1.0);
        active[1].tipsy = Some(1.0);
        begin_outro_attack_visual_clear(&mut cleared, 2, &active, &mut outro);

        assert_eq!(outro[0].drunk, Some(0.25));
        assert_eq!(outro[1].tipsy, Some(0.75));
    }

    #[test]
    fn outro_attack_visual_clear_only_copies_active_players() {
        let mut cleared = false;
        let mut active = [VisualOverrides::default(); MAX_PLAYERS];
        let mut outro = [VisualOverrides::default(); MAX_PLAYERS];
        active[0].drunk = Some(0.25);
        active[1].tipsy = Some(0.75);

        begin_outro_attack_visual_clear(&mut cleared, 1, &active, &mut outro);

        assert!(cleared);
        assert_eq!(outro[0].drunk, Some(0.25));
        assert!(!outro[1].any());
    }

    #[test]
    fn active_attack_refresh_applies_active_windows_and_eases() {
        let attack_windows = [attack_mask_window(
            0.0,
            2.0,
            parse_attack_mods("50% drunk,30% reverse,25% mini,stealth,dark,C650"),
        )];
        let lua_windows = [song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            0.0,
            2.0,
            2.0,
            0.0,
            90.0,
        )];

        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now: 1.0,
                delta_time: 0.5,
                attacks_cleared_for_outro: false,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 10.0,
                attack_windows: &attack_windows,
                song_lua_ease_windows: &lua_windows,
            },
            ActiveAttackRefreshState {
                attack_current_appearance: AppearanceEffects::default(),
                active_attack_visual: VisualOverrides::default(),
                active_attack_visibility: VisibilityOverrides::default(),
                active_attack_scroll: ScrollOverrides::default(),
                active_attack_mini_percent: None,
                outro_attack_visual: VisualOverrides::default(),
            },
        );

        assert!(!output.active_attack_clear_all);
        assert_near(output.attack_target_appearance.stealth, 1.0);
        assert_near(output.active_attack_appearance.stealth, 0.5);
        assert_eq!(output.active_attack_visual.drunk, Some(0.5));
        assert_eq!(output.active_attack_visibility.dark, Some(1.0));
        assert_eq!(output.active_attack_scroll.reverse, Some(0.3));
        assert_eq!(output.active_attack_mini_percent, Some(25.0));
        assert!(matches!(
            output.active_attack_scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));
        assert_eq!(output.player_transform.rotation_z, Some(45.0));
    }

    #[test]
    fn active_attack_refresh_outro_clears_visuals_and_preserves_visibility() {
        let lua_windows = [song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            0.0,
            2.0,
            2.0,
            0.0,
            90.0,
        )];
        let mut outro_visual = VisualOverrides::default();
        outro_visual.drunk = Some(0.5);
        let visibility = VisibilityOverrides {
            dark: Some(1.0),
            ..VisibilityOverrides::default()
        };

        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now: 1.0,
                delta_time: 1.0,
                attacks_cleared_for_outro: true,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 0.0,
                attack_windows: &[],
                song_lua_ease_windows: &lua_windows,
            },
            ActiveAttackRefreshState {
                attack_current_appearance: AppearanceEffects::default(),
                active_attack_visual: VisualOverrides::default(),
                active_attack_visibility: visibility,
                active_attack_scroll: ScrollOverrides {
                    reverse: Some(1.0),
                    ..ScrollOverrides::default()
                },
                active_attack_mini_percent: Some(50.0),
                outro_attack_visual: outro_visual,
            },
        );

        assert!(!output.active_attack_clear_all);
        assert!(!output.active_attack_visual.any());
        assert!(!output.outro_attack_visual.any());
        assert_eq!(output.active_attack_visibility.dark, Some(1.0));
        assert!(!output.active_attack_scroll.any());
        assert_eq!(output.active_attack_mini_percent, None);
        assert_eq!(output.player_transform.rotation_z, Some(45.0));
    }

    #[test]
    fn attack_mask_windows_filter_noops_and_invalid_durations() {
        let attacks = [
            ChartAttackWindow {
                start_second: 1.0,
                len_seconds: 0.0,
                mods: "drunk".to_string(),
            },
            ChartAttackWindow {
                start_second: 2.0,
                len_seconds: 1.0,
                mods: "unknown".to_string(),
            },
            ChartAttackWindow {
                start_second: f32::NAN,
                len_seconds: 1.0,
                mods: "drunk".to_string(),
            },
        ];

        assert!(build_attack_mask_windows(&attacks).is_empty());
    }

    #[test]
    fn attack_mask_window_keeps_runtime_mods() {
        let attack = ChartAttackWindow {
            start_second: 1.5,
            len_seconds: 2.25,
            mods: "*2 50% drunk,25% mini,C600".to_string(),
        };
        let window = attack_mask_window_from_parts(&attack, parse_attack_mods(&attack.mods))
            .expect("runtime mods should build an attack mask window");

        assert_near(window.start_second, 1.5);
        assert_near(window.end_second, 3.75);
        assert_near(window.sustain_end_second, 3.75);
        assert!(!window.persist_after_end);
        assert!(!window.clear_all);
        assert_eq!(window.chart, ChartAttackEffects::default());
        assert_eq!(window.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(window.mini_percent, Some(25.0));
        assert_eq!(window.mini_mode, MiniAttackMode::Absolute);
        assert_eq!(window.mini_speed, Some(1.0));
        assert_eq!(window.visual.drunk, Some(0.5));
        assert_eq!(window.visual_speed.drunk, Some(2.0));
    }

    #[test]
    fn song_lua_constant_attack_mask_window_persists_runtime_mods() {
        let window =
            build_song_lua_constant_attack_mask_window(1.25, 3.5, "50% drunk,25% mini,*2 C600")
                .expect("runtime SongLua mods should build a constant attack mask");

        assert_near(window.start_second, 1.25);
        assert_near(window.end_second, 3.5);
        assert_eq!(window.sustain_end_second, f32::MAX);
        assert!(window.persist_after_end);
        assert!(!window.clear_all);
        assert_eq!(window.chart, ChartAttackEffects::default());
        assert_eq!(window.visual.drunk, Some(0.5));
        assert_eq!(window.mini_percent, Some(25.0));
        assert_eq!(window.mini_mode, MiniAttackMode::Delta);
        assert_eq!(window.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
    }

    #[test]
    fn song_lua_constant_attack_mask_window_filters_noops_and_invalid_ranges() {
        assert!(build_song_lua_constant_attack_mask_window(2.0, 2.0, "50% drunk").is_none());
        assert!(build_song_lua_constant_attack_mask_window(2.0, 1.0, "50% drunk").is_none());
        assert!(build_song_lua_constant_attack_mask_window(1.0, 2.0, "unknown").is_none());
    }

    #[test]
    fn attack_mask_window_keeps_chart_masks_and_turn_bits() {
        let attack = ChartAttackWindow {
            start_second: 4.0,
            len_seconds: 3.0,
            mods: "mirror,mines,noholds,planted".to_string(),
        };
        let window = attack_mask_window_from_parts(&attack, parse_attack_mods(&attack.mods))
            .expect("chart mods should build an attack mask window");

        assert_eq!(window.chart.insert_mask, INSERT_MASK_BIT_MINES);
        assert_eq!(window.chart.remove_mask, REMOVE_MASK_BIT_NO_HOLDS);
        assert_eq!(window.chart.holds_mask, HOLDS_MASK_BIT_PLANTED);
        assert_eq!(
            window.chart.turn_bits,
            turn_option_bits(GameplayTurnOption::Mirror)
        );
        assert!(!window.clear_all);
        assert_eq!(window.scroll_speed, None);
        assert_eq!(window.mini_percent, None);
    }

    #[test]
    fn attack_mask_windows_keep_clearall() {
        let attacks = [ChartAttackWindow {
            start_second: 5.0,
            len_seconds: 1.0,
            mods: "clearall".to_string(),
        }];
        let windows = build_attack_mask_windows(&attacks);

        assert_eq!(windows.len(), 1);
        assert!(windows[0].clear_all);
        assert_eq!(windows[0].chart, ChartAttackEffects::default());
    }

    #[test]
    fn chart_attack_row_range_uses_timing_seconds() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);
        let attack = ChartAttackWindow {
            start_second: 0.5,
            len_seconds: 1.0,
            mods: "mirror".to_string(),
        };

        assert_eq!(
            chart_attack_row_range(&attack, &timing),
            Some((ROWS_PER_BEAT as usize / 2, ROWS_PER_BEAT as usize * 3 / 2)),
        );
        assert_eq!(
            chart_attack_turn_seed(99, 0, 0),
            chart_attack_turn_seed(99, 0, 0),
        );
        assert_ne!(
            chart_attack_turn_seed(99, 0, 0),
            chart_attack_turn_seed(99, 1, 0),
        );
    }

    #[test]
    fn attack_turn_mod_applies_mirror_and_special_turns() {
        let mut notes = (0..4)
            .map(|col| {
                let mut note = test_note_at(NoteType::Tap, None, false, 0, 0.0);
                note.column = col;
                note
            })
            .collect::<Vec<_>>();

        apply_attack_turn_mod(&mut notes, 0, 4, GameplayTurnOption::Mirror, 1, 0);

        let cols: Vec<_> = notes.iter().map(|note| note.column).collect();
        assert_eq!(cols, vec![3, 2, 1, 0]);

        apply_attack_turn_mod(&mut notes, 0, 4, GameplayTurnOption::None, 1, 0);
        let unchanged_cols: Vec<_> = notes.iter().map(|note| note.column).collect();
        assert_eq!(unchanged_cols, cols);
    }

    #[test]
    fn chart_attack_windows_apply_only_targeted_rows() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize * 2, 2.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 2;

        apply_chart_attack_windows(
            &mut notes,
            &[ChartAttackWindow {
                start_second: 0.5,
                len_seconds: 1.0,
                mods: "mirror".to_string(),
            }],
            &timing,
            0,
            4,
            0,
            7,
        );

        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (ROWS_PER_BEAT as usize * 2, 2),
            ],
        );
    }

    #[test]
    fn chart_attacks_for_mode_apply_enabled_chart_windows() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize * 2, 2.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 2;

        apply_chart_attacks_for_mode(
            &mut notes,
            Some("TIME=0.5:LEN=1:MODS=mirror"),
            GameplayAttackMode::On,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );

        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (ROWS_PER_BEAT as usize * 2, 2),
            ],
        );
    }

    #[test]
    fn chart_attacks_for_mode_noops_when_disabled_or_missing() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let original = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        let original_rows_and_cols: Vec<_> = original
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        let mut off_notes = original.clone();
        let mut missing_notes = original.clone();

        apply_chart_attacks_for_mode(
            &mut off_notes,
            Some("TIME=0:LEN=2:MODS=mirror"),
            GameplayAttackMode::Off,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );
        apply_chart_attacks_for_mode(
            &mut missing_notes,
            None,
            GameplayAttackMode::On,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );

        let off_rows_and_cols: Vec<_> = off_notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        let missing_rows_and_cols: Vec<_> = missing_notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(off_rows_and_cols, original_rows_and_cols);
        assert_eq!(missing_rows_and_cols, original_rows_and_cols);
    }

    #[test]
    fn chart_attack_transforms_apply_per_player_and_rebuild_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 4;
        notes[3].column = 5;
        let mut note_ranges = [(0usize, 2usize), (2usize, 4usize)];
        let disabled = ChartAttackTransformPlayer {
            chart_attacks: None,
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };
        let mut players = [disabled; MAX_PLAYERS];
        players[0] = ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0.5:LEN=1:MODS=mirror"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };

        apply_chart_attack_transforms(&mut notes, &mut note_ranges, 4, 2, &players, 7, 3.0);

        assert_eq!(note_ranges, [(0, 2), (2, 4)]);
        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (0, 4),
                (ROWS_PER_BEAT as usize, 5),
            ],
        );
    }

    #[test]
    fn chart_attack_transforms_duplicate_single_player_range() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        let mut note_ranges = [(0usize, 2usize), (99usize, 99usize)];
        let disabled = ChartAttackTransformPlayer {
            chart_attacks: None,
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };
        let mut players = [disabled; MAX_PLAYERS];
        players[0] = ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0.5:LEN=1:MODS=mirror"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };

        apply_chart_attack_transforms(&mut notes, &mut note_ranges, 4, 1, &players, 7, 3.0);

        assert_eq!(note_ranges[0], (0, 2));
        assert_eq!(note_ranges[1], note_ranges[0]);
    }

    #[test]
    fn active_attack_targets_mark_current_runtime_targets_only() {
        let windows = build_attack_mask_windows(&[
            ChartAttackWindow {
                start_second: 0.0,
                len_seconds: 1.0,
                mods: "tipsy".to_string(),
            },
            ChartAttackWindow {
                start_second: 1.0,
                len_seconds: 2.0,
                mods: "50% drunk,30% reverse,25% mini".to_string(),
            },
            ChartAttackWindow {
                start_second: 5.0,
                len_seconds: 1.0,
                mods: "clearall".to_string(),
            },
        ]);

        let targets = collect_active_attack_targets(&windows, 2.0);

        assert!(!targets.clear_all);
        assert_eq!(targets.visual.drunk, Some(0.0));
        assert_eq!(targets.visual.tipsy, None);
        assert_eq!(targets.scroll.reverse, Some(0.0));
        assert!(targets.mini_percent);
    }

    #[test]
    fn active_attack_targets_use_half_open_time_windows() {
        let windows = build_attack_mask_windows(&[ChartAttackWindow {
            start_second: 1.0,
            len_seconds: 1.0,
            mods: "clearall".to_string(),
        }]);

        assert!(!collect_active_attack_targets(&windows, 0.99).clear_all);
        assert!(collect_active_attack_targets(&windows, 1.0).clear_all);
        assert!(collect_active_attack_targets(&windows, 1.99).clear_all);
        assert!(!collect_active_attack_targets(&windows, 2.0).clear_all);
    }

    #[test]
    fn persisted_attack_targets_are_blocked_by_active_replacements() {
        assert!(persisted_target_allowed(false, true, Some(0.0)));
        assert!(persisted_target_allowed(true, false, None));
        assert!(!persisted_target_allowed(true, true, None));
        assert!(!persisted_target_allowed(true, false, Some(0.0)));

        let mut targets = AttackActiveTargets::default();
        assert!(persisted_mini_allowed(false, targets));
        assert!(persisted_mini_allowed(true, targets));

        targets.mini_percent = true;
        assert!(!persisted_mini_allowed(true, targets));

        targets.mini_percent = false;
        targets.clear_all = true;
        assert!(!persisted_mini_allowed(true, targets));
    }

    #[test]
    fn active_attack_mask_window_applies_values_and_speeds() {
        let mut mods = ParsedAttackMods {
            scroll_speed: Some(ScrollSpeedSetting::CMod(650.0)),
            mini_percent: Some(40.0),
            ..ParsedAttackMods::default()
        };
        mods.accel.boost = Some(0.75);
        mods.visual.drunk = Some(1.0);
        mods.visual_speed.drunk = Some(0.25);
        mods.appearance.hidden = Some(1.0);
        mods.appearance_speed.hidden = Some(0.5);
        mods.visibility.dark = Some(1.0);
        mods.scroll.reverse = Some(0.5);
        mods.scroll_approach_speed.reverse = Some(0.75);
        mods.perspective.tilt = Some(-1.0);
        let window = attack_mask_window(1.0, 4.0, mods);
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects::default());

        apply_active_attack_mask_window(
            &mut values,
            &window,
            AttackActiveTargets::default(),
            false,
            20.0,
        );

        assert_near(values.accel.boost.unwrap(), 0.75);
        assert_near(values.visual.drunk.unwrap(), 1.0);
        assert_near(values.visual_speed.drunk.unwrap(), 0.25);
        assert_near(values.appearance_target.hidden, 1.0);
        assert_near(values.appearance_speed.hidden, 0.5);
        assert_near(values.visibility.dark.unwrap(), 1.0);
        assert_near(values.scroll.reverse.unwrap(), 0.5);
        assert_near(values.scroll_approach_speed.reverse.unwrap(), 0.75);
        assert_near(values.perspective.tilt.unwrap(), -1.0);
        assert!(matches!(
            values.scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));
        assert_near(values.mini_percent.unwrap(), 40.0);
    }

    #[test]
    fn active_attack_mask_window_clearall_resets_values_and_delta_mini_base() {
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects {
            hidden: 1.0,
            ..AppearanceEffects::default()
        });
        values.accel.boost = Some(1.0);

        let mut mods = ParsedAttackMods {
            clear_all: true,
            mini_percent: Some(25.0),
            ..ParsedAttackMods::default()
        };
        mods.visual.drunk = Some(0.5);
        let mut window = attack_mask_window(1.0, 4.0, mods);
        window.mini_mode = MiniAttackMode::Delta;

        apply_active_attack_mask_window(
            &mut values,
            &window,
            AttackActiveTargets::default(),
            false,
            100.0,
        );

        assert!(values.clear_all);
        assert_eq!(values.accel.boost, None);
        assert_near(values.appearance_target.hidden, 0.0);
        assert_near(values.visual.drunk.unwrap(), 0.5);
        assert_near(values.mini_percent.unwrap(), 25.0);
    }

    #[test]
    fn active_attack_mask_window_blocks_persisted_replaced_targets() {
        let mut mods = ParsedAttackMods::default();
        mods.visual.drunk = Some(0.75);
        mods.visual.bumpy_cols[2] = Some(1.0);
        mods.scroll.reverse = Some(0.5);
        let window = attack_mask_window(1.0, 4.0, mods);
        let mut targets = AttackActiveTargets::default();
        targets.visual.drunk = Some(0.0);
        targets.scroll.reverse = Some(0.0);
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects::default());

        apply_active_attack_mask_window(&mut values, &window, targets, true, 0.0);

        assert_eq!(values.visual.drunk, None);
        assert_eq!(values.scroll.reverse, None);
        assert_near(values.visual.bumpy_cols[2].unwrap(), 1.0);
    }

    #[test]
    fn attack_mod_parser_keeps_scroll_override_and_partial_levels() {
        let mods = parse_attack_mods("0.5x,20% flip,50% hidden,30% blink,25% mini");

        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(0.5)));
        assert_eq!(mods.visual.flip, Some(0.2));
        assert_eq!(mods.appearance.hidden, Some(0.5));
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.mini_percent, Some(25.0));
    }

    #[test]
    fn attack_mod_parser_maps_chart_masks_and_turn_options() {
        let mods = parse_attack_mods(
            "wide,big,quick,bmrize,skippy,echo,stomp,mines,little,nomines,noholds,\
             nojumps,nohands,noquads,nolifts,nofakes,planted,floored,twister,norolls,\
             holdstorolls,mirror,left,right,lrmirror,udmirror,shuffle,blender,hypershuffle",
        );

        assert_eq!(
            mods.insert_mask,
            INSERT_MASK_BIT_WIDE
                | INSERT_MASK_BIT_BIG
                | INSERT_MASK_BIT_QUICK
                | INSERT_MASK_BIT_BMRIZE
                | INSERT_MASK_BIT_SKIPPY
                | INSERT_MASK_BIT_ECHO
                | INSERT_MASK_BIT_STOMP
                | INSERT_MASK_BIT_MINES,
        );
        assert_eq!(
            mods.remove_mask,
            REMOVE_MASK_BIT_LITTLE
                | REMOVE_MASK_BIT_NO_MINES
                | REMOVE_MASK_BIT_NO_HOLDS
                | REMOVE_MASK_BIT_NO_JUMPS
                | REMOVE_MASK_BIT_NO_HANDS
                | REMOVE_MASK_BIT_NO_QUADS
                | REMOVE_MASK_BIT_NO_LIFTS
                | REMOVE_MASK_BIT_NO_FAKES,
        );
        assert_eq!(
            mods.holds_mask,
            HOLDS_MASK_BIT_PLANTED
                | HOLDS_MASK_BIT_FLOORED
                | HOLDS_MASK_BIT_TWISTER
                | HOLDS_MASK_BIT_NO_ROLLS
                | HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        );
        assert_eq!(mods.turn_option, GameplayTurnOption::Random);
        assert_eq!(turn_option_bits(GameplayTurnOption::Mirror), 1 << 0);
        assert_eq!(turn_option_bits(GameplayTurnOption::Random), 1 << 7);
    }

    #[test]
    fn attack_mod_parser_clearall_discards_prior_mods_and_no_prefix_zeroes_levels() {
        let mods = parse_attack_mods("drunk,clearall,30% blink,no hidden");

        assert!(mods.clear_all);
        assert_eq!(mods.visual.drunk, None);
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.appearance.hidden, Some(0.0));
    }

    #[test]
    fn attack_mod_parser_accepts_scroll_perspective_and_approach_prefixes() {
        let mods = parse_attack_mods(
            "C600,*1000 sudden,*1000 -125% suddenoffset,*2.4 150% hiddenoffset,\
             150% drunk,200% expand,30% reverse,centered,50% incoming,dark,50% blind,75% cover",
        );

        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(mods.visual.drunk, Some(1.5));
        assert_eq!(mods.accel.expand, Some(2.0));
        assert_eq!(mods.appearance.sudden, Some(1.0));
        assert_eq!(mods.appearance.sudden_offset, Some(-1.25));
        assert_eq!(mods.appearance.hidden_offset, Some(1.5));
        assert_eq!(mods.appearance_speed.sudden, Some(1000.0));
        assert_eq!(mods.appearance_speed.sudden_offset, Some(1000.0));
        assert_eq!(mods.appearance_speed.hidden_offset, Some(2.4));
        assert_eq!(mods.scroll.reverse, Some(0.3));
        assert_eq!(mods.scroll.centered, Some(1.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visibility.dark, Some(1.0));
        assert_eq!(mods.visibility.blind, Some(0.5));
        assert_eq!(mods.visibility.cover, Some(0.75));
    }

    #[test]
    fn song_lua_runtime_mod_parser_accepts_itgmania_forms() {
        let mods = parse_song_lua_runtime_mods(
            "*9999 25 invert,*9999 no hidden,*9999 3x,*9999 -25 tiny,\
             *9999 25 mini,*9999 50 incoming,*9999 15 bumpy3,*9999 250 tiny2,\
             *9999 -125 bumpyperiod,*9999 100 pulseouter",
        );

        assert_eq!(mods.visual.invert, Some(0.25));
        assert_eq!(mods.appearance.hidden, Some(0.0));
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(3.0)));
        assert_eq!(mods.visual.tiny, Some(-0.25));
        assert_eq!(mods.mini_percent, Some(25.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visual.bumpy, None);
        assert_eq!(mods.visual.bumpy_cols[2], Some(0.15));
        assert_eq!(mods.visual.tiny_cols[1], Some(2.5));
        assert_eq!(mods.visual.bumpy_period, Some(-1.25));
        assert_eq!(mods.visual.pulse_outer, Some(1.0));
    }

    #[test]
    fn song_lua_runtime_mod_parser_scales_column_moves() {
        let mods = parse_song_lua_runtime_mods(
            "*10000 -80 movey1,*10000 40 movex2,*10000 -314 confusionoffset3,\
             *10000 -628 confusionoffset,*10000 -80 tiny",
        );

        assert_eq!(mods.visual.move_y_cols[0], Some(-0.8));
        assert_eq!(mods.visual.move_x_cols[1], Some(0.4));
        assert_eq!(mods.visual.confusion_offset_cols[2], Some(-3.14));
        assert_eq!(mods.visual.confusion_offset, Some(-6.28));
        assert_eq!(mods.visual.tiny, Some(-0.8));
        assert_eq!(mods.mini_percent, None);
    }

    #[test]
    fn song_lua_runtime_mod_parser_handles_each_word_count_form() {
        let mods = parse_song_lua_runtime_mods(
            "clearall,drunk,50 reverse ignored,*2 dizzy,*3 25 tipsy ignored,2x",
        );

        assert!(mods.clear_all);
        assert_eq!(mods.visual.drunk, Some(1.0));
        assert_eq!(mods.visual_speed.drunk, Some(1.0));
        assert_eq!(mods.scroll.reverse, Some(0.5));
        assert_eq!(mods.visual.dizzy, Some(1.0));
        assert_eq!(mods.visual_speed.dizzy, Some(2.0));
        assert_eq!(mods.visual.tipsy, Some(0.25));
        assert_eq!(mods.visual_speed.tipsy, Some(3.0));
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(2.0)));
    }

    #[test]
    fn effect_overrides_report_active_scalar_values() {
        assert!(!AccelOverrides::default().any());
        assert!(!AppearanceOverrides::default().any());
        assert!(!VisibilityOverrides::default().any());
        assert!(!ScrollOverrides::default().any());
        assert!(!PerspectiveOverrides::default().any());

        assert!(
            AccelOverrides {
                wave: Some(0.0),
                ..AccelOverrides::default()
            }
            .any()
        );
        assert!(
            AppearanceOverrides {
                stealth: Some(0.0),
                ..AppearanceOverrides::default()
            }
            .any()
        );
        assert!(
            VisibilityOverrides {
                cover: Some(0.0),
                ..VisibilityOverrides::default()
            }
            .any()
        );
        assert!(
            ScrollOverrides {
                centered: Some(0.0),
                ..ScrollOverrides::default()
            }
            .any()
        );
        assert!(
            PerspectiveOverrides {
                skew: Some(0.0),
                ..PerspectiveOverrides::default()
            }
            .any()
        );
    }

    #[test]
    fn visual_overrides_report_active_column_values() {
        assert!(!VisualOverrides::default().any());

        let mut bumpy = VisualOverrides::default();
        bumpy.bumpy_cols[MAX_COLS - 1] = Some(0.0);
        assert!(bumpy.any());

        let mut tiny = VisualOverrides::default();
        tiny.tiny_cols[1] = Some(0.25);
        assert!(tiny.any());

        let mut move_x = VisualOverrides::default();
        move_x.move_x_cols[0] = Some(-4.0);
        assert!(move_x.any());

        let mut move_y = VisualOverrides::default();
        move_y.move_y_cols[2] = Some(8.0);
        assert!(move_y.any());

        let mut confusion = VisualOverrides::default();
        confusion.confusion_offset_cols[3] = Some(90.0);
        assert!(confusion.any());
    }

    #[test]
    fn spacing_multiplier_clamps_and_scales_percent() {
        assert_eq!(SPACING_PERCENT_MIN, -100);
        assert_eq!(SPACING_PERCENT_MAX, 100);
        assert_near(spacing_multiplier_for_percent(0), 1.0);
        assert_near(spacing_multiplier_for_percent(25), 1.25);
        assert_near(spacing_multiplier_for_percent(-50), 0.5);
        assert_near(spacing_multiplier_for_percent(250), 2.0);
        assert_near(spacing_multiplier_for_percent(-250), 0.0);
    }

    #[test]
    fn toggle_flash_alpha_uses_hold_then_fade_countdown() {
        assert_eq!(toggle_flash_alpha(0.0), None);
        assert_eq!(toggle_flash_alpha(-1.0), None);
        assert_near(toggle_flash_alpha(TOGGLE_FLASH_DURATION).unwrap(), 1.0);
        assert_near(
            toggle_flash_alpha(TOGGLE_FLASH_DURATION - TOGGLE_FLASH_FADE_START).unwrap(),
            1.0,
        );
        assert_near(toggle_flash_alpha(0.35).unwrap(), 0.5);
        assert_near(toggle_flash_alpha(0.001).unwrap(), 0.001 / 0.7);
    }

    #[test]
    fn toggle_flash_alpha_preserves_overfull_timer_as_opaque() {
        assert_near(
            toggle_flash_alpha(TOGGLE_FLASH_DURATION + 1.0).unwrap(),
            1.0,
        );
    }

    #[test]
    fn gameplay_toggle_flash_state_reports_visible_text_and_ticks() {
        let mut state = GameplayToggleFlashState {
            text: Some("Auto"),
            timer: TOGGLE_FLASH_DURATION,
        };

        assert_eq!(state.visible_text().map(|(text, _)| text), Some("Auto"));

        state.tick(TOGGLE_FLASH_DURATION);

        assert_eq!(state.timer, 0.0);
        assert_eq!(state.visible_text(), None);
        state.text = None;
        state.timer = TOGGLE_FLASH_DURATION;
        assert_eq!(state.visible_text(), None);
    }

    #[test]
    fn gameplay_stage_runtime_state_tracks_score_and_practice_reset() {
        let mut state = GameplayStageRuntimeState::new(true, true, [true, false], [false, true]);

        assert!(state.autoplay_enabled);
        assert!(state.autoplay_used);
        assert_eq!(state.score_valid, [true, false]);
        assert_eq!(state.score_missed_holds_rolls, [false, true]);

        state.song_completed_naturally = true;
        state.disable_score();
        assert_eq!(state.score_valid, [false; MAX_PLAYERS]);

        state.reset_for_practice();

        assert!(!state.song_completed_naturally);
        assert!(!state.autoplay_used);
        assert!(state.autoplay_enabled);
    }

    #[test]
    fn positive_timer_tick_drains_only_active_timers() {
        let mut active = 0.5;
        tick_positive_timer(&mut active, 0.2);
        assert_near(active, 0.3);
        tick_positive_timer(&mut active, 1.0);
        assert_near(active, 0.0);

        let mut inactive = 0.0;
        tick_positive_timer(&mut inactive, 0.2);
        assert_near(inactive, 0.0);

        let mut negative = -0.5;
        tick_positive_timer(&mut negative, 0.2);
        assert_near(negative, -0.5);
    }

    #[test]
    fn approach_f32_steps_toward_target_without_overshoot() {
        let mut value = 0.0;
        approach_f32(&mut value, 1.0, 0.25);
        assert_near(value, 0.25);

        approach_f32(&mut value, 1.0, 2.0);
        assert_near(value, 1.0);

        approach_f32(&mut value, -1.0, 0.5);
        assert_near(value, 0.5);
    }

    #[test]
    fn approach_f32_handles_bad_inputs_like_runtime_policy() {
        let mut value = 0.5;
        approach_f32(&mut value, 1.0, 0.0);
        assert_near(value, 0.5);

        approach_f32(&mut value, 1.0, -1.0);
        assert_near(value, 0.5);

        value = f32::INFINITY;
        approach_f32(&mut value, 2.0, 0.25);
        assert_near(value, 2.0);

        approach_f32(&mut value, f32::NAN, 0.25);
        assert!(value.is_nan());
    }

    #[test]
    fn audio_commands_preserve_playback_payloads() {
        let cut = GameplayMusicCut {
            start_sec: 1.0,
            length_sec: 2.0,
            fade_in_sec: 0.25,
            fade_out_sec: 0.5,
        };
        let command = GameplayAudioCommand::PlayMusic {
            path: PathBuf::from("songs/test.ogg"),
            cut,
            looping: false,
            rate: 1.25,
        };

        assert_eq!(
            command,
            GameplayAudioCommand::PlayMusic {
                path: PathBuf::from("songs/test.ogg"),
                cut,
                looping: false,
                rate: 1.25,
            }
        );
        assert_eq!(
            GameplayAudioCommand::StopMusic,
            GameplayAudioCommand::StopMusic
        );
        assert_eq!(
            GameplayAudioCommand::SetMusicRate(1.5),
            GameplayAudioCommand::SetMusicRate(1.5)
        );
        assert_eq!(
            GameplayAudioCommand::PlayPreloadedAssistTick("assets/sounds/assist_tick.ogg"),
            GameplayAudioCommand::PlayPreloadedAssistTick("assets/sounds/assist_tick.ogg")
        );
    }

    #[test]
    fn gameplay_command_queue_drains_audio_and_session_commands() {
        let mut queue = GameplayCommandQueue::with_capacity(3, 1);
        queue.push_audio(GameplayAudioCommand::StopMusic);
        queue.push_audio(GameplayAudioCommand::SetMusicRate(1.25));
        queue.push_audio(GameplayAudioCommand::PlayPreloadedAssistTick(
            "assets/sounds/assist_tick.ogg",
        ));
        queue.push_session(GameplaySessionCommand::SetTimingTickMode(
            GameplayTimingTickMode::Assist,
        ));

        assert_eq!(
            queue.drain_audio().collect::<Vec<_>>(),
            vec![
                GameplayAudioCommand::StopMusic,
                GameplayAudioCommand::SetMusicRate(1.25),
                GameplayAudioCommand::PlayPreloadedAssistTick("assets/sounds/assist_tick.ogg"),
            ]
        );
        assert!(queue.drain_audio().next().is_none());
        assert_eq!(
            queue.drain_session().collect::<Vec<_>>(),
            vec![GameplaySessionCommand::SetTimingTickMode(
                GameplayTimingTickMode::Assist
            )]
        );
        assert!(queue.drain_session().next().is_none());
    }

    #[test]
    fn feedback_durations_match_runtime_policy() {
        assert_near(HOLD_JUDGMENT_TOTAL_DURATION, 0.8);
        assert_near(HELD_MISS_TOTAL_DURATION, 0.5);
        assert_near(RECEPTOR_GLOW_DURATION, 0.2);
        assert_near(COMBO_HUNDRED_MILESTONE_DURATION, 0.6);
        assert_near(COMBO_THOUSAND_MILESTONE_DURATION, 0.7);
    }

    #[test]
    fn combo_milestone_trigger_appends_or_resets_existing_kind() {
        let mut milestones = Vec::new();
        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Hundred);

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Hundred);
        assert_near(milestones[0].elapsed, 0.0);

        milestones[0].elapsed = 0.4;
        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Hundred);
        assert_eq!(milestones.len(), 1);
        assert_near(milestones[0].elapsed, 0.0);

        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Thousand);
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[1].kind, ComboMilestoneKind::Thousand);
        assert_near(milestones[1].elapsed, 0.0);
    }

    #[test]
    fn combo_milestone_tick_ages_and_drops_expired_kinds() {
        assert_near(
            combo_milestone_duration(ComboMilestoneKind::Hundred),
            COMBO_HUNDRED_MILESTONE_DURATION,
        );
        assert_near(
            combo_milestone_duration(ComboMilestoneKind::Thousand),
            COMBO_THOUSAND_MILESTONE_DURATION,
        );

        let mut milestones = vec![
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Hundred,
                elapsed: COMBO_HUNDRED_MILESTONE_DURATION - 0.1,
            },
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Thousand,
                elapsed: COMBO_THOUSAND_MILESTONE_DURATION - 0.2,
            },
        ];

        tick_combo_milestones(&mut milestones, 0.15);

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Thousand);
        assert_near(
            milestones[0].elapsed,
            COMBO_THOUSAND_MILESTONE_DURATION - 0.05,
        );

        tick_combo_milestones(&mut milestones, 0.05);
        assert!(milestones.is_empty());
    }

    #[test]
    fn combo_update_feedback_resets_window_counts_on_break() {
        let mut counts = WindowCounts {
            w0: 1,
            w1: 2,
            w2: 3,
            w3: 4,
            w4: 5,
            w5: 6,
            miss: 7,
        };
        let mut milestones = Vec::new();

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                combo_broken: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(counts.w0, 0);
        assert_eq!(counts.w1, 0);
        assert_eq!(counts.w2, 0);
        assert_eq!(counts.w3, 0);
        assert_eq!(counts.w4, 0);
        assert_eq!(counts.w5, 0);
        assert_eq!(counts.miss, 0);
        assert!(milestones.is_empty());
    }

    #[test]
    fn combo_update_feedback_triggers_milestones() {
        let mut counts = WindowCounts::default();
        let mut milestones = Vec::new();

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                hit_hundred_milestone: true,
                hit_thousand_milestone: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Thousand);
        assert_eq!(milestones[1].kind, ComboMilestoneKind::Hundred);
    }

    #[test]
    fn combo_update_feedback_resets_existing_milestone_elapsed() {
        let mut counts = WindowCounts::default();
        let mut milestones = vec![ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.5,
        }];

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                hit_hundred_milestone: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Hundred);
        assert_near(milestones[0].elapsed, 0.0);
    }

    #[test]
    fn mine_hit_combo_policy_preserves_combo_by_default() {
        let mut state = ComboState {
            combo: 50,
            miss_combo: 3,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..ComboState::default()
        };
        let original = state;

        let update = apply_mine_hit_combo_policy(&mut state);

        assert_eq!(state, original);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn mine_hit_player_state_counts_scored_hits() {
        let mut state = MineHitPlayerState {
            mines_hit: 2,
            mines_hit_for_score: 1,
            combo: ComboState {
                combo: 50,
                miss_combo: 3,
                full_combo_grade: Some(JudgeGrade::Great),
                current_combo_grade: Some(JudgeGrade::Great),
                ..ComboState::default()
            },
        };
        let original_combo = state.combo;

        let update = apply_mine_hit_player_state(&mut state, false, false);

        assert_eq!(state.mines_hit, 3);
        assert_eq!(state.mines_hit_for_score, 2);
        assert_eq!(state.combo, original_combo);
        assert_eq!(
            update,
            MineHitPlayerUpdate {
                counted_hit: true,
                counted_for_score: true,
                combo_update: ComboUpdate::default(),
                life_delta: deadsync_rules::life::LIFE_HIT_MINE,
                apply_life_change: true,
                capture_failed_ex_score_inputs: true,
            }
        );
    }

    #[test]
    fn mine_hit_player_state_skips_score_after_life_failure() {
        let mut state = MineHitPlayerState {
            mines_hit: 2,
            mines_hit_for_score: 1,
            combo: ComboState::default(),
        };

        let update = apply_mine_hit_player_state(&mut state, false, true);

        assert_eq!(state.mines_hit, 3);
        assert_eq!(state.mines_hit_for_score, 1);
        assert!(update.counted_hit);
        assert!(!update.counted_for_score);
        assert_near(update.life_delta, deadsync_rules::life::LIFE_HIT_MINE);
        assert!(update.apply_life_change);
        assert!(update.capture_failed_ex_score_inputs);
    }

    #[test]
    fn mine_hit_player_state_noops_when_scoring_blocked() {
        let mut state = MineHitPlayerState {
            mines_hit: 2,
            mines_hit_for_score: 1,
            combo: ComboState {
                combo: 50,
                ..ComboState::default()
            },
        };
        let original = state;

        let update = apply_mine_hit_player_state(&mut state, true, false);

        assert_eq!(state, original);
        assert_eq!(
            update,
            MineHitPlayerUpdate {
                life_delta: deadsync_rules::life::LIFE_HIT_MINE,
                ..MineHitPlayerUpdate::default()
            }
        );
        assert!(!update.apply_life_change);
        assert!(!update.capture_failed_ex_score_inputs);
    }

    #[test]
    fn hold_success_combo_policy_preserves_miss_combo() {
        let mut state = ComboState {
            combo: 12,
            miss_combo: 4,
            current_combo_grade: Some(JudgeGrade::Excellent),
            ..ComboState::default()
        };
        let original = state;

        let update = apply_hold_success_combo_policy(&mut state);

        assert_eq!(state, original);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn hold_let_go_combo_policy_clears_full_combo_without_breaking_combo() {
        assert!(!COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO);

        let mut state = ComboState {
            combo: 42,
            miss_combo: 5,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..ComboState::default()
        };

        let update = apply_hold_let_go_combo_policy(&mut state);

        assert_eq!(state.combo, 42);
        assert_eq!(state.miss_combo, 5);
        assert!(state.full_combo_grade.is_none());
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(state.first_fc_attempt_broken);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn column_flash_duration_uses_short_miss_and_judgment_fade() {
        assert_near(
            column_flash_duration(JudgeGrade::Miss),
            COLUMN_FLASH_MISS_DURATION,
        );
        assert_near(
            column_flash_duration(JudgeGrade::Fantastic),
            COLUMN_FLASH_JUDGMENT_DURATION,
        );
        assert_near(
            column_flash_duration(JudgeGrade::WayOff),
            COLUMN_FLASH_JUDGMENT_DURATION,
        );
    }

    #[test]
    fn column_flash_options_gate_grade_bits() {
        let options = ColumnFlashOptions {
            enabled: true,
            great: true,
            miss: true,
            ..ColumnFlashOptions::default()
        };

        assert!(column_flash_enabled_for_options(
            options,
            JudgeGrade::Great,
            false
        ));
        assert!(column_flash_enabled_for_options(
            options,
            JudgeGrade::Miss,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            options,
            JudgeGrade::Excellent,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            ColumnFlashOptions {
                enabled: false,
                great: true,
                ..ColumnFlashOptions::default()
            },
            JudgeGrade::Great,
            false
        ));
    }

    #[test]
    fn column_flash_options_split_fantastic_colors() {
        let blue_only = ColumnFlashOptions {
            enabled: true,
            blue_fantastic: true,
            ..ColumnFlashOptions::default()
        };
        let white_only = ColumnFlashOptions {
            enabled: true,
            white_fantastic: true,
            ..ColumnFlashOptions::default()
        };

        assert!(column_flash_enabled_for_options(
            blue_only,
            JudgeGrade::Fantastic,
            true
        ));
        assert!(!column_flash_enabled_for_options(
            blue_only,
            JudgeGrade::Fantastic,
            false
        ));
        assert!(column_flash_enabled_for_options(
            white_only,
            JudgeGrade::Fantastic,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            white_only,
            JudgeGrade::Fantastic,
            true
        ));
    }

    #[test]
    fn feedback_explosion_slots_tick_elapsed_and_expire() {
        let mut tap = Some(ActiveTapExplosion {
            window: "W1",
            bright: false,
            elapsed: 0.2,
            duration: 0.5,
            start_beat: 8.0,
        });
        tick_tap_explosion_slot(&mut tap, 0.2);
        assert_near(tap.expect("tap explosion should remain").elapsed, 0.4);
        tick_tap_explosion_slot(&mut tap, 0.1);
        assert!(tap.is_none());

        let mut instant_tap = Some(ActiveTapExplosion {
            window: "Miss",
            bright: false,
            elapsed: 0.0,
            duration: 0.0,
            start_beat: 0.0,
        });
        tick_tap_explosion_slot(&mut instant_tap, 0.0);
        assert!(instant_tap.is_none());

        let mut mine = Some(ActiveMineExplosion {
            elapsed: 0.1,
            duration: 0.3,
            started_at_screen_s: 2.0,
        });
        tick_mine_explosion_slot(&mut mine, 0.1);
        assert_near(
            mine.as_ref().expect("mine explosion should remain").elapsed,
            0.2,
        );
        tick_mine_explosion_slot(&mut mine, 0.1);
        assert!(mine.is_none());
    }

    #[test]
    fn feedback_slots_expire_at_runtime_durations() {
        let flash = ActiveColumnFlash {
            grade: JudgeGrade::Great,
            blue_fantastic: false,
            started_at_screen_s: 10.0,
        };
        assert!(!column_flash_expired_at(
            flash,
            10.0 + COLUMN_FLASH_JUDGMENT_DURATION - 0.001
        ));
        assert!(column_flash_expired_at(
            flash,
            10.0 + COLUMN_FLASH_JUDGMENT_DURATION + 0.001
        ));

        let miss_flash = ActiveColumnFlash {
            grade: JudgeGrade::Miss,
            ..flash
        };
        assert!(column_flash_expired_at(
            miss_flash,
            10.0 + COLUMN_FLASH_MISS_DURATION + 0.001
        ));

        let hold = HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 3.0,
        };
        assert!(!hold_judgment_expired_at(
            hold,
            3.0 + HOLD_JUDGMENT_TOTAL_DURATION - 0.001
        ));
        assert!(hold_judgment_expired_at(
            hold,
            3.0 + HOLD_JUDGMENT_TOTAL_DURATION + 0.001
        ));

        let held_miss = HeldMissRenderInfo {
            started_at_screen_s: 4.0,
        };
        assert!(!held_miss_judgment_expired_at(
            held_miss,
            4.0 + HELD_MISS_TOTAL_DURATION - 0.001
        ));
        assert!(held_miss_judgment_expired_at(
            held_miss,
            4.0 + HELD_MISS_TOTAL_DURATION + 0.001
        ));
    }

    #[test]
    fn render_info_constructors_copy_feedback_fields() {
        let judgment = test_judgment(JudgeGrade::Great);
        let judgment_info = judgment_render_info(judgment, 1.25);
        assert_eq!(judgment_info.judgment.grade, JudgeGrade::Great);
        assert_eq!(judgment_info.started_at_screen_s, 1.25);

        assert_eq!(
            mine_judgment_render_info(MineResult::Hit, 3, 2.5),
            MineJudgmentRenderInfo {
                result: MineResult::Hit,
                column: 3,
                started_at_screen_s: 2.5,
            }
        );

        let hold = hold_judgment_render_info(HoldResult::Held, 3.75);
        assert_eq!(hold.result, HoldResult::Held);
        assert_eq!(hold.started_at_screen_s, 3.75);

        assert_eq!(held_miss_render_info(4.5).started_at_screen_s, 4.5);
    }

    #[test]
    fn final_note_result_effects_mark_first_finalization() {
        let judgment = test_judgment(JudgeGrade::Great);

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: None,
                held_miss_column: None,
            }
        );
        assert_eq!(
            final_note_result_effects(false, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects::default()
        );
    }

    #[test]
    fn final_note_result_effects_trigger_miss_feedback() {
        let judgment = test_judgment(JudgeGrade::Miss);

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: None,
            }
        );
    }

    #[test]
    fn final_note_result_effects_record_held_miss_column_when_in_bounds() {
        let mut judgment = test_judgment(JudgeGrade::Miss);
        judgment.miss_because_held = true;

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: Some(2),
            }
        );
        assert_eq!(
            final_note_result_effects(true, &judgment, MAX_COLS, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(MAX_COLS),
                held_miss_column: None,
            }
        );
    }

    #[test]
    fn register_provisional_early_note_result_only_sets_first_result() {
        let first = test_judgment(JudgeGrade::Great);
        let second = test_judgment(JudgeGrade::Miss);
        let mut note = test_note(NoteType::Tap, None, false);

        assert!(register_provisional_early_note_result(&mut note, first));
        assert_eq!(
            note.early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
        assert!(!register_provisional_early_note_result(&mut note, second));
        assert_eq!(
            note.early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
    }

    #[test]
    fn provisional_early_note_result_marks_row_entry_once() {
        let first = test_judgment(JudgeGrade::Decent);
        let second = test_judgment(JudgeGrade::Miss);
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        let note_row_entry_indices = [0];

        assert_eq!(
            apply_provisional_early_note_result(
                &mut notes,
                &mut row_entries,
                &note_row_entry_indices,
                0,
                first,
            ),
            ProvisionalEarlyNoteResultUpdate {
                registered: true,
                marked_row_entry: true,
            }
        );
        assert_eq!(
            notes[0].early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
        assert!(row_entries[0].had_provisional_early_hit);

        row_entries[0].had_provisional_early_hit = false;
        assert_eq!(
            apply_provisional_early_note_result(
                &mut notes,
                &mut row_entries,
                &note_row_entry_indices,
                0,
                second,
            ),
            ProvisionalEarlyNoteResultUpdate::default()
        );
        assert_eq!(
            notes[0].early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
        assert!(!row_entries[0].had_provisional_early_hit);
    }

    #[test]
    fn provisional_early_note_result_ignores_invalid_note_index() {
        let mut notes = Vec::new();
        let mut row_entries = Vec::new();

        assert_eq!(
            apply_provisional_early_note_result(
                &mut notes,
                &mut row_entries,
                &[],
                0,
                test_judgment(JudgeGrade::Great),
            ),
            ProvisionalEarlyNoteResultUpdate::default()
        );
    }

    #[test]
    fn time_based_tap_miss_scan_stops_at_cutoff_row() {
        let note = test_note_at(NoteType::Tap, None, false, 96, 2.0);

        assert_eq!(
            time_based_tap_miss_scan(&note, 96),
            TimeBasedTapMissScan::Stop
        );
    }

    #[test]
    fn time_based_tap_miss_scan_skips_noneligible_notes() {
        let mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        let mut unjudgable = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        unjudgable.can_be_judged = false;
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));

        assert_eq!(
            time_based_tap_miss_scan(&mine, 96),
            TimeBasedTapMissScan::Skip
        );
        assert_eq!(
            time_based_tap_miss_scan(&unjudgable, 96),
            TimeBasedTapMissScan::Skip
        );
        assert_eq!(
            time_based_tap_miss_scan(&judged, 96),
            TimeBasedTapMissScan::Skip
        );
    }

    #[test]
    fn time_based_tap_miss_scan_accepts_unjudged_taps_before_cutoff() {
        let note = test_note_at(NoteType::Tap, None, false, 48, 1.0);

        assert_eq!(
            time_based_tap_miss_scan(&note, 96),
            TimeBasedTapMissScan::Miss
        );
    }

    #[test]
    fn time_based_tap_miss_judgment_builds_canonical_miss() {
        let note_time_ns = 1_000;
        let music_time_ns = note_time_ns + song_time_ns_from_seconds(0.2);
        let judgment = time_based_tap_miss_judgment(None, note_time_ns, music_time_ns, 2.0, true);

        assert_eq!(judgment.grade, JudgeGrade::Miss);
        assert_eq!(judgment.time_error_music_ns, song_time_ns_from_seconds(0.2));
        assert!((judgment.time_error_ms - 100.0).abs() < 0.001);
        assert_eq!(judgment.window, None);
        assert!(judgment.miss_because_held);
    }

    #[test]
    fn time_based_tap_miss_judgment_preserves_provisional_result() {
        let mut early = test_judgment(JudgeGrade::Decent);
        early.time_error_music_ns = -12_345;
        early.time_error_ms = -12.345;

        let judgment = time_based_tap_miss_judgment(
            Some(early),
            1_000,
            1_000 + song_time_ns_from_seconds(1.0),
            1.0,
            true,
        );

        assert_eq!(judgment.grade, early.grade);
        assert_eq!(judgment.time_error_music_ns, early.time_error_music_ns);
        assert_eq!(judgment.time_error_ms, early.time_error_ms);
        assert_eq!(judgment.window, early.window);
        assert_eq!(judgment.miss_because_held, early.miss_because_held);
    }

    #[test]
    fn next_time_based_tap_miss_skips_to_first_miss_event() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));
        let mut miss = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        miss.column = 2;
        let mut stop = test_note_at(NoteType::Tap, None, false, 144, 3.0);
        stop.column = 3;
        let mut notes = vec![judged, miss, stop];
        let note_times = [1_000, 2_000, 3_000];
        let held_window = [false, true, false];
        let mut hold_decay = [false; 3];
        let mut decaying = Vec::new();

        let step = apply_next_time_based_tap_miss_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            0,
            (0, 3),
            144,
            4_000,
            1.0,
            false,
        );

        let event = step.event.expect("second note should miss");
        assert_eq!(step.next_cursor, 2);
        assert_eq!(event.note_index, 1);
        assert_eq!(event.row_index, 96);
        assert_eq!(event.column, 2);
        assert_eq!(event.beat, 2.0);
        assert_eq!(event.note_time_ns, 2_000);
        assert_eq!(event.judgment.grade, JudgeGrade::Miss);
        assert!(event.miss_because_held);
        assert!(!event.queue_missed_hold_resolution);
        assert!(notes[1].result.is_none());
    }

    #[test]
    fn next_time_based_tap_miss_stops_at_cutoff_without_event() {
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 96, 2.0)];
        let note_times = [2_000];
        let held_window = [false];
        let mut hold_decay = [false; 1];
        let mut decaying = Vec::new();

        let step = apply_next_time_based_tap_miss_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            0,
            (0, 1),
            96,
            4_000,
            1.0,
            false,
        );

        assert_eq!(step.next_cursor, 0);
        assert!(step.event.is_none());
        assert!(notes[0].result.is_none());
    }

    #[test]
    fn next_time_based_tap_miss_updates_hold_miss_policy() {
        let mut notes = vec![test_note_at(
            NoteType::Hold,
            Some(test_hold()),
            false,
            48,
            1.0,
        )];
        let note_times = [1_000];
        let held_window = [false];
        let mut hold_decay = [false; 1];
        let mut decaying = Vec::new();

        let step = apply_next_time_based_tap_miss_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            0,
            (0, 1),
            96,
            4_000,
            1.0,
            false,
        );

        let event = step.event.expect("hold head should miss");
        assert!(event.queue_missed_hold_resolution);
        assert_eq!(
            notes[0].hold.as_ref().and_then(|hold| hold.result),
            Some(HoldResult::Missed)
        );
        assert!(hold_decay[0]);
        assert_eq!(decaying, vec![0]);
        assert!(notes[0].result.is_none());
    }

    #[test]
    fn collect_time_based_tap_misses_fills_bounded_event_buffer() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 2;
        let note_times = [1_000, 2_000, 3_000];
        let held_window = [false, true, false];
        let mut hold_decay = [false; 3];
        let mut decaying = Vec::new();
        let mut events = [None; 2];

        let first = collect_time_based_tap_misses_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            0,
            (0, 3),
            192,
            4_000,
            1.0,
            false,
            &mut events,
        );

        assert_eq!(
            first,
            TimeBasedTapMissPlayerUpdate {
                next_cursor: 2,
                event_count: 2,
                stopped: false,
            }
        );
        assert_eq!(events[0].map(|event| event.note_index), Some(0));
        assert_eq!(events[1].map(|event| event.note_index), Some(1));
        assert!(events[1].expect("second event").miss_because_held);
        assert!(notes.iter().all(|note| note.result.is_none()));

        let second = collect_time_based_tap_misses_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            first.next_cursor,
            (0, 3),
            192,
            4_000,
            1.0,
            false,
            &mut events,
        );

        assert_eq!(
            second,
            TimeBasedTapMissPlayerUpdate {
                next_cursor: 3,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(events[0].map(|event| event.note_index), Some(2));
    }

    #[test]
    fn collect_time_based_tap_misses_stops_before_cutoff_row() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let note_times = [1_000, 2_000];
        let held_window = [false, false];
        let mut hold_decay = [false; 2];
        let mut decaying = Vec::new();
        let mut events = [None; 4];

        let update = collect_time_based_tap_misses_for_player(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            0,
            (0, 2),
            96,
            4_000,
            1.0,
            false,
            &mut events,
        );

        assert_eq!(
            update,
            TimeBasedTapMissPlayerUpdate {
                next_cursor: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(events[0].map(|event| event.note_index), Some(0));
    }

    #[test]
    fn collect_time_based_tap_misses_scans_active_players() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
            test_note_at(NoteType::Tap, None, false, 192, 4.0),
        ];
        notes[2].column = 4;
        notes[3].column = 5;
        let note_times = [1_000, 2_000, 3_000, 4_000];
        let held_window = [false, true, false, false];
        let mut hold_decay = [false; 4];
        let mut decaying = Vec::new();
        let mut next_cursors = [0usize; MAX_PLAYERS];
        let mut events = [None; 4];

        let update = collect_time_based_tap_misses_for_players(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            &mut next_cursors,
            &[(0, 2), (2, 4)],
            &[144, 999],
            5_000,
            1.0,
            &[false, false],
            2,
            &mut events,
        );

        assert_eq!(
            update,
            TimeBasedTapMissPlayersUpdate {
                players_scanned: 2,
                event_count: 4,
                stopped: false,
            }
        );
        assert_eq!(&next_cursors[..2], &[2, 4]);
        assert_eq!(events[0].map(|event| event.player), Some(0));
        assert_eq!(events[0].map(|event| event.event.note_index), Some(0));
        assert_eq!(events[1].map(|event| event.player), Some(0));
        assert_eq!(events[1].map(|event| event.event.note_index), Some(1));
        assert!(
            events[1]
                .expect("held-window event")
                .event
                .miss_because_held
        );
        assert_eq!(events[2].map(|event| event.player), Some(1));
        assert_eq!(events[2].map(|event| event.event.note_index), Some(2));
        assert_eq!(events[3].map(|event| event.player), Some(1));
        assert_eq!(events[3].map(|event| event.event.note_index), Some(3));
        assert!(notes.iter().all(|note| note.result.is_none()));
    }

    #[test]
    fn collect_time_based_tap_misses_for_players_stops_when_buffer_fills() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let note_times = [1_000, 2_000];
        let held_window = [false, false];
        let mut hold_decay = [false; 2];
        let mut decaying = Vec::new();
        let mut next_cursors = [0usize; MAX_PLAYERS];
        let mut events = [None; 1];

        let first = collect_time_based_tap_misses_for_players(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            &mut next_cursors,
            &[(0, 2)],
            &[999],
            3_000,
            1.0,
            &[false],
            1,
            &mut events,
        );
        assert_eq!(
            first,
            TimeBasedTapMissPlayersUpdate {
                players_scanned: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(next_cursors[0], 1);
        assert_eq!(events[0].map(|event| event.event.note_index), Some(0));

        let second = collect_time_based_tap_misses_for_players(
            &mut notes,
            &note_times,
            &held_window,
            &mut hold_decay,
            &mut decaying,
            &mut next_cursors,
            &[(0, 2)],
            &[999],
            3_000,
            1.0,
            &[false],
            1,
            &mut events,
        );
        assert_eq!(
            second,
            TimeBasedTapMissPlayersUpdate {
                players_scanned: 1,
                event_count: 1,
                stopped: false,
            }
        );
        assert_eq!(next_cursors[0], 2);
        assert_eq!(events[0].map(|event| event.event.note_index), Some(1));
    }

    #[test]
    fn apply_final_note_result_sets_result_and_returns_first_effects() {
        let first = test_judgment(JudgeGrade::Miss);
        let second = test_judgment(JudgeGrade::Great);
        let mut note = test_note(NoteType::Tap, None, false);
        note.column = 2;

        let first_effects = apply_final_note_result(&mut note, first, MAX_COLS);
        assert_eq!(note.result.as_ref().map(|j| j.grade), Some(first.grade));
        assert_eq!(
            first_effects,
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: None,
            }
        );

        let second_effects = apply_final_note_result(&mut note, second, MAX_COLS);
        assert_eq!(note.result.as_ref().map(|j| j.grade), Some(second.grade));
        assert_eq!(second_effects, FinalNoteResultEffects::default());
    }

    #[test]
    fn final_note_result_to_rows_marks_row_entry_and_returns_effects() {
        let mut judgment = test_judgment(JudgeGrade::Miss);
        judgment.miss_because_held = true;
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        notes[0].column = 2;
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        let note_row_entry_indices = [0];

        let update = apply_final_note_result_to_rows(
            &mut notes,
            &mut row_entries,
            &note_row_entry_indices,
            0,
            judgment,
            MAX_COLS,
        );

        assert_eq!(
            update,
            FinalNoteResultUpdate {
                effects: FinalNoteResultEffects {
                    mark_row_finalized: true,
                    trigger_miss_flash_column: Some(2),
                    held_miss_column: Some(2),
                },
                marked_row_entry: true,
            }
        );
        assert_eq!(
            notes[0].result.as_ref().map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );
        assert_eq!(row_entries[0].unresolved_count, 0);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
    }

    #[test]
    fn final_note_result_to_rows_ignores_invalid_note_index() {
        let mut notes = Vec::new();
        let mut row_entries = Vec::new();

        assert_eq!(
            apply_final_note_result_to_rows(
                &mut notes,
                &mut row_entries,
                &[],
                0,
                test_judgment(JudgeGrade::Great),
                MAX_COLS,
            ),
            FinalNoteResultUpdate::default()
        );
    }

    #[test]
    fn danger_health_state_uses_life_threshold_and_fail_state() {
        assert_eq!(danger_health_state(1.0, false), HealthState::Alive);
        assert_eq!(danger_health_state(0.2, false), HealthState::Alive);
        assert_eq!(danger_health_state(0.199, false), HealthState::Danger);
        assert_eq!(danger_health_state(0.0, false), HealthState::Dead);
        assert_eq!(danger_health_state(1.0, true), HealthState::Dead);
    }

    #[test]
    fn danger_fx_enters_danger_and_flashes_recovery() {
        let mut fx = DangerFx::default();
        update_danger_fx_for_health(&mut fx, HealthState::Danger, 10.0, false);

        assert_eq!(danger_fx_rgba(&fx, 10.0), [0.0, 0.0, 0.0, 0.0]);
        assert!(danger_fx_rgba(&fx, 10.3)[3] > 0.0);

        update_danger_fx_for_health(&mut fx, HealthState::Alive, 11.0, false);
        let flash = danger_fx_rgba(&fx, 11.15);
        assert_eq!(flash[0], 0.0);
        assert_eq!(flash[1], 1.0);
        assert_eq!(flash[2], 0.0);
        assert!(flash[3] > 0.0);
    }

    #[test]
    fn danger_fx_hide_danger_only_flashes_death() {
        let mut fx = DangerFx::default();
        update_danger_fx_for_health(&mut fx, HealthState::Danger, 1.0, true);
        assert_eq!(danger_fx_rgba(&fx, 1.2), [0.0, 0.0, 0.0, 0.0]);

        update_danger_fx_for_health(&mut fx, HealthState::Dead, 2.0, true);
        let flash = danger_fx_rgba(&fx, 2.15);
        assert_eq!(flash[0], 1.0);
        assert_eq!(flash[1], 0.0);
        assert_eq!(flash[2], 0.0);
        assert!(flash[3] > 0.0);
    }

    #[test]
    fn gameplay_danger_fx_state_updates_and_resets_players() {
        let mut state = GameplayDangerFxState::default();

        state.update_player(1, HealthState::Danger, 10.0, false);
        assert_eq!(state.rgba(0, 10.3), [0.0, 0.0, 0.0, 0.0]);
        assert!(state.rgba(1, 10.3)[3] > 0.0);

        state.reset_player(1);
        assert_eq!(state.rgba(1, 10.3), [0.0, 0.0, 0.0, 0.0]);
        state.update_player(MAX_PLAYERS, HealthState::Dead, 11.0, false);
        assert_eq!(state.rgba(MAX_PLAYERS, 11.3), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn error_bar_window_indices_follow_timing_window_order() {
        assert_eq!(error_bar_window_ix(TimingWindow::W0), 0);
        assert_eq!(error_bar_window_ix(TimingWindow::W1), 1);
        assert_eq!(error_bar_window_ix(TimingWindow::W5), 5);
    }

    #[test]
    fn error_bar_trim_max_window_indices_match_profile_labels() {
        assert_eq!(
            gameplay_error_bar_trim_max_window_ix(GameplayErrorBarTrim::Off),
            4
        );
        assert_eq!(
            gameplay_error_bar_trim_max_window_ix(GameplayErrorBarTrim::Fantastic),
            0
        );
        assert_eq!(
            gameplay_error_bar_trim_max_window_ix(GameplayErrorBarTrim::Excellent),
            1
        );
        assert_eq!(
            gameplay_error_bar_trim_max_window_ix(GameplayErrorBarTrim::Great),
            2
        );
    }

    #[test]
    fn error_bar_push_tick_overwrites_single_or_rotates_multi() {
        let mut single = [None; 2];
        let mut single_next = 1;
        error_bar_push_tick(
            &mut single,
            &mut single_next,
            false,
            ErrorBarTick {
                started_at: 1.0,
                offset_s: 0.010,
                window: TimingWindow::W1,
            },
        );
        assert_eq!(single_next, 0);
        assert_eq!(single[0].map(|tick| tick.offset_s), Some(0.010));
        assert!(single[1].is_none());

        let mut multi = [None; 2];
        let mut multi_next = 0;
        for offset_s in [0.010, 0.020, 0.030] {
            error_bar_push_tick(
                &mut multi,
                &mut multi_next,
                true,
                ErrorBarTick {
                    started_at: 1.0,
                    offset_s,
                    window: TimingWindow::W1,
                },
            );
        }
        assert_eq!(multi_next, 1);
        assert_eq!(multi[0].map(|tick| tick.offset_s), Some(0.030));
        assert_eq!(multi[1].map(|tick| tick.offset_s), Some(0.020));
    }

    #[test]
    fn average_error_bar_interval_controls_sample_window() {
        let mut broad = VecDeque::from([(0.0, 0.010), (100.0, 0.020), (200.0, 0.030)]);
        let (broad_avg, broad_count) = error_bar_average_offset_s(&mut broad, 0.5, 0.050, 400);
        assert!((broad_avg - 0.040).abs() <= 1e-6);
        assert_eq!(broad_count, 2);

        let mut narrow = VecDeque::from([(0.0, 0.010), (100.0, 0.020), (200.0, 0.030)]);
        let (narrow_avg, narrow_count) = error_bar_average_offset_s(&mut narrow, 0.5, 0.050, 200);
        assert!((narrow_avg - 0.050).abs() <= 1e-6);
        assert_eq!(narrow_count, 1);
    }

    #[test]
    fn long_average_uses_short_interval_times_sixteen() {
        let mut samples = VecDeque::from([(0.0, 0.010), (3000.0, 0.020), (3300.0, 0.030)]);
        let mut total = 0.060;

        let (mean, len) = error_bar_long_term_offset_s(&mut samples, &mut total, 6.5, 0.040, 400);

        assert_eq!(len, 3);
        assert_eq!(samples.front().map(|(t, _)| *t), Some(3000.0));
        assert!((mean - 0.030).abs() <= 1e-6);
    }

    #[test]
    fn long_average_tracks_short_interval_changes() {
        let mut samples = VecDeque::from([(0.0, 0.010), (3000.0, 0.020), (3300.0, 0.030)]);
        let mut total = 0.060;

        let (mean, len) = error_bar_long_term_offset_s(&mut samples, &mut total, 6.5, 0.040, 200);

        assert_eq!(len, 2);
        assert_eq!(samples.front().map(|(t, _)| *t), Some(3300.0));
        assert!((mean - 0.035).abs() <= 1e-6);
    }

    #[test]
    fn input_queue_capacity_scales_by_field_count() {
        assert_eq!(input_queue_cap(0), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(4), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(5), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
        assert_eq!(input_queue_cap(8), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
    }

    #[test]
    fn gameplay_input_session_helpers_match_runtime_layout() {
        assert_eq!(GameplayInputPlayStyle::Single.cols_per_player(), 4);
        assert_eq!(GameplayInputPlayStyle::Versus.cols_per_player(), 4);
        assert_eq!(GameplayInputPlayStyle::Double.cols_per_player(), 8);
        assert_eq!(GameplayInputPlayStyle::Single.player_count(), 1);
        assert_eq!(GameplayInputPlayStyle::Versus.player_count(), 2);
        assert_eq!(GameplayInputPlayStyle::Double.player_count(), 1);
        assert_eq!(GameplayInputPlayStyle::Double.total_cols(), 8);

        assert_eq!(gameplay_player_side_index(GameplayInputPlayerSide::P2), 1);
        assert_eq!(
            gameplay_player_side_for_index(2),
            GameplayInputPlayerSide::P1
        );
        assert!(gameplay_runtime_player_is_p2(
            GameplayInputPlayStyle::Single,
            GameplayInputPlayerSide::P2
        ));
        assert!(!gameplay_runtime_player_is_p2(
            GameplayInputPlayStyle::Versus,
            GameplayInputPlayerSide::P2
        ));
        assert_eq!(
            gameplay_runtime_player_side(
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P2,
                0
            ),
            GameplayInputPlayerSide::P1
        );
        assert_eq!(
            gameplay_runtime_player_side(
                GameplayInputPlayStyle::Double,
                GameplayInputPlayerSide::P2,
                1
            ),
            GameplayInputPlayerSide::P2
        );

        let session = GameplaySession {
            play_style: GameplayInputPlayStyle::Versus,
            player_side: GameplayInputPlayerSide::P2,
            joined_sides: [true, false],
            active_profile_ids: [Some("p1".to_string()), Some("p2".to_string())],
            tick_mode: GameplayTimingTickMode::Hit,
        };
        assert!(session.side_joined(GameplayInputPlayerSide::P1));
        assert!(!session.side_joined(GameplayInputPlayerSide::P2));
        assert_eq!(
            session.active_profile_id_for_side(GameplayInputPlayerSide::P2),
            Some("p2".to_string())
        );
        assert_eq!(session.runtime_player_side(0), GameplayInputPlayerSide::P1);
        assert_eq!(session.runtime_player_side(1), GameplayInputPlayerSide::P2);
        assert!(!session.p2_runtime_player());
    }

    #[test]
    fn remap_live_input_lane_filters_other_side_for_single_p1() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P1,
                Lane::Left,
            ),
            Some(Lane::Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P1,
                Lane::P2Left,
            ),
            None
        );
    }

    #[test]
    fn remap_live_input_lane_remaps_single_p2_into_first_field() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::Left,
            ),
            None
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::P2Left,
            ),
            Some(Lane::Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::P2Right,
            ),
            Some(Lane::Right)
        );
    }

    #[test]
    fn remap_live_input_lane_keeps_double_and_versus_lanes() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Double,
                GameplayInputPlayerSide::P2,
                Lane::P2Left,
            ),
            Some(Lane::P2Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P1,
                Lane::P2Down,
            ),
            Some(Lane::P2Down)
        );
    }

    #[test]
    fn live_input_lane_for_queue_filters_autoplay_and_field_bounds() {
        assert_eq!(
            live_input_lane_for_queue(
                true,
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P1,
                Lane::Left,
                4,
            ),
            None
        );
        assert_eq!(
            live_input_lane_for_queue(
                false,
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P1,
                Lane::P2Left,
                4,
            ),
            None
        );
        assert_eq!(
            live_input_lane_for_queue(
                false,
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P1,
                Lane::P2Left,
                8,
            ),
            Some(Lane::P2Left)
        );
    }

    #[test]
    fn live_input_lane_for_queue_remaps_p2_single_lanes() {
        assert_eq!(
            live_input_lane_for_queue(
                false,
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::P2Right,
                4,
            ),
            Some(Lane::Right)
        );
        assert_eq!(
            live_input_lane_for_queue(
                false,
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::Right,
                4,
            ),
            None
        );
    }

    #[test]
    fn active_input_slot_helpers_normalize_and_match_lanes() {
        assert_eq!(MAX_ACTIVE_INPUT_SLOTS, 128);
        assert_eq!(input_lane_bit(0), 0b0000_0001);
        assert_eq!(input_lane_bit(3), 0b0000_1000);
        assert_eq!(normalized_input_slot(u32::MAX, 4, u32::MAX), 4);
        assert_eq!(normalized_input_slot(12, 4, u32::MAX), 12);

        let slots = [
            ActiveInputSlot {
                source: InputSource::Keyboard,
                input_slot: 7,
                lane_mask: input_lane_bit(2) | input_lane_bit(4),
            },
            ActiveInputSlot {
                source: InputSource::Gamepad,
                input_slot: 7,
                lane_mask: input_lane_bit(1),
            },
            EMPTY_ACTIVE_INPUT_SLOT,
        ];

        assert!(active_input_slot_lane_is_down(
            &slots,
            2,
            2,
            InputSource::Keyboard,
            7,
        ));
        assert!(!active_input_slot_lane_is_down(
            &slots,
            2,
            1,
            InputSource::Keyboard,
            7,
        ));
        assert!(active_input_slot_lane_is_down(
            &slots,
            2,
            1,
            InputSource::Gamepad,
            7,
        ));
        assert!(!active_input_slot_lane_is_down(
            &slots,
            1,
            1,
            InputSource::Gamepad,
            7,
        ));
    }

    #[test]
    fn unmapped_input_clock_warning_throttles_by_song_time() {
        assert!(should_warn_unmapped_input_clock(
            UNMAPPED_INPUT_CLOCK_WARN_NEVER_NS,
            0,
        ));
        assert!(!should_warn_unmapped_input_clock(0, 999_999_999));
        assert!(should_warn_unmapped_input_clock(
            0,
            UNMAPPED_INPUT_CLOCK_WARN_INTERVAL_NS,
        ));
        assert!(should_warn_unmapped_input_clock(
            5_000_000_000,
            4_000_000_000
        ));
    }

    #[test]
    fn gameplay_update_phase_policy_tracks_hot_and_total_time() {
        let phases = GameplayUpdatePhaseTimings {
            pre_notes_us: 10,
            input_edges_us: 30,
            density_sample_us: 99,
            danger_us: 40,
            ..GameplayUpdatePhaseTimings::default()
        };

        assert_eq!(gameplay_update_hot_phase(&phases), ("danger", 40));
        assert_eq!(gameplay_update_tracked_phase_total_us(&phases), 80);
    }

    #[test]
    fn gameplay_update_phase_policy_saturates_and_accumulates_maxima() {
        let phases = GameplayUpdatePhaseTimings {
            pre_notes_us: u32::MAX,
            autoplay_us: 1,
            ..GameplayUpdatePhaseTimings::default()
        };
        assert_eq!(gameplay_update_tracked_phase_total_us(&phases), u32::MAX);

        let mut max = GameplayUpdatePhaseTimings {
            input_edges_us: 7,
            input_queue_us: 9,
            ..GameplayUpdatePhaseTimings::default()
        };
        let sample = GameplayUpdatePhaseTimings {
            input_edges_us: 4,
            input_queue_us: 12,
            mine_avoid_us: 5,
            ..GameplayUpdatePhaseTimings::default()
        };
        accumulate_gameplay_update_phase_max(&mut max, &sample);

        assert_eq!(max.input_edges_us, 7);
        assert_eq!(max.input_queue_us, 12);
        assert_eq!(max.mine_avoid_us, 5);
    }

    #[test]
    fn gameplay_trace_frame_policy_flags_slow_total_or_hot_phase() {
        assert!(!gameplay_trace_frame_is_slow(
            GAMEPLAY_TRACE_SLOW_FRAME_US - 1,
            GAMEPLAY_TRACE_PHASE_SPIKE_US - 1,
        ));
        assert!(gameplay_trace_frame_is_slow(
            GAMEPLAY_TRACE_SLOW_FRAME_US,
            0,
        ));
        assert!(gameplay_trace_frame_is_slow(
            0,
            GAMEPLAY_TRACE_PHASE_SPIKE_US,
        ));
    }

    #[test]
    fn gameplay_update_trace_summary_records_and_resets_interval() {
        let mut summary = GameplayUpdateTraceSummary::default();
        let phases = GameplayUpdatePhaseTimings {
            input_edges_us: GAMEPLAY_TRACE_PHASE_SPIKE_US,
            input_queue_us: 7,
            ..GameplayUpdatePhaseTimings::default()
        };

        let frame = summary.record_frame(0.25, 1_500, phases, 3);

        assert_eq!(frame.frame_counter, 1);
        assert!(frame.slow);
        assert_eq!(frame.hot_phase_name, "input_edges");
        assert_eq!(frame.hot_phase_us, GAMEPLAY_TRACE_PHASE_SPIKE_US);
        assert_eq!(frame.phases.untracked_us, 500);
        assert_eq!(summary.elapsed_s, 0.25);
        assert_eq!(summary.frames, 1);
        assert_eq!(summary.slow_frames, 1);
        assert_eq!(summary.max_total_us, 1_500);
        assert_eq!(summary.max_phase.input_queue_us, 7);
        assert_eq!(summary.peak_pending_edges, 3);
        assert!(!summary.should_log_summary());

        summary.record_frame(GAMEPLAY_TRACE_SUMMARY_INTERVAL_S, 500, phases, 9);
        assert!(summary.should_log_summary());
        assert_eq!(summary.frame_counter, 2);
        assert_eq!(summary.peak_pending_edges, 9);

        summary.reset_interval();

        assert_eq!(summary.frame_counter, 2);
        assert_eq!(summary.frames, 0);
        assert_eq!(summary.slow_frames, 0);
        assert_eq!(summary.max_total_us, 0);
        assert_eq!(summary.peak_pending_edges, 0);
    }

    #[test]
    fn gameplay_update_trace_state_collects_capacity_growth() {
        let initial = GameplayCapacityTraceSnapshot {
            pending_edges_capacity: 4,
            replay_edges_capacity: 8,
            decaying_hold_capacity: 2,
            density_life_capacity: [3, 5],
            num_players: 2,
            ..GameplayCapacityTraceSnapshot::default()
        };
        let mut trace = GameplayUpdateTraceState::from_capacity_snapshot(&initial);

        let grown = GameplayCapacityTraceSnapshot {
            pending_edges_capacity: 6,
            pending_edges_len: 5,
            replay_edges_capacity: 8,
            replay_edges_len: 7,
            decaying_hold_capacity: 9,
            decaying_hold_len: 4,
            density_life_capacity: [3, 11],
            density_life_len: [2, 10],
            num_players: 2,
        };
        let mut events = [None; 3 + MAX_PLAYERS];
        let count = trace.collect_capacity_growth(&grown, &mut events);

        assert_eq!(count, 3);
        assert_eq!(
            events[0],
            Some(GameplayCapacityTraceEvent {
                kind: GameplayCapacityTraceKind::PendingEdges,
                old_capacity: 4,
                new_capacity: 6,
                len: 5,
            })
        );
        assert_eq!(
            events[1],
            Some(GameplayCapacityTraceEvent {
                kind: GameplayCapacityTraceKind::DecayingHoldIndices,
                old_capacity: 2,
                new_capacity: 9,
                len: 4,
            })
        );
        assert_eq!(
            events[2],
            Some(GameplayCapacityTraceEvent {
                kind: GameplayCapacityTraceKind::DensityGraphLifePoints(1),
                old_capacity: 5,
                new_capacity: 11,
                len: 10,
            })
        );

        let count = trace.collect_capacity_growth(&grown, &mut events);
        assert_eq!(count, 0);
    }

    #[test]
    fn gameplay_input_latency_trace_records_totals_and_maxima() {
        let mut trace = GameplayInputLatencyTrace::default();

        trace.record(1, 2, 3, 4, 5);
        trace.record_sample(GameplayInputLatencySample {
            capture_to_store_us: 10,
            store_to_emit_us: 1,
            emit_to_queue_us: 9,
            capture_to_queue_us: 20,
            capture_to_process_us: 2,
            queue_to_process_us: 7,
        });

        assert_eq!(trace.samples, 2);
        assert_eq!(trace.capture_to_store_total_us, 11);
        assert_eq!(trace.store_to_emit_total_us, 3);
        assert_eq!(trace.emit_to_queue_total_us, 12);
        assert_eq!(trace.capture_to_process_total_us, 6);
        assert_eq!(trace.queue_to_process_total_us, 12);
        assert_eq!(trace.capture_to_store_max_us, 10);
        assert_eq!(trace.store_to_emit_max_us, 2);
        assert_eq!(trace.emit_to_queue_max_us, 9);
        assert_eq!(trace.capture_to_process_max_us, 4);
        assert_eq!(trace.queue_to_process_max_us, 7);
    }

    #[test]
    fn gameplay_input_latency_sample_calculates_stage_deltas() {
        let captured = Instant::now();
        let stored = captured + Duration::from_micros(2);
        let emitted = captured + Duration::from_micros(5);
        let queued = captured + Duration::from_micros(11);
        let processed = captured + Duration::from_micros(17);

        let sample = gameplay_input_latency_sample(captured, stored, emitted, queued, processed);

        assert_eq!(
            sample,
            GameplayInputLatencySample {
                capture_to_store_us: 2,
                store_to_emit_us: 3,
                emit_to_queue_us: 6,
                capture_to_queue_us: 11,
                capture_to_process_us: 17,
                queue_to_process_us: 6,
            }
        );
    }

    #[test]
    fn gameplay_input_latency_trace_average_handles_empty_samples() {
        assert_eq!(GameplayInputLatencyTrace::avg_us(99, 0), 0.0);
        assert_eq!(GameplayInputLatencyTrace::avg_us(9, 2), 4.5);
    }

    #[test]
    fn saturating_elapsed_us_between_handles_order_and_overflow() {
        let start = Instant::now();
        let ten_us = start + Duration::from_micros(10);
        let too_large = start + Duration::from_micros(u64::from(u32::MAX) + 1);

        assert_eq!(saturating_elapsed_us_between(ten_us, start), 10);
        assert_eq!(saturating_elapsed_us_between(start, ten_us), 0);
        assert_eq!(saturating_elapsed_us_between(too_large, start), u32::MAX);
    }

    #[test]
    fn add_elapsed_us_saturates_destination() {
        let started = Instant::now() - Duration::from_micros(10);
        let mut dst = u32::MAX - 1;

        add_elapsed_us(&mut dst, started);

        assert_eq!(dst, u32::MAX);
    }

    #[test]
    fn active_input_slot_update_holds_until_last_alias_release() {
        let mut slots = [EMPTY_ACTIVE_INPUT_SLOT; 4];
        let mut slot_count = 0;
        let mut lane_counts = [0_u16; MAX_COLS];
        let mut transitions = Vec::new();

        for (source, slot, pressed) in [
            (InputSource::Keyboard, 10, true),
            (InputSource::Keyboard, 11, true),
            (InputSource::Keyboard, 10, false),
            (InputSource::Keyboard, 11, false),
            (InputSource::Gamepad, 7, true),
            (InputSource::Keyboard, 12, true),
            (InputSource::Gamepad, 7, false),
            (InputSource::Keyboard, 12, false),
        ] {
            let update = update_active_input_slot(
                &mut slots,
                &mut slot_count,
                &mut lane_counts,
                0,
                source,
                slot,
                pressed,
            );
            transitions.push((update.was_down, update.is_down, update.slot_was_down));
            assert!(!update.slot_table_full);
        }

        assert_eq!(
            transitions,
            vec![
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
            ]
        );
        assert_eq!(slot_count, 0);
        assert_eq!(lane_counts[0], 0);
    }

    #[test]
    fn active_input_slot_update_reports_full_table_without_mutating_counts() {
        let mut slots = [EMPTY_ACTIVE_INPUT_SLOT; 1];
        let mut slot_count = 0;
        let mut lane_counts = [0_u16; MAX_COLS];

        let first = update_active_input_slot(
            &mut slots,
            &mut slot_count,
            &mut lane_counts,
            0,
            InputSource::Keyboard,
            10,
            true,
        );
        let full = update_active_input_slot(
            &mut slots,
            &mut slot_count,
            &mut lane_counts,
            1,
            InputSource::Keyboard,
            11,
            true,
        );

        assert_eq!(
            first,
            LaneInputUpdate {
                was_down: false,
                is_down: true,
                slot_was_down: false,
                slot_table_full: false,
            }
        );
        assert_eq!(
            full,
            LaneInputUpdate {
                was_down: false,
                is_down: false,
                slot_was_down: false,
                slot_table_full: true,
            }
        );
        assert_eq!(slot_count, 1);
        assert_eq!(lane_counts[0], 1);
        assert_eq!(lane_counts[1], 0);
    }

    #[test]
    fn gameplay_input_state_tracks_lanes_slots_and_reset() {
        let mut state = GameplayInputState::default();

        let first = state.update_slot(0, InputSource::Keyboard, 10, true);
        let second = state.update_slot(0, InputSource::Keyboard, 11, true);
        state.press_lane(0, 123);

        assert_eq!(
            first,
            LaneInputUpdate {
                was_down: false,
                is_down: true,
                slot_was_down: false,
                slot_table_full: false,
            }
        );
        assert_eq!(
            second,
            LaneInputUpdate {
                was_down: true,
                is_down: true,
                slot_was_down: false,
                slot_table_full: false,
            }
        );
        assert!(state.lane_is_pressed(0));
        assert_eq!(state.lane_counts()[0], 2);
        assert_eq!(state.lane_pressed_since_ns[0], Some(123));
        assert!(state.slot_lane_is_down(0, InputSource::Keyboard, 10));

        let release = state.update_slot(0, InputSource::Keyboard, 10, false);
        assert_eq!(
            release,
            LaneInputUpdate {
                was_down: true,
                is_down: true,
                slot_was_down: true,
                slot_table_full: false,
            }
        );
        assert!(state.lane_is_pressed(0));
        state.release_lane(0);
        assert_eq!(state.lane_pressed_since_ns[0], None);

        state.reset_live_state();
        assert!(!state.lane_is_pressed(0));
        assert_eq!(state.lane_counts()[0], 0);
        assert_eq!(state.lane_pressed_since_ns[0], None);
        assert!(!state.slot_lane_is_down(0, InputSource::Keyboard, 11));
    }

    #[test]
    fn autosync_sample_math_rounds_mean_and_measures_stddev() {
        assert_eq!(AUTOSYNC_OFFSET_SAMPLE_COUNT, 24);
        assert_near(AUTOSYNC_STDDEV_MAX_SECONDS, 0.03);

        let positive = [1_000_000_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        assert_eq!(autosync_mean_ns(&positive), 1_000_000);
        assert_near(
            autosync_stddev_seconds(&positive, autosync_mean_ns(&positive)),
            0.0,
        );

        let mut mixed = [0_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        mixed[0] = 1;
        assert_eq!(autosync_mean_ns(&mixed), 0);
        mixed[0] = -1;
        assert_eq!(autosync_mean_ns(&mixed), 0);

        let mut spread = [0_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        spread[0] = song_time_ns_from_seconds(0.03);
        spread[1] = song_time_ns_from_seconds(-0.03);
        let stddev = autosync_stddev_seconds(&spread, autosync_mean_ns(&spread));
        assert!(stddev > 0.008);
        assert!(stddev < AUTOSYNC_STDDEV_MAX_SECONDS);
    }

    #[test]
    fn autosync_offset_sample_buffers_until_full() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        for _ in 0..AUTOSYNC_OFFSET_SAMPLE_COUNT - 1 {
            let result = apply_autosync_offset_sample(
                &mut samples,
                &mut sample_count,
                AutosyncMode::Song,
                song_time_ns_from_seconds(0.010),
            );

            assert_eq!(result, AutosyncSampleResult::default());
        }

        assert_eq!(sample_count, AUTOSYNC_OFFSET_SAMPLE_COUNT - 1);
    }

    #[test]
    fn autosync_offset_sample_returns_stable_song_correction() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        let mut result = AutosyncSampleResult::default();
        for _ in 0..AUTOSYNC_OFFSET_SAMPLE_COUNT {
            result = apply_autosync_offset_sample(
                &mut samples,
                &mut sample_count,
                AutosyncMode::Song,
                song_time_ns_from_seconds(0.010),
            );
        }

        assert_eq!(sample_count, 0);
        assert_eq!(
            result.correction,
            Some(AutosyncOffsetCorrection::Song(0.010))
        );
        assert_eq!(result.standard_deviation, Some(0.0));
    }

    #[test]
    fn autosync_offset_sample_returns_machine_correction() {
        let mut samples = [song_time_ns_from_seconds(0.020); AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        let result = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Machine,
            song_time_ns_from_seconds(0.020),
        );

        assert_eq!(
            result.correction,
            Some(AutosyncOffsetCorrection::Machine(0.020))
        );
        assert_eq!(result.standard_deviation, Some(0.0));
    }

    #[test]
    fn gameplay_autosync_runtime_state_applies_samples() {
        let mut state = GameplayAutosyncRuntimeState {
            mode: AutosyncMode::Machine,
            offset_samples: [song_time_ns_from_seconds(0.020); AUTOSYNC_OFFSET_SAMPLE_COUNT],
            offset_sample_count: AUTOSYNC_OFFSET_SAMPLE_COUNT - 1,
            standard_deviation: 1.0,
        };

        let result = state.apply_offset_sample(song_time_ns_from_seconds(0.020));

        assert_eq!(
            result.correction,
            Some(AutosyncOffsetCorrection::Machine(0.020))
        );
        assert_eq!(state.offset_sample_count, 0);
        assert_eq!(state.standard_deviation, 0.0);
    }

    #[test]
    fn autosync_offset_sample_no_correction_for_noisy_samples() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        samples[0] = song_time_ns_from_seconds(0.20);
        let mut sample_count = AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        let result = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Song,
            song_time_ns_from_seconds(-0.20),
        );

        assert_eq!(sample_count, 0);
        assert!(result.standard_deviation.unwrap() > AUTOSYNC_STDDEV_MAX_SECONDS);
        assert_eq!(result.correction, None);
    }

    #[test]
    fn autosync_offset_sample_ignores_invalid_and_off_mode() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        let invalid = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Song,
            INVALID_SONG_TIME_NS,
        );
        let off = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Off,
            song_time_ns_from_seconds(0.010),
        );

        assert_eq!(invalid, AutosyncSampleResult::default());
        assert_eq!(off, AutosyncSampleResult::default());
        assert_eq!(sample_count, 0);
    }

    #[test]
    fn autosync_row_hits_enabled_rejects_blocked_contexts() {
        assert!(autosync_row_hits_enabled(
            false,
            false,
            AutosyncMode::Song,
            false
        ));
        assert!(!autosync_row_hits_enabled(
            true,
            false,
            AutosyncMode::Song,
            false
        ));
        assert!(!autosync_row_hits_enabled(
            false,
            true,
            AutosyncMode::Song,
            false
        ));
        assert!(!autosync_row_hits_enabled(
            false,
            false,
            AutosyncMode::Off,
            false
        ));
        assert!(!autosync_row_hits_enabled(
            false,
            false,
            AutosyncMode::Machine,
            true
        ));
    }

    #[test]
    fn autosync_row_hit_offsets_collect_good_tap_offsets() {
        let mut fantastic = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        fantastic.result = Some(Judgment {
            time_error_music_ns: song_time_ns_from_seconds(-0.012),
            ..test_judgment(JudgeGrade::Fantastic)
        });
        let mut great = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        great.result = Some(Judgment {
            time_error_music_ns: song_time_ns_from_seconds(0.020),
            ..test_judgment(JudgeGrade::Great)
        });
        let mut decent = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        decent.result = Some(Judgment {
            time_error_music_ns: song_time_ns_from_seconds(0.030),
            ..test_judgment(JudgeGrade::Decent)
        });
        let notes = [
            fantastic,
            great,
            decent,
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        note_indices[2] = 2;
        note_indices[3] = 3;
        let row_entry = RowEntry {
            row_index: 48,
            time_ns: song_time_ns_from_seconds(1.0),
            nonmine_note_indices: note_indices,
            nonmine_note_count: 4,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let mut offsets = [0; MAX_COLS];

        let count = collect_autosync_row_hit_offsets(&notes, &row_entry, &mut offsets);

        assert_eq!(count, 2);
        assert_eq!(offsets[0], song_time_ns_from_seconds(0.012));
        assert_eq!(offsets[1], song_time_ns_from_seconds(-0.020));
    }

    #[test]
    fn display_clock_snaps_on_first_update() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(10.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(20.0),
            1.0 / 60.0,
            1.0,
            true,
            |event| events.push(event.kind),
        ));

        assert_near(display_time, 20.0);
        assert!(events.is_empty());
        assert_near(clock.health().error_seconds, 0.0);
        assert!(!clock.health().catching_up);
    }

    #[test]
    fn display_clock_advances_smoothly_toward_target() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(100.05),
            1.0 / 60.0,
            1.0,
            false,
            |event| events.push(event.kind),
        ));

        assert!(display_time > 100.0);
        assert!(display_time < 100.05);
        assert!(events.contains(&DisplayClockDiagEventKind::TargetJump));
        assert!(clock.health().catching_up);
    }

    #[test]
    fn display_clock_resets_when_far_from_target() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(101.0),
            1.0 / 60.0,
            1.0,
            false,
            |event| events.push(event.kind),
        ));

        assert_near(display_time, 101.0);
        assert_eq!(events, vec![DisplayClockDiagEventKind::ResetJump]);
        assert_near(clock.health().error_seconds, 0.0);
        assert!(!clock.health().catching_up);
    }

    #[test]
    fn gameplay_display_clock_state_steps_and_records_diag_events() {
        let mut state = GameplayDisplayClockState::new(song_time_ns_from_seconds(100.0));

        let display_time = song_time_ns_to_seconds(state.step(
            500,
            song_time_ns_from_seconds(100.05),
            1.0 / 60.0,
            1.0,
            false,
            true,
        ));

        assert!(display_time > 100.0);
        assert!(display_time < 100.05);
        assert!(state.health().catching_up);
        assert!(state.diag_trigger_seq() > 0);

        let mut recent = Vec::new();
        state.collect_diag_events(500, 1, &mut recent);
        assert!(
            recent
                .iter()
                .any(|event| event.kind == DisplayClockDiagEventKind::TargetJump)
        );

        let previous_seq = state.diag_trigger_seq();
        state.step(
            600,
            song_time_ns_from_seconds(100.1),
            1.0 / 60.0,
            1.0,
            false,
            false,
        );
        assert_eq!(state.diag_trigger_seq(), previous_seq);
    }

    #[test]
    fn beat_phase_state_tracks_freeze_delay_and_pause() {
        let mut state = GameplayBeatPhaseState::default();

        assert!(!state.is_in_freeze());
        assert!(!state.is_in_delay());
        assert!(!state.paused());

        state.set(true, false);
        assert!(state.is_in_freeze());
        assert!(!state.is_in_delay());
        assert!(state.paused());

        state.set(false, true);
        assert!(!state.is_in_freeze());
        assert!(state.is_in_delay());
        assert!(state.paused());

        state.set(false, false);
        assert!(!state.paused());
    }

    #[test]
    fn display_clock_diag_ring_collects_recent_events() {
        let step = DisplayClockStepEvent {
            kind: DisplayClockDiagEventKind::TargetJump,
            target_time_sec: 2.0,
            previous_time_sec: 1.0,
            current_time_sec: 1.5,
            error_seconds: 0.5,
            step_seconds: 0.1,
            limit_seconds: 0.2,
        };
        let mut ring = DisplayClockDiagRing::new();
        ring.push(DisplayClockDiagEvent::from_step_event(100, step));
        ring.push(DisplayClockDiagEvent::from_step_event(
            200,
            DisplayClockStepEvent {
                kind: DisplayClockDiagEventKind::ClampStep,
                ..step
            },
        ));

        let mut recent = Vec::new();
        ring.collect_recent(250, 100, &mut recent);

        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].at_host_nanos, 200);
        assert_eq!(recent[0].kind, DisplayClockDiagEventKind::ClampStep);
        assert!(ring.last_trigger_seq() > 0);
    }

    #[test]
    fn display_clock_diag_ring_keeps_bounded_recent_order() {
        let step = DisplayClockStepEvent {
            kind: DisplayClockDiagEventKind::TargetJump,
            target_time_sec: 0.0,
            previous_time_sec: 0.0,
            current_time_sec: 0.0,
            error_seconds: 0.0,
            step_seconds: 0.0,
            limit_seconds: 0.0,
        };
        let mut ring = DisplayClockDiagRing::new();
        for i in 1..=(DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT + 3) {
            ring.push(DisplayClockDiagEvent::from_step_event(i as u64, step));
        }

        let mut recent = Vec::new();
        ring.collect_recent(100, 100, &mut recent);

        assert_eq!(recent.len(), DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT);
        assert_eq!(recent[0].at_host_nanos, 4);
        assert_eq!(
            recent[DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT - 1].at_host_nanos,
            (DISPLAY_CLOCK_STUTTER_DIAG_EVENT_COUNT + 3) as u64
        );
    }

    #[test]
    fn timing_tick_mode_cycles_and_labels_match_runtime_text() {
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Off),
            GameplayTimingTickMode::Assist
        );
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Assist),
            GameplayTimingTickMode::Hit
        );
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Hit),
            GameplayTimingTickMode::Off
        );

        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Off),
            None
        );
        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Assist),
            Some("Assist Tick")
        );
        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Hit),
            Some("Hit Tick")
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Off),
            "off"
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Assist),
            "assist tick"
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Hit),
            "hit tick"
        );
    }

    #[test]
    fn offset_adjust_keys_map_to_slots_and_deltas() {
        assert_near(OFFSET_ADJUST_STEP_SECONDS, 0.001);
        assert_eq!(
            offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Decrease),
            0
        );
        assert_eq!(
            offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Increase),
            1
        );
        assert_near(
            offset_adjust_delta_for_key(GameplayOffsetAdjustKey::Decrease),
            -0.001,
        );
        assert_near(
            offset_adjust_delta_for_key(GameplayOffsetAdjustKey::Increase),
            0.001,
        );
    }

    #[test]
    fn offset_adjust_target_uses_global_song_or_none() {
        assert_eq!(
            offset_adjust_target(true, false),
            GameplayOffsetAdjustTarget::Global
        );
        assert_eq!(
            offset_adjust_target(true, true),
            GameplayOffsetAdjustTarget::Global
        );
        assert_eq!(
            offset_adjust_target(false, false),
            GameplayOffsetAdjustTarget::Song
        );
        assert_eq!(
            offset_adjust_target(false, true),
            GameplayOffsetAdjustTarget::None
        );
    }

    #[test]
    fn offset_delta_target_ignores_tiny_noops() {
        assert_eq!(offset_delta_target_seconds(1.0, 0.0), None);
        assert_eq!(
            offset_delta_target_seconds(1.0, OFFSET_DELTA_EPSILON_SECONDS * 0.5),
            None
        );
        assert_eq!(
            offset_delta_target_seconds(1.0, OFFSET_DELTA_EPSILON_SECONDS),
            None
        );
        assert_eq!(
            offset_delta_target_seconds(1.0, OFFSET_DELTA_EPSILON_SECONDS * 2.0),
            Some(1.0 + OFFSET_DELTA_EPSILON_SECONDS * 2.0)
        );
        assert_eq!(offset_delta_target_seconds(1.0, -0.25), Some(0.75));
    }

    #[test]
    fn offset_adjust_repeat_ready_respects_delay_and_interval() {
        assert!(!offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY - Duration::from_millis(1),
            OFFSET_ADJUST_REPEAT_INTERVAL,
        ));
        assert!(!offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY,
            OFFSET_ADJUST_REPEAT_INTERVAL - Duration::from_millis(1),
        ));
        assert!(offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY,
            OFFSET_ADJUST_REPEAT_INTERVAL,
        ));
    }

    #[test]
    fn offset_adjust_hold_state_starts_and_clears_slots() {
        let now = Instant::now();
        let mut held_since = [None; 2];
        let mut last_at = [None; 2];

        let delta = start_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Decrease,
            now,
        );

        assert_near(delta, -OFFSET_ADJUST_STEP_SECONDS);
        assert_eq!(held_since[0], Some(now));
        assert_eq!(last_at[0], Some(now));
        assert_eq!(held_since[1], None);
        assert_eq!(last_at[1], None);

        clear_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Decrease,
        );

        assert_eq!(held_since, [None; 2]);
        assert_eq!(last_at, [None; 2]);
    }

    #[test]
    fn gameplay_offset_adjust_hold_state_wraps_repeat_state() {
        let start = Instant::now();
        let mut state = GameplayOffsetAdjustHoldState::default();

        let delta = state.start(GameplayOffsetAdjustKey::Decrease, start);

        assert_near(delta, -OFFSET_ADJUST_STEP_SECONDS);
        assert_eq!(
            state.held_since_for_key(GameplayOffsetAdjustKey::Decrease),
            Some(start)
        );
        assert_eq!(
            state.last_at_for_key(GameplayOffsetAdjustKey::Decrease),
            Some(start)
        );
        assert_eq!(
            state.tick(
                GameplayOffsetAdjustKey::Decrease,
                start + OFFSET_ADJUST_REPEAT_DELAY
            ),
            Some(-OFFSET_ADJUST_STEP_SECONDS)
        );
        state.clear(GameplayOffsetAdjustKey::Decrease);
        assert_eq!(
            state.held_since_for_key(GameplayOffsetAdjustKey::Decrease),
            None
        );
        assert_eq!(
            state.last_at_for_key(GameplayOffsetAdjustKey::Decrease),
            None
        );
    }

    #[test]
    fn offset_adjust_hold_state_ticks_after_delay_and_interval() {
        let start = Instant::now();
        let mut held_since = [None; 2];
        let mut last_at = [None; 2];
        start_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Increase,
            start,
        );

        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                start + OFFSET_ADJUST_REPEAT_DELAY - Duration::from_millis(1),
            ),
            None
        );

        let first_tick = start + OFFSET_ADJUST_REPEAT_DELAY;
        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick,
            ),
            Some(OFFSET_ADJUST_STEP_SECONDS)
        );
        assert_eq!(
            last_at[offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Increase)],
            Some(first_tick)
        );

        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick + OFFSET_ADJUST_REPEAT_INTERVAL - Duration::from_millis(1),
            ),
            None
        );
        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick + OFFSET_ADJUST_REPEAT_INTERVAL,
            ),
            Some(OFFSET_ADJUST_STEP_SECONDS)
        );
    }

    #[test]
    fn replay_capacity_uses_recording_budget() {
        assert_eq!(replay_edge_cap(4, 0, true, 120.0), 0);
        assert_eq!(replay_edge_cap(4, 0, false, 0.0), 4 * 64);
        assert_eq!(
            replay_edge_cap(4, 0, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(
            replay_edge_cap(4, 120, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(replay_edge_cap(4, 4000, false, 2.0), 8000);
        assert_eq!(
            replay_edge_cap(8, 1000, false, 1.0),
            8 * REPLAY_EDGE_RATE_PER_SEC
        );
    }

    fn replay_input_edge_at(lane_index: u8, time_ns: SongTimeNs) -> ReplayInputEdge {
        ReplayInputEdge {
            lane_index,
            pressed: true,
            source: InputSource::Keyboard,
            event_music_time_ns: time_ns,
        }
    }

    #[test]
    fn replay_input_builder_filters_invalid_lanes_and_times() {
        let input = [
            replay_input_edge_at(0, 10),
            replay_input_edge_at(4, 20),
            replay_input_edge_at(1, INVALID_SONG_TIME_NS),
        ];

        let replay = build_replay_input_edges(&input, 1, 4, 4, 0, [0, 0]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].lane_index, 0);
        assert_eq!(replay[0].event_music_time_ns, 10);
    }

    #[test]
    fn replay_input_builder_shifts_by_player_beat_zero() {
        let input = [replay_input_edge_at(5, 100)];

        let replay = build_replay_input_edges(&input, 2, 4, 8, 30, [30, 80]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].lane_index, 5);
        assert_eq!(replay[0].event_music_time_ns, 150);
    }

    #[test]
    fn replay_input_builder_skips_shift_for_invalid_recorded_offset() {
        let input = [replay_input_edge_at(5, 100)];

        let replay = build_replay_input_edges(&input, 2, 4, 8, INVALID_SONG_TIME_NS, [30, 80]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_music_time_ns, 100);
    }

    #[test]
    fn replay_input_builder_sorts_only_when_needed() {
        let input = [
            replay_input_edge_at(0, 30),
            replay_input_edge_at(1, 10),
            replay_input_edge_at(2, 20),
        ];

        let replay = build_replay_input_edges(&input, 1, 4, 4, 0, [0, 0]);

        assert_eq!(
            replay
                .iter()
                .map(|edge| edge.event_music_time_ns)
                .collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    fn recorded_edge_at(time_ns: SongTimeNs) -> RecordedLaneEdge {
        RecordedLaneEdge {
            lane_index: 1,
            pressed: true,
            source: InputSource::Keyboard,
            event_music_time_ns: time_ns,
        }
    }

    #[test]
    fn replay_edge_readiness_waits_for_event_time() {
        let input = [recorded_edge_at(20)];
        let mut cursor = 0;

        assert!(next_ready_replay_edge(&input, &mut cursor, 19).is_none());
        assert_eq!(cursor, 0);

        let edge = next_ready_replay_edge(&input, &mut cursor, 20).expect("ready edge");
        assert_eq!(edge.event_music_time_ns, 20);
        assert_eq!(cursor, 1);
    }

    #[test]
    fn replay_edge_readiness_consumes_in_order_and_handles_end() {
        let input = [recorded_edge_at(10), recorded_edge_at(30)];
        let mut cursor = 0;

        assert_eq!(
            next_ready_replay_edge(&input, &mut cursor, 30)
                .expect("first edge")
                .event_music_time_ns,
            10
        );
        assert_eq!(
            next_ready_replay_edge(&input, &mut cursor, 30)
                .expect("second edge")
                .event_music_time_ns,
            30
        );
        assert!(next_ready_replay_edge(&input, &mut cursor, 30).is_none());
        assert_eq!(cursor, 2);
    }

    #[test]
    fn replay_edge_readiness_ignores_cursor_beyond_input() {
        let input = [recorded_edge_at(10)];
        let mut cursor = 5;

        assert!(next_ready_replay_edge(&input, &mut cursor, 30).is_none());
        assert_eq!(cursor, 5);
    }

    #[test]
    fn collect_ready_replay_edges_filters_lanes_and_stops_at_future() {
        let input = [
            RecordedLaneEdge {
                lane_index: 0,
                ..recorded_edge_at(10)
            },
            RecordedLaneEdge {
                lane_index: 6,
                ..recorded_edge_at(20)
            },
            RecordedLaneEdge {
                lane_index: 2,
                ..recorded_edge_at(30)
            },
            RecordedLaneEdge {
                lane_index: 1,
                ..recorded_edge_at(50)
            },
        ];
        let mut cursor = 0;
        let mut events = [None; MAX_COLS];

        let count = collect_ready_replay_edges(&input, &mut cursor, 30, 4, &mut events);

        assert_eq!(count, 2);
        assert_eq!(cursor, 3);
        assert_eq!(events[0].map(|edge| edge.lane_index), Some(0));
        assert_eq!(events[1].map(|edge| edge.lane_index), Some(2));
        assert!(events[2].is_none());
    }

    #[test]
    fn collect_ready_replay_edges_respects_event_buffer() {
        let input = [
            RecordedLaneEdge {
                lane_index: 0,
                ..recorded_edge_at(10)
            },
            RecordedLaneEdge {
                lane_index: 1,
                ..recorded_edge_at(20)
            },
        ];
        let mut cursor = 0;
        let mut events = [None; 1];

        let count = collect_ready_replay_edges(&input, &mut cursor, 20, 4, &mut events);

        assert_eq!(count, 1);
        assert_eq!(cursor, 1);
        assert_eq!(events[0].map(|edge| edge.lane_index), Some(0));
    }

    #[test]
    fn gameplay_replay_input_state_collects_and_resets_cursor() {
        let input = vec![recorded_edge_at(10), recorded_edge_at(20)];
        let mut state = GameplayReplayInputState::new(input);
        let mut events = [None; MAX_COLS];

        assert!(!state.is_empty());
        assert_eq!(state.collect_ready(10, 4, &mut events), 1);
        assert_eq!(events[0].map(|edge| edge.event_music_time_ns), Some(10));

        events.fill(None);
        assert_eq!(state.collect_ready(10, 4, &mut events), 0);
        state.reset_cursor();
        assert_eq!(state.collect_ready(20, 4, &mut events), 2);
        assert_eq!(events[0].map(|edge| edge.event_music_time_ns), Some(10));
        assert_eq!(events[1].map(|edge| edge.event_music_time_ns), Some(20));
    }

    #[test]
    fn gameplay_replay_runtime_state_tracks_mode_capture_and_reset() {
        let input = GameplayReplayInputState::new(vec![recorded_edge_at(10)]);
        let mut state = GameplayReplayRuntimeState::new(input, true, 4);
        let mut events = [None; MAX_COLS];

        assert!(state.mode);
        assert!(state.capture_enabled);
        assert!(state.edges.capacity() >= 4);

        state.edges.push(recorded_edge_at(20));
        assert_eq!(state.input.collect_ready(10, 4, &mut events), 1);
        events.fill(None);

        state.reset_for_restart();

        assert!(state.edges.is_empty());
        assert_eq!(state.input.collect_ready(10, 4, &mut events), 1);

        state.disable_replay_mode();

        assert!(!state.mode);
        assert!(!state.capture_enabled);
    }

    #[test]
    fn effective_player_global_offset_adds_optional_player_shift() {
        let shifts = [0.010, -0.020, 0.0];

        assert_near(
            effective_player_global_offset_seconds(-0.008, &shifts, 0),
            0.002,
        );
        assert_near(
            effective_player_global_offset_seconds(-0.008, &shifts, 1),
            -0.028,
        );
        assert_near(
            effective_player_global_offset_seconds(-0.008, &shifts, 9),
            -0.008,
        );
    }

    #[test]
    fn offset_state_tracks_initial_current_and_player_shift() {
        let mut state = GameplayOffsetState::new(-0.008, [0.010, -0.020], 0.125);

        assert_near(state.initial_global_offset_seconds(), -0.008);
        assert_near(state.global_offset_seconds(), -0.008);
        assert_near(state.initial_song_offset_seconds(), 0.125);
        assert_near(state.song_offset_seconds(), 0.125);
        assert_near(state.effective_player_global_offset_seconds(0), 0.002);
        assert_near(state.effective_player_global_offset_seconds(1), -0.028);
        assert_near(state.effective_player_global_offset_seconds(9), -0.008);

        state.set_global_offset_seconds(0.015);
        state.set_song_offset_seconds(-0.025);
        state.set_player_global_offset_shift_seconds(1, 0.030);
        state.set_player_global_offset_shift_seconds(9, 1.0);

        assert_near(state.initial_global_offset_seconds(), -0.008);
        assert_near(state.global_offset_seconds(), 0.015);
        assert_near(state.initial_song_offset_seconds(), 0.125);
        assert_near(state.song_offset_seconds(), -0.025);
        assert_near(state.effective_player_global_offset_seconds(1), 0.045);
        assert_near(state.effective_player_global_offset_seconds(9), 0.015);
    }

    #[test]
    fn column_scroll_dirs_apply_reverse_split_alternate_and_cross() {
        let reverse = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&reverse[..4], &[-1.0, -1.0, -1.0, -1.0]);

        let split = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                split: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&split[..4], &[1.0, 1.0, -1.0, -1.0]);

        let alternate = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                alternate: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&alternate[..4], &[1.0, -1.0, 1.0, -1.0]);

        let cross = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                cross: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&cross[..4], &[1.0, -1.0, -1.0, 1.0]);
    }

    #[test]
    fn scroll_reverse_percent_matches_itg_column_rules() {
        let options = ScrollReverseOptions {
            reverse: 1.0,
            split: 1.0,
            alternate: 1.0,
            cross: 0.0,
        };

        assert_near(scroll_reverse_percent_for_column(options, 0, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(options, 1, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(options, 2, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(options, 3, 4), 1.0);
    }

    #[test]
    fn scroll_reverse_percent_handles_cross_wrap_and_empty_fields() {
        let cross = ScrollReverseOptions {
            cross: 1.0,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_percent_for_column(cross, 0, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(cross, 1, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(cross, 2, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(cross, 3, 4), 0.0);

        let wrapped = ScrollReverseOptions {
            reverse: 3.25,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_percent_for_column(wrapped, 0, 4), 0.75);
        assert_near(scroll_reverse_percent_for_column(wrapped, 0, 0), 0.0);
    }

    #[test]
    fn scroll_reverse_scale_maps_percent_to_direction() {
        let reverse = ScrollReverseOptions {
            reverse: 1.0,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_scale_for_column(reverse, 0, 4), -1.0);
        assert_near(
            scroll_reverse_scale_for_column(ScrollReverseOptions::default(), 0, 4),
            1.0,
        );
    }

    #[test]
    fn scroll_effects_build_from_flags_and_reuse_column_policy() {
        let scroll = ScrollEffects::from_flags(true, true, false, false, true);
        assert_near(scroll.reverse, 1.0);
        assert_near(scroll.split, 1.0);
        assert_near(scroll.alternate, 0.0);
        assert_near(scroll.cross, 0.0);
        assert_near(scroll.centered, 1.0);
        assert_near(scroll.reverse_percent_for_column(0, 4), 1.0);
        assert_near(scroll.reverse_percent_for_column(3, 4), 0.0);
        assert_near(scroll.reverse_scale_for_column(0, 4), -1.0);
    }

    #[test]
    fn song_lua_target_matching_uses_one_based_player_ids() {
        assert!(song_lua_target_matches_player(None, 0));
        assert!(song_lua_target_matches_player(Some(1), 0));
        assert!(song_lua_target_matches_player(Some(2), 1));
        assert!(!song_lua_target_matches_player(Some(2), 0));
    }

    #[test]
    fn song_lua_window_seconds_use_len_end_and_global_offset() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::Len,
                &timing,
                0.25,
            ),
            Some((1.0, 3.0))
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.25,
            ),
            Some((1.0, 2.0))
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Second,
                5.0,
                7.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.25,
            ),
            Some((4.75, 6.75))
        );
    }

    #[test]
    fn song_lua_window_seconds_reject_invalid_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                3.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.0,
            ),
            None
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Second,
                f32::NAN,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.0,
            ),
            None
        );
    }

    #[test]
    fn song_lua_sustain_end_uses_span_policy_and_only_extends() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 5);

        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::Len,
                Some(1.0),
                &timing,
                0.0,
                3.0,
            ),
            4.0,
        );
        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                Some(4.0),
                &timing,
                0.0,
                2.0,
            ),
            4.0,
        );
        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                Some(1.5),
                &timing,
                0.0,
                2.0,
            ),
            2.0,
        );
    }

    #[test]
    fn song_lua_message_second_uses_beat_timing() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(song_lua_message_second(2.0, &timing, 99.0), Some(2.0));
    }

    #[test]
    fn scroll_overrides_approach_targets_by_speed() {
        let mut current = ScrollOverrides {
            reverse: Some(0.0),
            split: Some(1.0),
            cross: Some(0.25),
            ..ScrollOverrides::default()
        };
        let target = ScrollOverrides {
            reverse: Some(1.0),
            split: None,
            alternate: Some(0.5),
            cross: Some(1.0),
            ..ScrollOverrides::default()
        };
        let speed = ScrollOverrides {
            reverse: Some(2.0),
            alternate: None,
            cross: Some(0.0),
            ..ScrollOverrides::default()
        };
        let base = ScrollEffects {
            alternate: 0.25,
            ..ScrollEffects::default()
        };

        approach_scroll_overrides_to_target(&mut current, target, speed, base, 0.25);

        assert_near(current.reverse.unwrap(), 0.5);
        assert_eq!(current.split, None);
        assert_near(current.alternate.unwrap(), 0.5);
        assert_near(current.cross.unwrap(), 0.25);
    }

    #[test]
    fn column_scroll_dirs_apply_mods_per_four_panel_group() {
        let dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                alternate: true,
                ..ColumnScrollFlags::default()
            },
            8,
        );
        assert_eq!(&dirs[..8], &[-1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0]);
    }

    #[test]
    fn column_scroll_dirs_ignore_columns_after_requested_count() {
        let dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            2,
        );
        assert_eq!(&dirs[..4], &[-1.0, -1.0, 1.0, 1.0]);

        let full = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            MAX_COLS + 10,
        );
        assert!(full.iter().all(|dir| *dir == -1.0));
    }

    #[test]
    fn gameplay_tween_eases_expected_curves() {
        assert_near(GameplayTween::Linear.ease(0.5), 0.5);
        assert_near(GameplayTween::Accelerate.ease(0.5), 0.25);
        assert_near(GameplayTween::Decelerate.ease(0.5), 0.75);
        assert_near(GameplayTween::Linear.ease(-1.0), 0.0);
        assert_near(GameplayTween::Linear.ease(2.0), 1.0);
    }

    #[test]
    fn song_lua_ease_targets_normalize_column_mods() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            1.0,
            2.0,
            4.0,
            "Bumpy4",
            25.0,
            75.0,
            Some("outQuad"),
            Some(0.5),
            Some(1.5),
        ));

        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.target, SongLuaEaseMaskTarget::VisualBumpyColumn(3));
        assert_near(window.start_second, 1.0);
        assert_near(window.end_second, 2.0);
        assert_near(window.sustain_end_second, 4.0);
        assert_near(window.from, 0.25);
        assert_near(window.to, 0.75);
        assert_eq!(window.easing.as_deref(), Some("outQuad"));
        assert_eq!(window.opt1, Some(0.5));
        assert_eq!(window.opt2, Some(1.5));
    }

    #[test]
    fn song_lua_ease_targets_expand_perspective_aliases() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "incoming",
            20.0,
            60.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::PerspectiveTilt);
        assert_near(windows[0].from, -0.2);
        assert_near(windows[0].to, -0.6);
        assert_eq!(windows[1].target, SongLuaEaseMaskTarget::PerspectiveSkew);
        assert_near(windows[1].from, 0.2);
        assert_near(windows[1].to, 0.6);
    }

    #[test]
    fn song_lua_ease_targets_keep_raw_speed_and_mini_values() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "cmod",
            300.0,
            650.0,
            None,
            None,
            None,
        ));
        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "mini",
            25.0,
            50.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::ScrollSpeedC);
        assert_near(windows[0].from, 300.0);
        assert_near(windows[0].to, 650.0);
        assert_eq!(windows[1].target, SongLuaEaseMaskTarget::MiniPercent);
        assert_near(windows[1].from, 25.0);
        assert_near(windows[1].to, 50.0);
    }

    #[test]
    fn song_lua_ease_targets_handle_aliases_and_reject_unknown() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "reverse vanish",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert_eq!(
            windows[0].target,
            SongLuaEaseMaskTarget::AppearanceRandomVanish
        );
        assert_near(windows[0].to, 1.0);

        assert!(!append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert!(!append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "unsupported",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn song_lua_ease_targets_convert_confusion_y_offset() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "confusionyoffset",
            std::f32::consts::PI * 50.0,
            std::f32::consts::PI * 100.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::ConfusionYOffsetY);
        assert_near(windows[0].from, 90.0);
        assert_near(windows[0].to, 180.0);
    }

    #[test]
    fn song_lua_runtime_ease_window_appends_mod_targets() {
        let mut windows = Vec::new();

        assert_eq!(
            append_song_lua_runtime_ease_window(
                &mut windows,
                1.0,
                2.0,
                3.0,
                SongLuaRuntimeEaseTarget::Mod("drunk"),
                25.0,
                75.0,
                Some("linear"),
                Some(0.25),
                Some(0.75),
            ),
            SongLuaRuntimeEaseAppend::Appended
        );

        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.target, SongLuaEaseMaskTarget::VisualDrunk);
        assert_near(window.start_second, 1.0);
        assert_near(window.end_second, 2.0);
        assert_near(window.sustain_end_second, 3.0);
        assert_near(window.from, 0.25);
        assert_near(window.to, 0.75);
        assert_eq!(window.easing.as_deref(), Some("linear"));
        assert_eq!(window.opt1, Some(0.25));
        assert_eq!(window.opt2, Some(0.75));
    }

    #[test]
    fn song_lua_runtime_ease_window_appends_player_targets() {
        let mut windows = Vec::new();

        assert_eq!(
            append_song_lua_runtime_ease_window(
                &mut windows,
                4.0,
                5.0,
                6.0,
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerRotationZ),
                -45.0,
                90.0,
                Some("outQuad"),
                None,
                Some(1.5),
            ),
            SongLuaRuntimeEaseAppend::Appended
        );

        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.target, SongLuaEaseMaskTarget::PlayerRotationZ);
        assert_near(window.from, -45.0);
        assert_near(window.to, 90.0);
        assert_eq!(window.easing.as_deref(), Some("outQuad"));
        assert_eq!(window.opt1, None);
        assert_eq!(window.opt2, Some(1.5));
    }

    #[test]
    fn song_lua_runtime_ease_window_reports_unsupported_and_ignored() {
        let mut windows = Vec::new();

        assert_eq!(
            append_song_lua_runtime_ease_window(
                &mut windows,
                0.0,
                1.0,
                1.0,
                SongLuaRuntimeEaseTarget::Mod("unsupported"),
                0.0,
                100.0,
                None,
                None,
                None,
            ),
            SongLuaRuntimeEaseAppend::Unsupported
        );
        assert_eq!(
            append_song_lua_runtime_ease_window(
                &mut windows,
                0.0,
                1.0,
                1.0,
                SongLuaRuntimeEaseTarget::Function,
                0.0,
                100.0,
                None,
                None,
                None,
            ),
            SongLuaRuntimeEaseAppend::Ignored
        );
        assert!(windows.is_empty());
    }

    #[test]
    fn song_lua_ease_factor_defaults_to_clamped_linear() {
        assert_near(song_lua_ease_factor(None, 0.25, None, None), 0.25);
        assert_near(song_lua_ease_factor(Some("linear"), -1.0, None, None), 0.0);
        assert_near(song_lua_ease_factor(Some("linear"), 2.0, None, None), 1.0);
        assert_near(
            song_lua_ease_factor(Some("unknown"), 0.75, None, None),
            0.75,
        );
    }

    #[test]
    fn song_lua_ease_factor_matches_core_polynomial_curves() {
        assert_near(song_lua_ease_factor(Some("instant"), 0.0, None, None), 1.0);
        assert_near(song_lua_ease_factor(Some("inQuad"), 0.5, None, None), 0.25);
        assert_near(song_lua_ease_factor(Some("outQuad"), 0.5, None, None), 0.75);
        assert_near(
            song_lua_ease_factor(Some("inOutQuad"), 0.25, None, None),
            0.125,
        );
        assert_near(
            song_lua_ease_factor(Some("outInQuad"), 0.25, None, None),
            0.375,
        );
    }

    #[test]
    fn song_lua_ease_factor_handles_bounce_back_and_elastic() {
        assert_near(song_lua_ease_factor(Some("inBounce"), 0.0, None, None), 0.0);
        assert_near(
            song_lua_ease_factor(Some("outBounce"), 1.0, None, None),
            1.0,
        );

        for easing in ["inBack", "outInBack", "inElastic", "outInElastic"] {
            assert!(song_lua_ease_factor(Some(easing), 0.35, Some(1.0), Some(0.2)).is_finite());
        }
    }

    #[test]
    fn song_lua_column_offsets_hold_after_ease_until_cutoff() {
        let windows = [SongLuaColumnOffsetWindowRuntime {
            column: 2,
            start_second: 1.0,
            end_second: 1.5,
            sustain_end_second: 3.0,
            from_y: 33.75,
            to_y: 0.0,
            easing: Some("linear".to_string()),
            opt1: None,
            opt2: None,
        }];

        assert!(
            song_lua_column_offset_window_value(&windows[0], 1.25)
                .is_some_and(|value| (value - 16.875).abs() <= 0.001)
        );
        assert!(song_lua_column_y_offset(&windows, 2, 2.0).abs() <= 0.001);
        assert_eq!(song_lua_column_y_offset(&windows, 1, 2.0), 0.0);
        assert_eq!(song_lua_column_y_offset(&windows, 2, 3.01), 0.0);
    }

    #[test]
    fn song_lua_ease_window_value_interpolates_and_sustains() {
        let window = song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::AppearanceStealth,
            1.0,
            3.0,
            5.0,
            10.0,
            30.0,
        );

        assert!(song_lua_ease_window_value(&window, 0.99).is_none());
        assert_near(song_lua_ease_window_value(&window, 2.0).unwrap(), 20.0);
        assert_near(song_lua_ease_window_value(&window, 4.0).unwrap(), 30.0);
        assert!(song_lua_ease_window_value(&window, 5.0).is_none());
        assert!(song_lua_ease_window_value(&window, f32::NAN).is_none());
    }

    #[test]
    fn song_lua_ease_window_value_snaps_invalid_durations_to_target() {
        let window =
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::MiniPercent, 2.0, 2.0, 4.0, 0.0, 50.0);

        assert_near(song_lua_ease_window_value(&window, 2.5).unwrap(), 50.0);
    }

    #[test]
    fn song_lua_ease_tails_stop_at_next_same_target() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                4.0,
                5.0,
                5.0,
                1.0,
                0.0,
            ),
        ];

        song_lua_extend_ease_tails(&mut windows, &[]);

        assert_near(windows[0].sustain_end_second, 4.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_ease_tails_stop_at_constant_masks() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerX, 1.0, 2.0, 2.0, 0.0, 64.0),
        ];
        let mut mods = ParsedAttackMods {
            clear_all: true,
            ..ParsedAttackMods::default()
        };
        mods.appearance.hidden = Some(1.0);
        let constant = attack_mask_window(3.0, 6.0, mods);

        song_lua_extend_ease_tails(&mut windows, &[constant]);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_ease_tails_match_column_constant_targets() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::VisualBumpyColumn(2),
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::VisualBumpyColumn(3),
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
        ];
        let mut mods = ParsedAttackMods::default();
        mods.visual.bumpy_cols[2] = Some(1.0);
        let constant = attack_mask_window(3.0, 6.0, mods);

        song_lua_extend_ease_tails(&mut windows, &[constant]);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_stop_at_next_same_column() {
        let mut windows = [
            song_lua_column_offset_window(2, 1.0, 2.0, 2.0),
            song_lua_column_offset_window(2, 4.0, 5.0, 5.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 4.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_ignore_other_columns_and_same_tick() {
        let mut windows = [
            song_lua_column_offset_window(0, 1.0, 2.0, 2.0),
            song_lua_column_offset_window(1, 3.0, 4.0, 4.0),
            song_lua_column_offset_window(0, 1.0005, 2.0, 2.0),
            song_lua_column_offset_window(0, 5.0, 6.0, 6.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 5.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
        assert_near(windows[2].sustain_end_second, 5.0);
        assert_eq!(windows[3].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_clamp_explicit_sustain_to_cutoff() {
        let mut windows = [
            song_lua_column_offset_window(0, 1.0, 2.0, 3.0),
            song_lua_column_offset_window(0, 5.0, 6.0, 6.0),
            song_lua_column_offset_window(1, 1.0, 2.0, 8.0),
            song_lua_column_offset_window(1, 5.0, 6.0, 6.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_near(windows[2].sustain_end_second, 5.0);
    }

    #[test]
    fn song_lua_column_offset_window_runtime_copies_fields() {
        let window = build_song_lua_column_offset_window_runtime(
            3,
            1.25,
            2.5,
            4.0,
            -32.0,
            96.0,
            Some("outQuad"),
            Some(0.25),
            Some(0.75),
        );

        assert_eq!(window.column, 3);
        assert_near(window.start_second, 1.25);
        assert_near(window.end_second, 2.5);
        assert_near(window.sustain_end_second, 4.0);
        assert_near(window.from_y, -32.0);
        assert_near(window.to_y, 96.0);
        assert_eq!(window.easing.as_deref(), Some("outQuad"));
        assert_eq!(window.opt1, Some(0.25));
        assert_eq!(window.opt2, Some(0.75));
    }

    #[test]
    fn song_lua_note_hide_windows_cover_matching_column_bounds() {
        let windows = [
            SongLuaNoteHideWindowRuntime {
                column: 2,
                start_beat: 40.0,
                end_beat: 44.0,
            },
            SongLuaNoteHideWindowRuntime {
                column: 3,
                start_beat: 48.0,
                end_beat: 52.0,
            },
        ];

        assert!(song_lua_note_hidden(&windows, 2, 40.0));
        assert!(song_lua_note_hidden(&windows, 2, 44.0));
        assert!(song_lua_note_hidden(&windows, 2, 39.99995));
        assert!(!song_lua_note_hidden(&windows, 1, 42.0));
        assert!(!song_lua_note_hidden(&windows, 2, 44.01));
    }

    #[test]
    fn song_lua_note_hide_window_runtime_copies_fields() {
        let window = build_song_lua_note_hide_window_runtime(3, 12.5, 24.0);

        assert_eq!(window.column, 3);
        assert_near(window.start_beat, 12.5);
        assert_near(window.end_beat, 24.0);
    }

    #[test]
    fn song_lua_field_note_hide_maps_global_columns() {
        let windows = [SongLuaNoteHideWindowRuntime {
            column: 2,
            start_beat: 40.0,
            end_beat: 44.0,
        }];

        assert!(song_lua_field_note_hidden(&windows, 4, 6, 42.0));
        assert!(!song_lua_field_note_hidden(&windows, 4, 5, 42.0));
        assert!(song_lua_field_note_hidden(&windows, 0, 2, 42.0));
    }

    #[test]
    fn song_lua_message_events_offset_event_times_only() {
        let mut events = [
            SongLuaOverlayMessageRuntime {
                event_second: 1.25,
                command_index: 2,
            },
            SongLuaOverlayMessageRuntime {
                event_second: 3.5,
                command_index: 7,
            },
        ];

        offset_song_lua_message_events(&mut events, 4.0);

        assert_near(events[0].event_second, 5.25);
        assert_eq!(events[0].command_index, 2);
        assert_near(events[1].event_second, 7.5);
        assert_eq!(events[1].command_index, 7);
    }

    #[test]
    fn song_lua_message_events_ignore_zero_and_nonfinite_offsets() {
        let original = [
            SongLuaOverlayMessageRuntime {
                event_second: 1.25,
                command_index: 2,
            },
            SongLuaOverlayMessageRuntime {
                event_second: 3.5,
                command_index: 7,
            },
        ];
        let mut events = original;

        offset_song_lua_message_events(&mut events, 0.0);
        assert_eq!(events, original);

        offset_song_lua_message_events(&mut events, f32::NAN);
        assert_eq!(events, original);
    }

    #[test]
    fn song_lua_overlay_message_runtime_copies_fields() {
        let event = build_song_lua_overlay_message_runtime(3.25, 7);

        assert_near(event.event_second, 3.25);
        assert_eq!(event.command_index, 7);
    }

    #[test]
    fn song_lua_message_command_indices_match_case_insensitively() {
        let long_command = "A".repeat(129);
        let indices = build_song_lua_message_command_indices([
            (0, "Hide"),
            (1, "Show"),
            (2, "hide"),
            (3, "ÄHide"),
            (4, long_command.as_str()),
        ]);

        assert_eq!(song_lua_message_command_index(&indices, "hide"), Some(0));
        assert_eq!(song_lua_message_command_index(&indices, "HIDE"), Some(0));
        assert_eq!(song_lua_message_command_index(&indices, "show"), Some(1));
        assert_eq!(song_lua_message_command_index(&indices, "ÄHIDE"), Some(3));
        assert_eq!(song_lua_message_command_index(&indices, &long_command), Some(4));
        assert_eq!(song_lua_message_command_index(&indices, "missing"), None);
    }

    #[test]
    fn song_lua_overlay_ease_window_runtime_copies_fields() {
        let window = build_song_lua_overlay_ease_window_runtime(
            5,
            1.0,
            2.0,
            4.0,
            Some(3.0),
            11u8,
            22u8,
            Some("inOutQuad"),
            Some(0.5),
            Some(1.5),
        );

        assert_eq!(window.overlay_index, 5);
        assert_near(window.start_second, 1.0);
        assert_near(window.end_second, 2.0);
        assert_near(window.sustain_end_second, 4.0);
        assert_eq!(window.cutoff_second, Some(3.0));
        assert_eq!(window.from, 11);
        assert_eq!(window.to, 22);
        assert_eq!(window.easing.as_deref(), Some("inOutQuad"));
        assert_eq!(window.opt1, Some(0.5));
        assert_eq!(window.opt2, Some(1.5));
    }

    #[test]
    fn song_lua_overlay_eases_group_by_overlay_and_sort_times() {
        let windows = vec![
            song_lua_overlay_ease_window(1, 4.0, 5.0, 5.0, None),
            song_lua_overlay_ease_window(0, 3.0, 4.0, 4.0, None),
            song_lua_overlay_ease_window(1, 1.0, 3.0, 3.0, None),
            song_lua_overlay_ease_window(3, 0.0, 1.0, 1.0, None),
            song_lua_overlay_ease_window(1, 1.0, 2.0, 2.0, None),
        ];

        let (flat, ranges) = group_song_lua_overlay_eases(2, windows);

        assert_eq!(ranges, vec![0..1, 1..4]);
        assert_eq!(flat.len(), 4);
        assert_eq!(flat[0].overlay_index, 0);
        assert_near(flat[1].start_second, 1.0);
        assert_near(flat[1].end_second, 2.0);
        assert_near(flat[2].start_second, 1.0);
        assert_near(flat[2].end_second, 3.0);
        assert_near(flat[3].start_second, 4.0);
    }

    #[test]
    fn song_lua_overlay_eases_offset_window_times_and_cutoffs() {
        let mut windows = [
            song_lua_overlay_ease_window(0, 1.0, 2.0, 4.0, Some(3.0)),
            song_lua_overlay_ease_window(1, 5.0, 6.0, 6.0, None),
        ];

        offset_song_lua_overlay_eases(&mut windows, 7.0);

        assert_near(windows[0].start_second, 8.0);
        assert_near(windows[0].end_second, 9.0);
        assert_near(windows[0].sustain_end_second, 11.0);
        assert_near(windows[0].cutoff_second.unwrap(), 10.0);
        assert_near(windows[1].start_second, 12.0);
        assert_eq!(windows[1].cutoff_second, None);
    }

    #[test]
    fn song_lua_overlay_eases_ignore_zero_and_nonfinite_offsets() {
        let original = [
            song_lua_overlay_ease_window(0, 1.0, 2.0, 4.0, Some(3.0)),
            song_lua_overlay_ease_window(1, 5.0, 6.0, 6.0, None),
        ];
        let mut windows = original.clone();

        offset_song_lua_overlay_eases(&mut windows, 0.0);
        assert_eq!(windows, original);

        offset_song_lua_overlay_eases(&mut windows, f32::INFINITY);
        assert_eq!(windows, original);
    }

    #[test]
    fn song_lua_player_transform_target_updates_player_values() {
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerX,
            f32::NAN,
            &mut player,
        );
        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::VisualDrunk,
            1.0,
            &mut player,
        );
        assert_eq!(player, SongLuaPlayerTransformValues::default());

        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerZoom,
            1.25,
            &mut player,
        );
        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerSkewY,
            -0.5,
            &mut player,
        );

        assert_near(player.zoom_x.unwrap(), 1.25);
        assert_near(player.zoom_y.unwrap(), 1.25);
        assert_near(player.zoom_z.unwrap(), 1.25);
        assert_near(player.skew_y.unwrap(), -0.5);
    }

    #[test]
    fn song_lua_player_transform_resolve_filters_and_defaults_values() {
        let resolved = SongLuaPlayerTransformValues {
            x: Some(f32::NAN),
            y: Some(32.0),
            z: Some(f32::INFINITY),
            rotation_x: Some(12.0),
            rotation_z: None,
            rotation_y: Some(f32::NEG_INFINITY),
            skew_x: Some(-0.25),
            skew_y: Some(f32::NAN),
            zoom_x: None,
            zoom_y: Some(1.5),
            zoom_z: Some(f32::NAN),
            confusion_y_offset: Some(9.0),
        }
        .resolve();

        assert_eq!(resolved.x, None);
        assert_eq!(resolved.y, Some(32.0));
        assert_near(resolved.z, 0.0);
        assert_near(resolved.rotation_x, 12.0);
        assert_near(resolved.rotation_z, 0.0);
        assert_near(resolved.rotation_y, 0.0);
        assert_near(resolved.skew_x, -0.25);
        assert_near(resolved.skew_y, 0.0);
        assert_near(resolved.zoom_x, 1.0);
        assert_near(resolved.zoom_y, 1.5);
        assert_near(resolved.zoom_z, 1.0);
        assert_near(resolved.confusion_y_offset, 9.0);
    }

    #[test]
    fn song_lua_player_transform_defaults_match_runtime_state() {
        let defaults = song_lua_player_transforms_default();
        assert_eq!(defaults.len(), MAX_PLAYERS);
        for transform in defaults {
            assert_eq!(transform.x, None);
            assert_eq!(transform.y, None);
            assert_near(transform.z, 0.0);
            assert_near(transform.rotation_x, 0.0);
            assert_near(transform.rotation_z, 0.0);
            assert_near(transform.rotation_y, 0.0);
            assert_near(transform.skew_x, 0.0);
            assert_near(transform.skew_y, 0.0);
            assert_near(transform.zoom_x, 1.0);
            assert_near(transform.zoom_y, 1.0);
            assert_near(transform.zoom_z, 1.0);
            assert_near(transform.confusion_y_offset, 0.0);
        }
    }

    #[test]
    fn song_lua_eased_target_updates_effect_outputs() {
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance = AppearanceEffects::default();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::AccelBoost,
            0.75,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::VisualBumpyColumn(2),
            1.5,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::AppearanceStealth,
            0.25,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::VisibilityDark,
            1.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollReverse,
            0.5,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::PerspectiveTilt,
            -1.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::MiniPercent,
            30.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );

        assert_near(accel.boost.unwrap(), 0.75);
        assert_near(visual.bumpy_cols[2].unwrap(), 1.5);
        assert_near(appearance.stealth, 0.25);
        assert_near(visibility.dark.unwrap(), 1.0);
        assert_near(scroll.reverse.unwrap(), 0.5);
        assert_near(perspective.tilt.unwrap(), -1.0);
        assert_near(mini_percent.unwrap(), 30.0);
    }

    #[test]
    fn song_lua_eased_target_handles_scroll_speed_and_player_targets() {
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance = AppearanceEffects::default();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollSpeedC,
            -100.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert!(scroll_speed.is_none());

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollSpeedC,
            650.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert!(matches!(
            scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            45.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert_near(player.rotation_z.unwrap(), 45.0);
    }

    #[test]
    fn song_lua_player_eases_apply_only_player_targets() {
        let windows = [
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerZoom, 1.0, 3.0, 3.0, 0.0, 2.0),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::VisualDrunk, 1.0, 3.0, 3.0, 0.0, 1.0),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerX, 3.0, 4.0, 4.0, 0.0, 100.0),
        ];
        let mut player = SongLuaPlayerTransformValues::default();

        apply_song_lua_player_eases(&mut player, &windows, 2.0);

        assert_near(player.zoom_x.unwrap(), 1.0);
        assert_near(player.zoom_y.unwrap(), 1.0);
        assert_near(player.zoom_z.unwrap(), 1.0);
        assert_eq!(player.x, None);
    }

    #[test]
    fn song_lua_attack_eases_apply_active_windows_and_mini_delta() {
        let windows = [
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::AccelBoost, 1.0, 3.0, 3.0, 0.0, 1.0),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                3.0,
                3.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::ScrollSpeedC,
                1.0,
                3.0,
                3.0,
                300.0,
                600.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::MiniPercent,
                1.0,
                3.0,
                3.0,
                10.0,
                20.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::PlayerRotationZ,
                1.0,
                3.0,
                3.0,
                0.0,
                90.0,
            ),
        ];
        let mut attack = ActiveAttackMaskValues::new(AppearanceEffects::default());
        let mut appearance = AppearanceEffects::default();
        let mut player = SongLuaPlayerTransformValues::default();

        apply_song_lua_attack_eases(
            &mut attack,
            &mut appearance,
            &mut player,
            &windows,
            2.0,
            30.0,
        );

        assert_near(attack.accel.boost.unwrap(), 0.5);
        assert_near(appearance.stealth, 0.5);
        assert!(matches!(
            attack.scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 450.0).abs() <= 0.000_001
        ));
        assert_near(attack.mini_percent.unwrap(), 45.0);
        assert_near(player.rotation_z.unwrap(), 45.0);
    }

    #[test]
    fn noteskin_effect_defaults_match_runtime_fallbacks() {
        let effects = GameplayNoteskinEffects::default();

        let glow = effects.receptor_glow_behavior_for_player(0);
        assert_near(glow.duration, 0.2);
        assert!(glow.blend_add);

        let default_step = effects.receptor_step_behavior_for_col(0, 0, None);
        assert_near(default_step.duration, 0.11);
        assert!(default_step.interrupts);

        let scored_step = effects.receptor_step_behavior_for_col(0, 0, Some("W1"));
        assert_near(scored_step.duration, 0.0);
        assert!(!scored_step.interrupts);

        assert_eq!(effects.tap_explosion_duration(0, 0, "W1", false), None);
        assert_near(effects.mine_explosion_duration(0), MINE_EXPLOSION_DURATION);
    }

    #[test]
    fn judge_grades_map_to_noteskin_windows() {
        assert_eq!(grade_to_window(JudgeGrade::Fantastic), Some("W1"));
        assert_eq!(grade_to_window(JudgeGrade::Excellent), Some("W2"));
        assert_eq!(grade_to_window(JudgeGrade::Great), Some("W3"));
        assert_eq!(grade_to_window(JudgeGrade::Decent), Some("W4"));
        assert_eq!(grade_to_window(JudgeGrade::WayOff), Some("W5"));
        assert_eq!(grade_to_window(JudgeGrade::Miss), Some("Miss"));
    }

    #[test]
    fn tap_explosion_options_map_judgment_windows() {
        let options = TapExplosionOptions {
            fantastic: true,
            excellent: true,
            great: true,
            decent: true,
            way_off: true,
            miss: true,
            held: true,
            holding: true,
        };

        for window in ["W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held"] {
            assert!(tap_explosion_enabled_for_options(options, window));
        }
        assert!(!tap_explosion_enabled_for_options(options, "Holding"));
    }

    #[test]
    fn tap_explosion_options_gate_disabled_windows() {
        let options = TapExplosionOptions {
            miss: true,
            held: true,
            ..TapExplosionOptions::default()
        };

        assert!(tap_explosion_enabled_for_options(options, "Miss"));
        assert!(tap_explosion_enabled_for_options(options, "Held"));
        assert!(!tap_explosion_enabled_for_options(options, "W0"));
        assert!(!tap_explosion_enabled_for_options(options, "W1"));
        assert!(!tap_explosion_enabled_for_options(options, "W3"));
    }

    #[test]
    fn hold_explosion_options_use_holding_bit_only() {
        assert!(hold_explosion_enabled_for_options(TapExplosionOptions {
            holding: true,
            ..TapExplosionOptions::default()
        }));
        assert!(!hold_explosion_enabled_for_options(TapExplosionOptions {
            held: true,
            ..TapExplosionOptions::default()
        }));
    }

    #[test]
    fn fantastic_window_options_select_play_and_blue_windows() {
        let base = FantasticWindowOptions {
            base_fa_plus_s: 0.015,
            custom_fantastic_window_s: None,
            fa_plus_10ms_blue_window: false,
        };
        assert_near(fantastic_window_seconds(base), 0.015);
        assert_near(blue_fantastic_window_ms(base), 15.0);

        let ten_ms = FantasticWindowOptions {
            fa_plus_10ms_blue_window: true,
            ..base
        };
        assert_near(fantastic_window_seconds(ten_ms), 0.015);
        assert_near(blue_fantastic_window_ms(ten_ms), 10.0);

        let custom = FantasticWindowOptions {
            custom_fantastic_window_s: Some(0.012),
            ..ten_ms
        };
        assert_near(fantastic_window_seconds(custom), 0.012);
        assert_near(blue_fantastic_window_ms(custom), 12.0);
    }

    #[test]
    fn player_judgment_timing_applies_custom_fantastic_and_rate() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: Some(0.012),
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.5,
        );

        let fa_plus_ns = timing
            .profile_music_ns
            .fa_plus_window_ns
            .expect("fa+ window");
        assert!((fa_plus_ns - song_time_ns_from_seconds(0.018)).abs() <= 1);
        assert_eq!(
            timing.largest_tap_window_music_ns,
            timing.profile_music_ns.windows_ns[4]
        );
    }

    #[test]
    fn player_judgment_timing_respects_disabled_windows() {
        let mut disabled = [false; 5];
        disabled[3] = true;
        disabled[4] = true;
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            disabled,
            1.0,
        );

        assert_eq!(timing.disabled_windows, disabled);
        assert_eq!(
            timing.largest_tap_window_music_ns,
            timing.profile_music_ns.windows_ns[2]
        );
    }

    #[test]
    fn note_hit_eval_for_timing_classifies_offsets() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.0,
        );
        let note_time_ns = song_time_ns_from_seconds(10.0);
        let event_time_ns = note_time_ns + timing.profile_music_ns.windows_ns[2];

        let hit = note_hit_eval_for_timing(timing, note_time_ns, event_time_ns).expect("valid hit");

        assert_eq!(hit.note_time_ns, note_time_ns);
        assert_eq!(
            hit.measured_offset_music_ns,
            timing.profile_music_ns.windows_ns[2]
        );
        assert_eq!(hit.grade, JudgeGrade::Great);
        assert_eq!(hit.window, TimingWindow::W3);
    }

    #[test]
    fn note_hit_eval_for_timing_rejects_beyond_largest_window() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.0,
        );
        let note_time_ns = song_time_ns_from_seconds(10.0);
        let event_time_ns = note_time_ns + timing.largest_tap_window_music_ns + 1;

        assert!(note_hit_eval_for_timing(timing, note_time_ns, event_time_ns).is_none());
    }

    #[test]
    fn final_note_hit_judgment_uses_resolved_offset_and_rate() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(12.0),
            measured_offset_music_ns: song_time_ns_from_seconds(0.020),
            grade: JudgeGrade::Excellent,
            window: TimingWindow::W2,
        };
        let resolved_offset = song_time_ns_from_seconds(-0.010);

        let (judgment, event_time_ns) = final_note_hit_judgment(hit, resolved_offset, 2.0);

        assert_eq!(event_time_ns, hit.note_time_ns + resolved_offset);
        assert_eq!(judgment.grade, JudgeGrade::Excellent);
        assert_eq!(judgment.window, Some(TimingWindow::W2));
        assert_eq!(judgment.time_error_music_ns, resolved_offset);
        assert_near(judgment.time_error_ms, -5.0);
        assert!(!judgment.miss_because_held);
    }

    #[test]
    fn final_note_hit_plan_includes_receptor_window() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(12.0),
            measured_offset_music_ns: song_time_ns_from_seconds(0.020),
            grade: JudgeGrade::Great,
            window: TimingWindow::W3,
        };
        let resolved_offset = song_time_ns_from_seconds(0.030);

        let plan = final_note_hit_plan(hit, resolved_offset, 1.5);

        assert_eq!(
            plan.judgment_event_time_ns,
            hit.note_time_ns + resolved_offset
        );
        assert_eq!(plan.judgment.grade, JudgeGrade::Great);
        assert_eq!(plan.judgment.window, Some(TimingWindow::W3));
        assert_eq!(plan.judgment.time_error_music_ns, resolved_offset);
        assert_near(plan.judgment.time_error_ms, 20.0);
        assert_eq!(plan.receptor_window, Some("W3"));
    }

    #[test]
    fn hit_active_hold_start_plans_holds_and_rolls_only() {
        let start = song_time_ns_from_seconds(2.0);
        let end = song_time_ns_from_seconds(4.0);
        let current = song_time_ns_from_seconds(2.1);

        assert_eq!(
            hit_active_hold_start(NoteType::Hold, 7, 3, start, Some(end), current),
            Some(HitActiveHoldStart {
                column: 3,
                note_index: 7,
                start_time_ns: start,
                end_time_ns: end,
                current_time_ns: current,
            })
        );
        assert_eq!(
            hit_active_hold_start(NoteType::Roll, 7, 3, start, Some(end), current),
            Some(HitActiveHoldStart {
                column: 3,
                note_index: 7,
                start_time_ns: start,
                end_time_ns: end,
                current_time_ns: current,
            })
        );
        assert_eq!(
            hit_active_hold_start(NoteType::Tap, 7, 3, start, Some(end), current),
            None
        );
        assert_eq!(
            hit_active_hold_start(NoteType::Hold, 7, 3, start, None, current),
            None
        );
    }

    #[test]
    fn note_hit_judgment_uses_supplied_offset_and_hit_window() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: -45_000_000,
            grade: JudgeGrade::Decent,
            window: TimingWindow::W4,
        };

        let judgment = note_hit_judgment(hit, 30_000_000, 1.5);

        assert_eq!(judgment.grade, JudgeGrade::Decent);
        assert_eq!(judgment.window, Some(TimingWindow::W4));
        assert_eq!(judgment.time_error_music_ns, 30_000_000);
        assert_near(judgment.time_error_ms, 20.0);
        assert!(!judgment.miss_because_held);
    }

    #[test]
    fn provisional_early_hit_plan_uses_measured_offset_and_life_delta() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: -45_000_000,
            grade: JudgeGrade::Decent,
            window: TimingWindow::W4,
        };

        let plan = provisional_early_hit_plan(hit, 1.5, false);

        assert_eq!(plan.judgment.grade, JudgeGrade::Decent);
        assert_eq!(plan.judgment.window, Some(TimingWindow::W4));
        assert_eq!(
            plan.judgment.time_error_music_ns,
            hit.measured_offset_music_ns
        );
        assert_near(plan.judgment.time_error_ms, -30.0);
        assert_near(
            plan.life_delta,
            deadsync_rules::life::judge_life_delta(JudgeGrade::Decent),
        );
        assert!(plan.apply_life_change);
        assert!(plan.capture_failed_ex_score_inputs);
    }

    #[test]
    fn provisional_early_hit_plan_disables_scoring_side_effects_when_blocked() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: -45_000_000,
            grade: JudgeGrade::WayOff,
            window: TimingWindow::W5,
        };

        let plan = provisional_early_hit_plan(hit, 1.0, true);

        assert_eq!(plan.judgment.grade, JudgeGrade::WayOff);
        assert_near(
            plan.life_delta,
            deadsync_rules::life::judge_life_delta(JudgeGrade::WayOff),
        );
        assert!(!plan.apply_life_change);
        assert!(!plan.capture_failed_ex_score_inputs);
    }

    #[test]
    fn early_rescore_hit_decision_requires_single_rescore_track() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: -45_000_000,
            grade: JudgeGrade::Decent,
            window: TimingWindow::W4,
        };

        assert_eq!(early_rescore_hit_decision(0, hit, false), None);
        assert_eq!(early_rescore_hit_decision(2, hit, false), None);
    }

    #[test]
    fn early_rescore_hit_decision_classifies_provisional_paths() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: -45_000_000,
            grade: JudgeGrade::Decent,
            window: TimingWindow::W4,
        };

        assert_eq!(
            early_rescore_hit_decision(1, hit, false),
            Some(EarlyRescoreHitDecision::Provisional)
        );
        assert_eq!(
            early_rescore_hit_decision(1, hit, true),
            Some(EarlyRescoreHitDecision::DuplicateProvisional)
        );
    }

    #[test]
    fn early_rescore_hit_decision_ignores_bad_rehits() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: 45_000_000,
            grade: JudgeGrade::Decent,
            window: TimingWindow::W4,
        };

        assert_eq!(
            early_rescore_hit_decision(1, hit, true),
            Some(EarlyRescoreHitDecision::IgnoreBadRehit)
        );
    }

    #[test]
    fn early_rescore_hit_decision_allows_good_final_hits() {
        let hit = NoteHitEval {
            note_time_ns: song_time_ns_from_seconds(8.0),
            measured_offset_music_ns: 20_000_000,
            grade: JudgeGrade::Excellent,
            window: TimingWindow::W2,
        };

        assert_eq!(
            early_rescore_hit_decision(1, hit, true),
            Some(EarlyRescoreHitDecision::FinalSingleTrackHit)
        );
    }

    #[test]
    fn fantastic_feedback_requires_fa_plus_and_fantastic_grade() {
        let fantastic = test_judgment(JudgeGrade::Fantastic);
        let excellent = test_judgment(JudgeGrade::Excellent);

        assert!(!tap_judgment_uses_bright_explosion_for_options(
            FantasticFeedbackOptions::default(),
            &fantastic,
        ));
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            FantasticFeedbackOptions {
                show_fa_plus_window: true,
                ..FantasticFeedbackOptions::default()
            },
            &excellent,
        ));
    }

    #[test]
    fn fantastic_feedback_uses_w1_for_bright_tap_explosion() {
        let mut white = test_judgment(JudgeGrade::Fantastic);
        white.window = Some(TimingWindow::W1);
        let mut blue = white.clone();
        blue.window = Some(TimingWindow::W0);
        let options = FantasticFeedbackOptions {
            show_fa_plus_window: true,
            ..FantasticFeedbackOptions::default()
        };

        assert!(tap_judgment_uses_bright_explosion_for_options(
            options, &white
        ));
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            options, &blue
        ));
    }

    #[test]
    fn fantastic_feedback_uses_10ms_blue_window_when_enabled() {
        let mut blue = test_judgment(JudgeGrade::Fantastic);
        blue.window = Some(TimingWindow::W0);
        blue.time_error_ms = FA_PLUS_W010_MS;
        let mut white = blue.clone();
        white.time_error_ms = FA_PLUS_W010_MS + 0.001;
        let options = FantasticFeedbackOptions {
            show_fa_plus_window: true,
            fa_plus_10ms_blue_window: true,
            ..FantasticFeedbackOptions::default()
        };

        assert!(!tap_judgment_uses_bright_explosion_for_options(
            options, &blue
        ));
        assert!(tap_judgment_uses_bright_explosion_for_options(
            options, &white
        ));

        let split_options = FantasticFeedbackOptions {
            split_15_10ms: true,
            ..options
        };
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            split_options,
            &white,
        ));

        let custom_options = FantasticFeedbackOptions {
            custom_fantastic_window: true,
            ..options
        };
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            custom_options,
            &white,
        ));
    }

    #[test]
    fn noteskin_effect_setters_clamp_player_and_column_reads() {
        let mut effects = GameplayNoteskinEffects::default();
        let last_player = MAX_PLAYERS - 1;
        let last_col = MAX_COLS - 1;
        effects.set_receptor_step_behavior(
            0,
            0,
            Some("W3"),
            GameplayReceptorStepBehavior {
                duration: 0.4,
                zoom_start: 0.5,
                zoom_end: 1.5,
                tween: GameplayTween::Accelerate,
                interrupts: false,
            },
        );
        effects.set_tap_explosion_duration(0, 0, "Held", true, Some(0.7));
        effects.set_mine_explosion_duration(0, 0.9);
        effects.set_tap_explosion_duration(last_player, last_col, "Held", true, Some(0.8));
        effects.set_mine_explosion_duration(last_player, 1.1);

        assert_near(
            effects
                .receptor_step_behavior_for_col(0, 0, Some("W3"))
                .duration,
            0.4,
        );
        assert_eq!(
            effects.tap_explosion_duration(0, 0, "Held", true),
            Some(0.7)
        );
        assert_near(effects.mine_explosion_duration(MAX_PLAYERS), 1.1);
        assert_eq!(
            effects.tap_explosion_duration(MAX_PLAYERS, MAX_COLS, "Held", true),
            Some(0.8)
        );
    }

    #[test]
    fn receptor_behaviors_sample_zoom_and_glow() {
        let step = GameplayReceptorStepBehavior {
            duration: 1.0,
            zoom_start: 0.5,
            zoom_end: 1.5,
            tween: GameplayTween::Linear,
            interrupts: true,
        };
        assert_near(step.sample_zoom(0.5), 1.0);

        let glow = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 1.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };
        let (press_alpha, press_zoom) = glow.sample_press(0.5);
        assert_near(press_alpha, 0.5);
        assert_near(press_zoom, 1.5);
        let (lift_alpha, lift_zoom) = glow.sample_lift(0.5, 1.0, 2.0);
        assert_near(lift_alpha, 0.5);
        assert_near(lift_zoom, 1.5);
    }

    #[test]
    fn receptor_glow_visual_selects_press_hold_lift_or_idle() {
        let behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 1.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let press = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                press_timer: 0.5,
                lane_pressed: true,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("active press tween should render");
        assert_near(press.0, 0.5);
        assert_near(press.1, 1.5);

        let held = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                lane_pressed: true,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("held lane should render press end state");
        assert_near(held.0, 1.0);
        assert_near(held.1, 2.0);

        let lift = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                lift_timer: 0.5,
                lift_start_alpha: 1.0,
                lift_start_zoom: 2.0,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("active lift tween should render");
        assert_near(lift.0, 0.5);
        assert_near(lift.1, 1.5);

        assert!(receptor_glow_visual(behavior, GameplayReceptorGlowState::default()).is_none());
    }

    #[test]
    fn receptor_glow_duration_and_lift_start_use_runtime_fallbacks() {
        let mut behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 0.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        assert_near(receptor_glow_duration(behavior), RECEPTOR_GLOW_DURATION);
        let (alpha, zoom) = receptor_glow_lift_start(behavior, 0.5);
        assert_near(alpha, 0.5);
        assert_near(zoom, 1.5);

        behavior.press_duration = 0.0;
        let (alpha, zoom) = receptor_glow_lift_start(behavior, 0.5);
        assert_near(alpha, 1.0);
        assert_near(zoom, 2.0);
    }

    #[test]
    fn receptor_glow_timer_entries_match_press_pulse_and_release_policy() {
        let mut behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.25,
            press_alpha_end: 0.75,
            press_zoom_start: 1.25,
            press_zoom_end: 1.75,
            press_tween: GameplayTween::Linear,
            duration: 0.5,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let press = receptor_glow_press_timers(behavior);
        assert_near(press.press_timer, 1.0);
        assert_near(press.lift_timer, 0.0);
        assert_near(press.lift_start_alpha, 0.75);
        assert_near(press.lift_start_zoom, 1.75);

        let pulse = receptor_glow_pulse_timers(behavior);
        assert_near(pulse.press_timer, 0.0);
        assert_near(pulse.lift_timer, 0.5);
        assert_near(pulse.lift_start_alpha, 0.25);
        assert_near(pulse.lift_start_zoom, 1.25);

        behavior.duration = 0.0;
        let pulse = receptor_glow_pulse_timers(behavior);
        assert_near(pulse.lift_timer, RECEPTOR_GLOW_DURATION);

        let release = receptor_glow_release_timers(behavior, 0.5);
        assert_near(release.press_timer, 0.0);
        assert_near(release.lift_timer, RECEPTOR_GLOW_DURATION);
        assert_near(release.lift_start_alpha, 0.5);
        assert_near(release.lift_start_zoom, 1.5);
    }

    #[test]
    fn receptor_glow_timer_tick_handles_pressed_release_and_lift_decay() {
        let behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 0.4,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let pressed = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.7,
                lift_timer: 0.25,
                lift_start_alpha: 0.4,
                lift_start_zoom: 1.4,
            },
            true,
            0.2,
        );
        assert_near(pressed.press_timer, 0.5);
        assert_near(pressed.lift_timer, 0.0);
        assert_near(pressed.lift_start_alpha, 0.4);
        assert_near(pressed.lift_start_zoom, 1.4);

        let release = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.3,
                lift_timer: 0.0,
                lift_start_alpha: 0.0,
                lift_start_zoom: 1.0,
            },
            false,
            0.3,
        );
        assert_near(release.press_timer, 0.0);
        assert_near(release.lift_timer, 0.4);
        assert_near(release.lift_start_alpha, 0.7);
        assert_near(release.lift_start_zoom, 1.7);

        let lift = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.0,
                lift_timer: 0.25,
                lift_start_alpha: 0.7,
                lift_start_zoom: 1.7,
            },
            false,
            0.1,
        );
        assert_near(lift.lift_timer, 0.15);
        assert_near(lift.lift_start_alpha, 0.7);
        assert_near(lift.lift_start_zoom, 1.7);
    }

    #[test]
    fn receptor_glow_column_tick_uses_player_mapping_and_bounds() {
        let mut effects = GameplayNoteskinEffects::default();
        effects.set_receptor_glow_behavior(
            0,
            GameplayReceptorGlowBehavior {
                duration: 0.5,
                ..GameplayReceptorGlowBehavior::default()
            },
        );
        effects.set_receptor_glow_behavior(
            1,
            GameplayReceptorGlowBehavior {
                duration: 1.0,
                ..GameplayReceptorGlowBehavior::default()
            },
        );
        let input_lane_counts = [1, 0, 0, 0, 0, 0];
        let mut press_timers = [0.4, 0.4, 0.4, 0.4, 0.0, 9.0];
        let mut lift_timers = [0.5, 0.5, 0.5, 0.5, 1.0, 9.0];
        let mut lift_start_alpha = [0.1, 0.2, 0.3, 0.4, 0.5, 9.0];
        let mut lift_start_zoom = [1.1, 1.2, 1.3, 1.4, 1.5, 9.0];

        tick_receptor_glow_columns(
            &effects,
            5,
            2,
            4,
            &input_lane_counts,
            &mut press_timers,
            &mut lift_timers,
            &mut lift_start_alpha,
            &mut lift_start_zoom,
            0.25,
        );

        assert_near(press_timers[0], 0.15);
        assert_near(lift_timers[0], 0.0);
        assert_near(press_timers[1], 0.15);
        assert_near(lift_timers[1], 0.5);
        assert_near(lift_start_alpha[1], 0.2);
        assert_near(lift_start_zoom[1], 1.2);
        assert_near(lift_timers[4], 0.75);
        assert_near(press_timers[5], 9.0);
        assert_near(lift_timers[5], 9.0);
    }

    #[test]
    fn receptor_feedback_state_resets_ticks_and_samples_bop() {
        let mut state = GameplayReceptorFeedbackState::default();
        assert_near(state.glow_lift_start_zoom[0], 1.0);

        state.set_glow_timers(
            0,
            GameplayReceptorGlowTimers {
                press_timer: 0.4,
                lift_timer: 0.5,
                lift_start_alpha: 0.25,
                lift_start_zoom: 1.25,
            },
        );
        state.start_bop(
            0,
            GameplayReceptorStepBehavior {
                duration: 0.5,
                zoom_start: 1.0,
                zoom_end: 1.5,
                tween: GameplayTween::Linear,
                interrupts: true,
            },
        );
        assert_near(state.bop_zoom(0), 1.0);

        let mut effects = GameplayNoteskinEffects::default();
        effects.set_receptor_glow_behavior(
            0,
            GameplayReceptorGlowBehavior {
                press_duration: 1.0,
                ..GameplayReceptorGlowBehavior::default()
            },
        );
        state.tick(&effects, 1, 1, 4, &[1], 0.2);

        assert_near(state.glow_press_timers[0], 0.2);
        assert_near(state.glow_lift_timers[0], 0.0);
        assert_near(state.bop_timers[0], 0.3);
        assert!(state.bop_zoom(0) > 1.0);

        state.clear_lift_glow(0);
        assert_near(state.glow_lift_timers[0], 0.0);

        state.reset_for_practice();
        assert_near(state.glow_lift_start_zoom[0], 0.0);
        assert_near(state.bop_timers[0], 0.0);
        assert_near(state.bop_behaviors[0].duration, 0.0);

        state.glow_lift_start_zoom[0] = 0.0;
        state.reset_for_autoplay();
        assert_near(state.glow_lift_start_zoom[0], 1.0);
    }

    #[test]
    fn autoplay_random_offset_w1_uses_full_window_without_fa_plus() {
        let mut rng = TurnRng::new(1);
        let mut profile = TimingProfile::default_itg_with_fa_plus();
        profile.fa_plus_window_s = None;
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let outer = profile_ns.windows_ns[0];
        for _ in 0..32 {
            let offset =
                autoplay_random_offset_music_ns_for_window(&mut rng, profile_ns, TimingWindow::W1);
            assert!(offset.abs() <= outer);
        }
    }

    #[test]
    fn autoplay_random_offset_w1_excludes_w0_band_when_enabled() {
        let mut rng = TurnRng::new(2);
        let profile = TimingProfile::default_itg_with_fa_plus();
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let inner = profile_ns
            .fa_plus_window_ns
            .expect("default profile has W0");
        let outer = profile_ns.windows_ns[0];
        for _ in 0..32 {
            let offset =
                autoplay_random_offset_music_ns_for_window(&mut rng, profile_ns, TimingWindow::W1);
            assert!(offset.abs() >= inner);
            assert!(offset.abs() <= outer);
        }
    }

    #[test]
    fn autoplay_judgment_offset_preserves_measured_input_when_not_live() {
        let mut rng = TurnRng::new(3);
        let profile_ns =
            TimingProfileNs::from_profile_scaled(&TimingProfile::default_itg_with_fa_plus(), 1.0);

        assert_eq!(
            autoplay_judgment_offset_music_ns(false, &mut rng, profile_ns, TimingWindow::W1, 1234),
            1234
        );
    }

    #[test]
    fn autoplay_judgment_offset_randomizes_live_autoplay_window() {
        let mut rng = TurnRng::new(4);
        let profile = TimingProfile::default_itg_with_fa_plus();
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let inner = profile_ns
            .fa_plus_window_ns
            .expect("default profile has W0");
        let outer = profile_ns.windows_ns[0];

        let offset = autoplay_judgment_offset_music_ns(
            true,
            &mut rng,
            profile_ns,
            TimingWindow::W1,
            outer.saturating_mul(10),
        );

        assert!(offset.abs() >= inner);
        assert!(offset.abs() <= outer);
    }

    #[test]
    fn live_autoplay_and_scoring_block_flags_exclude_replay_mode() {
        assert!(live_autoplay_enabled_from_flags(true, false));
        assert!(!live_autoplay_enabled_from_flags(true, true));
        assert!(!live_autoplay_enabled_from_flags(false, false));
        assert!(autoplay_blocks_scoring_from_flags(true, false));
        assert!(!autoplay_blocks_scoring_from_flags(true, true));
        assert!(!autoplay_blocks_scoring_from_flags(false, false));
    }

    #[test]
    fn autoplay_enable_cursor_clamps_to_player_note_range() {
        assert_eq!(autoplay_cursor_for_enable(4, (10, 20)), 10);
        assert_eq!(autoplay_cursor_for_enable(14, (10, 20)), 14);
        assert_eq!(autoplay_cursor_for_enable(30, (10, 20)), 20);
        assert_eq!(autoplay_cursor_for_enable(30, (20, 10)), 20);
    }

    #[test]
    fn gameplay_autoplay_runtime_state_tracks_cursors_and_offsets() {
        let mut state = GameplayAutoplayRuntimeState::new(3, [0; MAX_PLAYERS]);
        state.set_cursor_for_enable(0, 4, (10, 20));
        state.set_cursor_for_enable(1, 14, (10, 20));
        state.set_cursor(1, 30);
        state.set_cursor(MAX_PLAYERS, 40);

        assert_eq!(state.cursor(0), 10);
        assert_eq!(state.cursor(1), 30);
        assert_eq!(state.cursor(MAX_PLAYERS), 0);

        let profile_ns =
            TimingProfileNs::from_profile_scaled(&TimingProfile::default_itg_with_fa_plus(), 1.0);
        assert_eq!(
            state.judgment_offset_music_ns(false, profile_ns, TimingWindow::W1, 1234),
            1234
        );
    }

    #[test]
    fn collect_next_autoplay_row_events_filters_ready_row() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 12, 0.25),
            test_note_at(NoteType::Mine, None, false, 12, 0.25),
            test_note_at(NoteType::Lift, None, false, 12, 0.25),
            test_note_at(NoteType::Tap, None, true, 12, 0.25),
            test_note_at(NoteType::Tap, None, false, 24, 0.5),
        ];
        notes[0].result = Some(test_judgment(JudgeGrade::Fantastic));
        notes[1].column = 0;
        notes[2].column = 1;
        notes[3].column = 2;
        notes[4].column = 3;
        notes[5].column = 0;
        let note_times = [0, 1_000, 1_000, 1_000, 1_000, 2_000];
        let mut events = [None; MAX_COLS];

        let update = collect_next_autoplay_row_events(
            &notes,
            &note_times,
            (0, notes.len()),
            0,
            4,
            1_000,
            &mut events,
        );

        assert_eq!(
            update,
            AutoplayRowEventsUpdate {
                cursor: 5,
                row_time_ns: 1_000,
                event_count: 2,
                row_ready: true,
            }
        );
        assert_eq!(
            events[0],
            Some(AutoplayNoteEvent {
                note_index: 1,
                column: 0,
                action: AutoplayNoteAction::Tap,
            })
        );
        assert_eq!(
            events[1],
            Some(AutoplayNoteEvent {
                note_index: 3,
                column: 2,
                action: AutoplayNoteAction::Lift,
            })
        );
    }

    #[test]
    fn collect_next_autoplay_row_events_waits_for_future_row() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 12, 0.25),
        ];
        notes[0].result = Some(test_judgment(JudgeGrade::Fantastic));
        let note_times = [0, 2_000];
        let mut events = [None; MAX_COLS];

        let update = collect_next_autoplay_row_events(
            &notes,
            &note_times,
            (0, notes.len()),
            0,
            4,
            1_999,
            &mut events,
        );

        assert_eq!(
            update,
            AutoplayRowEventsUpdate {
                cursor: 1,
                row_time_ns: 2_000,
                event_count: 0,
                row_ready: false,
            }
        );
        assert!(events.iter().all(Option::is_none));
    }

    fn active_hold_for_autoplay(end_time_ns: SongTimeNs) -> ActiveHold {
        ActiveHold {
            note_index: 7,
            start_time_ns: 1_000,
            end_time_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 1_000,
        }
    }

    fn active_roll_for_autoplay(end_time_ns: SongTimeNs) -> ActiveHold {
        ActiveHold {
            note_type: NoteType::Roll,
            ..active_hold_for_autoplay(end_time_ns)
        }
    }

    #[test]
    fn collect_active_autoplay_roll_columns_filters_live_rolls() {
        let mut let_go_roll = active_roll_for_autoplay(2_000);
        let_go_roll.let_go = true;
        let active_holds = vec![
            Some(active_roll_for_autoplay(2_000)),
            Some(active_hold_for_autoplay(2_000)),
            Some(let_go_roll),
            Some(active_roll_for_autoplay(2_000)),
        ];
        let mut columns = [usize::MAX; MAX_COLS];

        let count = collect_active_autoplay_roll_columns(&active_holds, 4, &mut columns);

        assert_eq!(count, 2);
        assert_eq!(&columns[..count], &[0, 3]);
    }

    #[test]
    fn collect_active_autoplay_roll_columns_respects_bounds() {
        let active_holds = vec![
            Some(active_roll_for_autoplay(2_000)),
            Some(active_roll_for_autoplay(2_000)),
            Some(active_roll_for_autoplay(2_000)),
        ];
        let mut columns = [usize::MAX; 1];

        let count = collect_active_autoplay_roll_columns(&active_holds, 3, &mut columns);

        assert_eq!(count, 1);
        assert_eq!(columns, [0]);

        let mut columns = [usize::MAX; MAX_COLS];
        let count = collect_active_autoplay_roll_columns(&active_holds, 2, &mut columns);

        assert_eq!(count, 2);
        assert_eq!(&columns[..count], &[0, 1]);
    }

    #[test]
    fn autoplay_due_active_hold_waits_until_cutoff() {
        let active = active_hold_for_autoplay(2_000);

        assert_eq!(autoplay_due_active_hold_resolution(&active, 1_999), None);
    }

    #[test]
    fn autoplay_due_active_hold_succeeds_when_engaged() {
        let active = active_hold_for_autoplay(2_000);

        assert_eq!(
            autoplay_due_active_hold_resolution(&active, 2_000),
            Some(ActiveHoldResolution::Success { note_index: 7 })
        );
    }

    #[test]
    fn autoplay_due_active_hold_lets_go_when_marked_or_depleted() {
        let mut let_go = active_hold_for_autoplay(2_000);
        let_go.let_go = true;
        let mut depleted = active_hold_for_autoplay(3_000);
        depleted.life = 0.0;

        assert_eq!(
            autoplay_due_active_hold_resolution(&let_go, 2_500),
            Some(ActiveHoldResolution::LetGo {
                note_index: 7,
                time_ns: 2_000,
            })
        );
        assert_eq!(
            autoplay_due_active_hold_resolution(&depleted, 3_500),
            Some(ActiveHoldResolution::LetGo {
                note_index: 7,
                time_ns: 3_000,
            })
        );
    }

    #[test]
    fn collect_due_autoplay_active_hold_resolutions_clears_due_holds() {
        let mut let_go = active_hold_for_autoplay(3_000);
        let_go.note_index = 8;
        let_go.let_go = true;
        let mut active_holds = vec![
            Some(active_hold_for_autoplay(2_000)),
            Some(let_go),
            Some(active_hold_for_autoplay(4_000)),
        ];
        let mut events = [None; MAX_COLS];

        let update =
            collect_due_autoplay_active_hold_resolutions(&mut active_holds, 3, 3_000, &mut events);

        assert_eq!(
            update,
            ActiveHoldColumnsUpdate {
                columns_scanned: 3,
                event_count: 2,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(ActiveHoldColumnResolution {
                column: 0,
                resolution: ActiveHoldResolution::Success { note_index: 7 },
            })
        );
        assert_eq!(
            events[1],
            Some(ActiveHoldColumnResolution {
                column: 1,
                resolution: ActiveHoldResolution::LetGo {
                    note_index: 8,
                    time_ns: 3_000,
                },
            })
        );
        assert!(active_holds[0].is_none());
        assert!(active_holds[1].is_none());
        assert!(active_holds[2].is_some());
    }

    #[test]
    fn collect_due_autoplay_active_hold_resolutions_stops_when_buffer_fills() {
        let mut active_holds = vec![
            Some(active_hold_for_autoplay(2_000)),
            Some(active_hold_for_autoplay(2_500)),
        ];
        let mut events = [None; 1];

        let update =
            collect_due_autoplay_active_hold_resolutions(&mut active_holds, 2, 3_000, &mut events);

        assert_eq!(
            update,
            ActiveHoldColumnsUpdate {
                columns_scanned: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(
            events[0],
            Some(ActiveHoldColumnResolution {
                column: 0,
                resolution: ActiveHoldResolution::Success { note_index: 7 },
            })
        );
        assert!(active_holds[0].is_none());
        assert!(active_holds[1].is_some());
    }

    #[test]
    fn lane_edges_classify_press_and_release() {
        assert!(lane_press_started(true, false, true));
        assert!(!lane_press_started(true, true, true));
        assert!(lane_release_finished(false, true, false));
        assert!(!lane_release_finished(false, true, true));

        assert!(lane_edge_judges_tap(true, false));
        assert!(!lane_edge_judges_tap(true, true));
        assert!(lane_edge_judges_lift(false, true));
        assert!(!lane_edge_judges_lift(false, false));
    }

    #[test]
    fn autoplay_keeps_active_holds_pressed() {
        assert!(active_hold_counts_as_pressed(true, false));
        assert!(active_hold_counts_as_pressed(true, true));
        assert!(active_hold_counts_as_pressed(false, true));
        assert!(!active_hold_counts_as_pressed(false, false));
    }

    fn test_note(note_type: NoteType, hold: Option<HoldData>, is_fake: bool) -> Note {
        test_note_at(note_type, hold, is_fake, 0, 0.0)
    }

    fn test_note_at(
        note_type: NoteType,
        hold: Option<HoldData>,
        is_fake: bool,
        row_index: usize,
        beat: f32,
    ) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold,
            mine_result: None,
            is_fake,
            can_be_judged: true,
        }
    }

    fn test_hold() -> HoldData {
        HoldData {
            end_row_index: 48,
            end_beat: 1.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: 0,
            last_held_beat: 0.0,
        }
    }

    fn test_judgment(grade: JudgeGrade) -> Judgment {
        Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade,
            window: match grade {
                JudgeGrade::Fantastic => Some(TimingWindow::W1),
                JudgeGrade::Excellent => Some(TimingWindow::W2),
                JudgeGrade::Great => Some(TimingWindow::W3),
                JudgeGrade::Decent => Some(TimingWindow::W4),
                JudgeGrade::WayOff => Some(TimingWindow::W5),
                JudgeGrade::Miss => None,
            },
            miss_because_held: false,
        }
    }

    fn test_chart(stats: ArrowStats, mines_nonfake: u32, has_chart_attacks: bool) -> ChartData {
        ChartData {
            chart_type: String::new(),
            difficulty: String::new(),
            description: String::new(),
            chart_name: String::new(),
            meter: 0,
            step_artist: String::new(),
            music_path: None,
            short_hash: String::new(),
            stats,
            tech_counts: TechCounts::default(),
            mines_nonfake,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn test_active_hold(
        note_type: NoteType,
        is_pressed: bool,
        let_go: bool,
        life: f32,
    ) -> ActiveHold {
        ActiveHold {
            note_index: 42,
            start_time_ns: 0,
            end_time_ns: 1_000_000_000,
            note_type,
            let_go,
            is_pressed,
            life,
            last_update_time_ns: 0,
        }
    }

    #[test]
    fn score_validity_rejects_rate_below_one() {
        let chart = test_chart(ArrowStats::default(), 0, false);
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                music_rate: 0.75,
                ..ScoreValidityOptions::default()
            },
        );

        assert_eq!(reasons, vec!["music rate is below 1.0x"]);

        let normalized = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                music_rate: f32::NAN,
                ..ScoreValidityOptions::default()
            },
        );

        assert!(normalized.is_empty());
    }

    #[test]
    fn score_validity_rejects_matching_remove_masks() {
        let chart = test_chart(
            ArrowStats {
                jumps: 1,
                hands: 1,
                holds: 1,
                rolls: 1,
                lifts: 1,
                fakes: 1,
                ..ArrowStats::default()
            },
            1,
            false,
        );
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                chart_effects: ChartAttackEffects {
                    remove_mask: REMOVE_MASK_BIT_NO_HOLDS
                        | REMOVE_MASK_BIT_NO_MINES
                        | REMOVE_MASK_BIT_NO_JUMPS
                        | REMOVE_MASK_BIT_NO_HANDS
                        | REMOVE_MASK_BIT_NO_QUADS
                        | REMOVE_MASK_BIT_NO_LIFTS
                        | REMOVE_MASK_BIT_NO_FAKES
                        | REMOVE_MASK_BIT_LITTLE,
                    holds_mask: HOLDS_MASK_BIT_NO_ROLLS,
                    ..ChartAttackEffects::default()
                },
                ..ScoreValidityOptions::default()
            },
        );

        assert!(reasons.contains(&"No Holds is enabled on a chart with holds"));
        assert!(reasons.contains(&"No Mines is enabled on a chart with mines"));
        assert!(reasons.contains(&"No Jumps is enabled on a chart with jumps"));
        assert!(reasons.contains(&"No Hands is enabled on a chart with hands"));
        assert!(reasons.contains(&"No Quads is enabled on a chart with quads"));
        assert!(reasons.contains(&"No Lifts is enabled on a chart with lifts"));
        assert!(reasons.contains(&"No Fakes is enabled on a chart with fakes"));
        assert!(reasons.contains(&"No Rolls is enabled on a chart with rolls"));
        assert!(reasons.contains(&"Little is enabled"));
    }

    #[test]
    fn score_validity_rejects_insert_and_hold_masks() {
        let chart = test_chart(ArrowStats::default(), 0, false);
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                chart_effects: ChartAttackEffects {
                    insert_mask: INSERT_MASK_BIT_ECHO,
                    holds_mask: HOLDS_MASK_BIT_PLANTED
                        | HOLDS_MASK_BIT_FLOORED
                        | HOLDS_MASK_BIT_TWISTER,
                    ..ChartAttackEffects::default()
                },
                ..ScoreValidityOptions::default()
            },
        );

        assert!(reasons.contains(&"Echo is enabled"));
        assert!(reasons.contains(&"Planted is enabled"));
        assert!(reasons.contains(&"Floored is enabled"));
        assert!(reasons.contains(&"Twister is enabled"));
    }

    #[test]
    fn score_validity_rejects_attack_modes() {
        let chart_attacks = test_chart(ArrowStats::default(), 0, true);
        let no_chart_attacks = test_chart(ArrowStats::default(), 0, false);

        assert_eq!(
            score_invalid_reason_lines_for_options(
                &chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::Off,
                    ..ScoreValidityOptions::default()
                },
            ),
            vec!["AttackMode=Off is enabled on a chart with attacks"]
        );
        assert_eq!(
            score_invalid_reason_lines_for_options(
                &no_chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::Random,
                    ..ScoreValidityOptions::default()
                },
            ),
            vec!["AttackMode=Random is enabled"]
        );
        assert!(
            score_invalid_reason_lines_for_options(
                &chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::On,
                    ..ScoreValidityOptions::default()
                },
            )
            .is_empty()
        );
    }

    #[test]
    fn autosync_mode_status_lines_match_runtime_labels() {
        assert_eq!(autosync_mode_status_line(AutosyncMode::Off), None);
        assert_eq!(
            autosync_mode_status_line(AutosyncMode::Song),
            Some("AutoSync Song")
        );
        assert_eq!(
            autosync_mode_status_line(AutosyncMode::Machine),
            Some("AutoSync Machine")
        );
    }

    #[test]
    fn next_autosync_mode_cycles_and_skips_song_for_courses() {
        assert_eq!(
            next_autosync_mode(AutosyncMode::Off, false),
            AutosyncMode::Song
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Song, false),
            AutosyncMode::Machine
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Machine, false),
            AutosyncMode::Off
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Off, true),
            AutosyncMode::Machine
        );
    }

    #[test]
    fn gameplay_raw_key_plan_handles_command_keys() {
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::Restart,
                true,
                true,
                true,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::Restart
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::Restart,
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::None
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::Restart,
                true,
                true,
                true,
                true,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::Reload
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::Autosync,
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                true,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::SetAutosyncMode(AutosyncMode::Machine)
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::TimingTick,
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Assist,
                false,
            ),
            GameplayRawKeyPlan::SetTimingTickMode(GameplayTimingTickMode::Hit)
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::Autoplay,
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                true,
            ),
            GameplayRawKeyPlan::SetAutoplayEnabled(false)
        );
    }

    #[test]
    fn gameplay_raw_key_action_matches_restart_plan() {
        assert_eq!(
            gameplay_raw_key_action_for_plan(GameplayRawKeyPlan::Restart),
            RawKeyAction::Restart
        );
        assert_eq!(
            gameplay_raw_key_action_for_plan(GameplayRawKeyPlan::Reload),
            RawKeyAction::Reload
        );
        assert_eq!(
            gameplay_raw_key_action_for_plan(GameplayRawKeyPlan::SetAutoplayEnabled(true)),
            RawKeyAction::None
        );
        assert_eq!(
            gameplay_raw_key_action_for_plan(GameplayRawKeyPlan::None),
            RawKeyAction::None
        );
    }

    #[test]
    fn gameplay_raw_key_plan_handles_offset_adjust_keys() {
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease),
                false,
                false,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::ClearOffsetAdjust(GameplayOffsetAdjustKey::Decrease)
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
                true,
                true,
                false,
                true,
                AutosyncMode::Off,
                true,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::StartOffsetAdjust {
                key: GameplayOffsetAdjustKey::Increase,
                target: GameplayOffsetAdjustTarget::Global,
            }
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::StartOffsetAdjust {
                key: GameplayOffsetAdjustKey::Increase,
                target: GameplayOffsetAdjustTarget::Song,
            }
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
                true,
                true,
                false,
                false,
                AutosyncMode::Off,
                true,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::StartOffsetAdjust {
                key: GameplayOffsetAdjustKey::Increase,
                target: GameplayOffsetAdjustTarget::None,
            }
        );
        assert_eq!(
            gameplay_raw_key_plan(
                GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
                true,
                false,
                false,
                false,
                AutosyncMode::Off,
                false,
                GameplayTimingTickMode::Off,
                false,
            ),
            GameplayRawKeyPlan::None
        );
    }

    #[test]
    fn course_display_totals_copy_chart_totals() {
        let mut chart = test_chart(
            ArrowStats {
                total_steps: 123,
                ..ArrowStats::default()
            },
            0,
            false,
        );
        chart.possible_grade_points = 456;
        chart.holds_total = 7;
        chart.rolls_total = 8;
        chart.mines_total = 9;

        let totals = course_display_totals_for_chart(&chart);

        assert_eq!(totals.possible_grade_points, 456);
        assert_eq!(totals.total_steps, 123);
        assert_eq!(totals.holds_total, 7);
        assert_eq!(totals.rolls_total, 8);
        assert_eq!(totals.mines_total, 9);
    }

    #[test]
    fn course_display_totals_for_player_uses_override_or_live_totals() {
        let possible_grade_points = [100, 200];
        let total_steps = [10, 20];
        let holds_total = [1, 2];
        let rolls_total = [3, 4];
        let mines_total = [5, 6];

        let live = course_display_totals_for_player(
            None,
            &possible_grade_points,
            &total_steps,
            &holds_total,
            &rolls_total,
            &mines_total,
            1,
        );
        assert_eq!(live.possible_grade_points, 200);
        assert_eq!(live.total_steps, 20);
        assert_eq!(live.holds_total, 2);
        assert_eq!(live.rolls_total, 4);
        assert_eq!(live.mines_total, 6);

        let overrides = [
            CourseDisplayTotals::default(),
            CourseDisplayTotals {
                possible_grade_points: 900,
                total_steps: 90,
                holds_total: 9,
                rolls_total: 8,
                mines_total: 7,
            },
        ];
        let overridden = course_display_totals_for_player(
            Some(&overrides),
            &possible_grade_points,
            &total_steps,
            &holds_total,
            &rolls_total,
            &mines_total,
            1,
        );
        assert_eq!(overridden.possible_grade_points, 900);
        assert_eq!(overridden.total_steps, 90);
        assert_eq!(overridden.holds_total, 9);
        assert_eq!(overridden.rolls_total, 8);
        assert_eq!(overridden.mines_total, 7);

        assert_eq!(
            course_display_totals_for_player(
                Some(&overrides),
                &possible_grade_points,
                &total_steps,
                &holds_total,
                &rolls_total,
                &mines_total,
                MAX_PLAYERS,
            )
            .total_steps,
            0
        );
    }

    #[test]
    fn gameplay_chart_totals_state_returns_live_and_override_totals() {
        let state =
            GameplayChartTotalsState::new([100, 200], [10, 20], [1, 2], [3, 4], [5, 6], [7, 8]);

        let live = state.display_totals(None, 1);

        assert_eq!(live.possible_grade_points, 200);
        assert_eq!(live.total_steps, 20);
        assert_eq!(live.holds_total, 2);
        assert_eq!(live.rolls_total, 4);
        assert_eq!(live.mines_total, 6);
        assert_eq!(state.hands_total[1], 8);

        let overrides = [
            CourseDisplayTotals::default(),
            CourseDisplayTotals {
                possible_grade_points: 900,
                total_steps: 90,
                holds_total: 9,
                rolls_total: 8,
                mines_total: 7,
            },
        ];
        let overridden = state.display_totals(Some(&overrides), 1);

        assert_eq!(overridden.possible_grade_points, 900);
        assert_eq!(overridden.total_steps, 90);
        assert_eq!(overridden.holds_total, 9);
        assert_eq!(overridden.rolls_total, 8);
        assert_eq!(overridden.mines_total, 7);
    }

    #[test]
    fn gameplay_visible_timing_state_tracks_player_time_and_delay() {
        let mut state = GameplayVisibleTimingState {
            global_visual_delay_seconds: 0.010,
            player_visual_delay_seconds: [0.005, -0.002],
            ..GameplayVisibleTimingState::default()
        };

        assert_near(state.visual_delay_seconds(0), 0.015);
        assert_near(state.visual_delay_seconds(1), 0.008);
        assert_near(state.visual_delay_seconds(MAX_PLAYERS), 0.010);

        state.set_player_time(1, 123, 0.123, 4.5);

        assert_eq!(state.current_music_time_ns[1], 123);
        assert_near(state.current_music_time[1], 0.123);
        assert_near(state.current_beat[1], 4.5);

        state.set_player_time(MAX_PLAYERS, 999, 9.99, 99.0);

        assert_eq!(state.current_music_time_ns[0], 0);
    }

    #[test]
    fn course_display_carry_merges_stage_counts() {
        let carry = course_display_carry_for_stage(
            CourseDisplayCarry {
                life: 0.25,
                judgment_counts: [1, 2, 3, 4, 5, 6],
                scoring_counts: [6, 5, 4, 3, 2, 1],
                full_combo_grade: Some(JudgeGrade::Great),
                current_combo_window_counts: WindowCounts {
                    w1: 9,
                    ..WindowCounts::default()
                },
                window_counts: WindowCounts {
                    w1: 5,
                    ..WindowCounts::default()
                },
                window_counts_10ms_blue: WindowCounts {
                    w0: 6,
                    ..WindowCounts::default()
                },
                window_counts_display_blue: WindowCounts {
                    w2: 7,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 8,
                holds_let_go_for_score: 9,
                rolls_held_for_score: 10,
                rolls_let_go_for_score: 11,
                mines_hit_for_score: 12,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayStage {
                life: 1.25,
                judgment_counts: [10, 20, 30, 40, 50, 60],
                scoring_counts: [60, 50, 40, 30, 20, 10],
                full_combo_grade: Some(JudgeGrade::Excellent),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_window_counts: WindowCounts {
                    w0: 3,
                    ..WindowCounts::default()
                },
                combo: 0,
                window_counts: WindowCounts {
                    w1: 13,
                    ..WindowCounts::default()
                },
                window_counts_10ms_blue: WindowCounts {
                    w0: 14,
                    ..WindowCounts::default()
                },
                window_counts_display_blue: WindowCounts {
                    w2: 15,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 16,
                holds_let_go_for_score: 17,
                rolls_held_for_score: 18,
                rolls_let_go_for_score: 19,
                mines_hit_for_score: 20,
                ..CourseDisplayStage::default()
            },
        );

        assert_eq!(carry.life, 1.0);
        assert_eq!(carry.judgment_counts, [11, 22, 33, 44, 55, 66]);
        assert_eq!(carry.scoring_counts, [66, 55, 44, 33, 22, 11]);
        assert_eq!(carry.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(carry.current_combo_grade, Some(JudgeGrade::Fantastic));
        assert_eq!(carry.current_combo_window_counts.w0, 0);
        assert_eq!(carry.current_combo_window_counts.w1, 0);
        assert_eq!(carry.current_combo_window_counts.miss, 0);
        assert_eq!(carry.window_counts.w1, 18);
        assert_eq!(carry.window_counts_10ms_blue.w0, 20);
        assert_eq!(carry.window_counts_display_blue.w2, 22);
        assert_eq!(carry.holds_held_for_score, 24);
        assert_eq!(carry.holds_let_go_for_score, 26);
        assert_eq!(carry.rolls_held_for_score, 28);
        assert_eq!(carry.rolls_let_go_for_score, 30);
        assert_eq!(carry.mines_hit_for_score, 32);
    }

    #[test]
    fn course_display_carry_clears_full_combo_after_break() {
        let carry = course_display_carry_for_stage(
            CourseDisplayCarry {
                full_combo_grade: Some(JudgeGrade::Fantastic),
                ..CourseDisplayCarry::default()
            },
            CourseDisplayStage {
                combo: 4,
                current_combo_window_counts: WindowCounts {
                    w1: 4,
                    ..WindowCounts::default()
                },
                first_fc_attempt_broken: true,
                ..CourseDisplayStage::default()
            },
        );

        assert!(carry.full_combo_grade.is_none());
        assert!(carry.first_fc_attempt_broken);
        assert_eq!(carry.current_combo_window_counts.w1, 4);
    }

    #[test]
    fn course_display_carry_for_stages_keeps_players_separate() {
        let previous = [
            CourseDisplayCarry {
                judgment_counts: [1, 0, 0, 0, 0, 0],
                life: 0.25,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayCarry {
                judgment_counts: [2, 0, 0, 0, 0, 0],
                life: 0.5,
                ..CourseDisplayCarry::default()
            },
        ];
        let stages = [
            CourseDisplayStage {
                life: 0.75,
                judgment_counts: [10, 0, 0, 0, 0, 0],
                ..CourseDisplayStage::default()
            },
            CourseDisplayStage {
                life: 0.9,
                judgment_counts: [20, 0, 0, 0, 0, 0],
                ..CourseDisplayStage::default()
            },
        ];

        let carry = course_display_carry_for_stages(Some(&previous), stages, 2);

        assert_eq!(carry[0].life, 0.75);
        assert_eq!(carry[0].judgment_counts[0], 11);
        assert_eq!(carry[1].life, 0.9);
        assert_eq!(carry[1].judgment_counts[0], 22);
    }

    #[test]
    fn course_display_carry_for_stages_duplicates_single_player() {
        let stages = [
            CourseDisplayStage {
                life: 0.6,
                judgment_counts: [3, 0, 0, 0, 0, 0],
                holds_held: 4,
                ..CourseDisplayStage::default()
            },
            CourseDisplayStage {
                life: 0.2,
                judgment_counts: [99, 0, 0, 0, 0, 0],
                holds_held: 99,
                ..CourseDisplayStage::default()
            },
        ];

        let carry = course_display_carry_for_stages(None, stages, 1);

        assert_eq!(carry[0].life, 0.6);
        assert_eq!(carry[0].judgment_counts[0], 3);
        assert_eq!(carry[0].holds_held, 4);
        assert_eq!(carry[1].life, carry[0].life);
        assert_eq!(carry[1].judgment_counts, carry[0].judgment_counts);
        assert_eq!(carry[1].holds_held, carry[0].holds_held);
    }

    #[test]
    fn course_display_carry_for_player_uses_defaults_and_bounds() {
        let carry = [
            CourseDisplayCarry {
                life: 0.4,
                judgment_counts: [1, 0, 0, 0, 0, 0],
                ..CourseDisplayCarry::default()
            },
            CourseDisplayCarry {
                life: 0.8,
                judgment_counts: [2, 0, 0, 0, 0, 0],
                ..CourseDisplayCarry::default()
            },
        ];

        assert_eq!(course_display_carry_for_player(None, 0).life, 0.0);
        assert_eq!(
            course_display_carry_for_player(Some(&carry), 1).judgment_counts[0],
            2
        );
        assert_eq!(
            course_display_carry_for_player(Some(&carry), MAX_PLAYERS).life,
            0.0
        );
    }

    #[test]
    fn course_display_state_returns_carry_totals_and_timing() {
        let carry = [
            CourseDisplayCarry {
                life: 0.25,
                judgment_counts: [7, 0, 0, 0, 0, 0],
                ..CourseDisplayCarry::default()
            },
            CourseDisplayCarry {
                life: 0.75,
                judgment_counts: [11, 0, 0, 0, 0, 0],
                ..CourseDisplayCarry::default()
            },
        ];
        let totals = [
            CourseDisplayTotals {
                possible_grade_points: 100,
                total_steps: 25,
                ..CourseDisplayTotals::default()
            },
            CourseDisplayTotals {
                possible_grade_points: 200,
                total_steps: 50,
                ..CourseDisplayTotals::default()
            },
        ];
        let timing = CourseDisplayTiming {
            elapsed_seconds: 12.5,
            total_seconds: 90.0,
        };

        let empty = GameplayCourseDisplayState::default();
        assert!(!empty.is_course_stage());
        assert!(empty.totals().is_none());
        assert!(empty.timing().is_none());
        assert_eq!(empty.carry_for_player(0).life, 0.0);

        let state = GameplayCourseDisplayState::new(Some(carry), Some(totals), Some(timing));

        assert!(state.is_course_stage());
        assert_eq!(state.carry_for_player(1).judgment_counts[0], 11);
        assert_eq!(state.carry_for_player(MAX_PLAYERS).life, 0.0);
        assert_eq!(state.totals().unwrap()[1].possible_grade_points, 200);
        assert_eq!(state.totals().unwrap()[1].total_steps, 50);
        assert_near(state.timing().unwrap().elapsed_seconds, 12.5);
        assert_near(state.timing().unwrap().total_seconds, 90.0);
    }

    #[test]
    fn course_display_timing_sums_sanitized_stage_music_lengths() {
        let stages = [15.0, f32::NAN, -2.0, 45.0];
        let timing = course_display_timing_for_stages(&stages, 3, |seconds| *seconds);

        assert_near(timing.elapsed_seconds, 15.0);
        assert_near(timing.total_seconds, 60.0);
    }

    #[test]
    fn note_range_state_returns_bounds_and_clears_for_benchmark() {
        let mut ranges = [(0usize, 0usize); MAX_PLAYERS];
        ranges[0] = (2, 5);
        ranges[1] = (8, 13);
        let mut state = GameplayNoteRangeState::new(ranges);

        assert_eq!(state.range(0), (2, 5));
        assert_eq!(state.range(1), (8, 13));
        assert_eq!(state.range(MAX_PLAYERS), (0, 0));
        assert_eq!(state.ranges(), &ranges);

        state.clear_for_benchmark();
        assert_eq!(state.ranges(), &[(0, 0); MAX_PLAYERS]);
    }

    #[test]
    fn notefield_motion_state_sets_samples_and_bounds() {
        let mut state = GameplayNotefieldMotionState::default();

        assert_near(state.field_zoom(0), 1.0);
        assert_near(state.column_scroll_dir(0), 1.0);
        assert_eq!(state.scroll_speed(0), ScrollSpeedSetting::default());
        assert_near(state.scroll_reference_bpm(), 0.0);
        assert_near(state.field_zoom(MAX_PLAYERS), 1.0);
        assert_near(state.column_scroll_dir(MAX_COLS), 1.0);
        assert!(!state.reverse_scroll(0));
        assert_eq!(state.column_scroll_dir_count(), MAX_COLS);

        state.set_reverse_scroll(1, true);
        state.set_column_scroll_dir(2, -1.0);
        state.set_player_motion(1, 420.0, 0.75, 1000.0, 240.0, 2.5);
        state.set_reverse_scroll(MAX_PLAYERS, true);
        state.set_column_scroll_dir(MAX_COLS, -1.0);

        assert!(state.reverse_scroll(1));
        assert_near(state.column_scroll_dir(2), -1.0);
        assert_near(state.scroll_pixels_per_second(1), 420.0);
        assert_near(state.field_zoom(1), 0.75);
        assert_near(state.draw_distance_before_targets(1), 1000.0);
        assert_near(state.draw_distance_after_targets(1), 240.0);
        assert_near(state.scroll_travel_time(1), 2.5);
        assert!(!state.reverse_scroll(MAX_PLAYERS));
        assert_near(state.column_scroll_dir(MAX_COLS), 1.0);

        let configured = GameplayNotefieldMotionState::new(
            [
                ScrollSpeedSetting::CMod(620.0),
                ScrollSpeedSetting::XMod(2.0),
            ],
            175.0,
            [0.5, 0.75],
            [420.0, 840.0],
            [1.25, 2.5],
            [900.0, 1000.0],
            [120.0, 240.0],
            [false, true],
            {
                let mut dirs = [1.0; MAX_COLS];
                dirs[2] = -1.0;
                dirs
            },
        );

        assert_eq!(configured.scroll_speed(0), ScrollSpeedSetting::CMod(620.0));
        assert_near(configured.scroll_reference_bpm(), 175.0);
        assert!(configured.reverse_scroll(1));
        assert_near(configured.column_scroll_dir(2), -1.0);
        assert_near(configured.scroll_pixels_per_second(1), 840.0);
        assert_near(configured.field_zoom(1), 0.75);
        assert_near(configured.draw_distance_before_targets(1), 1000.0);
        assert_near(configured.draw_distance_after_targets(1), 240.0);
        assert_near(configured.scroll_travel_time(1), 2.5);
        assert!(!configured.reverse_scroll(MAX_PLAYERS));
        assert_near(configured.column_scroll_dir(MAX_COLS), 1.0);
    }

    #[test]
    fn course_life_carry_restores_finite_lifemeter_only() {
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: 0.32,
                    ..CourseDisplayCarry::default()
                }),
            ),
            0.32,
        );
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: 1.25,
                    ..CourseDisplayCarry::default()
                }),
            ),
            1.0,
        );
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: f32::NAN,
                    ..CourseDisplayCarry::default()
                }),
            ),
            0.5,
        );
        assert_near(course_life_after_carry(0.5, None), 0.5);
    }

    #[test]
    fn course_combo_carry_restores_combo_color_state() {
        let carry = CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_window_counts: WindowCounts {
                w0: 7,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };
        let mut state = CourseComboCarryState::default();

        apply_course_combo_carry_state(&mut state, true, false, 37, Some(carry));

        assert_eq!(state.combo, 37);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(state.current_combo_window_counts.w0, 7);
        assert!(!state.first_fc_attempt_broken);
    }

    #[test]
    fn course_combo_carry_marks_broken_attempts_without_combo() {
        let carry = CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_window_counts: WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };
        let mut state = CourseComboCarryState::default();

        apply_course_combo_carry_state(&mut state, true, false, 0, Some(carry));

        assert_eq!(state.combo, 0);
        assert!(state.full_combo_grade.is_none());
        assert!(state.current_combo_grade.is_none());
        assert_eq!(state.current_combo_window_counts.w1, 0);
        assert!(state.first_fc_attempt_broken);
    }

    #[test]
    fn course_combo_carry_does_not_restore_in_replay_mode() {
        let mut state = CourseComboCarryState {
            combo: 9,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..CourseComboCarryState::default()
        };

        apply_course_combo_carry_state(
            &mut state,
            true,
            true,
            37,
            Some(CourseDisplayCarry::default()),
        );

        assert_eq!(state.combo, 9);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(state.first_fc_attempt_broken);
    }

    #[test]
    fn display_window_counts_mode_selects_cached_or_custom_splits() {
        assert_eq!(
            display_window_counts_mode(None, 10.0),
            DisplayWindowCountsMode::Canonical
        );
        assert_eq!(
            display_window_counts_mode(Some(FA_PLUS_W0_MS), 10.0),
            DisplayWindowCountsMode::Canonical
        );
        assert_eq!(
            display_window_counts_mode(Some(FA_PLUS_W010_MS), 15.0),
            DisplayWindowCountsMode::TenMsBlue
        );
        assert_eq!(
            display_window_counts_mode(Some(12.5), 12.5),
            DisplayWindowCountsMode::DisplayBlue
        );
        assert!(matches!(
            display_window_counts_mode(Some(12.5), 10.0),
            DisplayWindowCountsMode::CustomBlue { split_ms }
                if (split_ms - 12.5).abs() <= 0.000_1
        ));
    }

    #[test]
    fn display_window_counts_current_and_carry_use_selected_bucket() {
        let sources = DisplayWindowCountsSources {
            canonical: WindowCounts {
                w1: 1,
                ..WindowCounts::default()
            },
            ten_ms_blue: WindowCounts {
                w0: 2,
                ..WindowCounts::default()
            },
            display_blue: WindowCounts {
                w2: 3,
                ..WindowCounts::default()
            },
        };
        let carry = CourseDisplayCarry {
            window_counts: WindowCounts {
                w1: 10,
                ..WindowCounts::default()
            },
            window_counts_10ms_blue: WindowCounts {
                w0: 20,
                ..WindowCounts::default()
            },
            window_counts_display_blue: WindowCounts {
                w2: 30,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };

        let canonical = display_window_counts_with_carry(
            display_window_counts_current(sources, DisplayWindowCountsMode::Canonical).unwrap(),
            carry,
            DisplayWindowCountsMode::Canonical,
        );
        let ten_ms = display_window_counts_with_carry(
            display_window_counts_current(sources, DisplayWindowCountsMode::TenMsBlue).unwrap(),
            carry,
            DisplayWindowCountsMode::TenMsBlue,
        );
        let custom = display_window_counts_with_carry(
            WindowCounts {
                miss: 4,
                ..WindowCounts::default()
            },
            carry,
            DisplayWindowCountsMode::CustomBlue { split_ms: 12.5 },
        );

        assert_eq!(canonical.w1, 11);
        assert_eq!(ten_ms.w0, 22);
        assert!(
            display_window_counts_current(
                sources,
                DisplayWindowCountsMode::CustomBlue { split_ms: 12.5 },
            )
            .is_none()
        );
        assert_eq!(custom.miss, 4);
        assert_eq!(custom.w2, 30);
    }

    #[test]
    fn display_window_counts_for_notes_uses_cached_and_custom_counts() {
        let sources = DisplayWindowCountsSources {
            canonical: WindowCounts {
                w1: 1,
                ..WindowCounts::default()
            },
            ten_ms_blue: WindowCounts {
                w0: 2,
                ..WindowCounts::default()
            },
            display_blue: WindowCounts {
                w2: 3,
                ..WindowCounts::default()
            },
        };
        let carry = CourseDisplayCarry {
            window_counts: WindowCounts {
                w1: 10,
                ..WindowCounts::default()
            },
            window_counts_display_blue: WindowCounts {
                w2: 30,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };

        let canonical = display_window_counts_for_notes(sources, carry, &[], None, 20.0);
        assert_eq!(canonical.w1, 11);

        let mut note = test_note(NoteType::Tap, None, false);
        note.result = Some(Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 11.0,
            time_error_music_ns: 11_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        });

        let custom = display_window_counts_for_notes(sources, carry, &[note], Some(12.5), 20.0);
        assert_eq!(custom.w0, 1);
        assert_eq!(custom.w2, 30);
    }

    #[test]
    fn record_display_window_counts_updates_all_cached_buckets() {
        let judgment = Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 16.0,
            time_error_music_ns: 16_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        let mut canonical = WindowCounts::default();
        let mut ten_ms_blue = WindowCounts::default();
        let mut display_blue = WindowCounts::default();

        record_display_window_counts_for_judgment(
            &mut canonical,
            &mut ten_ms_blue,
            &mut display_blue,
            &judgment,
            20.0,
        );

        assert_eq!(canonical.w1, 1);
        assert_eq!(ten_ms_blue.w1, 1);
        assert_eq!(display_blue.w0, 1);
    }

    #[test]
    fn window_counts_state_records_sets_and_resets() {
        let judgment = Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 16.0,
            time_error_music_ns: 16_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        let mut state = GameplayWindowCountsState::default();

        state.record_judgment(0, &judgment, 20.0);

        assert_eq!(state.canonical(0).w1, 1);
        assert_eq!(state.ten_ms_blue(0).w1, 1);
        assert_eq!(state.display_blue(0).w0, 1);
        assert_eq!(state.sources(MAX_PLAYERS).canonical.w1, 0);

        state.set_player_for_benchmark(
            1,
            WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            WindowCounts {
                w0: 4,
                ..WindowCounts::default()
            },
            WindowCounts {
                w2: 5,
                ..WindowCounts::default()
            },
        );
        let sources = state.sources(1);
        assert_eq!(sources.canonical.w1, 3);
        assert_eq!(sources.ten_ms_blue.w0, 4);
        assert_eq!(sources.display_blue.w2, 5);

        state.reset();

        assert_eq!(state.canonical(0).w1, 0);
        assert_eq!(state.canonical(1).w1, 0);
        assert_eq!(state.ten_ms_blue(1).w0, 0);
        assert_eq!(state.display_blue(1).w2, 0);
    }

    #[test]
    fn record_combo_window_count_uses_canonical_blue_window() {
        let judgment = Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 16.0,
            time_error_music_ns: 16_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        let mut counts = WindowCounts::default();

        record_combo_window_count_for_judgment(&mut counts, &judgment);

        assert_eq!(counts.w1, 1);
        assert_eq!(counts.w0, 0);
    }

    #[test]
    fn display_judgment_count_combines_stage_and_course_carry() {
        let carry = CourseDisplayCarry {
            judgment_counts: [10, 20, 30, 40, 50, 60],
            ..CourseDisplayCarry::default()
        };

        assert_eq!(
            display_judgment_count_for_grade([1, 2, 3, 4, 5, 6], carry, JudgeGrade::Great),
            33
        );
    }

    #[test]
    fn target_score_fixed_grades_map_to_percentages() {
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::CMinus),
            Some(50.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::S),
            Some(89.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::SPlus),
            Some(92.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::MachineBest),
            None
        );
    }

    #[test]
    fn target_score_resolves_best_score_settings() {
        assert_eq!(
            resolve_target_score_percent(GameplayTargetScoreSetting::MachineBest, Some(91.0), None),
            91.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::MachineBest,
                Some(91.0),
                Some(94.0),
            ),
            94.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::PersonalBest,
                Some(91.0),
                Some(94.0),
            ),
            91.0
        );
    }

    #[test]
    fn target_score_resolution_defaults_to_s_percent() {
        assert_eq!(
            resolve_target_score_percent(GameplayTargetScoreSetting::MachineBest, None, None),
            89.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::PersonalBest,
                None,
                Some(94.0)
            ),
            89.0
        );
    }

    #[test]
    fn mini_indicator_mode_for_options_uses_requested_mode_first() {
        let options = GameplayMiniIndicatorOptions {
            requested_mode: GameplayMiniIndicatorMode::RivalScoring,
            subtractive_scoring: true,
            pacemaker: true,
            ..GameplayMiniIndicatorOptions::default()
        };

        assert_eq!(
            mini_indicator_mode_for_options(options),
            GameplayMiniIndicatorMode::RivalScoring
        );
    }

    #[test]
    fn mini_indicator_mode_for_options_falls_back_to_scoring_flags() {
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions {
                subtractive_scoring: true,
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }),
            GameplayMiniIndicatorMode::SubtractiveScoring
        );
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions {
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }),
            GameplayMiniIndicatorMode::Pacemaker
        );
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions::default()),
            GameplayMiniIndicatorMode::None
        );
    }

    #[test]
    fn mini_indicator_needs_stream_data_for_measure_or_mode() {
        assert!(!mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions::default()
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                measure_counter_enabled: true,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                requested_mode: GameplayMiniIndicatorMode::StreamProg,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
    }

    #[test]
    fn mini_indicator_runtime_state_returns_values_and_clears_segments() {
        let mut segments: [Vec<StreamSegment>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        segments[1].push(StreamSegment {
            start: 2,
            end: 5,
            is_break: false,
        });
        let mut state = GameplayMiniIndicatorRuntimeState::new(
            segments,
            [0.0, 3.5],
            [89.0, 94.25],
            [0.0, 92.5],
        );

        assert_eq!(state.stream_segments(1).len(), 1);
        assert_eq!(state.total_stream_measures(1), 3.5);
        assert_eq!(state.target_score_percent(1), 94.25);
        assert_eq!(state.rival_score_percent(1), 92.5);

        assert!(state.stream_segments(MAX_PLAYERS).is_empty());
        assert_eq!(state.total_stream_measures(MAX_PLAYERS), 0.0);
        assert_eq!(state.target_score_percent(MAX_PLAYERS), 89.0);
        assert_eq!(state.rival_score_percent(MAX_PLAYERS), 0.0);

        state.clear_stream_segments();

        assert!(state.stream_segments(1).is_empty());
        assert_eq!(state.total_stream_measures(1), 3.5);
    }

    #[test]
    fn ex_score_data_combines_live_inputs_with_course_carry() {
        let data = ex_score_data_from_display_inputs(
            ExScoreInputs {
                counts: WindowCounts {
                    w1: 3,
                    ..WindowCounts::default()
                },
                counts_10ms: WindowCounts {
                    w0: 2,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 4,
                holds_let_go_for_score: 1,
                rolls_held_for_score: 5,
                rolls_let_go_for_score: 2,
                mines_hit_for_score: 6,
            },
            CourseDisplayCarry {
                holds_held_for_score: 7,
                holds_let_go_for_score: 3,
                rolls_held_for_score: 8,
                rolls_let_go_for_score: 4,
                mines_hit_for_score: 9,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayTotals {
                total_steps: 10,
                holds_total: 11,
                rolls_total: 12,
                mines_total: 13,
                ..CourseDisplayTotals::default()
            },
        );

        assert_eq!(data.counts.w1, 3);
        assert_eq!(data.counts_10ms.w0, 2);
        assert_eq!(data.holds_held, 11);
        assert_eq!(data.holds_resolved, 15);
        assert_eq!(data.rolls_held, 13);
        assert_eq!(data.rolls_resolved, 19);
        assert_eq!(data.mines_hit, 15);
        assert_eq!(data.total_steps, 10);
        assert_eq!(data.holds_total, 11);
        assert_eq!(data.rolls_total, 12);
        assert_eq!(data.mines_total, 13);
    }

    #[test]
    fn ex_score_inputs_from_display_copies_stage_counters() {
        let inputs = ex_score_inputs_from_display(
            WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            WindowCounts {
                w0: 2,
                ..WindowCounts::default()
            },
            ItgScoreStage {
                scoring_counts: [1, 2, 3, 4, 5, 6],
                holds_held_for_score: 7,
                holds_let_go_for_score: 8,
                rolls_held_for_score: 9,
                rolls_let_go_for_score: 10,
                mines_hit_for_score: 11,
            },
        );

        assert_eq!(inputs.counts.w1, 3);
        assert_eq!(inputs.counts_10ms.w0, 2);
        assert_eq!(inputs.holds_held_for_score, 7);
        assert_eq!(inputs.holds_let_go_for_score, 8);
        assert_eq!(inputs.rolls_held_for_score, 9);
        assert_eq!(inputs.rolls_let_go_for_score, 10);
        assert_eq!(inputs.mines_hit_for_score, 11);
    }

    #[test]
    fn effective_ex_score_inputs_prefers_failed_snapshot() {
        let live = ExScoreInputs {
            counts: WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            mines_hit_for_score: 4,
            ..ExScoreInputs::default()
        };
        let failed = ExScoreInputs {
            counts: WindowCounts {
                w2: 7,
                ..WindowCounts::default()
            },
            mines_hit_for_score: 8,
            ..ExScoreInputs::default()
        };

        let selected_live = effective_ex_score_inputs(live, None);
        let selected_failed = effective_ex_score_inputs(live, Some(failed));

        assert_eq!(selected_live.counts.w1, 3);
        assert_eq!(selected_live.mines_hit_for_score, 4);
        assert_eq!(selected_failed.counts.w2, 7);
        assert_eq!(selected_failed.mines_hit_for_score, 8);
    }

    #[test]
    fn failed_ex_score_capture_requires_fail_time() {
        let mut snapshot = None;
        let live = ExScoreInputs {
            mines_hit_for_score: 3,
            ..ExScoreInputs::default()
        };

        assert!(!capture_failed_ex_score_inputs(&mut snapshot, None, live));
        assert!(snapshot.is_none());
    }

    #[test]
    fn failed_ex_score_capture_records_first_snapshot_only() {
        let mut snapshot = None;
        let first = ExScoreInputs {
            holds_held_for_score: 2,
            ..ExScoreInputs::default()
        };
        let second = ExScoreInputs {
            holds_held_for_score: 9,
            ..ExScoreInputs::default()
        };

        assert!(capture_failed_ex_score_inputs(
            &mut snapshot,
            Some(10.0),
            first
        ));
        assert!(!capture_failed_ex_score_inputs(
            &mut snapshot,
            Some(12.0),
            second
        ));
        assert_eq!(snapshot.unwrap().holds_held_for_score, 2);
    }

    #[test]
    fn itg_score_inputs_combine_stage_and_course_carry() {
        let inputs = itg_score_inputs_from_display(
            ItgScoreStage {
                scoring_counts: [1, 2, 3, 4, 5, 6],
                holds_held_for_score: 7,
                holds_let_go_for_score: 8,
                rolls_held_for_score: 9,
                rolls_let_go_for_score: 10,
                mines_hit_for_score: 11,
            },
            CourseDisplayCarry {
                scoring_counts: [10, 20, 30, 40, 50, 60],
                holds_held_for_score: 12,
                holds_let_go_for_score: 13,
                rolls_held_for_score: 14,
                rolls_let_go_for_score: 15,
                mines_hit_for_score: 16,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayTotals {
                possible_grade_points: 500,
                ..CourseDisplayTotals::default()
            },
        );

        assert_eq!(inputs.scoring_counts, [11, 22, 33, 44, 55, 66]);
        assert_eq!(inputs.holds_held_for_score, 19);
        assert_eq!(inputs.holds_resolved_for_score, 40);
        assert_eq!(inputs.rolls_held_for_score, 23);
        assert_eq!(inputs.rolls_resolved_for_score, 48);
        assert_eq!(inputs.mines_hit_for_score, 27);
        assert_eq!(inputs.possible_grade_points, 500);
    }

    #[test]
    fn itg_score_percent_helpers_preserve_display_units() {
        let inputs = ItgScoreInputs {
            scoring_counts: [1, 0, 0, 0, 0, 0],
            possible_grade_points: 10,
            ..ItgScoreInputs::default()
        };

        assert_eq!(itg_score_percent_from_inputs(inputs), 0.5);
        assert_eq!(predictive_itg_score_percent_from_inputs(inputs), 100.0);
    }

    #[test]
    fn score_display_mode_helpers_select_normal_or_predictive_percent() {
        let itg_inputs = ItgScoreInputs {
            scoring_counts: [1, 0, 0, 0, 0, 0],
            possible_grade_points: 10,
            ..ItgScoreInputs::default()
        };
        assert_eq!(
            display_itg_score_percent_for_mode(itg_inputs, GameplayScoreDisplayMode::Normal),
            50.0
        );
        assert_eq!(
            display_itg_score_percent_for_mode(itg_inputs, GameplayScoreDisplayMode::Predictive),
            100.0
        );

        let ex_score = judgment::ExScoreData {
            counts: WindowCounts {
                w0: 1,
                w1: 1,
                w2: 1,
                w3: 1,
                miss: 1,
                ..WindowCounts::default()
            },
            counts_10ms: WindowCounts {
                w0: 1,
                ..WindowCounts::default()
            },
            holds_held: 1,
            holds_resolved: 2,
            rolls_held: 1,
            rolls_resolved: 2,
            mines_hit: 1,
            total_steps: 6,
            holds_total: 2,
            rolls_total: 2,
            mines_total: 2,
        };

        assert_eq!(
            display_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Normal),
            judgment::ex_score_percent(&ex_score)
        );
        assert_eq!(
            display_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Predictive),
            judgment::predictive_ex_score_percents(&ex_score).0
        );
        assert_eq!(
            display_hard_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Normal),
            judgment::hard_ex_score_percent(&ex_score)
        );
        assert_eq!(
            display_hard_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Predictive),
            judgment::predictive_hard_ex_score_percents(&ex_score).0
        );
    }

    fn dense_note_data(measures: usize, rows_per_measure: usize, lanes: usize) -> Vec<u8> {
        let mut data = Vec::new();
        let row = match lanes {
            8 => b"10000000\n".as_slice(),
            _ => b"1000\n".as_slice(),
        };
        for measure in 0..measures {
            for _ in 0..rows_per_measure {
                data.extend_from_slice(row);
            }
            data.extend_from_slice(if measure + 1 == measures { b";" } else { b"," });
            data.push(b'\n');
        }
        data
    }

    #[test]
    fn stream_segments_for_note_data_uses_chart_note_bytes() {
        let data = dense_note_data(8, 32, 4);
        let (segments, total_stream, total_break) = stream_segments_for_note_data(&data, 4, true);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 8);
        assert!(!segments[0].is_break);
        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn stream_segments_for_note_data_supports_double_charts() {
        let data = dense_note_data(3, 16, 8);
        let (segments, total_stream, total_break) = stream_segments_for_note_data(&data, 8, false);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 3);
        assert!(!segments[0].is_break);
        assert_eq!(total_stream, 3.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn zmod_fail_stream_progress_reports_measure_within_stream() {
        let data = dense_note_data(4, 16, 4);

        assert_eq!(
            zmod_fail_stream_progress_for_note_data(&data, 4, 9.0),
            Some((3, 4))
        );
    }

    #[test]
    fn zmod_fail_stream_progress_requires_sixteenth_density() {
        let data = dense_note_data(4, 15, 4);

        assert_eq!(
            zmod_fail_stream_progress_for_note_data(&data, 4, 9.0),
            None
        );
        assert_eq!(
            zmod_fail_stream_progress_for_note_data(&data, 4, f32::NAN),
            None
        );
    }

    #[test]
    fn measure_counter_segments_use_optional_threshold() {
        let densities = [12usize, 12, 0, 16];

        assert!(measure_counter_segments_for_densities(&densities, None).is_empty());

        let segments = measure_counter_segments_for_densities(&densities, Some(12));

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 2);
        assert!(!segments[0].is_break);
        assert_eq!(segments[1].start, 3);
        assert_eq!(segments[1].end, 4);
        assert!(!segments[1].is_break);
    }

    #[test]
    fn zmod_stream_totals_for_densities_uses_constant_bpm_policy() {
        let densities = [32usize; 8];
        let (segments, total_stream, total_break) =
            zmod_stream_totals_for_densities(&densities, true);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 8);
        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn hold_head_render_flags_wait_for_receptor_and_live_hold() {
        let active = test_active_hold(NoteType::Hold, true, false, 1.0);
        let exhausted = test_active_hold(NoteType::Hold, true, false, 0.0);
        let let_go = test_active_hold(NoteType::Hold, true, true, 1.0);

        assert_eq!(
            hold_head_render_flags(Some(&active), 99.99, 100.0),
            (false, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&active), 100.0, 100.0),
            (true, true)
        );
        assert_eq!(
            hold_head_render_flags(Some(&exhausted), 100.0, 100.0),
            (false, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&let_go), 100.0, 100.0),
            (false, false)
        );
        assert_eq!(hold_head_render_flags(None, 100.0, 100.0), (false, false));
    }

    #[test]
    fn hold_head_render_flags_keep_rolls_active_between_taps() {
        let released_hold = test_active_hold(NoteType::Hold, false, false, 1.0);
        let released_roll = test_active_hold(NoteType::Roll, false, false, 1.0);

        assert_eq!(
            hold_head_render_flags(Some(&released_hold), 100.0, 100.0),
            (true, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&released_roll), 100.0, 100.0),
            (true, true)
        );
    }

    #[test]
    fn hold_explosion_active_requires_receptor_and_live_hold() {
        let active = test_active_hold(NoteType::Hold, true, false, 1.0);
        let exhausted = test_active_hold(NoteType::Hold, true, false, 0.0);
        let let_go = test_active_hold(NoteType::Hold, true, true, 1.0);

        assert!(!hold_explosion_active(Some(&active), 99.99, 100.0));
        assert!(hold_explosion_active(Some(&active), 100.0, 100.0));
        assert!(!hold_explosion_active(Some(&exhausted), 100.0, 100.0));
        assert!(!hold_explosion_active(Some(&let_go), 100.0, 100.0));
        assert!(!hold_explosion_active(None, 100.0, 100.0));
    }

    #[test]
    fn replaced_active_hold_settle_time_ignores_same_note() {
        assert_eq!(replaced_active_hold_settle_time(7, 100, 7, 200), None);
    }

    #[test]
    fn replaced_active_hold_settle_time_ignores_overlapping_hold() {
        assert_eq!(replaced_active_hold_settle_time(7, 300, 8, 200), None);
    }

    #[test]
    fn replaced_active_hold_settle_time_returns_previous_end() {
        assert_eq!(replaced_active_hold_settle_time(7, 200, 8, 200), Some(200));
        assert_eq!(replaced_active_hold_settle_time(7, 150, 8, 200), Some(150));
    }

    #[test]
    fn begin_hold_life_decay_starts_decay_once() {
        let mut hold = test_hold();
        hold.life = 0.25;
        let mut active = [false; 3];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 1, 123);
        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 1, 456);

        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(hold.let_go_starting_life, 0.25);
        assert_eq!(active, [false, true, false]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn begin_hold_life_decay_clamps_starting_life() {
        let mut high = test_hold();
        high.life = 2.0;
        let mut low = test_hold();
        low.life = -0.5;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut high, &mut active, &mut indices, 0, 10);
        begin_hold_life_decay(&mut low, &mut active, &mut indices, 1, 20);

        assert_eq!(high.let_go_starting_life, MAX_HOLD_LIFE);
        assert_eq!(low.let_go_starting_life, 0.0);
    }

    #[test]
    fn begin_hold_life_decay_ignores_out_of_range_tracking_slot() {
        let mut hold = test_hold();
        let mut active = [false; 1];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 4, 99);

        assert_eq!(hold.let_go_started_at, Some(99));
        assert_eq!(active, [false]);
        assert!(indices.is_empty());
    }

    #[test]
    fn apply_hold_let_go_result_marks_and_starts_decay() {
        let mut hold = test_hold();
        hold.life = 0.4;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        assert!(apply_hold_let_go_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            1,
            123,
        ));

        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(hold.let_go_starting_life, 0.4);
        assert_eq!(active, [false, true]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn apply_hold_let_go_result_rejects_duplicate_but_allows_missing_hold() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        let mut active = [false; 1];
        let mut indices = Vec::new();

        assert!(!apply_hold_let_go_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            0,
            10,
        ));
        assert!(apply_hold_let_go_result(
            None,
            &mut active,
            &mut indices,
            0,
            10,
        ));
        assert!(indices.is_empty());
    }

    #[test]
    fn apply_hold_success_result_marks_held_and_clears_decay() {
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(10);
        hold.let_go_starting_life = 0.2;
        hold.last_held_row_index = 1;
        hold.last_held_beat = 0.25;
        let mut active = [false, true];

        assert!(apply_hold_success_result(Some(&mut hold), &mut active, 1));

        assert_eq!(hold.result, Some(HoldResult::Held));
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
        assert_eq!(hold.last_held_row_index, hold.end_row_index);
        assert_eq!(hold.last_held_beat, hold.end_beat);
        assert_eq!(active, [false, false]);
    }

    #[test]
    fn apply_hold_success_result_rejects_duplicate_but_allows_missing_hold() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::Held);
        let mut active = [true];

        assert!(!apply_hold_success_result(Some(&mut hold), &mut active, 0));
        assert!(apply_hold_success_result(None, &mut active, 0));
        assert_eq!(active, [true]);
    }

    #[test]
    fn hold_let_go_update_marks_hold_and_returns_stats() {
        let mut hold = test_hold();
        hold.life = 0.4;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        let update = apply_hold_let_go_update(
            Some(&mut hold),
            &mut active,
            &mut indices,
            1,
            NoteType::Roll,
            123,
            false,
            false,
        )
        .expect("let-go update");

        assert_eq!(update.result, HoldResult::LetGo);
        assert_eq!(update.stats_update.rolls_let_go_for_score, 1);
        assert!(update.stats_update.update_grade_totals);
        assert_eq!(
            update.effects,
            HoldResolutionEffects {
                show_judgment: true,
                reset_receptor_glow: true,
                trigger_hold_explosion: false,
            }
        );
        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(active, [false, true]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn hold_let_go_update_rejects_duplicate_results() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        let mut active = [false; 1];
        let mut indices = Vec::new();

        assert!(
            apply_hold_let_go_update(
                Some(&mut hold),
                &mut active,
                &mut indices,
                0,
                NoteType::Hold,
                123,
                false,
                false,
            )
            .is_none()
        );
        assert!(indices.is_empty());
    }

    #[test]
    fn hold_success_update_marks_hold_and_returns_stats() {
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(10);
        let mut active = [false, true];

        let update = apply_hold_success_update(
            Some(&mut hold),
            &mut active,
            1,
            NoteType::Hold,
            false,
            false,
        )
        .expect("success update");

        assert_eq!(update.result, HoldResult::Held);
        assert_eq!(update.stats_update.holds_held, 1);
        assert_eq!(update.stats_update.holds_held_for_score, 1);
        assert!(update.stats_update.update_grade_totals);
        assert_eq!(
            update.effects,
            HoldResolutionEffects {
                show_judgment: true,
                reset_receptor_glow: false,
                trigger_hold_explosion: true,
            }
        );
        assert_eq!(hold.result, Some(HoldResult::Held));
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(active, [false, false]);
    }

    #[test]
    fn hold_success_update_respects_scoring_blocks() {
        let mut hold = test_hold();
        let mut active = [false];

        let update =
            apply_hold_success_update(Some(&mut hold), &mut active, 0, NoteType::Hold, true, false)
                .expect("success update");

        assert_eq!(update.result, HoldResult::Held);
        assert_eq!(
            update.stats_update,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
    }

    #[test]
    fn hold_resolution_updates_grade_totals_matches_result_policy() {
        let update = HoldResultStatsUpdate {
            update_grade_totals: true,
            ..HoldResultStatsUpdate::ZERO
        };

        assert!(hold_resolution_updates_grade_totals(
            HoldResult::LetGo,
            update,
            false
        ));
        assert!(!hold_resolution_updates_grade_totals(
            HoldResult::LetGo,
            update,
            true
        ));
        assert!(hold_resolution_updates_grade_totals(
            HoldResult::Held,
            update,
            true
        ));
        assert!(!hold_resolution_updates_grade_totals(
            HoldResult::Missed,
            update,
            false
        ));
        assert!(!hold_resolution_updates_grade_totals(
            HoldResult::Held,
            HoldResultStatsUpdate::ZERO,
            false
        ));
    }

    #[test]
    fn time_based_hold_miss_result_scores_without_marking_missed() {
        let mut hold = test_hold();
        hold.life = 0.4;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        assert!(apply_time_based_hold_miss_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            1,
            123,
            JudgeGrade::Miss,
            true,
        ));

        assert_eq!(hold.result, None);
        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(hold.let_go_starting_life, 0.4);
        assert_eq!(active, [false, true]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn time_based_hold_miss_result_marks_unscored_hold_missed() {
        let mut hold = test_hold();
        hold.life = 0.5;
        let mut active = [false; 1];
        let mut indices = Vec::new();

        assert!(apply_time_based_hold_miss_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            0,
            456,
            JudgeGrade::Miss,
            false,
        ));

        assert_eq!(hold.result, Some(HoldResult::Missed));
        assert_eq!(hold.let_go_started_at, Some(456));
        assert_eq!(hold.let_go_starting_life, 0.5);
        assert_eq!(active, [true]);
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn time_based_hold_miss_result_ignores_non_miss_held_or_missing_hold() {
        let mut active = [false; 1];
        let mut indices = Vec::new();

        assert!(!apply_time_based_hold_miss_result(
            None,
            &mut active,
            &mut indices,
            0,
            1,
            JudgeGrade::Miss,
            false,
        ));

        let mut great = test_hold();
        assert!(!apply_time_based_hold_miss_result(
            Some(&mut great),
            &mut active,
            &mut indices,
            0,
            1,
            JudgeGrade::Great,
            false,
        ));

        let mut held = test_hold();
        held.result = Some(HoldResult::Held);
        assert!(!apply_time_based_hold_miss_result(
            Some(&mut held),
            &mut active,
            &mut indices,
            0,
            1,
            JudgeGrade::Miss,
            false,
        ));

        assert_eq!(active, [false]);
        assert!(indices.is_empty());
    }

    #[test]
    fn decay_let_go_hold_life_step_reduces_life_by_real_time() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 1.0;
        hold.let_go_started_at = Some(1_000);
        hold.let_go_starting_life = 1.0;

        let current_time_ns = 1_000 + song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_HOLD * 0.25);

        assert!(decay_let_go_hold_life_step(
            &mut hold,
            NoteType::Hold,
            current_time_ns,
            1.0,
        ));
        assert!((hold.life - 0.75).abs() < 0.000_01);
    }

    #[test]
    fn decay_let_go_hold_life_step_respects_music_rate_and_zero_crossing() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 1.0;
        hold.let_go_started_at = Some(2_000);
        hold.let_go_starting_life = 0.5;

        let current_time_ns = 2_000 + song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_HOLD * 0.5);

        assert!(decay_let_go_hold_life_step(
            &mut hold,
            NoteType::Hold,
            current_time_ns,
            2.0,
        ));
        assert!((hold.life - 0.25).abs() < 0.000_01);

        let current_time_ns = 2_000 + song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_HOLD);

        assert!(!decay_let_go_hold_life_step(
            &mut hold,
            NoteType::Hold,
            current_time_ns,
            1.0,
        ));
        assert_eq!(hold.life, 0.0);
    }

    #[test]
    fn decay_let_go_hold_life_step_stops_for_held_or_unstarted_hold() {
        let mut held = test_hold();
        held.result = Some(HoldResult::Held);
        held.life = 0.4;
        held.let_go_started_at = Some(10);
        held.let_go_starting_life = 0.4;

        assert!(!decay_let_go_hold_life_step(
            &mut held,
            NoteType::Hold,
            20,
            1.0,
        ));
        assert_eq!(held.life, 0.4);

        let mut unstarted = test_hold();
        unstarted.result = Some(HoldResult::LetGo);
        unstarted.life = 0.6;

        assert!(!decay_let_go_hold_life_step(
            &mut unstarted,
            NoteType::Roll,
            20,
            1.0,
        ));
        assert_eq!(unstarted.life, 0.6);
    }

    #[test]
    fn decay_let_go_hold_life_for_indices_keeps_active_decay() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 1.0;
        hold.let_go_started_at = Some(1_000);
        hold.let_go_starting_life = 1.0;
        let mut notes = vec![test_note_at(NoteType::Hold, Some(hold), false, 48, 1.0)];
        let mut active = [true];
        let mut indices = vec![0];

        let update = decay_let_go_hold_life_for_indices(
            &mut notes,
            &mut active,
            &mut indices,
            1_000 + song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_HOLD * 0.25),
            1.0,
        );

        assert_eq!(
            update,
            HoldLifeDecayUpdate {
                remaining_count: 1,
                removed_count: 0,
            }
        );
        assert_eq!(indices, vec![0]);
        assert_eq!(active, [true]);
        assert!(notes[0].hold.as_ref().unwrap().life > 0.0);
    }

    #[test]
    fn decay_let_go_hold_life_for_indices_removes_finished_or_stale_entries() {
        let mut finished = test_hold();
        finished.result = Some(HoldResult::LetGo);
        finished.life = 0.2;
        finished.let_go_started_at = Some(1_000);
        finished.let_go_starting_life = 0.2;
        let no_hold = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        let mut notes = vec![
            test_note_at(NoteType::Hold, Some(finished), false, 48, 1.0),
            no_hold,
        ];
        let mut active = [true, true];
        let mut indices = vec![0, 1, 99];

        let update = decay_let_go_hold_life_for_indices(
            &mut notes,
            &mut active,
            &mut indices,
            1_000 + song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_HOLD),
            1.0,
        );

        assert_eq!(
            update,
            HoldLifeDecayUpdate {
                remaining_count: 0,
                removed_count: 3,
            }
        );
        assert!(indices.is_empty());
        assert_eq!(active, [false, false]);
        assert_eq!(notes[0].hold.as_ref().unwrap().life, 0.0);
    }

    #[test]
    fn pending_missed_hold_queue_dedupes_and_bounds() {
        let mut pending = [false; 3];
        let mut indices = Vec::new();

        assert!(queue_pending_missed_hold_resolution(
            &mut pending,
            &mut indices,
            1,
        ));
        assert!(!queue_pending_missed_hold_resolution(
            &mut pending,
            &mut indices,
            1,
        ));
        assert!(!queue_pending_missed_hold_resolution(
            &mut pending,
            &mut indices,
            3,
        ));

        assert_eq!(pending, [false, true, false]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn pending_missed_hold_resolution_action_selects_feedback_or_scored_let_go() {
        assert_eq!(
            pending_missed_hold_resolution_action(Some(HoldResult::Missed), None, false),
            PendingMissedHoldResolution::ShowMissedFeedback,
        );
        assert_eq!(
            pending_missed_hold_resolution_action(None, Some(JudgeGrade::Miss), true),
            PendingMissedHoldResolution::ScoreLetGo,
        );
        assert_eq!(
            pending_missed_hold_resolution_action(None, Some(JudgeGrade::Miss), false),
            PendingMissedHoldResolution::None,
        );
        assert_eq!(
            pending_missed_hold_resolution_action(None, Some(JudgeGrade::Great), true),
            PendingMissedHoldResolution::None,
        );
        assert_eq!(
            pending_missed_hold_resolution_action(
                Some(HoldResult::Held),
                Some(JudgeGrade::Miss),
                true,
            ),
            PendingMissedHoldResolution::None,
        );
        assert_eq!(
            pending_missed_hold_resolution_action(
                Some(HoldResult::LetGo),
                Some(JudgeGrade::Miss),
                true,
            ),
            PendingMissedHoldResolution::None,
        );
    }

    #[test]
    fn pending_missed_hold_resolution_step_waits_until_hold_end() {
        let note = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);

        assert_eq!(
            pending_missed_hold_resolution_for_note(Some(&note), Some(10), 9, MAX_COLS, true),
            PendingMissedHoldResolutionStep::Wait,
        );
    }

    #[test]
    fn pending_missed_hold_resolution_step_removes_stale_entries() {
        assert_eq!(
            pending_missed_hold_resolution_for_note(None, Some(10), 10, MAX_COLS, true),
            PendingMissedHoldResolutionStep::Remove,
        );
        assert_eq!(
            pending_missed_hold_resolution_for_note(None, None, 10, MAX_COLS, true),
            PendingMissedHoldResolutionStep::Remove,
        );

        let mut note = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);
        note.column = MAX_COLS;
        assert_eq!(
            pending_missed_hold_resolution_for_note(Some(&note), Some(10), 10, MAX_COLS, true),
            PendingMissedHoldResolutionStep::Remove,
        );
    }

    #[test]
    fn pending_missed_hold_resolution_step_resolves_action() {
        let mut missed = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);
        missed.hold.as_mut().unwrap().result = Some(HoldResult::Missed);
        assert_eq!(
            pending_missed_hold_resolution_for_note(Some(&missed), Some(10), 10, MAX_COLS, false),
            PendingMissedHoldResolutionStep::Resolve(
                PendingMissedHoldResolution::ShowMissedFeedback
            ),
        );

        let mut scored = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);
        scored.result = Some(test_judgment(JudgeGrade::Miss));
        assert_eq!(
            pending_missed_hold_resolution_for_note(Some(&scored), Some(10), 10, MAX_COLS, true),
            PendingMissedHoldResolutionStep::Resolve(PendingMissedHoldResolution::ScoreLetGo),
        );
    }

    #[test]
    fn collect_pending_missed_hold_resolutions_drains_bounded_events() {
        let mut missed = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);
        missed.hold.as_mut().unwrap().result = Some(HoldResult::Missed);
        missed.column = 0;
        let mut scored = test_note_at(NoteType::Hold, Some(test_hold()), false, 96, 2.0);
        scored.result = Some(test_judgment(JudgeGrade::Miss));
        scored.column = 1;
        let mut waiting = test_note_at(NoteType::Hold, Some(test_hold()), false, 144, 3.0);
        waiting.hold.as_mut().unwrap().result = Some(HoldResult::Missed);
        waiting.column = 2;
        let notes = vec![missed, scored, waiting];
        let hold_end_times = [Some(10), Some(10), Some(30)];
        let mut pending = [true, true, true];
        let mut indices = vec![0, 1, 2];
        let score_by_col = [true, true, true];
        let mut events = [None; 1];

        let first = collect_pending_missed_hold_resolutions(
            &notes,
            &hold_end_times,
            &mut pending,
            &mut indices,
            10,
            &score_by_col,
            &mut events,
        );

        assert_eq!(
            first,
            PendingMissedHoldResolutionUpdate {
                event_count: 1,
                finished: false,
            }
        );
        assert_eq!(
            events[0],
            Some(PendingMissedHoldResolutionEvent {
                note_index: 0,
                column: 0,
                end_time_ns: 10,
                resolution: PendingMissedHoldResolution::ShowMissedFeedback,
            })
        );
        assert_eq!(pending, [false, true, true]);
        assert!(!indices.contains(&0));

        let second = collect_pending_missed_hold_resolutions(
            &notes,
            &hold_end_times,
            &mut pending,
            &mut indices,
            10,
            &score_by_col,
            &mut events,
        );

        assert_eq!(
            second,
            PendingMissedHoldResolutionUpdate {
                event_count: 1,
                finished: true,
            }
        );
        assert_eq!(
            events[0],
            Some(PendingMissedHoldResolutionEvent {
                note_index: 1,
                column: 1,
                end_time_ns: 10,
                resolution: PendingMissedHoldResolution::ScoreLetGo,
            })
        );
        assert_eq!(pending, [false, false, true]);
        assert_eq!(indices, vec![2]);
    }

    #[test]
    fn collect_pending_missed_hold_resolutions_clears_stale_entries() {
        let mut no_action = test_note_at(NoteType::Hold, Some(test_hold()), false, 48, 1.0);
        no_action.result = Some(test_judgment(JudgeGrade::Great));
        let notes = vec![no_action];
        let hold_end_times = [Some(10)];
        let mut pending = [true, true, true];
        let mut indices = vec![0, 1, 2];
        let mut events = [None; 2];

        let update = collect_pending_missed_hold_resolutions(
            &notes,
            &hold_end_times,
            &mut pending,
            &mut indices,
            10,
            &[true],
            &mut events,
        );

        assert_eq!(
            update,
            PendingMissedHoldResolutionUpdate {
                event_count: 0,
                finished: true,
            }
        );
        assert_eq!(pending, [false, false, false]);
        assert!(indices.is_empty());
        assert!(events.iter().all(Option::is_none));
    }

    #[test]
    fn hold_result_stats_update_counts_held_holds_and_rolls() {
        let hold = hold_result_stats_update(NoteType::Hold, HoldResult::Held, false, false);
        let roll = hold_result_stats_update(NoteType::Roll, HoldResult::Held, false, false);

        assert_eq!(hold.holds_held, 1);
        assert_eq!(hold.holds_held_for_score, 1);
        assert!(hold.decrement_hands_holding);
        assert!(hold.update_grade_totals);
        assert_eq!(roll.rolls_held, 1);
        assert_eq!(roll.rolls_held_for_score, 1);
        assert!(roll.decrement_hands_holding);
        assert!(roll.update_grade_totals);
    }

    #[test]
    fn hold_result_stats_update_counts_let_go_for_score_only() {
        let hold = hold_result_stats_update(NoteType::Hold, HoldResult::LetGo, false, false);
        let roll = hold_result_stats_update(NoteType::Roll, HoldResult::LetGo, false, false);

        assert_eq!(hold.holds_held, 0);
        assert_eq!(hold.holds_let_go_for_score, 1);
        assert!(hold.update_grade_totals);
        assert_eq!(roll.rolls_held, 0);
        assert_eq!(roll.rolls_let_go_for_score, 1);
        assert!(roll.update_grade_totals);
    }

    #[test]
    fn hold_result_stats_update_respects_scoring_block_and_dead_state() {
        let blocked = hold_result_stats_update(NoteType::Hold, HoldResult::Held, true, false);
        let dead_held = hold_result_stats_update(NoteType::Hold, HoldResult::Held, false, true);
        let dead_let_go = hold_result_stats_update(NoteType::Hold, HoldResult::LetGo, false, true);

        assert_eq!(
            blocked,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
        assert_eq!(dead_held.holds_held, 1);
        assert_eq!(dead_held.holds_held_for_score, 0);
        assert!(!dead_held.update_grade_totals);
        assert_eq!(
            dead_let_go,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
    }

    #[test]
    fn hold_result_stats_update_ignores_non_hold_note_types() {
        let update = hold_result_stats_update(NoteType::Tap, HoldResult::Held, false, false);

        assert_eq!(
            update,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
    }

    #[test]
    fn hold_result_stats_application_decrements_hands_and_adds_counts() {
        let mut state = HoldResultStatsState {
            hands_holding_count_for_stats: 2,
            holds_held: 10,
            holds_held_for_score: 20,
            holds_let_go_for_score: 30,
            rolls_held: 40,
            rolls_held_for_score: 50,
            rolls_let_go_for_score: 60,
        };

        apply_hold_result_stats_update(
            &mut state,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                holds_held: 1,
                holds_held_for_score: 2,
                holds_let_go_for_score: 3,
                rolls_held: 4,
                rolls_held_for_score: 5,
                rolls_let_go_for_score: 6,
                update_grade_totals: true,
            },
        );

        assert_eq!(state.hands_holding_count_for_stats, 1);
        assert_eq!(state.holds_held, 11);
        assert_eq!(state.holds_held_for_score, 22);
        assert_eq!(state.holds_let_go_for_score, 33);
        assert_eq!(state.rolls_held, 44);
        assert_eq!(state.rolls_held_for_score, 55);
        assert_eq!(state.rolls_let_go_for_score, 66);
    }

    #[test]
    fn hold_result_stats_application_saturates_and_clamps_hand_count() {
        let mut zero_hands = HoldResultStatsState {
            hands_holding_count_for_stats: 0,
            holds_held: u32::MAX,
            ..HoldResultStatsState::default()
        };

        apply_hold_result_stats_update(
            &mut zero_hands,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                holds_held: 1,
                ..HoldResultStatsUpdate::ZERO
            },
        );

        assert_eq!(zero_hands.hands_holding_count_for_stats, 0);
        assert_eq!(zero_hands.holds_held, u32::MAX);

        let mut negative_hands = HoldResultStatsState {
            hands_holding_count_for_stats: -3,
            ..HoldResultStatsState::default()
        };
        apply_hold_result_stats_update(
            &mut negative_hands,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            },
        );
        assert_eq!(negative_hands.hands_holding_count_for_stats, -3);
    }

    #[test]
    fn hold_let_go_player_state_applies_stats_and_combo_policy() {
        let stats_update =
            hold_result_stats_update(NoteType::Hold, HoldResult::LetGo, false, false);
        let mut state = HoldResolutionPlayerState {
            stats: HoldResultStatsState {
                hands_holding_count_for_stats: 1,
                ..HoldResultStatsState::default()
            },
            combo: ComboState {
                combo: 7,
                miss_combo: 2,
                full_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                first_fc_attempt_broken: false,
            },
        };

        let update = apply_hold_let_go_player_state(&mut state, stats_update, false);

        assert_eq!(update.stats_update, stats_update);
        assert_near(update.life_delta, deadsync_rules::life::LIFE_LET_GO);
        assert!(update.apply_life_change);
        assert!(update.capture_failed_ex_score_inputs);
        assert_eq!(state.stats.hands_holding_count_for_stats, 0);
        assert_eq!(state.stats.holds_let_go_for_score, 1);
        assert_eq!(state.combo.full_combo_grade, None);
        assert!(state.combo.first_fc_attempt_broken);
        if COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO {
            assert_eq!(state.combo.combo, 0);
            assert_eq!(state.combo.miss_combo, 3);
            assert_eq!(state.combo.current_combo_grade, None);
            assert!(update.combo_update.combo_broken);
        } else {
            assert_eq!(state.combo.combo, 7);
            assert_eq!(state.combo.miss_combo, 2);
            assert_eq!(state.combo.current_combo_grade, Some(JudgeGrade::Fantastic));
            assert_eq!(update.combo_update, ComboUpdate::default());
        }
    }

    #[test]
    fn hold_success_player_state_respects_scoring_block() {
        let stats_update = hold_result_stats_update(NoteType::Hold, HoldResult::Held, true, false);
        let mut state = HoldResolutionPlayerState {
            stats: HoldResultStatsState {
                hands_holding_count_for_stats: 1,
                ..HoldResultStatsState::default()
            },
            combo: ComboState {
                combo: 7,
                miss_combo: 2,
                full_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                first_fc_attempt_broken: false,
            },
        };

        let update = apply_hold_success_player_state(&mut state, stats_update, true);

        assert_eq!(update.stats_update, stats_update);
        assert_eq!(update.combo_update, ComboUpdate::default());
        assert_near(update.life_delta, deadsync_rules::life::LIFE_HELD);
        assert!(!update.apply_life_change);
        assert!(!update.capture_failed_ex_score_inputs);
        assert_eq!(state.stats.hands_holding_count_for_stats, 0);
        assert_eq!(state.stats.holds_held, 0);
        assert_eq!(state.combo.combo, 7);
        assert_eq!(state.combo.miss_combo, 2);
        assert_eq!(state.combo.full_combo_grade, Some(JudgeGrade::Fantastic));
        assert_eq!(state.combo.current_combo_grade, Some(JudgeGrade::Fantastic));
        assert!(!state.combo.first_fc_attempt_broken);
    }

    #[test]
    fn started_active_hold_state_resets_hold_and_builds_active_slot() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 0.25;
        hold.let_go_started_at = Some(10);
        hold.let_go_starting_life = 0.25;

        let active = started_active_hold_state(Some(&mut hold), 3, NoteType::Hold, 100, 200, 150);

        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
        assert_eq!(active.note_index, 3);
        assert_eq!(active.start_time_ns, 100);
        assert_eq!(active.end_time_ns, 200);
        assert_eq!(active.note_type, NoteType::Hold);
        assert!(!active.let_go);
        assert!(active.is_pressed);
        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, 150);
    }

    #[test]
    fn started_active_hold_state_allows_missing_hold_data() {
        let active = started_active_hold_state(None, 4, NoteType::Roll, 10, 20, 12);

        assert_eq!(active.note_index, 4);
        assert_eq!(active.start_time_ns, 10);
        assert_eq!(active.end_time_ns, 20);
        assert_eq!(active.note_type, NoteType::Roll);
        assert_eq!(active.life, MAX_HOLD_LIFE);
    }

    #[test]
    fn refresh_roll_life_for_step_restores_life_and_progress_time() {
        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.start_time_ns = 100;
        active.end_time_ns = 300;
        active.last_update_time_ns = 120;
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(150);
        hold.let_go_starting_life = 0.2;

        assert!(refresh_roll_life_for_step(&mut active, &mut hold, 250));

        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, 250);
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
    }

    #[test]
    fn refresh_roll_life_for_step_caps_progress_at_roll_end() {
        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.start_time_ns = 100;
        active.end_time_ns = 300;
        active.last_update_time_ns = 120;
        let mut hold = test_hold();

        assert!(refresh_roll_life_for_step(&mut active, &mut hold, 400));

        assert_eq!(active.last_update_time_ns, 300);
    }

    #[test]
    fn refresh_roll_life_for_step_rejects_non_roll_or_inactive_hold() {
        let mut hold_active = test_active_hold(NoteType::Hold, true, false, 0.2);
        let mut roll_let_go = test_active_hold(NoteType::Roll, true, true, 0.2);
        let mut exhausted_roll = test_active_hold(NoteType::Roll, true, false, 0.0);
        let mut hold = test_hold();

        assert!(!refresh_roll_life_for_step(&mut hold_active, &mut hold, 1));
        assert!(!refresh_roll_life_for_step(&mut roll_let_go, &mut hold, 1));
        assert!(!refresh_roll_life_for_step(
            &mut exhausted_roll,
            &mut hold,
            1,
        ));
    }

    #[test]
    fn refresh_roll_life_for_step_rejects_invalid_time_and_resolved_hold() {
        let mut early = test_active_hold(NoteType::Roll, true, false, 0.2);
        early.start_time_ns = 100;
        let mut invalid = early.clone();
        let mut resolved = early.clone();
        let mut hold = test_hold();
        let mut resolved_hold = test_hold();
        resolved_hold.result = Some(HoldResult::LetGo);

        assert!(!refresh_roll_life_for_step(&mut early, &mut hold, 99));
        assert!(!refresh_roll_life_for_step(
            &mut invalid,
            &mut hold,
            i64::MIN,
        ));
        assert!(!refresh_roll_life_for_step(
            &mut resolved,
            &mut resolved_hold,
            100,
        ));
    }

    #[test]
    fn sync_active_hold_pressed_column_updates_active_slot() {
        let active = test_active_hold(NoteType::Hold, false, false, MAX_HOLD_LIFE);
        let mut active_holds = [Some(active), None];

        assert!(sync_active_hold_pressed_column(
            &mut active_holds,
            0,
            false,
            true,
        ));
        assert!(active_holds[0].as_ref().unwrap().is_pressed);

        assert!(sync_active_hold_pressed_column(
            &mut active_holds,
            0,
            true,
            false,
        ));
        assert!(active_holds[0].as_ref().unwrap().is_pressed);

        assert!(sync_active_hold_pressed_column(
            &mut active_holds,
            0,
            false,
            false,
        ));
        assert!(!active_holds[0].as_ref().unwrap().is_pressed);
    }

    #[test]
    fn sync_active_hold_pressed_column_ignores_missing_slots() {
        let mut active_holds = [None];

        assert!(!sync_active_hold_pressed_column(
            &mut active_holds,
            0,
            false,
            true,
        ));
        assert!(!sync_active_hold_pressed_column(
            &mut active_holds,
            2,
            false,
            true,
        ));
    }

    #[test]
    fn refresh_roll_life_for_active_column_updates_roll_hold() {
        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.note_index = 0;
        active.start_time_ns = 100;
        active.end_time_ns = 300;
        active.last_update_time_ns = 120;
        let mut active_holds = [Some(active)];
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(150);
        let mut notes = [test_note_at(NoteType::Roll, Some(hold), false, 0, 0.0)];

        assert!(refresh_roll_life_for_active_column(
            &mut active_holds,
            &mut notes,
            0,
            250,
        ));

        assert_eq!(active_holds[0].as_ref().unwrap().last_update_time_ns, 250);
        let hold = notes[0].hold.as_ref().unwrap();
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
    }

    #[test]
    fn refresh_roll_life_for_active_column_ignores_invalid_slots() {
        let mut active_holds = [None];
        let mut notes = [test_note_at(
            NoteType::Roll,
            Some(test_hold()),
            false,
            0,
            0.0,
        )];
        assert!(!refresh_roll_life_for_active_column(
            &mut active_holds,
            &mut notes,
            0,
            100,
        ));

        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.note_index = 9;
        let mut active_holds = [Some(active)];
        assert!(!refresh_roll_life_for_active_column(
            &mut active_holds,
            &mut notes,
            0,
            100,
        ));

        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.note_index = 0;
        let mut active_holds = [Some(active)];
        let mut notes = [test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        assert!(!refresh_roll_life_for_active_column(
            &mut active_holds,
            &mut notes,
            0,
            100,
        ));
    }

    #[test]
    fn advance_active_hold_to_time_resolves_success_at_tail() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        let target_time_ns = active.end_time_ns;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            target_time_ns,
            1.0,
        );

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::Success { note_index: 42 })
        );
        assert!(result.clear_active);
        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(hold.life, MAX_HOLD_LIFE);
    }

    #[test]
    fn advance_active_hold_to_time_resolves_let_go_at_zero_crossing() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Roll, false, false, 0.5);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        hold.life = 0.5;
        let target_time_ns = active.end_time_ns;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            target_time_ns,
            1.0,
        );

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::LetGo {
                note_index: 42,
                time_ns: song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_ROLL * 0.5),
            })
        );
        assert!(result.clear_active);
        assert!(active.let_go);
        assert_eq!(active.life, 0.0);
        assert_eq!(hold.life, 0.0);
    }

    #[test]
    fn advance_active_hold_to_time_resolves_pre_exhausted_life_at_start() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, false, false, 0.0);
        active.start_time_ns = 100;
        active.last_update_time_ns = 50;
        let mut hold = test_hold();

        let result = advance_active_hold_to_time(&mut active, &mut hold, &timing, 0, 0.0, 200, 1.0);

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::LetGo {
                note_index: 42,
                time_ns: 100,
            })
        );
        assert!(result.clear_active);
        assert!(active.let_go);
    }

    #[test]
    fn advance_active_hold_to_time_updates_held_progress_row() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(2.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        hold.end_row_index = 192;
        hold.end_beat = 4.0;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            song_time_ns_from_seconds(0.5),
            1.0,
        );

        assert_eq!(result, ActiveHoldAdvance::default());
        assert!(hold.last_held_row_index > 0);
        assert!(hold.last_held_beat > 0.0);
        assert_eq!(active.last_update_time_ns, song_time_ns_from_seconds(0.5));
    }

    #[test]
    fn integrate_active_hold_column_resolves_and_clears_success() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 0;
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut active_holds = [Some(active), None];
        let mut note = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        note.hold.as_mut().unwrap().end_row_index = 48;
        note.hold.as_mut().unwrap().end_beat = 1.0;
        let mut notes = [note];

        let resolution = integrate_active_hold_column(
            &mut active_holds,
            &mut notes,
            0,
            &timing,
            song_time_ns_from_seconds(1.0),
            1.0,
        );

        assert_eq!(
            resolution,
            Some(ActiveHoldResolution::Success { note_index: 0 })
        );
        assert!(active_holds[0].is_none());
        assert_eq!(notes[0].hold.as_ref().unwrap().life, MAX_HOLD_LIFE);

        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 0;
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut active_holds = [Some(active), None];
        let mut note = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        note.hold.as_mut().unwrap().end_row_index = 48;
        note.hold.as_mut().unwrap().end_beat = 1.0;
        let mut notes = [note];

        let resolution = integrate_active_hold_column(
            &mut active_holds,
            &mut notes,
            0,
            &timing,
            song_time_ns_from_seconds(1.0),
            f32::NAN,
        );

        assert_eq!(
            resolution,
            Some(ActiveHoldResolution::Success { note_index: 0 })
        );
    }

    #[test]
    fn integrate_active_hold_column_clears_stale_active_slots() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 9;
        let mut active_holds = [Some(active)];
        let mut notes = [test_note_at(
            NoteType::Hold,
            Some(test_hold()),
            false,
            0,
            0.0,
        )];

        assert_eq!(
            integrate_active_hold_column(&mut active_holds, &mut notes, 0, &timing, 100, 1.0),
            None
        );
        assert!(active_holds[0].is_none());

        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 0;
        let mut active_holds = [Some(active)];
        let mut notes = [test_note_at(NoteType::Tap, None, false, 0, 0.0)];

        assert_eq!(
            integrate_active_hold_column(&mut active_holds, &mut notes, 0, &timing, 100, 1.0),
            None
        );
        assert!(active_holds[0].is_none());
    }

    #[test]
    fn integrate_active_hold_column_ignores_invalid_inputs() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 0;
        let original = active.clone();
        let mut active_holds = [Some(active)];
        let mut notes = [test_note_at(
            NoteType::Hold,
            Some(test_hold()),
            false,
            0,
            0.0,
        )];

        assert_eq!(
            integrate_active_hold_column(&mut active_holds, &mut notes, 2, &timing, 100, 1.0),
            None
        );
        let kept = active_holds[0].as_ref().expect("active hold should remain");
        assert_eq!(kept.note_index, original.note_index);
        assert_eq!(kept.last_update_time_ns, original.last_update_time_ns);

        assert_eq!(
            integrate_active_hold_column(
                &mut active_holds,
                &mut notes,
                0,
                &timing,
                INVALID_SONG_TIME_NS,
                1.0,
            ),
            None
        );
        let kept = active_holds[0].as_ref().expect("active hold should remain");
        assert_eq!(kept.note_index, original.note_index);
        assert_eq!(kept.last_update_time_ns, original.last_update_time_ns);
    }

    #[test]
    fn update_active_hold_columns_syncs_pressed_state_and_reports_resolutions() {
        let timing = test_timing(192);
        let timing_players = [&timing; MAX_PLAYERS];
        let mut success = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        success.note_index = 0;
        success.start_time_ns = 0;
        success.end_time_ns = song_time_ns_from_seconds(1.0);
        success.last_update_time_ns = 0;
        let mut let_go = test_active_hold(NoteType::Hold, true, false, 0.0);
        let_go.note_index = 1;
        let_go.start_time_ns = 0;
        let_go.end_time_ns = song_time_ns_from_seconds(2.0);
        let_go.last_update_time_ns = 0;
        let mut active_holds = [Some(success), Some(let_go)];
        let mut first_hold = test_hold();
        first_hold.end_row_index = 48;
        first_hold.end_beat = 1.0;
        let mut second_hold = test_hold();
        second_hold.end_row_index = 96;
        second_hold.end_beat = 2.0;
        let mut notes = [
            test_note_at(NoteType::Hold, Some(first_hold), false, 0, 0.0),
            test_note_at(NoteType::Hold, Some(second_hold), false, 0, 0.0),
        ];
        let mut inputs = [false; MAX_COLS];
        inputs[0] = true;
        let mut events = [None; MAX_COLS];

        let update = update_active_hold_columns(
            &mut active_holds,
            &mut notes,
            &inputs,
            2,
            4,
            1,
            &timing_players,
            song_time_ns_from_seconds(1.0),
            1.0,
            false,
            &mut events,
        );

        assert_eq!(
            update,
            ActiveHoldColumnsUpdate {
                columns_scanned: 2,
                event_count: 2,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(ActiveHoldColumnResolution {
                column: 0,
                resolution: ActiveHoldResolution::Success { note_index: 0 },
            })
        );
        assert_eq!(
            events[1],
            Some(ActiveHoldColumnResolution {
                column: 1,
                resolution: ActiveHoldResolution::LetGo {
                    note_index: 1,
                    time_ns: 0,
                },
            })
        );
        assert!(active_holds[0].is_none());
        assert!(active_holds[1].is_none());
    }

    #[test]
    fn update_active_hold_columns_stops_before_overflowing_events() {
        let timing = test_timing(192);
        let timing_players = [&timing; MAX_PLAYERS];
        let mut active0 = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active0.note_index = 0;
        active0.end_time_ns = 100;
        let mut active1 = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active1.note_index = 1;
        active1.end_time_ns = 100;
        let mut active_holds = [Some(active0), Some(active1)];
        let mut notes = [
            test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0),
            test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0),
        ];
        let inputs = [true; MAX_COLS];
        let mut events = [None; 1];

        let update = update_active_hold_columns(
            &mut active_holds,
            &mut notes,
            &inputs,
            2,
            4,
            1,
            &timing_players,
            100,
            1.0,
            false,
            &mut events,
        );

        assert_eq!(
            update,
            ActiveHoldColumnsUpdate {
                columns_scanned: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(
            events[0],
            Some(ActiveHoldColumnResolution {
                column: 0,
                resolution: ActiveHoldResolution::Success { note_index: 0 },
            })
        );
        assert!(active_holds[0].is_none());
        assert!(active_holds[1].is_some());
    }

    #[test]
    fn settle_replaced_active_hold_column_resolves_previous_hold() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.note_index = 0;
        active.start_time_ns = 0;
        active.end_time_ns = 100;
        active.last_update_time_ns = 0;
        let mut active_holds = [Some(active)];
        let mut previous_hold = test_hold();
        previous_hold.end_row_index = 48;
        previous_hold.end_beat = 1.0;
        let mut notes = [
            test_note_at(NoteType::Hold, Some(previous_hold), false, 0, 0.0),
            test_note_at(NoteType::Hold, Some(test_hold()), false, 96, 2.0),
        ];

        let event = settle_replaced_active_hold_column(
            &mut active_holds,
            &mut notes,
            0,
            1,
            100,
            &timing,
            1.0,
        );

        assert_eq!(
            event,
            Some(ActiveHoldColumnResolution {
                column: 0,
                resolution: ActiveHoldResolution::Success { note_index: 0 },
            })
        );
        assert!(active_holds[0].is_none());
    }

    #[test]
    fn start_active_hold_column_resets_hold_and_sets_slot() {
        let mut active_holds = [None];
        let mut hold = test_hold();
        hold.life = 0.25;
        hold.let_go_started_at = Some(7);
        hold.let_go_starting_life = 0.25;
        let mut notes = [test_note_at(NoteType::Roll, Some(hold), false, 48, 1.0)];

        assert!(start_active_hold_column(
            &mut active_holds,
            &mut notes,
            0,
            0,
            10,
            20,
            12,
        ));

        let active = active_holds[0].as_ref().expect("active hold is inserted");
        assert_eq!(active.note_index, 0);
        assert_eq!(active.note_type, NoteType::Roll);
        assert_eq!(active.start_time_ns, 10);
        assert_eq!(active.end_time_ns, 20);
        assert_eq!(active.last_update_time_ns, 12);
        let hold = notes[0].hold.as_ref().expect("hold data remains");
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
    }

    #[test]
    fn let_go_head_beat_stays_at_receptor_until_visible_clock_catches_up() {
        let waiting = let_go_head_beat(100.0, 108.0, 102.0, 101.25);
        let caught_up = let_go_head_beat(100.0, 108.0, 102.0, 103.0);
        let beyond_tail = let_go_head_beat(100.0, 108.0, 110.0, 120.0);

        assert!((waiting - 101.25).abs() <= 1.0e-6);
        assert!((caught_up - 102.0).abs() <= 1.0e-6);
        assert!((beyond_tail - 108.0).abs() <= 1.0e-6);
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_timing(last_row: usize) -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(last_row),
        )
    }

    fn test_song(chart_end: f32, audio_len: f32) -> SongData {
        SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: String::new(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: audio_len,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: chart_end,
            charts: Vec::new(),
        }
    }

    #[test]
    fn note_types_define_rescore_and_edge_matching() {
        for note_type in [
            NoteType::Tap,
            NoteType::Lift,
            NoteType::Hold,
            NoteType::Roll,
        ] {
            assert!(counts_for_early_rescore(note_type));
        }
        assert!(!counts_for_early_rescore(NoteType::Mine));
        assert!(!counts_for_early_rescore(NoteType::Fake));

        assert!(lane_edge_matches_note_type(true, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(false, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(true, NoteType::Lift));
        assert!(lane_edge_matches_note_type(false, NoteType::Lift));
        assert!(!lane_edge_matches_note_type(true, NoteType::Mine));
        assert!(!lane_edge_matches_note_type(false, NoteType::Mine));
    }

    #[test]
    fn note_display_predicates_match_gameplay_visual_rules() {
        assert!(row_final_grade_hides_note(JudgeGrade::Fantastic));
        assert!(row_final_grade_hides_note(JudgeGrade::Excellent));
        assert!(row_final_grade_hides_note(JudgeGrade::Great));
        assert!(!row_final_grade_hides_note(JudgeGrade::Decent));
        assert!(!row_final_grade_hides_note(JudgeGrade::WayOff));
        assert!(!row_final_grade_hides_note(JudgeGrade::Miss));

        assert!(note_has_displayable_hold(&test_note(
            NoteType::Hold,
            Some(test_hold()),
            false,
        )));
        assert!(note_has_displayable_hold(&test_note(
            NoteType::Roll,
            Some(test_hold()),
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Hold,
            None,
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Tap,
            Some(test_hold()),
            false,
        )));
    }

    #[test]
    fn held_miss_tracking_only_uses_taps_holds_and_rolls() {
        assert!(note_tracks_held_miss(NoteType::Tap));
        assert!(note_tracks_held_miss(NoteType::Hold));
        assert!(note_tracks_held_miss(NoteType::Roll));
        assert!(!note_tracks_held_miss(NoteType::Mine));
        assert!(!note_tracks_held_miss(NoteType::Lift));
        assert!(!note_tracks_held_miss(NoteType::Fake));
    }

    #[test]
    fn held_miss_window_marks_first_pressed_track_in_window() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 0, 0.0);
        tap.column = 0;
        let mut duplicate_track = test_note_at(NoteType::Tap, None, false, 1, 0.0);
        duplicate_track.column = 0;
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 2, 0.0);
        hold.column = 1;
        let mut roll = test_note_at(NoteType::Roll, Some(test_hold()), false, 3, 0.0);
        roll.column = 2;
        let mut mine = test_note_at(NoteType::Mine, None, false, 4, 0.0);
        mine.column = 3;
        let mut lift = test_note_at(NoteType::Lift, None, false, 5, 0.0);
        lift.column = 2;
        let mut unjudgable = test_note_at(NoteType::Tap, None, false, 6, 0.0);
        unjudgable.column = 3;
        unjudgable.can_be_judged = false;
        let notes = [tap, duplicate_track, hold, roll, mine, lift, unjudgable];
        let note_times = [1_000, 1_010, 1_020, 1_040, 1_050, 1_060, 1_070];
        let mut held_window = [false; 7];
        let mut inputs = [false; MAX_COLS];
        inputs[0] = true;
        inputs[1] = true;
        inputs[3] = true;

        track_held_miss_window_for_player(
            &notes,
            &note_times,
            &mut held_window,
            (0, notes.len()),
            (0, 4),
            0,
            &inputs,
            1_000,
            50,
        );

        assert_eq!(held_window, [true, false, true, false, false, false, false]);
    }

    #[test]
    fn held_miss_windows_for_players_uses_ranges_and_player_windows() {
        let mut p1_tap = test_note_at(NoteType::Tap, None, false, 0, 0.0);
        p1_tap.column = 0;
        let mut p2_tap = test_note_at(NoteType::Tap, None, false, 1, 0.0);
        p2_tap.column = 4;
        let mut p2_late = test_note_at(NoteType::Tap, None, false, 2, 0.0);
        p2_late.column = 5;
        let notes = [p1_tap, p2_tap, p2_late];
        let note_times = [1_000, 1_010, 1_200];
        let note_ranges = [(0, 1), (1, 3)];
        let next_cursors = [0, 1];
        let largest_windows = [0, 50];
        let mut held_window = [false; 3];
        let mut inputs = [false; MAX_COLS];
        inputs[0] = true;
        inputs[4] = true;
        inputs[5] = true;

        let update = track_held_miss_windows_for_players(
            &notes,
            &note_times,
            &mut held_window,
            &note_ranges,
            &next_cursors,
            &largest_windows,
            2,
            4,
            &inputs,
            1_000,
        );

        assert_eq!(update, HeldMissWindowUpdate { players_scanned: 1 });
        assert_eq!(held_window, [false, true, false]);
    }

    #[test]
    fn column_cues_skip_fake_notes_and_mark_mines() {
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Tap, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Lift, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, false)),
            Some(true)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, true)),
            None
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Fake, None, false)),
            None
        );
    }

    #[test]
    fn column_cue_builder_filters_fakes_and_preserves_timed_gaps() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 0;
        let mut fake = test_note_at(NoteType::Tap, None, true, 96, 2.0);
        fake.column = 1;
        fake.can_be_judged = false;
        let mut later = test_note_at(NoteType::Tap, None, false, 192, 4.0);
        later.column = 2;
        let notes = [first, fake, later];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(4.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, 0.0);

        assert_eq!(cues.len(), 2);
        assert_near(cues[0].start_time, 0.0);
        assert_near(cues[0].duration, 1.0);
        assert_eq!(cues[0].columns.len(), 1);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, false);
        assert_near(cues[1].start_time, 1.0);
        assert_near(cues[1].duration, 3.0);
        assert_eq!(cues[1].columns.len(), 1);
        assert_eq!(cues[1].columns[0].column, 2);
    }

    #[test]
    fn column_cue_builder_sorts_dedups_and_offsets_first_visible_time() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 2;
        let mut duplicate = test_note_at(NoteType::Lift, None, false, 48, 1.0);
        duplicate.column = 2;
        let mut mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        mine.column = 0;
        let notes = [first, duplicate, mine];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, -0.5);

        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 1.5);
        assert_eq!(cues[0].columns.len(), 2);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, true);
        assert_eq!(cues[0].columns[1].column, 2);
        assert_eq!(cues[0].columns[1].is_mine, false);
    }

    #[test]
    fn active_column_cue_returns_latest_started_cue() {
        let cues = [
            ColumnCue {
                start_time: 1.0,
                duration: 0.5,
                columns: Vec::new(),
            },
            ColumnCue {
                start_time: 3.0,
                duration: 0.5,
                columns: Vec::new(),
            },
        ];

        assert!(active_column_cue(&[], 2.0).is_none());
        assert!(active_column_cue(&cues, 0.99).is_none());
        assert_eq!(active_column_cue(&cues, 1.0).unwrap().start_time, 1.0);
        assert_eq!(active_column_cue(&cues, 2.0).unwrap().start_time, 1.0);
        assert_eq!(active_column_cue(&cues, 3.0).unwrap().start_time, 3.0);
        assert_eq!(active_column_cue(&cues, 9.0).unwrap().start_time, 3.0);
    }

    #[test]
    fn active_column_cues_returns_overlapping_window() {
        // Two consecutive cues overlapping by the fade time, as the crossover
        // builder emits them (cue2.start == cue1.end - fade).
        let fade = CROSSOVER_CUE_FADE_SECONDS;
        let cues = [
            ColumnCue {
                start_time: 1.0,
                duration: 0.5,
                columns: Vec::new(),
            },
            ColumnCue {
                start_time: 1.5 - fade,
                duration: 0.5,
                columns: Vec::new(),
            },
        ];

        // Before anything starts: empty.
        assert!(active_column_cues(&cues, 0.99).is_empty());
        // Only the first cue is active well inside its window.
        let only_first = active_column_cues(&cues, 1.2);
        assert_eq!(only_first.len(), 1);
        assert_eq!(only_first[0].start_time, 1.0);
        // Inside the overlap region both cues render simultaneously.
        let overlap = active_column_cues(&cues, 1.5 - fade + 0.01);
        assert_eq!(overlap.len(), 2);
        assert_eq!(overlap[0].start_time, 1.0);
        assert_eq!(overlap[1].start_time, 1.5 - fade);
        // After the first cue ends, only the second remains.
        let only_second = active_column_cues(&cues, 1.6);
        assert_eq!(only_second.len(), 1);
        assert_eq!(only_second[0].start_time, 1.5 - fade);
        // After both end, and for empty input: empty.
        assert!(active_column_cues(&cues, 9.0).is_empty());
        assert!(active_column_cues(&[], 2.0).is_empty());
    }

    fn xover_anno(beat: f32, note_count: u8, column_mask: u8, is_crossover: bool) -> CrossoverRow {
        debug_assert_eq!(
            u32::from(note_count),
            column_mask.count_ones(),
            "xover_anno note_count must equal the number of set columns",
        );
        CrossoverRow {
            beat,
            column_mask,
            crossover: is_crossover,
            bracket: note_count > 1,
        }
    }

    fn xover_time(beat: f32) -> f32 {
        beat * 0.5
    }

    #[test]
    fn crossover_rows_encode_notes_and_hold_tails_for_parity() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        tap.column = 1;
        let mut lift = test_note_at(NoteType::Lift, None, false, 48, 1.0);
        lift.column = 2;
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 144, 3.0);
        hold.column = 3;
        hold.hold.as_mut().unwrap().end_row_index = 192;
        hold.hold.as_mut().unwrap().end_beat = 4.0;
        let mut roll = test_note_at(NoteType::Roll, Some(test_hold()), false, 240, 5.0);
        roll.column = 0;
        roll.hold.as_mut().unwrap().end_row_index = 288;
        roll.hold.as_mut().unwrap().end_beat = 6.0;
        let mut mine = test_note_at(NoteType::Mine, None, false, 336, 7.0);
        mine.column = 2;

        let (rows, beats) = build_crossover_rows::<4>(&[tap, lift, hold, roll, mine], (0, 5), 0);

        assert_eq!(
            rows,
            vec![
                [b'0', b'0', b'L', b'0'],
                [b'0', b'1', b'0', b'0'],
                [b'0', b'0', b'0', b'2'],
                [b'0', b'0', b'0', b'3'],
                [b'4', b'0', b'0', b'0'],
                [b'3', b'0', b'0', b'0'],
                [b'0', b'0', b'M', b'0'],
            ]
        );
        assert_eq!(beats, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn crossover_rows_filter_columns_and_fake_notes() {
        let mut before = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        before.column = 1;
        let mut in_range = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        in_range.column = 2;
        let mut after = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        after.column = 6;
        let mut fake_tap = test_note_at(NoteType::Tap, None, true, 96, 2.0);
        fake_tap.column = 3;
        let mut fake_mine = test_note_at(NoteType::Mine, None, true, 144, 3.0);
        fake_mine.column = 4;
        let notes = [before, in_range, after, fake_tap, fake_mine];

        let (rows, beats) = build_crossover_rows::<4>(&notes, (0, notes.len()), 2);

        assert_eq!(
            rows,
            vec![[b'1', b'0', b'0', b'0'], [b'0', b'0', b'M', b'0']]
        );
        assert_eq!(beats, vec![1.0, 3.0]);
    }

    #[test]
    fn crossover_rows_real_arrows_replace_same_cell_mines() {
        let mut mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        mine.column = 0;
        let mut tap = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap.column = 0;
        let mut same_lane_mine = test_note_at(NoteType::Mine, None, false, 96, 2.0);
        same_lane_mine.column = 1;
        let notes = [mine, tap, same_lane_mine];

        let (rows, beats) = build_crossover_rows::<4>(&notes, (0, notes.len()), 0);

        assert_eq!(
            rows,
            vec![[b'1', b'0', b'0', b'0'], [b'0', b'M', b'0', b'0']]
        );
        assert_eq!(beats, vec![1.0, 2.0]);
    }

    #[test]
    fn crossover_arrow_col_picks_outer_and_inner_panels() {
        assert_eq!(crossover_arrow_col(0b0001, true), Some(0));
        assert_eq!(crossover_arrow_col(0b0001, false), None);
        assert_eq!(crossover_arrow_col(0b1000, true), Some(3));
        assert_eq!(crossover_arrow_col(0b0010, false), Some(1));
        assert_eq!(crossover_arrow_col(0b0010, true), None);
        assert_eq!(crossover_arrow_col(0b0100, false), Some(2));
        assert_eq!(crossover_arrow_col(0b1001, true), Some(0));
        assert_eq!(crossover_arrow_col(0b0110, false), Some(1));
        assert_eq!(crossover_arrow_col(1 << 4, true), Some(4));
        assert_eq!(crossover_arrow_col(1 << 5, false), Some(5));
    }

    #[test]
    fn crossover_cue_builder_emits_single_and_scooby_cues() {
        let single = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&single, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 0.575);
        assert_eq!(cues[0].columns.len(), 2);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, false);
        assert_eq!(cues[0].columns[1].column, 1);

        let scooby = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 1, 0b1000, true),
        ];
        let cues = build_crossover_cues_core(&scooby, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].columns.len(), 3);
        assert_eq!(cues[0].columns[2].column, 3);
        assert_eq!(cues[0].columns[2].is_mine, true);
    }

    #[test]
    fn crossover_cue_builder_uses_quantization_and_gap_policy() {
        let isolated = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(5.0, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&isolated, xover_time, 0, 500, 8, false, 0.0);
        assert!(cues.is_empty());

        let gap = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(2.0, 1, 0b0001, true),
            xover_anno(2.4, 1, 0b0010, false),
        ];
        let cues = build_crossover_cues_core(&gap, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].duration, 1.575);
        assert_near(cues[0].start_time, -0.5);
    }

    #[test]
    fn crossover_cue_builder_clamps_overlap_and_first_visible_offset() {
        let overlapping = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(0.6, 1, 0b0100, false),
            xover_anno(0.7, 1, 0b1000, true),
        ];
        let cues = build_crossover_cues_core(&overlapping, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 2);
        assert_near(cues[1].start_time, 0.0);
        assert_near(cues[1].duration, 0.375);

        let early = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&early, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.8);
        assert_near(cues[0].duration, 0.875);

        let later = [
            xover_anno(2.0, 1, 0b0010, false),
            xover_anno(2.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&later, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, 0.5);
        assert_near(cues[0].duration, 0.575);
    }

    #[test]
    fn crossover_cue_builder_merges_overlapping_same_column_cues() {
        // Two crossover cues that overlap in time and share column 1. Merging
        // keeps that column lit continuously instead of reflashing it.
        let shared = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(0.6, 1, 0b0010, false),
            xover_anno(0.7, 1, 0b1000, true),
        ];
        let cues = build_crossover_cues_core(&shared, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 0.875);
        let mut merged_cols: Vec<usize> = cues[0].columns.iter().map(|c| c.column).collect();
        merged_cols.sort_unstable();
        assert_eq!(merged_cols, vec![0, 1, 3]);
    }

    #[test]
    fn crossover_cue_builder_offsets_columns_and_respects_brackets() {
        let shifted = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&shifted, xover_time, 4, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].columns[0].column, 4);
        assert_eq!(cues[0].columns[1].column, 5);

        let bracket = [
            xover_anno(0.0, 1, 0b0100, false),
            xover_anno(0.5, 2, 0b0011, true),
        ];
        let excluded = build_crossover_cues_core(&bracket, xover_time, 0, 500, 8, false, 0.0);
        assert!(excluded.is_empty());
        let included = build_crossover_cues_core(&bracket, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(included.len(), 1);

        let bracket_scooby = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 2, 0b1100, true),
        ];
        let excluded =
            build_crossover_cues_core(&bracket_scooby, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].columns.len(), 2);
        let included = build_crossover_cues_core(&bracket_scooby, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(included.len(), 1);
        assert_eq!(included[0].columns.len(), 3);
        assert_eq!(included[0].columns[2].is_mine, true);
    }

    #[test]
    fn late_resolution_uses_largest_gameplay_window() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(late_note_resolution_window_ns(&timing_profile, 1.0));
        assert!((seconds - 0.3515).abs() <= 1e-6);
    }

    #[test]
    fn max_step_distance_scales_with_music_rate() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(max_step_distance_ns(&timing_profile, 1.5));
        assert!((seconds - 0.52725).abs() <= 1e-6);
    }

    #[test]
    fn judged_row_lookahead_uses_current_time_plus_step_distance() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let current = song_time_ns_from_seconds(10.0);
        let distance = max_step_distance_ns(&timing_profile, 1.5);

        assert_eq!(
            judged_row_lookahead_time_ns(current, &timing_profile, 1.5),
            current.saturating_add(distance)
        );
        assert_eq!(
            judged_row_lookahead_time_ns(SongTimeNs::MAX, &timing_profile, 1.0),
            SongTimeNs::MAX
        );
    }

    #[test]
    fn song_audio_end_time_uses_positive_chart_or_audio_end() {
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 10.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(f32::NAN, 10.0)),
            song_time_ns_from_seconds(10.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 0.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(song_audio_end_time_ns(&test_song(0.0, 0.0)), 0);
    }

    #[test]
    fn stage_music_cut_uses_negative_lead_in() {
        let cut = stage_music_cut(2.5);
        assert_eq!(cut.start_sec, -2.5);
        assert!(cut.length_sec.is_infinite());
        assert_eq!(cut.fade_in_sec, 0.0);
        assert_eq!(cut.fade_out_sec, 0.0);

        let clamped = stage_music_cut(-1.0);
        assert_eq!(clamped.start_sec, 0.0);
    }

    #[test]
    fn current_song_clock_snapshot_prefers_mapped_audio_time() {
        let now = Instant::now();
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    stream_seconds: 12.0,
                    music_nanos: song_time_ns_from_seconds(4.25),
                    music_seconds_per_second: 1.5,
                    has_music_mapping: true,
                    valid_at: now,
                    valid_at_host_nanos: 99,
                },
                timing_diag_enabled: true,
                timing_diag_callback_gap_ns: 123,
                ..GameplayAudioSnapshot::default()
            },
            0.75,
            2.0,
            -0.1,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(4.25));
        assert_eq!(snapshot.seconds_per_second, 1.5);
        assert!(snapshot.mapped_audio);
        assert_eq!(snapshot.valid_at_host_nanos, 99);
        assert!(snapshot.timing_diag_enabled);
        assert_eq!(snapshot.timing_diag_callback_gap_ns, 123);
    }

    #[test]
    fn audio_clock_state_tracks_lead_in_stream_and_output_delay() {
        let mut state = GameplayAudioClockState::new(-1.25, 2.0, -0.1);

        assert_eq!(state.lead_in_seconds(), -1.25);
        assert_eq!(state.positive_lead_in_seconds(), 0.0);
        assert_eq!(state.stream_position_seconds(), 2.0);
        assert_eq!(state.output_delay_seconds(), -0.1);

        state.set_lead_in_seconds(1.5);
        state.set_stream_position_seconds(3.25);
        state.set_output_delay_seconds(-0.5);
        assert_eq!(state.positive_lead_in_seconds(), 1.5);
        assert_eq!(state.stream_position_seconds(), 3.25);
        assert_eq!(state.output_delay_seconds(), 0.0);

        state.set_audio_snapshot(GameplayAudioSnapshot {
            stream_clock: GameplayStreamClockSnapshot {
                stream_seconds: 7.5,
                ..GameplayStreamClockSnapshot::default()
            },
            output_delay_seconds: 0.125,
            ..GameplayAudioSnapshot::default()
        });
        assert_eq!(state.stream_position_seconds(), 7.5);
        assert_eq!(state.output_delay_seconds(), 0.125);
        assert_eq!(state.lead_in_seconds(), 1.5);
    }

    #[test]
    fn music_rate_state_normalizes_and_reports_changes() {
        let mut state = GameplayMusicRateState::new(f32::NAN);
        assert_eq!(state.rate(), 1.0);
        assert!(!state.set_rate(1.0));

        assert!(state.set_rate(1.5));
        assert_eq!(state.rate(), 1.5);
        assert!(!state.set_rate(1.5));

        assert!(state.set_rate(-2.0));
        assert_eq!(state.rate(), 1.0);
    }

    #[test]
    fn song_position_state_tracks_music_and_display_positions() {
        let mut state = GameplaySongPositionState::new(12.0, 1_250_000_000, 11.5, 1.2);

        assert_eq!(state.current_beat, 12.0);
        assert_eq!(state.current_music_time_ns, 1_250_000_000);
        assert_eq!(state.current_beat_display, 11.5);
        assert_eq!(state.current_music_time_display, 1.2);

        state.set_music_position(13.0, 1_300_000_000);
        state.set_display_position(12.75, 1.275);

        assert_eq!(state.current_beat, 13.0);
        assert_eq!(state.current_music_time_ns, 1_300_000_000);
        assert_eq!(state.current_beat_display, 12.75);
        assert_near(state.current_music_time_display, 1.275);
    }

    #[test]
    fn exit_input_prompt_state_snapshots_hold_and_transition() {
        let mut state = GameplayExitInputState::default();
        let hold_at = Instant::now();
        state.arm_hold(HoldToExitKey::Start, hold_at);

        let prompt = state.prompt_state();
        assert_eq!(prompt.hold_to_exit_key, Some(HoldToExitKey::Start));
        assert_eq!(prompt.hold_to_exit_start, Some(hold_at));
        assert!(prompt.hold_to_exit_aborted_at.is_none());
        assert!(prompt.exit_transition.is_none());

        let exit_at = Instant::now();
        assert!(state.begin_exit(ExitTransitionKind::Cancel, exit_at));
        let prompt = state.prompt_state();
        assert!(prompt.hold_to_exit_key.is_none());
        assert!(prompt.hold_to_exit_start.is_none());
        assert_eq!(
            prompt.exit_transition.map(|transition| transition.kind),
            Some(ExitTransitionKind::Cancel)
        );
    }

    #[test]
    fn current_song_clock_snapshot_falls_back_for_invalid_mapped_slope() {
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    music_nanos: song_time_ns_from_seconds(2.0),
                    music_seconds_per_second: f32::NAN,
                    has_music_mapping: true,
                    ..GameplayStreamClockSnapshot::default()
                },
                ..GameplayAudioSnapshot::default()
            },
            1.25,
            0.0,
            0.0,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(2.0));
        assert_eq!(snapshot.seconds_per_second, 1.25);
        assert!(snapshot.mapped_audio);
    }

    #[test]
    fn current_song_clock_snapshot_maps_unmapped_stream_position() {
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    stream_seconds: 3.0,
                    has_music_mapping: false,
                    ..GameplayStreamClockSnapshot::default()
                },
                ..GameplayAudioSnapshot::default()
            },
            1.5,
            2.0,
            -0.1,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(1.45));
        assert_eq!(snapshot.seconds_per_second, 1.5);
        assert!(!snapshot.mapped_audio);
    }

    #[test]
    fn song_clock_music_time_reconstructs_past_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(120.0),
            seconds_per_second: 1.5,
            mapped_audio: true,
            valid_at: base + Duration::from_millis(24),
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };

        let edge_time = song_time_ns_to_seconds(song_clock_music_time_ns(snapshot, base, 0));

        assert!((edge_time - 119.964).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_music_time_handles_future_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(64.0),
            seconds_per_second: 2.0,
            mapped_audio: true,
            valid_at: base,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };

        let edge_time = song_time_ns_to_seconds(song_clock_music_time_ns(
            snapshot,
            base + Duration::from_millis(5),
            0,
        ));

        assert!((edge_time - 64.01).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_music_time_prefers_host_clock_when_available() {
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(32.0),
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: Instant::now(),
            valid_at_host_nanos: 2_000_000_000,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };

        let edge_time = song_time_ns_to_seconds(song_clock_music_time_ns(
            snapshot,
            Instant::now(),
            1_997_000_000,
        ));

        assert!((edge_time - 31.997).abs() < 0.000_5);
    }

    #[test]
    fn recent_step_tracks_count_current_press_inside_jump_window() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));
        pressed_since_ns[4] = Some(song_time_ns_from_seconds(10.0));

        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, song_time_ns_from_seconds(10.0)),
            2
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 4, 8, song_time_ns_from_seconds(10.0)),
            1
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, INVALID_SONG_TIME_NS),
            0
        );
    }

    #[test]
    fn recent_step_calories_use_recent_track_count_and_weight() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));

        assert!(
            (recent_step_calories(
                &pressed_since_ns,
                0,
                4,
                song_time_ns_from_seconds(10.0),
                120,
            ) - judgment::step_calories(120, 2))
            .abs()
                <= 1e-6
        );
    }

    #[test]
    fn recent_step_calories_ignore_invalid_event_time() {
        let pressed_since_ns = [Some(song_time_ns_from_seconds(10.0)); MAX_COLS];

        assert_eq!(
            recent_step_calories(&pressed_since_ns, 0, 4, INVALID_SONG_TIME_NS, 120),
            0.0
        );
    }

    #[test]
    fn visible_notefield_time_subtracts_visual_delay() {
        let music_time_ns = song_time_ns_from_seconds(100.0);
        let visible = song_time_ns_to_seconds(visible_notefield_time_ns(music_time_ns, 0.010));

        assert!((visible - 99.990).abs() < 0.000_5);
    }

    #[test]
    fn stream_position_to_music_time_applies_lead_in_rate_and_offset_anchor() {
        assert_near(music_time_from_stream_position(3.0, 2.0, -0.100, 1.5), 1.45);
        assert_near(music_time_from_stream_position(3.0, -2.0, 0.0, 1.0), 3.0);
        assert_near(music_time_from_stream_position(3.0, 2.0, -0.100, 0.0), 1.0);
        assert_near(
            music_time_from_stream_position(3.0, 2.0, -0.100, f32::NAN),
            1.0,
        );
    }

    #[test]
    fn assist_clap_rows_include_judgable_lifts_and_skip_fakes() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, true, 24, 0.5),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Roll, Some(test_hold()), false, 144, 3.0),
        ];
        notes[3]
            .hold
            .as_mut()
            .expect("roll has hold data")
            .end_row_index = 192;

        assert_eq!(
            build_assist_clap_rows(&notes, (0, notes.len())),
            vec![48, 144]
        );
        assert_eq!(build_assist_clap_rows(&notes, (2, 2)), Vec::<usize>::new());
    }

    #[test]
    fn assist_clap_cursor_skips_rows_at_or_before_current_row() {
        let rows = [48, 96, 144];

        assert_eq!(assist_clap_cursor_for_row(&rows, -1), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 47), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 48), 1);
        assert_eq!(assist_clap_cursor_for_row(&rows, 120), 2);
        assert_eq!(assist_clap_cursor_for_row(&rows, 144), 3);
    }

    #[test]
    fn assist_clap_schedule_keeps_cursor_current_when_disabled() {
        let rows = [48, 96, 144];

        assert_eq!(
            assist_clap_schedule_update(&rows, 0, 0, 96, 144, false, false),
            AssistClapScheduleUpdate {
                cursor: 2,
                last_crossed_row: 96,
                schedule_start: 0,
                schedule_end: 0,
            }
        );
    }

    #[test]
    fn assist_clap_schedule_returns_rows_in_lookahead() {
        let rows = [48, 96, 144, 192];

        assert_eq!(
            assist_clap_schedule_update(&rows, 0, 0, 48, 144, true, false),
            AssistClapScheduleUpdate {
                cursor: 3,
                last_crossed_row: 48,
                schedule_start: 0,
                schedule_end: 3,
            }
        );
    }

    #[test]
    fn gameplay_assist_clap_state_tracks_cursor_and_timeline_generation() {
        let mut state = GameplayAssistClapState::new(vec![48, 96, 144, 192]);

        assert!(state.note_sfx_generation(7));
        assert!(!state.note_sfx_generation(7));
        state.reset_for_row(48);

        assert_eq!(
            state.schedule_update(48, 144, true, false),
            AssistClapScheduleUpdate {
                cursor: 3,
                last_crossed_row: 48,
                schedule_start: 1,
                schedule_end: 3,
            }
        );
        assert_eq!(
            state.schedule_update(96, 192, true, true),
            AssistClapScheduleUpdate {
                cursor: 4,
                last_crossed_row: 96,
                schedule_start: 2,
                schedule_end: 4,
            }
        );
    }

    #[test]
    fn assist_clap_schedule_reanchors_on_timeline_reset() {
        let rows = [48, 96, 144, 192];

        assert_eq!(
            assist_clap_schedule_update(&rows, 0, 48, 96, 144, true, true),
            AssistClapScheduleUpdate {
                cursor: 3,
                last_crossed_row: 96,
                schedule_start: 2,
                schedule_end: 3,
            }
        );
    }

    #[test]
    fn assist_clap_schedule_does_not_rewind_on_backward_jitter() {
        let rows = [48, 96, 144, 192];

        assert_eq!(
            assist_clap_schedule_update(&rows, 2, 144, 96, 192, true, false),
            AssistClapScheduleUpdate {
                cursor: 4,
                last_crossed_row: 144,
                schedule_start: 2,
                schedule_end: 4,
            }
        );
    }

    #[test]
    fn assist_clap_music_seconds_uses_no_offset_timing() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize),
        );

        assert_eq!(assist_clap_music_seconds_for_row(&timing, 999), None);
        assert_near(
            assist_clap_music_seconds_for_row(&timing, ROWS_PER_BEAT as usize)
                .expect("row has beat mapping") as f32,
            1.0,
        );
    }

    #[test]
    fn assist_lookahead_horizon_adds_margin_and_scales_by_slope() {
        let h = assist_lookahead_music_horizon_seconds(0.020, 1.0);
        assert!((h - 0.070).abs() <= 1e-6, "h={h}");

        let h2 = assist_lookahead_music_horizon_seconds(0.020, 2.0);
        assert!((h2 - 0.140).abs() <= 1e-6, "h2={h2}");

        assert!(
            (assist_lookahead_music_horizon_seconds(0.0, f32::NAN)
                - ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS)
                .abs()
                <= 1e-6
        );
        assert!(assist_lookahead_music_horizon_seconds(-1.0, 1.0) >= 0.0);
    }

    #[test]
    fn assist_lookahead_future_row_uses_no_offset_horizon() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let music_time_ns = timing.get_time_for_beat_ns(1.0);
        let song_row = ROWS_PER_BEAT as i32;
        let future_time_ns = song_time_ns_add_seconds(music_time_ns, 0.070);

        assert_eq!(
            assist_lookahead_future_row(&timing, 0.0, 0.020, music_time_ns, 1.0, song_row),
            assist_row_no_offset_for_timing(&timing, 0.0, future_time_ns).max(song_row),
        );
        assert!(
            assist_lookahead_future_row(&timing, 0.0, 0.020, music_time_ns, 1.0, song_row)
                > song_row
        );
    }

    #[test]
    fn assist_lookahead_future_row_never_rewinds_before_song_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize),
        );

        assert_eq!(
            assist_lookahead_future_row(&timing, 0.0, 0.0, 0, 1.0, 999),
            999,
        );
    }

    #[test]
    fn end_timing_state_tracks_note_music_and_audio_end() {
        let mut state = GameplayEndTimingState::new(10, 20, 30);

        assert_eq!(state.notes_end_time_ns(), 10);
        assert_eq!(state.music_end_time_ns(), 20);
        assert_eq!(state.audio_end_time_ns(), 30);

        state.set_note_and_music_end_times(40, 50);
        assert_eq!(state.notes_end_time_ns(), 40);
        assert_eq!(state.music_end_time_ns(), 50);
        assert_eq!(state.audio_end_time_ns(), 30);
    }

    #[test]
    fn end_times_wait_for_audio_tail() {
        let notes = [test_note_at(NoteType::Tap, None, false, 96, 2.0)];
        let note_times = [song_time_ns_from_seconds(2.0)];
        let hold_end_times = [None];
        let audio_end_time_ns = song_time_ns_from_seconds(10.0);

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, audio_end_time_ns);

        assert!(notes_end_time_ns < audio_end_time_ns);
        assert_eq!(music_end_time_ns, audio_end_time_ns);
    }

    #[test]
    fn end_times_use_judgable_and_relevant_tails_separately() {
        let mut fake = test_note_at(NoteType::Fake, None, true, 240, 5.0);
        fake.can_be_judged = false;
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0), fake];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(5.0),
        ];
        let hold_end_times = [None, None];

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, 0);

        assert!(notes_end_time_ns < note_times[1]);
        assert!(music_end_time_ns > note_times[1]);
    }

    #[test]
    fn missed_note_cutoff_row_matches_stop_delay_rules() {
        let row_to_beat = test_row_to_beat(ROWS_PER_BEAT as usize * 4);
        let stop_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                stops: vec![StopSegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let stop_cutoff_time = stop_timing
            .get_time_for_beat_ns(1.0)
            .saturating_add(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&stop_timing, stop_cutoff_time),
            ROWS_PER_BEAT as usize + 1
        );

        let delay_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                delays: vec![DelaySegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let delay_cutoff_time = delay_timing
            .get_time_for_beat_ns(1.0)
            .saturating_sub(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&delay_timing, delay_cutoff_time),
            ROWS_PER_BEAT as usize
        );
    }

    #[test]
    fn missed_note_cutoff_row_uses_chart_row_indices() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &[0.0, 4.0, 8.0],
        );

        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(3.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.1)),
            2
        );
    }

    #[test]
    fn missed_note_cutoff_row_for_music_time_applies_late_distance() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let cutoff_time_ns = timing.get_time_for_beat_ns(3.0);
        let music_time_ns =
            cutoff_time_ns.saturating_add(max_step_distance_ns(&timing_profile, 1.5));

        assert_eq!(
            missed_note_cutoff_row_for_music_time(&timing_profile, &timing, 1.5, music_time_ns),
            missed_note_cutoff_row_for_timing(&timing, cutoff_time_ns),
        );
    }

    #[test]
    fn missed_note_cutoff_rows_for_players_uses_active_timing_data() {
        let timing_a = test_timing(ROWS_PER_BEAT as usize * 4);
        let timing_b = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        );
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let timing_players = [&timing_a, &timing_b];
        let timing_players: [&TimingData; MAX_PLAYERS] =
            std::array::from_fn(|player| timing_players[player.min(1)]);
        let cutoff_a_time_ns = timing_a.get_time_for_beat_ns(2.0);
        let music_time_ns =
            cutoff_a_time_ns.saturating_add(max_step_distance_ns(&timing_profile, 1.0));

        let rows = missed_note_cutoff_rows_for_players(
            &timing_profile,
            &timing_players,
            1.0,
            music_time_ns,
            2,
        );

        assert_eq!(
            rows[0],
            missed_note_cutoff_row_for_timing(&timing_a, cutoff_a_time_ns)
        );
        assert_eq!(
            rows[1],
            missed_note_cutoff_row_for_music_time(&timing_profile, &timing_b, 1.0, music_time_ns)
        );
    }

    #[test]
    fn timing_row_floor_steps_back_when_row_is_after_beat() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 2);

        assert_eq!(timing_row_floor(&timing, 1.0), ROWS_PER_BEAT as usize);
        assert_eq!(
            timing_row_floor(&timing, 1.0 - 0.001),
            ROWS_PER_BEAT as usize - 1
        );
        assert_eq!(timing_row_floor(&timing, -1.0), 0);
    }

    #[test]
    fn assist_row_no_offset_cancels_global_offset() {
        let timing = TimingData::from_segments(
            0.0,
            0.100,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let music_time_ns = song_time_ns_from_seconds(1.0);
        let direct_row = timing_row_floor(&timing, timing.get_beat_for_time_ns(music_time_ns));

        assert!(direct_row > ROWS_PER_BEAT as usize);
        assert_eq!(
            assist_row_no_offset_for_timing(&timing, 0.100, music_time_ns),
            ROWS_PER_BEAT as i32
        );
    }

    fn eventful_cache_timing(global_offset: f32, bpm_scale: f32) -> TimingData {
        TimingData::from_segments(
            0.0,
            global_offset,
            &TimingSegments {
                bpms: vec![
                    (0.0, 120.0 * bpm_scale),
                    (4.0, 180.0 * bpm_scale),
                    (8.0, 90.0 * bpm_scale),
                    (16.0, 240.0 * bpm_scale),
                ],
                stops: vec![StopSegment {
                    beat: 6.0,
                    duration: 0.250,
                }],
                delays: vec![DelaySegment {
                    beat: 10.0,
                    duration: 0.125,
                }],
                warps: vec![WarpSegment {
                    beat: 14.0,
                    length: 1.5,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 24),
        )
    }

    fn assert_beat_info_same(actual: BeatInfo, expected: BeatInfo) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.is_in_freeze, expected.is_in_freeze);
        assert_eq!(actual.is_in_delay, expected.is_in_delay);
    }

    fn assert_time_to_beat_cache_parity(
        cache: &mut GameplayTimeToBeatCaches,
        timing: &TimingData,
        player_timing: &TimingData,
        global_offset_seconds: f32,
        times: impl IntoIterator<Item = SongTimeNs>,
    ) {
        let players = std::array::from_fn(|player| {
            if player == 0 { timing } else { player_timing }
        });
        let profile = TimingProfile::default_itg_with_fa_plus();
        for time_ns in times {
            assert_beat_info_same(
                cache.song_info(timing, time_ns),
                timing.get_beat_info_from_time_ns(time_ns),
            );
            let display_time_ns = time_ns.saturating_sub(7_000_000);
            assert_eq!(
                cache.display_beat(timing, display_time_ns).to_bits(),
                timing.get_beat_for_time_ns(display_time_ns).to_bits(),
            );
            let assist_row =
                assist_row_no_offset_for_timing(timing, global_offset_seconds, time_ns);
            assert_eq!(
                cache.assist_row_no_offset(timing, global_offset_seconds, time_ns),
                assist_row,
            );
            assert_eq!(
                cache.assist_future_row(
                    timing,
                    global_offset_seconds,
                    0.030,
                    time_ns,
                    1.35,
                    assist_row,
                ),
                assist_lookahead_future_row(
                    timing,
                    global_offset_seconds,
                    0.030,
                    time_ns,
                    1.35,
                    assist_row,
                ),
            );
            for (player, timing_player) in players.iter().take(2).enumerate() {
                let visible_time_ns = time_ns.saturating_sub((player as i64 + 1) * 11_000_000);
                assert_eq!(
                    cache.visible_beat(player, timing_player, visible_time_ns).to_bits(),
                    timing_player.get_beat_for_time_ns(visible_time_ns).to_bits(),
                );
                let expected_rows = lane_search_rows_for_timing(timing_player, time_ns);
                assert_eq!(cache.lane_search_rows(player, timing_player, time_ns), expected_rows);
                assert_eq!(cache.lane_search_rows(player, timing_player, time_ns), expected_rows);
            }
            let expected_cutoffs =
                missed_note_cutoff_rows_for_players(&profile, &players, 1.35, time_ns, 2);
            assert_eq!(
                cache.missed_note_cutoff_rows(&profile, &players, 1.35, time_ns, 2),
                expected_cutoffs,
            );
            assert_eq!(
                cache.missed_note_cutoff_rows(&profile, &players, 1.35, time_ns, 2),
                expected_cutoffs,
            );
        }
    }

    #[test]
    fn time_to_beat_caches_match_uncached_across_events_and_rewinds() {
        let timing = eventful_cache_timing(0.012, 1.0);
        let player_timing = eventful_cache_timing(-0.021, 1.1);
        let players = std::array::from_fn(|player| {
            if player == 0 { &timing } else { &player_timing }
        });
        let mut cache = GameplayTimeToBeatCaches::new(&timing, &players);
        let start = song_time_ns_from_seconds(-0.5);
        let end = timing
            .get_time_for_beat_ns(24.0)
            .saturating_add(song_time_ns_from_seconds(0.5));
        let step = (end - start) / 255;

        assert_time_to_beat_cache_parity(
            &mut cache,
            &timing,
            &player_timing,
            0.012,
            (0..=255).map(|i| start.saturating_add(step.saturating_mul(i))),
        );
        assert_time_to_beat_cache_parity(
            &mut cache,
            &timing,
            &player_timing,
            0.012,
            (0..=64).rev().map(|i| start.saturating_add(step.saturating_mul(i * 3))),
        );
    }

    #[test]
    fn time_to_beat_cache_reset_invalidates_same_timestamp_memos() {
        let mut timing = eventful_cache_timing(0.0, 1.0);
        let time_ns = timing.get_time_for_beat_ns(8.0);
        let players = std::array::from_fn(|_| &timing);
        let mut cache = GameplayTimeToBeatCaches::new(&timing, &players);
        let before = cache.lane_search_rows(0, &timing, time_ns);

        timing.set_global_offset_seconds(0.250);
        let players = std::array::from_fn(|_| &timing);
        cache.reset(&timing, &players);
        let after = cache.lane_search_rows(0, &timing, time_ns);

        assert_eq!(after, lane_search_rows_for_timing(&timing, time_ns));
        assert_ne!(after, before);
    }

    #[test]
    fn note_count_stats_group_rows_and_clamp_range() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];

        let stats = build_note_count_stats(&notes, (0, 99));

        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].beat, 1.0);
        assert_eq!(stats[0].notes_lower, 0);
        assert_eq!(stats[0].notes_upper, 2);
        assert_eq!(stats[1].beat, 2.0);
        assert_eq!(stats[1].notes_lower, 2);
        assert_eq!(stats[1].notes_upper, 3);
    }

    #[test]
    fn note_count_stats_state_returns_player_slices() {
        let mut stats: [Vec<NoteCountStat>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        stats[1].push(NoteCountStat {
            beat: 4.0,
            notes_lower: 7,
            notes_upper: 9,
        });
        let state = GameplayNoteCountStatsState::new(stats);

        assert!(state.player_stats(0).is_empty());
        assert_eq!(state.player_stats(1)[0].beat, 4.0);
        assert_eq!(state.player_stats(1)[0].notes_lower, 7);
        assert!(state.player_stats(MAX_PLAYERS).is_empty());
    }

    #[test]
    fn lane_index_state_returns_slices_flags_and_clears() {
        let mut note_indices: [Vec<usize>; MAX_COLS] = std::array::from_fn(|_| Vec::new());
        let mut note_row_indices: [Vec<usize>; MAX_COLS] = std::array::from_fn(|_| Vec::new());
        let mut hold_indices: [Vec<usize>; MAX_COLS] = std::array::from_fn(|_| Vec::new());
        note_indices[1].push(7);
        note_row_indices[1].push(8);
        hold_indices[1].push(9);
        let mut state = GameplayLaneIndexState::new(
            note_indices,
            note_row_indices,
            hold_indices,
            vec![12, 24],
            vec![0, 3],
        );

        assert_eq!(state.note_indices(1), &[7]);
        assert_eq!(state.note_row_indices(1), &[8]);
        assert_eq!(state.hold_indices(1), &[9]);
        assert_eq!(state.note_itg_rows(), &[12, 24]);
        assert_eq!(state.tap_row_hold_roll_flags(1), 3);
        assert!(state.note_indices(MAX_COLS).is_empty());
        assert_eq!(state.tap_row_hold_roll_flags(99), 0);

        state.clear_for_benchmark();

        assert!(state.note_indices(1).is_empty());
        assert!(state.note_row_indices(1).is_empty());
        assert!(state.hold_indices(1).is_empty());
        assert!(state.note_itg_rows().is_empty());
        assert_eq!(state.tap_row_hold_roll_flags(1), 0);
    }

    #[test]
    fn row_index_state_clears_ranges_cursors_and_caches() {
        let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        row_map_cache[0].extend([u32::MAX, 4]);
        let mut state = GameplayRowIndexState::new(
            [(1, 3), (4, 6)],
            [2, 5],
            row_map_cache,
            vec![0, 1, u32::MAX],
        );

        assert_eq!(state.row_entry_ranges[0], (1, 3));
        assert_eq!(state.judged_row_cursor[1], 5);
        assert_eq!(state.row_map_cache[0][1], 4);
        assert_eq!(state.note_row_entry_indices[1], 1);

        state.clear_for_benchmark();

        assert_eq!(state.row_entry_ranges, [(0, 0); MAX_PLAYERS]);
        assert_eq!(state.judged_row_cursor, [0; MAX_PLAYERS]);
        assert!(state.row_map_cache[0].is_empty());
        assert!(state.note_row_entry_indices.is_empty());
    }

    #[test]
    fn mine_scan_state_initializes_cursor_data_and_clears() {
        let mut mine_note_ix: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        let mut mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS] =
            std::array::from_fn(|_| Vec::new());
        mine_note_ix[1].extend([4, 8]);
        mine_note_time_ns[1].extend([40, 80]);
        let mut state = GameplayMineScanState::new([2, 5], mine_note_ix, mine_note_time_ns);

        assert_eq!(state.next_tap_miss_cursor, [2, 5]);
        assert_eq!(state.next_mine_avoid_cursor, [2, 5]);
        assert_eq!(state.next_mine_ix_cursor, [0; MAX_PLAYERS]);
        assert_eq!(state.mine_note_ix[1], [4, 8]);
        assert_eq!(state.mine_note_time_ns[1], [40, 80]);

        state.set_next_tap_miss_cursor(1, 9);
        state.pending_mine_hit_indices.push(4);
        state.clear_for_benchmark();

        assert_eq!(state.next_tap_miss_cursor, [0; MAX_PLAYERS]);
        assert_eq!(state.next_mine_avoid_cursor, [0; MAX_PLAYERS]);
        assert_eq!(state.next_mine_ix_cursor, [0; MAX_PLAYERS]);
        assert!(state.mine_note_ix[1].is_empty());
        assert!(state.mine_note_time_ns[1].is_empty());
        assert!(state.pending_mine_hit_indices.is_empty());
    }

    #[test]
    fn hold_runtime_state_initializes_resets_and_clears() {
        let mut state = GameplayHoldRuntimeState::new(3, 4);
        assert_eq!(state.decaying_hold_indices.capacity(), 4);
        assert_eq!(state.hold_decay_active, [false; 3]);
        assert_eq!(state.tap_miss_held_window, [false; 3]);
        assert_eq!(state.pending_missed_hold_resolution, [false; 3]);
        assert!(state.pending_missed_hold_indices.is_empty());

        state.decaying_hold_indices.extend([1, 2]);
        state.hold_decay_active[1] = true;
        state.tap_miss_held_window[2] = true;
        state.pending_missed_hold_resolution[0] = true;
        state.pending_missed_hold_indices.push(0);
        state.reset_live_state();

        assert!(state.decaying_hold_indices.is_empty());
        assert_eq!(state.hold_decay_active, [false; 3]);
        assert_eq!(state.tap_miss_held_window, [false; 3]);
        assert_eq!(state.pending_missed_hold_resolution, [false; 3]);
        assert!(state.pending_missed_hold_indices.is_empty());

        state.decaying_hold_indices.push(1);
        state.clear_for_benchmark();
        assert!(state.decaying_hold_indices.is_empty());
        assert!(state.hold_decay_active.is_empty());
        assert!(state.tap_miss_held_window.is_empty());
        assert!(state.pending_missed_hold_resolution.is_empty());
    }

    #[test]
    fn cue_runtime_state_returns_values_sets_cues_and_clears() {
        let mut measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS] =
            std::array::from_fn(|_| Vec::new());
        let mut column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        let mut crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
        measure_counter_segments[0].push(StreamSegment {
            start: 1,
            end: 3,
            is_break: false,
        });
        column_cues[0].push(ColumnCue {
            start_time: 1.0,
            duration: 2.0,
            columns: vec![ColumnCueColumn {
                column: 1,
                is_mine: false,
            }],
        });
        crossover_cues[1].push(ColumnCue {
            start_time: 3.0,
            duration: 4.0,
            columns: vec![ColumnCueColumn {
                column: 5,
                is_mine: true,
            }],
        });
        let mut state =
            GameplayCueRuntimeState::new(measure_counter_segments, column_cues, crossover_cues);

        assert_eq!(state.measure_counter_segments(0)[0].start, 1);
        assert_eq!(state.column_cues(0)[0].columns[0].column, 1);
        assert!(state.column_cues(MAX_PLAYERS).is_empty());
        assert!(state.crossover_cues(0).is_empty());
        assert_eq!(state.crossover_cues(1)[0].columns[0].column, 5);

        state.set_column_cues_for_benchmark(
            1,
            vec![ColumnCue {
                start_time: 8.0,
                duration: 1.0,
                columns: vec![ColumnCueColumn {
                    column: 7,
                    is_mine: false,
                }],
            }],
        );
        assert_eq!(state.column_cues(1)[0].columns[0].column, 7);

        state.clear_for_benchmark();

        assert!(state.measure_counter_segments(0).is_empty());
        assert!(state.column_cues(0).is_empty());
        assert!(state.column_cues(1).is_empty());
        assert!(state.crossover_cues(1).is_empty());
    }

    #[test]
    fn crossover_cue_anchor_tracks_entry_time() {
        let new_state = || {
            let measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS] =
                std::array::from_fn(|_| Vec::new());
            let column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
            let mut crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS] =
                std::array::from_fn(|_| Vec::new());
            // Two well-separated cues so seeking lands clearly inside one.
            crossover_cues[0].push(ColumnCue {
                start_time: 1.0,
                duration: 1.0,
                columns: Vec::new(),
            });
            crossover_cues[0].push(ColumnCue {
                start_time: 5.0,
                duration: 1.0,
                columns: Vec::new(),
            });
            GameplayCueRuntimeState::new(measure_counter_segments, column_cues, crossover_cues)
        };

        // Normal forward play: each cue is caught near its start, so it anchors
        // to its own start (natural fade-in).
        let mut state = new_state();
        state.update_crossover_cue_anchors(0, 0.5);
        state.update_crossover_cue_anchors(0, 1.0 + 0.01);
        assert_eq!(state.crossover_cue_entry_time(0, 0), Some(1.0));

        // Seek straight into the middle of cue 1: it anchors to the landing time,
        // so the renderer fades it in from there instead of popping it in.
        state.update_crossover_cue_anchors(0, 5.5);
        assert_eq!(state.crossover_cue_entry_time(0, 1), Some(5.5));
        // Cue 0 keeps its earlier natural anchor.
        assert_eq!(state.crossover_cue_entry_time(0, 0), Some(1.0));

        // Rewind before cue 1's start, then catch it from the start: it
        // re-anchors to its own start.
        state.update_crossover_cue_anchors(0, 4.0);
        assert_eq!(state.crossover_cue_entry_time(0, 1), None);
        state.update_crossover_cue_anchors(0, 5.0 + 0.01);
        assert_eq!(state.crossover_cue_entry_time(0, 1), Some(5.0));

        // Out-of-range / untracked players have no anchor (callers fall back to
        // the cue's own start).
        assert_eq!(state.crossover_cue_entry_time(0, 99), None);
        assert_eq!(state.crossover_cue_entry_time(1, 0), None);
    }

    #[test]
    fn hold_feedback_state_returns_slices_and_clears() {
        let mut state = GameplayHoldFeedbackState::default();
        state.hold_judgments[1] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 2.0,
        });
        state.held_miss_judgments[2] = Some(HeldMissRenderInfo {
            started_at_screen_s: 3.0,
        });

        assert_eq!(
            state.hold_judgment(1).map(|judgment| judgment.result),
            Some(HoldResult::Held)
        );
        assert!(state.hold_judgment(MAX_COLS).is_none());
        assert_eq!(state.hold_judgments(1, 2).len(), 2);
        assert_eq!(state.hold_judgments(MAX_COLS, 1).len(), 0);
        assert_eq!(state.held_miss_judgments(2, 1).len(), 1);

        state.clear();

        assert!(state.hold_judgments.iter().all(Option::is_none));
        assert!(state.held_miss_judgments.iter().all(Option::is_none));
    }

    #[test]
    fn visual_feedback_state_returns_values_sets_taps_and_clears() {
        let mut state = GameplayVisualFeedbackState::default();
        state.tap_explosions[1] = Some(ActiveTapExplosion {
            window: "W1",
            bright: true,
            elapsed: 0.1,
            duration: 0.5,
            start_beat: 4.0,
        });
        state.column_flashes[2] = Some(ActiveColumnFlash {
            grade: JudgeGrade::Great,
            blue_fantastic: false,
            started_at_screen_s: 3.0,
        });
        state.last_tap_judgments[3] = Some(ColumnTapJudgment {
            grade: JudgeGrade::Excellent,
            blue_fantastic: false,
            at_screen_s: 7.0,
        });
        state.mine_explosions[4] = Some(ActiveMineExplosion {
            elapsed: 0.0,
            duration: 1.0,
            started_at_screen_s: 8.0,
        });

        assert_eq!(
            state.tap_explosions(1, 1)[0].map(|explosion| explosion.window),
            Some("W1")
        );
        assert_eq!(
            state.column_flashes(2, 1)[0].map(|flash| flash.grade),
            Some(JudgeGrade::Great)
        );
        assert_eq!(
            state.last_tap_judgment(3).map(|judgment| judgment.grade),
            Some(JudgeGrade::Excellent)
        );
        assert_eq!(state.mine_started_at_screen_s(4), Some(8.0));
        assert!(state.mine_explosions(MAX_COLS, 1).is_empty());

        state.set_tap_explosion_for_benchmark(
            0,
            Some(ActiveTapExplosion {
                window: "W2",
                bright: false,
                elapsed: 0.0,
                duration: 0.25,
                start_beat: 2.0,
            }),
        );
        assert_eq!(
            state.tap_explosions(0, 1)[0].map(|explosion| explosion.window),
            Some("W2")
        );

        state.clear();

        assert!(state.tap_explosions.iter().all(Option::is_none));
        assert!(state.column_flashes.iter().all(Option::is_none));
        assert!(state.mine_explosions.iter().all(Option::is_none));
        assert!(state.last_tap_judgment(3).is_some());
    }

    #[test]
    fn first_time_index_lookup_uses_range_and_clamps_bounds() {
        let times = [10, 20, 30, 40];

        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 5), 1);
        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 25), 2);
        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 35), 3);
        assert_eq!(first_time_index_at_or_after(&times, (2, 99), 35), 3);
        assert_eq!(first_time_index_at_or_after(&times, (99, 100), 35), 4);
    }

    #[test]
    fn first_row_entry_lookup_uses_row_time_and_clamps_bounds() {
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(20), row(30), row(40)];

        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 5), 1);
        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 25), 2);
        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 35), 3);
        assert_eq!(
            first_row_entry_index_at_or_after_time(&rows, (2, 99), 35),
            3
        );
        assert_eq!(
            first_row_entry_index_at_or_after_time(&rows, (99, 100), 35),
            4
        );
    }

    #[test]
    fn score_rows_finalized_accepts_all_finalized_ranges() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(true), row(true)];
        let ranges = [(0, 2), (2, 3)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn score_rows_finalized_rejects_pending_row_in_active_range() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(false), row(true)];
        let ranges = [(0, 2), (2, 3)];

        assert!(!score_rows_finalized_for_players(&rows, &ranges, 2));
        assert!(score_rows_finalized_for_players(&rows, &ranges, 0));
    }

    #[test]
    fn score_rows_finalized_ignores_inactive_player_ranges() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(false)];
        let ranges = [(0, 1), (1, 2)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 1));
        assert!(!score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn score_rows_finalized_clamps_empty_out_of_bounds_ranges() {
        let rows: [RowEntry; 0] = [];
        let ranges = [(99, 100), (0, 0)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn practice_player_cursors_seek_notes_rows_and_mines() {
        let note_times = [10, 20, 30, 40];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30), row(50)];
        let mine_times = [15, 35];
        let mine_ix = [7, 9];

        assert_eq!(
            practice_player_cursors(
                &note_times,
                (1, 4),
                &rows,
                (0, 3),
                &mine_times,
                &mine_ix,
                25
            ),
            PracticePlayerCursors {
                note_cursor: 2,
                row_cursor: 1,
                mine_ix_cursor: 1,
                mine_avoid_cursor: 9,
            }
        );
    }

    #[test]
    fn practice_player_cursors_fall_back_to_note_end_after_last_mine() {
        let note_times = [10, 20, 30, 40];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30), row(50)];
        let mine_times = [15, 35];
        let mine_ix = [7, 9];

        assert_eq!(
            practice_player_cursors(
                &note_times,
                (1, 4),
                &rows,
                (0, 3),
                &mine_times,
                &mine_ix,
                99
            ),
            PracticePlayerCursors {
                note_cursor: 4,
                row_cursor: 3,
                mine_ix_cursor: 2,
                mine_avoid_cursor: 4,
            }
        );
    }

    #[test]
    fn practice_cursors_for_players_keeps_player_ranges_separate() {
        let note_times = [10, 20, 30, 40, 50, 60];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30), row(50), row(70)];
        let note_ranges = [(0, 3), (3, 6)];
        let row_ranges = [(0, 2), (2, 4)];
        let p1_mine_times = [15, 25];
        let p2_mine_times = [45, 55];
        let p1_mine_ix = [1, 2];
        let p2_mine_ix = [4, 5];

        let cursors = practice_cursors_for_players(
            &note_times,
            &note_ranges,
            &rows,
            &row_ranges,
            [p1_mine_times.as_slice(), p2_mine_times.as_slice()],
            [p1_mine_ix.as_slice(), p2_mine_ix.as_slice()],
            2,
            35,
        );

        assert_eq!(cursors.note_cursor, [3, 3]);
        assert_eq!(cursors.row_cursor, [2, 2]);
        assert_eq!(cursors.mine_ix_cursor, [2, 0]);
        assert_eq!(cursors.mine_avoid_cursor, [3, 4]);
    }

    #[test]
    fn practice_cursors_for_players_leaves_inactive_players_default() {
        let note_times = [10, 20, 30, 40];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30)];
        let mine_times = [15];
        let mine_ix = [1];

        let cursors = practice_cursors_for_players(
            &note_times,
            &[(0, 4), (99, 99)],
            &rows,
            &[(0, 2), (99, 99)],
            [mine_times.as_slice(), [].as_slice()],
            [mine_ix.as_slice(), [].as_slice()],
            1,
            20,
        );

        assert_eq!(cursors.note_cursor[0], 1);
        assert_eq!(cursors.row_cursor[0], 1);
        assert_eq!(cursors.mine_ix_cursor[0], 1);
        assert_eq!(cursors.mine_avoid_cursor[0], 4);
        assert_eq!(cursors.note_cursor[1], 0);
        assert_eq!(cursors.row_cursor[1], 0);
        assert_eq!(cursors.mine_ix_cursor[1], 0);
        assert_eq!(cursors.mine_avoid_cursor[1], 0);
    }

    #[test]
    fn row_entry_counts_unresolved_notes_and_rescore_tracks() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        judged.early_result = Some(Judgment {
            time_error_ms: -12.0,
            time_error_music_ns: -12_000_000,
            grade: JudgeGrade::Decent,
            window: Some(TimingWindow::W4),
            miss_because_held: false,
        });
        let notes = [
            judged,
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.row_index, 48);
        assert_eq!(row_entry.time_ns, note_times[0]);
        assert_eq!(row_entry.note_indices(), &[0, 1]);
        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
        assert_eq!(row_entry.unresolved_count, 1);
        assert_eq!(row_entry.unresolved_nonlift_count, 0);
        assert!(row_entry.had_provisional_early_hit);
        assert_eq!(row_entry.final_outcome, None);
    }

    #[test]
    fn row_entry_counts_unresolved_nonlift_notes() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.unresolved_count, 2);
        assert_eq!(row_entry.unresolved_nonlift_count, 1);
    }

    #[test]
    fn practice_reset_clears_note_mine_and_hold_results() {
        let judgment = Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        };
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 0.25;
        hold.let_go_started_at = Some(100);
        hold.let_go_starting_life = 0.25;
        hold.last_held_row_index = 0;
        hold.last_held_beat = 0.0;

        let mut notes = vec![
            test_note_at(NoteType::Hold, Some(hold), false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
        ];
        notes[0].result = Some(judgment);
        notes[0].early_result = Some(judgment);
        notes[1].mine_result = Some(MineResult::Hit);
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];

        reset_practice_notes_and_rows(&mut notes, &mut row_entries, &note_times);

        assert!(notes[0].result.is_none());
        assert!(notes[0].early_result.is_none());
        assert_eq!(notes[1].mine_result, None);
        let reset_hold = notes[0].hold.as_ref().expect("hold data");
        assert_eq!(reset_hold.result, None);
        assert_eq!(reset_hold.life, MAX_HOLD_LIFE);
        assert_eq!(reset_hold.let_go_started_at, None);
        assert_eq!(reset_hold.let_go_starting_life, MAX_HOLD_LIFE);
        assert_eq!(reset_hold.last_held_row_index, notes[0].row_index);
        assert_eq!(reset_hold.last_held_beat, notes[0].beat);
    }

    #[test]
    fn practice_reset_rebuilds_row_entries_from_reset_notes() {
        let judgment = Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        };
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        notes[0].result = Some(judgment);
        notes[0].early_result = Some(judgment);
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        reset_practice_notes_and_rows(&mut notes, &mut row_entries, &note_times);

        assert_eq!(row_entries[0].note_indices(), &[0]);
        assert_eq!(row_entries[0].unresolved_count, 1);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 1);
        assert!(!row_entries[0].had_provisional_early_hit);
        assert_eq!(row_entries[0].final_outcome, None);
    }

    #[test]
    fn timing_cache_refresh_updates_notes_holds_rows_and_mines() {
        let timing_p1 = test_timing(ROWS_PER_BEAT as usize * 4);
        let mut timing_p2 = test_timing(ROWS_PER_BEAT as usize * 4);
        timing_p2.set_global_offset_seconds(0.5);
        let timing_players: [&TimingData; MAX_PLAYERS] = [&timing_p1, &timing_p2];

        let mut hold = test_hold();
        hold.end_beat = 2.0;
        let mut p1_tap = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        p1_tap.column = 0;
        let mut p2_hold = test_note_at(NoteType::Hold, Some(hold), false, 48, 1.0);
        p2_hold.column = 4;
        let mut p2_mine = test_note_at(NoteType::Mine, None, false, 96, 2.0);
        p2_mine.column = 5;
        let notes = vec![p1_tap, p2_hold, p2_mine];
        let mut note_time_cache_ns = vec![0; notes.len()];
        let mut hold_end_time_cache_ns = vec![None; notes.len()];
        let mut row_note_indices = [usize::MAX; MAX_COLS];
        row_note_indices[0] = 1;
        let mut row_entries = vec![build_row_entry(48, row_note_indices, 1, &notes, &[0; 3])];
        let mine_note_ix: [Vec<usize>; MAX_PLAYERS] = [Vec::new(), vec![2]];
        let mut mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS] = [Vec::new(), Vec::new()];

        refresh_timing_caches_for_offset_change(
            &notes,
            &timing_players,
            2,
            4,
            &mut note_time_cache_ns,
            &mut hold_end_time_cache_ns,
            &mut row_entries,
            &mine_note_ix,
            &mut mine_note_time_ns,
        );

        assert_eq!(note_time_cache_ns[0], timing_p1.get_time_for_beat_ns(1.0));
        assert_eq!(note_time_cache_ns[1], timing_p2.get_time_for_beat_ns(1.0));
        assert_eq!(note_time_cache_ns[2], timing_p2.get_time_for_beat_ns(2.0));
        assert_eq!(
            hold_end_time_cache_ns[1],
            Some(timing_p2.get_time_for_beat_ns(2.0))
        );
        assert_eq!(row_entries[0].time_ns, note_time_cache_ns[1]);
        assert!(mine_note_time_ns[0].is_empty());
        assert_eq!(mine_note_time_ns[1], vec![note_time_cache_ns[2]]);
    }

    #[test]
    fn row_entry_provisional_early_marks_valid_row_only() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        let note_row_entry_indices = [u32::MAX, 0];

        assert!(!mark_row_entry_provisional_early_result(
            &mut row_entries,
            &note_row_entry_indices,
            0,
        ));
        assert!(!row_entries[0].had_provisional_early_hit);
        assert!(mark_row_entry_provisional_early_result(
            &mut row_entries,
            &note_row_entry_indices,
            1,
        ));
        assert!(row_entries[0].had_provisional_early_hit);
    }

    #[test]
    fn row_entry_note_finalized_decrements_counts() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let mut row_entries = vec![build_row_entry(48, note_indices, 2, &notes, &note_times)];
        let note_row_entry_indices = [0, 0];

        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            0,
            NoteType::Tap,
        ));
        assert_eq!(row_entries[0].unresolved_count, 1);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            1,
            NoteType::Lift,
        ));
        assert_eq!(row_entries[0].unresolved_count, 0);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
    }

    #[test]
    fn row_entry_note_finalized_saturates_and_ignores_missing_rows() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].unresolved_count = 0;
        row_entries[0].unresolved_nonlift_count = 0;
        let note_row_entry_indices = [0, 99];

        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            0,
            NoteType::Tap,
        ));
        assert_eq!(row_entries[0].unresolved_count, 0);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
        assert!(!mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            1,
            NoteType::Tap,
        ));
        assert!(!mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            2,
            NoteType::Tap,
        ));
    }

    #[test]
    fn cached_row_lookup_uses_row_map_and_final_outcome() {
        let mut note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        note.result = Some(test_judgment(JudgeGrade::Great));
        let notes = [note];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut row_map_cache = vec![u32::MAX; 49];
        row_map_cache[48] = 0;

        let row_entry =
            row_entry_for_cached_row(&row_entries, &row_map_cache, 48).expect("cached row");
        let outcome = finalized_row_outcome_for_cached_row(&row_entries, &row_map_cache, 48)
            .expect("finalized row outcome");

        assert_eq!(row_entry.note_indices(), &[0]);
        assert_eq!(outcome.final_grade, JudgeGrade::Great);
        assert_eq!(row_entry_index_for_cached_row(&row_map_cache, 47), None);
    }

    #[test]
    fn completed_row_hide_policy_uses_cached_final_grade() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        let mut row_map_cache = vec![u32::MAX; 49];
        row_map_cache[48] = 0;

        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        assert!(completed_row_hides_note(&row_entries, &row_map_cache, 48));
        let visibility = CompletedRowVisibility::new(&row_entries, &row_map_cache);
        assert!(visibility.hides_note(48));

        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Decent,
        });
        assert!(!completed_row_hides_note(&row_entries, &row_map_cache, 48));
        assert!(!completed_row_hides_note(&row_entries, &row_map_cache, 47));
    }

    #[test]
    fn completed_row_judgment_waits_for_all_notes_and_returns_indices() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));
        let pending = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        let notes = [judged, pending];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);
        assert!(completed_row_final_judgment(&notes, &row_entry).is_none());

        let mut notes = notes;
        notes[1].result = Some(test_judgment(JudgeGrade::Great));
        let judgment =
            completed_row_final_judgment(&notes, &row_entry).expect("completed row judgment");
        let finalized =
            finalized_row_judgment_for_entry(&notes, &row_entry).expect("finalized row judgment");
        let plan = completed_row_tap_feedback_plan(&notes, &row_entry)
            .expect("completed row tap feedback plan");
        let (indices, len, flash_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed row flash judgment");

        assert_eq!(judgment.grade, JudgeGrade::Great);
        assert_eq!(finalized.judgment.grade, JudgeGrade::Great);
        assert_eq!(finalized.note_count, 2);
        assert_eq!(finalized.outcome.final_grade, JudgeGrade::Great);
        assert_eq!(flash_judgment.grade, JudgeGrade::Great);
        assert_eq!(plan.judgment.grade, JudgeGrade::Great);
        assert_eq!(plan.receptor_window, Some("W3"));
        assert_eq!(&plan.note_indices[..plan.note_count], &[0, 1]);
        assert_eq!(&indices[..len], &[0, 1]);
    }

    #[test]
    fn finalized_row_judgment_rejects_missing_note_indices() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));
        let notes = [judged];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 99;
        let row_entry = RowEntry {
            row_index: 48,
            time_ns: song_time_ns_from_seconds(1.0),
            nonmine_note_indices: note_indices,
            nonmine_note_count: 2,
            rescore_track_count: 2,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };

        assert!(finalized_row_judgment_for_entry(&notes, &row_entry).is_none());
    }

    #[test]
    fn judged_row_cursor_skips_finalized_and_finds_ready_rows() {
        let mut row1_note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        row1_note.result = Some(test_judgment(JudgeGrade::Great));
        let row2_note = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        let mut row3_note = test_note_at(NoteType::Tap, None, false, 144, 3.0);
        row3_note.result = Some(test_judgment(JudgeGrade::Great));
        row3_note.early_result = Some(test_judgment(JudgeGrade::Decent));
        let notes = [row1_note, row2_note, row3_note];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row1_indices = [usize::MAX; MAX_COLS];
        let mut row2_indices = [usize::MAX; MAX_COLS];
        let mut row3_indices = [usize::MAX; MAX_COLS];
        row1_indices[0] = 0;
        row2_indices[0] = 1;
        row3_indices[0] = 2;
        let mut row_entries = vec![
            build_row_entry(48, row1_indices, 1, &notes, &note_times),
            build_row_entry(96, row2_indices, 1, &notes, &note_times),
            build_row_entry(144, row3_indices, 1, &notes, &note_times),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let lookahead = song_time_ns_from_seconds(3.5);
        let cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });
        let entry_cursor = advance_judged_row_cursor_for_entries(
            &row_entries,
            (0, row_entries.len()),
            0,
            lookahead,
        );
        let ready = next_ready_row_in_lookahead(cursor, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });

        assert_eq!(cursor, 1);
        assert_eq!(entry_cursor, cursor);
        assert_eq!(ready, Some((2, 144, true)));
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::Decent));
        assert!(!suppress_final_bad_rescore_visual(true, JudgeGrade::Great));
    }

    #[test]
    fn ready_judged_row_collection_skips_pending_and_finalized_rows() {
        let mut row1_note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        row1_note.result = Some(test_judgment(JudgeGrade::Great));
        let row2_note = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        let mut row3_note = test_note_at(NoteType::Tap, None, false, 144, 3.0);
        row3_note.result = Some(test_judgment(JudgeGrade::Great));
        row3_note.early_result = Some(test_judgment(JudgeGrade::Decent));
        let notes = [row1_note, row2_note, row3_note];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row1_indices = [usize::MAX; MAX_COLS];
        let mut row2_indices = [usize::MAX; MAX_COLS];
        let mut row3_indices = [usize::MAX; MAX_COLS];
        row1_indices[0] = 0;
        row2_indices[0] = 1;
        row3_indices[0] = 2;
        let mut row_entries = vec![
            build_row_entry(48, row1_indices, 1, &notes, &note_times),
            build_row_entry(96, row2_indices, 1, &notes, &note_times),
            build_row_entry(144, row3_indices, 1, &notes, &note_times),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut events = [None; 4];

        let update = collect_ready_judged_row_events(
            &row_entries,
            (0, row_entries.len()),
            0,
            song_time_ns_from_seconds(3.5),
            &mut events,
        );

        assert_eq!(
            update,
            ReadyJudgedRowsUpdate {
                next_scan_start: 3,
                event_count: 1,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(ReadyJudgedRowEvent {
                row_entry_index: 2,
                row_index: 144,
                skip_life_change: true,
            })
        );
    }

    #[test]
    fn ready_judged_row_collection_stops_when_event_buffer_fills() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        notes[0].result = Some(test_judgment(JudgeGrade::Great));
        notes[1].result = Some(test_judgment(JudgeGrade::Excellent));
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
        ];
        let mut row1_indices = [usize::MAX; MAX_COLS];
        let mut row2_indices = [usize::MAX; MAX_COLS];
        row1_indices[0] = 0;
        row2_indices[0] = 1;
        let row_entries = vec![
            build_row_entry(48, row1_indices, 1, &notes, &note_times),
            build_row_entry(96, row2_indices, 1, &notes, &note_times),
        ];
        let mut events = [None; 1];

        let first = collect_ready_judged_row_events(
            &row_entries,
            (0, row_entries.len()),
            0,
            song_time_ns_from_seconds(3.0),
            &mut events,
        );
        assert_eq!(
            first,
            ReadyJudgedRowsUpdate {
                next_scan_start: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(
            events[0],
            Some(ReadyJudgedRowEvent {
                row_entry_index: 0,
                row_index: 48,
                skip_life_change: false,
            })
        );

        let second = collect_ready_judged_row_events(
            &row_entries,
            (0, row_entries.len()),
            first.next_scan_start,
            song_time_ns_from_seconds(3.0),
            &mut events,
        );
        assert_eq!(
            second,
            ReadyJudgedRowsUpdate {
                next_scan_start: 2,
                event_count: 1,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(ReadyJudgedRowEvent {
                row_entry_index: 1,
                row_index: 96,
                skip_life_change: false,
            })
        );
    }

    #[test]
    fn finalized_row_awards_hand_for_notes_or_carried_holds() {
        assert!(finalized_row_awards_hand(JudgeGrade::Great, 3, 0));
        assert!(finalized_row_awards_hand(JudgeGrade::Excellent, 2, 1));
        assert!(finalized_row_awards_hand(JudgeGrade::Fantastic, 1, 2));
        assert!(!finalized_row_awards_hand(JudgeGrade::Great, 2, 0));
    }

    #[test]
    fn finalized_row_awards_hand_suppresses_bad_rows() {
        assert!(!finalized_row_awards_hand(JudgeGrade::Miss, 4, 0));
        assert!(!finalized_row_awards_hand(JudgeGrade::WayOff, 4, 0));
        assert!(finalized_row_awards_hand(JudgeGrade::Decent, 3, 0));
    }

    #[test]
    fn row_finalization_player_state_records_counts_combo_and_hands() {
        let mut state = RowFinalizationPlayerState::default();
        let judgment = test_judgment(JudgeGrade::Great);

        let update = apply_row_finalization_player_state(&mut state, &judgment, 2, 1, false);

        let ix = judgment::display_judge_ix(JudgeGrade::Great);
        assert_eq!(state.judgment_counts[ix], 1);
        assert_eq!(state.scoring_counts[ix], 1);
        assert_eq!(state.current_combo_window_counts.w3, 1);
        assert_eq!(state.combo.combo, 2);
        assert_eq!(state.combo.miss_combo, 0);
        assert_eq!(state.combo.current_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(state.hands_achieved, 1);
        assert_eq!(
            update,
            RowFinalizationPlayerUpdate {
                combo_update: ComboUpdate::default(),
                update_grade_totals: true,
                awarded_hand: true,
            }
        );
    }

    #[test]
    fn row_finalization_plan_keeps_visuals_for_scoring_blocked_rows() {
        let judgment = test_judgment(JudgeGrade::Excellent);
        let plan = row_finalization_plan(
            FinalizedRowJudgment {
                judgment,
                note_count: 2,
                outcome: FinalizedRowOutcome {
                    final_grade: JudgeGrade::Excellent,
                },
            },
            true,
            false,
        );

        assert_eq!(plan.judgment.grade, JudgeGrade::Excellent);
        assert_eq!(plan.note_count, 2);
        assert_eq!(
            plan.outcome,
            FinalizedRowOutcome {
                final_grade: JudgeGrade::Excellent,
            }
        );
        assert!(plan.show_final_visual);
        assert_near(
            plan.life_delta,
            deadsync_rules::life::judge_life_delta(JudgeGrade::Excellent),
        );
        assert!(!plan.record_display_window_counts);
        assert!(!plan.apply_player_state);
        assert!(!plan.apply_life_change);
        assert!(!plan.capture_failed_ex_score_inputs);
    }

    #[test]
    fn row_finalization_plan_suppresses_bad_early_rescore_life() {
        let judgment = test_judgment(JudgeGrade::Decent);
        let plan = row_finalization_plan(
            FinalizedRowJudgment {
                judgment,
                note_count: 1,
                outcome: FinalizedRowOutcome {
                    final_grade: JudgeGrade::Decent,
                },
            },
            false,
            true,
        );

        assert_eq!(plan.judgment.grade, JudgeGrade::Decent);
        assert_eq!(plan.note_count, 1);
        assert_eq!(
            plan.outcome,
            FinalizedRowOutcome {
                final_grade: JudgeGrade::Decent,
            }
        );
        assert!(!plan.show_final_visual);
        assert_near(
            plan.life_delta,
            deadsync_rules::life::judge_life_delta(JudgeGrade::Decent),
        );
        assert!(plan.record_display_window_counts);
        assert!(plan.apply_player_state);
        assert!(!plan.apply_life_change);
        assert!(!plan.capture_failed_ex_score_inputs);
    }

    #[test]
    fn row_finalization_plan_for_entry_uses_completed_row_judgment() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.result = Some(test_judgment(JudgeGrade::Fantastic));
        let mut second = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        second.result = Some(test_judgment(JudgeGrade::Great));
        let notes = [first, second];
        let mut indices = [usize::MAX; MAX_COLS];
        indices[0] = 0;
        indices[1] = 1;
        let note_times = [1_000, 1_000];
        let row_entry = build_row_entry(48, indices, 2, &notes, &note_times);

        let plan =
            row_finalization_plan_for_entry(&notes, &row_entry, false, false).expect("row plan");

        assert_eq!(plan.judgment.grade, JudgeGrade::Great);
        assert_eq!(plan.note_count, 2);
        assert_eq!(
            plan.outcome,
            FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }
        );
        assert!(plan.show_final_visual);
        assert_near(
            plan.life_delta,
            deadsync_rules::life::judge_life_delta(JudgeGrade::Great),
        );
        assert!(plan.record_display_window_counts);
        assert!(plan.apply_player_state);
        assert!(plan.apply_life_change);
        assert!(plan.capture_failed_ex_score_inputs);
    }

    #[test]
    fn row_finalization_plan_for_entry_rejects_incomplete_rows() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let mut indices = [usize::MAX; MAX_COLS];
        indices[0] = 0;
        let note_times = [1_000];
        let row_entry = build_row_entry(48, indices, 1, &notes, &note_times);

        assert!(row_finalization_plan_for_entry(&notes, &row_entry, false, false).is_none());
    }

    #[test]
    fn row_finalization_player_state_skips_scoring_for_dead_player() {
        let mut state = RowFinalizationPlayerState {
            combo: ComboState {
                combo: 12,
                full_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                ..ComboState::default()
            },
            ..RowFinalizationPlayerState::default()
        };
        let judgment = test_judgment(JudgeGrade::Miss);

        let update = apply_row_finalization_player_state(&mut state, &judgment, 1, 0, true);

        let ix = judgment::display_judge_ix(JudgeGrade::Miss);
        assert_eq!(state.judgment_counts[ix], 1);
        assert_eq!(state.scoring_counts[ix], 0);
        assert_eq!(state.current_combo_window_counts.miss, 1);
        assert_eq!(state.combo.combo, 0);
        assert_eq!(state.combo.miss_combo, 1);
        assert_eq!(state.combo.full_combo_grade, None);
        assert!(state.combo.first_fc_attempt_broken);
        assert_eq!(state.hands_achieved, 0);
        assert_eq!(
            update,
            RowFinalizationPlayerUpdate {
                combo_update: ComboUpdate {
                    combo_broken: true,
                    ..ComboUpdate::default()
                },
                update_grade_totals: false,
                awarded_hand: false,
            }
        );
    }

    #[test]
    fn carried_holds_down_at_row_counts_engaged_prior_holds() {
        let mut held = test_hold();
        held.last_held_row_index = 96;
        let notes = vec![
            test_note_at(NoteType::Hold, Some(held), false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let mut active = vec![None; 4];
        let mut hold = test_active_hold(NoteType::Hold, true, false, 1.0);
        hold.note_index = 0;
        active[1] = Some(hold);

        let count = carried_holds_down_at_row(&notes, &active, (0, 4), 96);

        assert_eq!(count, 1);
    }

    #[test]
    fn carried_holds_down_at_row_ignores_inactive_and_future_holds() {
        let mut carried = test_hold();
        carried.last_held_row_index = 96;
        let mut short = test_hold();
        short.last_held_row_index = 72;
        let notes = vec![
            test_note_at(NoteType::Hold, Some(carried.clone()), false, 48, 1.0),
            test_note_at(NoteType::Hold, Some(short), false, 48, 1.0),
            test_note_at(NoteType::Hold, Some(carried), false, 96, 2.0),
        ];
        let mut active = vec![None; 5];
        let mut let_go = test_active_hold(NoteType::Hold, true, true, 1.0);
        let_go.note_index = 0;
        active[1] = Some(let_go);
        let mut depleted = test_active_hold(NoteType::Hold, true, false, 0.0);
        depleted.note_index = 0;
        active[2] = Some(depleted);
        let mut ended_before_row = test_active_hold(NoteType::Hold, true, false, 1.0);
        ended_before_row.note_index = 1;
        active[3] = Some(ended_before_row);
        let mut starts_on_row = test_active_hold(NoteType::Hold, true, false, 1.0);
        starts_on_row.note_index = 2;
        active[4] = Some(starts_on_row);

        let count = carried_holds_down_at_row(&notes, &active, (0, 5), 96);

        assert_eq!(count, 0);
    }

    #[test]
    fn carried_holds_down_at_row_clamps_ranges_and_note_indices() {
        let mut held = test_hold();
        held.last_held_row_index = 144;
        let notes = vec![test_note_at(NoteType::Hold, Some(held), false, 48, 1.0)];
        let mut active = vec![None; 3];
        let mut valid = test_active_hold(NoteType::Hold, true, false, 1.0);
        valid.note_index = 0;
        active[1] = Some(valid);
        let mut invalid_note = test_active_hold(NoteType::Hold, true, false, 1.0);
        invalid_note.note_index = 99;
        active[2] = Some(invalid_note);

        assert_eq!(carried_holds_down_at_row(&notes, &active, (1, 99), 96), 1);
        assert_eq!(carried_holds_down_at_row(&notes, &active, (99, 100), 96), 0);
    }

    #[test]
    fn row_grids_group_sorted_rows_and_ignore_out_of_range_columns() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let mut notes = notes;
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 3;
        notes[3].column = 5;

        let rows = build_row_grids(&notes, (0, notes.len()), 0, 4);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index, 48);
        assert_eq!(rows[0].note_indices[0], 1);
        assert_eq!(rows[0].note_indices[2], 0);
        assert_eq!(rows[1].row_index, 96);
        assert_eq!(rows[1].note_indices[3], 2);
        assert_eq!(rows[1].note_indices[0], usize::MAX);
    }

    #[test]
    fn player_rows_filter_by_column_range_and_sort_unique() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
        ];
        notes[0].column = 5;
        notes[1].column = 2;
        notes[2].column = 3;
        notes[3].column = 9;

        assert_eq!(local_player_col(5, 2, 4), Some(3));
        assert_eq!(local_player_col(1, 2, 4), None);
        assert_eq!(player_rows(&notes, 2, 4), vec![48, 96]);

        sort_player_notes(&mut notes);
        let rows_and_cols: Vec<(usize, usize)> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(rows_and_cols, vec![(48, 2), (48, 3), (96, 5), (144, 9)]);
    }

    #[test]
    fn column_field_helpers_resolve_player_and_local_column() {
        assert_eq!(player_index_for_column(1, 4, 7), 0);
        assert_eq!(player_index_for_column(2, 0, 7), 0);
        assert_eq!(player_index_for_column(2, 4, 0), 0);
        assert_eq!(player_index_for_column(2, 4, 3), 0);
        assert_eq!(player_index_for_column(2, 4, 4), 1);
        assert_eq!(player_index_for_column(2, 4, 9), 1);

        assert_eq!(local_column_for_field(0, 6), 6);
        assert_eq!(local_column_for_field(4, 6), 2);

        assert_eq!(player_column_range(4, 0), (0, 4));
        assert_eq!(player_column_range(4, 1), (4, 8));

        let ranges = [(0, 10), (10, 20)];
        assert_eq!(player_note_range_for_ranges(&ranges, 2, 0), (0, 10));
        assert_eq!(player_note_range_for_ranges(&ranges, 2, 1), (10, 20));
        assert_eq!(player_note_range_for_ranges(&ranges, 2, 2), (0, 0));
        assert_eq!(player_note_range_for_ranges(&ranges[..1], 2, 1), (0, 0));
    }

    #[test]
    fn simultaneous_limit_counts_active_holds_before_row_taps() {
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 0;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;
        let mut tap1 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap1.column = 1;
        let mut tap2 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap2.column = 2;
        let mut notes = vec![hold, tap1, tap2];

        enforce_max_simultaneous_notes(&mut notes, 2, 0, 4);

        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].column, notes[0].row_index), (0, 0));
        assert_eq!((notes[1].column, notes[1].row_index), (2, 48));
    }

    #[test]
    fn row_track_helpers_count_taps_holds_and_first_tracks() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, true, 48, 1.0),
            test_note_at(NoteType::Hold, Some(test_hold()), false, 24, 0.5),
        ];
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 1;
        notes[3].column = 3;
        notes[3]
            .hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;

        assert_eq!(count_nonempty_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_or_hold_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_tracks_at_row(&notes, 48, 0, 4), 2);
        assert_eq!(first_nonempty_track_at_row(&notes, 48, 0, 4), Some(0));
        assert_eq!(first_tap_track_at_row(&notes, 48, 0, 4), Some(0));
        assert!(is_hold_body_at_row(&notes, 48, 3));
        assert_eq!(count_held_tracks_at_row(&notes, 48, 0, 4), 1);
        assert!(cell_has_any_note(&notes, 48, 1));
        assert!(!cell_has_nonfake_note(&notes, 48, 1));
        assert_eq!(stomp_mirror_track(1, 4), 2);
    }

    #[test]
    fn added_notes_replace_existing_cell_and_use_timing() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];

        assert!(set_added_mine_note(&mut notes, &timing, 48, 0));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_type, NoteType::Mine);
        assert_eq!(notes[0].beat, 1.0);

        assert!(set_added_tap_note(&mut notes, &timing, 96, 1));
        assert!(cell_has_any_note(&notes, 96, 1));
        remove_cell_notes(&mut notes, 96, 1);
        assert!(!cell_has_any_note(&notes, 96, 1));
    }

    #[test]
    fn mines_insert_converts_every_sixth_nonempty_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(5 * ROWS_PER_BEAT as usize),
        );
        let mut notes = (0..6)
            .map(|i| {
                test_note_at(
                    NoteType::Tap,
                    None,
                    false,
                    i * ROWS_PER_BEAT as usize,
                    i as f32,
                )
            })
            .collect::<Vec<_>>();

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            5 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 5 * ROWS_PER_BEAT as usize && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn mines_insert_adds_mine_half_beat_after_hold_end() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(3 * ROWS_PER_BEAT as usize),
        );
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 1;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 2 * ROWS_PER_BEAT as usize;
        let mut notes = vec![hold];

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            3 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 2 * ROWS_PER_BEAT as usize + (ROWS_PER_BEAT as usize / 2)
                && note.column == 1
                && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn mines_insert_batched_conversion_matches_legacy_chart() {
        let last_row = 40 * ROWS_PER_BEAT as usize;
        let timing = test_timing(last_row);
        let mut current = (0..120)
            .map(|index| {
                let row = index * (ROWS_PER_BEAT as usize / 4);
                let mut note = test_note_at(
                    NoteType::Tap,
                    None,
                    false,
                    row,
                    row as f32 / ROWS_PER_BEAT as f32,
                );
                note.column = index % 4;
                note
            })
            .collect::<Vec<_>>();
        for (index, row) in [24, 96, 240, 480, 1584, 1596].into_iter().enumerate() {
            let mut hold = test_note_at(
                NoteType::Hold,
                Some(test_hold()),
                false,
                row,
                row as f32 / ROWS_PER_BEAT as f32,
            );
            hold.column = if index < 4 { index } else { 0 };
            let hold_data = hold.hold.as_mut().expect("hold fixture");
            hold_data.end_row_index = row + ROWS_PER_BEAT as usize;
            hold_data.end_beat = hold_data.end_row_index as f32 / ROWS_PER_BEAT as f32;
            current.push(hold);
        }
        let mut blocker = test_note_at(NoteType::Tap, None, false, 108, 2.25);
        blocker.column = 1;
        let context = vec![blocker];
        let mut legacy = current.clone();

        apply_mines_insert(&mut current, &context, &timing, 0, 4, 0, last_row);
        apply_mines_insert_legacy_for_bench(
            &mut legacy,
            &context,
            &timing,
            0,
            4,
            0,
            last_row,
        );

        assert_eq!(current.len(), legacy.len());
        for (current, legacy) in current.iter().zip(&legacy) {
            assert_eq!(current.row_index, legacy.row_index);
            assert_eq!(current.column, legacy.column);
            assert_eq!(current.note_type, legacy.note_type);
            assert_eq!(
                current.hold.as_ref().map(|hold| hold.end_row_index),
                legacy.hold.as_ref().map(|hold| hold.end_row_index)
            );
        }
    }

    #[test]
    fn intelligent_insert_adds_middle_tap_between_matching_endpoints() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 2;

        apply_insert_intelligent_taps(
            &mut notes,
            &timing,
            0,
            4,
            ROWS_PER_BEAT as usize,
            (ROWS_PER_BEAT / 2) as usize,
            ROWS_PER_BEAT as usize,
            false,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == (ROWS_PER_BEAT / 2) as usize
                && note.column == 1
                && note.note_type == NoteType::Tap
        }));
    }

    #[test]
    fn wide_stomp_and_echo_insert_expected_taps() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        );

        let mut wide = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        wide[0].column = 1;
        apply_wide_insert(&mut wide, &timing, 0, 4);
        assert!(
            wide.iter()
                .any(|note| note.row_index == 0 && note.column != 1)
        );

        let mut stomp = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        stomp[0].column = 1;
        apply_stomp_insert(&mut stomp, &timing, 0, 4);
        assert!(
            stomp
                .iter()
                .any(|note| note.row_index == 0 && note.column == 2)
        );

        let mut echo = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        echo[0].column = 3;
        apply_echo_insert(&mut echo, &timing, 0, 4);
        assert!(
            echo.iter()
                .any(|note| { note.row_index == (ROWS_PER_BEAT / 2) as usize && note.column == 3 })
        );
    }

    #[test]
    fn convert_taps_to_holds_sets_hold_metadata() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        notes[0].column = 0;

        convert_taps_to_holds(&mut notes, &timing, 0, 4, 1);

        assert_eq!(notes[0].note_type, NoteType::Hold);
        let hold = notes[0].hold.as_ref().expect("tap converted to hold");
        assert_eq!(hold.end_row_index, ROWS_PER_BEAT as usize);
        assert_eq!(hold.life, INITIAL_HOLD_LIFE);
        assert_eq!(hold.last_held_row_index, 0);
    }

    #[test]
    fn uncommon_remove_masks_filter_convert_and_cap_notes() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 5);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize / 2, 0.5),
            test_note_at(NoteType::Mine, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, true, ROWS_PER_BEAT as usize * 2, 2.0),
            test_note_at(NoteType::Lift, None, false, ROWS_PER_BEAT as usize * 3, 3.0),
            test_note_at(
                NoteType::Hold,
                Some(test_hold()),
                false,
                ROWS_PER_BEAT as usize * 4,
                4.0,
            ),
        ];
        for (column, note) in notes.iter_mut().enumerate() {
            note.column = column % 4;
        }

        apply_uncommon_masks_with_masks(
            &mut notes,
            0,
            REMOVE_MASK_BIT_LITTLE
                | REMOVE_MASK_BIT_NO_MINES
                | REMOVE_MASK_BIT_NO_HOLDS
                | REMOVE_MASK_BIT_NO_HANDS
                | REMOVE_MASK_BIT_NO_LIFTS
                | REMOVE_MASK_BIT_NO_FAKES,
            HOLDS_MASK_BIT_NO_ROLLS,
            &timing,
            0,
            4,
            &[],
            None,
            0,
        );

        assert!(
            notes
                .iter()
                .all(|note| note.row_index % ROWS_PER_BEAT as usize == 0)
        );
        assert!(notes.iter().all(|note| {
            !note.is_fake
                && note.note_type != NoteType::Mine
                && note.note_type != NoteType::Lift
                && note.note_type != NoteType::Hold
                && note.hold.is_none()
        }));
        assert!(count_tap_tracks_at_row(&notes, 0, 0, 4) <= 2);
    }

    #[test]
    fn uncommon_insert_and_hold_masks_delegate_to_transforms() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 2;

        apply_uncommon_masks_with_masks(
            &mut notes,
            INSERT_MASK_BIT_BIG,
            0,
            HOLDS_MASK_BIT_PLANTED,
            &timing,
            0,
            4,
            &[],
            None,
            0,
        );

        let inserted = notes
            .iter()
            .find(|note| {
                note.row_index == ROWS_PER_BEAT as usize / 2
                    && note.column == 1
                    && note.note_type == NoteType::Hold
            })
            .expect("big insert tap converted to hold");
        assert_eq!(
            inserted
                .hold
                .as_ref()
                .expect("inserted note converted to hold")
                .life,
            INITIAL_HOLD_LIFE
        );
    }

    #[test]
    fn uncommon_chart_transforms_preserve_ranges_without_masks() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 4;
        let mut ranges = [(0, 1), (1, 2)];

        apply_uncommon_chart_transforms(
            &mut notes,
            &mut ranges,
            4,
            2,
            &[ChartAttackEffects::default(); MAX_PLAYERS],
            &timing_refs,
        );

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].note_type, NoteType::Tap);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[1].note_type, NoteType::Mine);
        assert_eq!(notes[1].column, 4);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (1, 2));
    }

    #[test]
    fn uncommon_chart_transforms_rebuild_per_player_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 4;
        let mut ranges = [(0, 1), (1, 2)];
        let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
        effects[1].remove_mask = REMOVE_MASK_BIT_NO_MINES;

        apply_uncommon_chart_transforms(&mut notes, &mut ranges, 4, 2, &effects, &timing_refs);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].column, 0);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (1, 1));
    }

    #[test]
    fn uncommon_chart_transforms_duplicate_single_player_range() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        let mut ranges = [(0, 2), (0, 0)];
        let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
        effects[0].remove_mask = REMOVE_MASK_BIT_NO_MINES;

        apply_uncommon_chart_transforms(&mut notes, &mut ranges, 4, 1, &effects, &timing_refs);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_type, NoteType::Tap);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (0, 1));
    }

    #[test]
    fn notes_row_sorted_allows_equal_rows_only_in_order() {
        let sorted = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let unsorted = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];

        assert!(notes_row_sorted(&sorted));
        assert!(!notes_row_sorted(&unsorted));
    }

    #[test]
    fn turn_options_mirror_only_player_range_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Mirror, GameplayTurnOption::None];
        apply_turn_options(&mut notes, [(0, 4), (4, 8)], 4, 2, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![3, 2, 1, 0, 4, 5, 6, 7]);
    }

    #[test]
    fn turn_options_left_maps_four_panel_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Left, GameplayTurnOption::None];
        let notes_len = notes.len();
        apply_turn_options(&mut notes, [(0, notes_len), (0, 0)], 4, 1, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![1, 3, 0, 2]);
    }

    #[test]
    fn shuffle_turn_retries_an_identity_permutation() {
        let mut first = [0, 1, 2, 3];
        TurnRng::new(29).shuffle(&mut first);
        assert_eq!(first, [0, 1, 2, 3]);
        assert_eq!(
            turn_take_from(GameplayTurnOption::Shuffle, 4, 29),
            Some(vec![2, 0, 1, 3])
        );
    }

    #[test]
    fn turn_seed_uses_simfile_path() {
        let mut first = test_song(0.0, 0.0);
        first.simfile_path = PathBuf::from("packs/a/song.ssc");
        let mut same = test_song(0.0, 0.0);
        same.simfile_path = PathBuf::from("packs/a/song.ssc");
        let mut other = test_song(0.0, 0.0);
        other.simfile_path = PathBuf::from("packs/b/song.ssc");

        assert_eq!(turn_seed_for_song(&first), turn_seed_for_song(&same));
        assert_ne!(turn_seed_for_song(&first), turn_seed_for_song(&other));
    }

    #[test]
    fn note_and_mine_window_bounds_use_left_open_right_closed_time() {
        let times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        let note_indices = [0, 1, 2, 3];
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
            test_note_at(NoteType::Tap, None, false, 192, 4.0),
        ];

        assert_eq!(mine_window_bounds_ns(&times, times[0], times[2]), (0, 3));
        assert_eq!(crossed_mine_bounds_ns(&times, times[0], times[2]), (1, 3));
        assert_eq!(
            lane_note_window_bounds_ns(&note_indices, &times, times[0], times[2]),
            (0, 3)
        );
        assert_eq!(
            lane_note_window_bounds_rows(&note_indices, &notes, 96, 192),
            (1, 3)
        );
    }

    #[test]
    fn lane_note_window_bounds_use_time_not_frozen_stop_beat() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 185.0)],
                stops: vec![StopSegment {
                    beat: 32.0,
                    duration: 0.973,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let stop_beat = 32.0;
        let note_time = timing.get_time_for_beat(stop_beat);
        let lookahead_time = note_time + 0.5;
        let lookahead_beat = timing.get_beat_for_time(lookahead_time);
        let note_times_ns = [song_time_ns_from_seconds(note_time)];
        let note_indices = [0usize];

        assert!((lookahead_beat - stop_beat).abs() < 0.000_5);
        assert_eq!(
            lane_note_window_bounds_ns(
                &note_indices,
                &note_times_ns,
                0,
                song_time_ns_from_seconds(lookahead_time),
            ),
            (0, 1)
        );
        assert!(!(stop_beat < lookahead_beat));
    }

    #[test]
    fn crossed_held_mine_predicate_accepts_judgable_same_column_mine() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        note.column = 1;

        assert!(crossed_held_mine_can_hit(&note, 1));
    }

    #[test]
    fn crossed_held_mine_predicate_filters_noneligible_mines() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap.column = 1;
        let mut fake_mine = test_note_at(NoteType::Mine, None, true, 48, 1.0);
        fake_mine.column = 1;
        let mut wrong_column = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        wrong_column.column = 2;
        let mut already_scored = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        already_scored.column = 1;
        already_scored.mine_result = Some(MineResult::Avoided);
        let mut unjudgable = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        unjudgable.column = 1;
        unjudgable.can_be_judged = false;

        assert!(!crossed_held_mine_can_hit(&tap, 1));
        assert!(!crossed_held_mine_can_hit(&fake_mine, 1));
        assert!(!crossed_held_mine_can_hit(&wrong_column, 1));
        assert!(!crossed_held_mine_can_hit(&already_scored, 1));
        assert!(!crossed_held_mine_can_hit(&unjudgable, 1));
    }

    #[test]
    fn crossed_held_mine_marking_marks_crossed_same_column_mines() {
        let mine_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
        ];
        let mine_ix = [0, 1, 2];
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 72, 1.5),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
        ];
        for note in &mut notes {
            note.column = 2;
        }

        let mut marked = Vec::new();
        assert!(mark_crossed_held_mine_candidates(
            &mut notes,
            &mine_ix,
            &mine_times,
            2,
            mine_times[0],
            mine_times[2],
            25_000_000,
            1.0,
            |note_index, mark| marked.push((note_index, mark)),
        ));

        assert_eq!(marked.len(), 2);
        assert_eq!(marked[0].0, 1);
        assert_eq!(marked[0].1.note_time_ns, mine_times[1]);
        assert_eq!(marked[1].0, 2);
        assert_eq!(marked[1].1.note_time_ns, mine_times[2]);
        assert_eq!(notes[0].mine_result, None);
        assert_eq!(notes[1].mine_result, Some(MineResult::Hit));
        assert_eq!(notes[2].mine_result, Some(MineResult::Hit));
    }

    #[test]
    fn crossed_held_mine_marking_filters_invalid_candidates() {
        let mine_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
        ];
        let mine_ix = [0, 99, 1, 2];
        let mut wrong_column = test_note_at(NoteType::Mine, None, false, 72, 1.5);
        wrong_column.column = 1;
        let mut already_hit = test_note_at(NoteType::Mine, None, false, 96, 2.0);
        already_hit.column = 2;
        already_hit.mine_result = Some(MineResult::Hit);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            wrong_column,
            already_hit,
        ];

        let mut marked = Vec::new();
        assert!(!mark_crossed_held_mine_candidates(
            &mut notes,
            &mine_ix,
            &mine_times,
            2,
            mine_times[0],
            mine_times[2],
            25_000_000,
            1.0,
            |note_index, mark| marked.push((note_index, mark)),
        ));
        assert!(marked.is_empty());
        assert_eq!(notes[0].mine_result, None);
        assert_eq!(notes[1].mine_result, None);
        assert_eq!(notes[2].mine_result, Some(MineResult::Hit));
    }

    #[test]
    fn crossed_held_mine_marking_rejects_invalid_ranges() {
        let mine_times = [song_time_ns_from_seconds(1.0)];
        let mine_ix = [0];
        let mut notes = vec![test_note_at(NoteType::Mine, None, false, 48, 1.0)];
        notes[0].column = 0;

        let mut marked = Vec::new();
        assert!(!mark_crossed_held_mine_candidates(
            &mut notes,
            &mine_ix,
            &mine_times,
            0,
            INVALID_SONG_TIME_NS,
            mine_times[0],
            25_000_000,
            1.0,
            |note_index, mark| marked.push((note_index, mark)),
        ));
        assert!(!mark_crossed_held_mine_candidates(
            &mut notes,
            &mine_ix,
            &mine_times,
            0,
            mine_times[0],
            mine_times[0],
            25_000_000,
            1.0,
            |note_index, mark| marked.push((note_index, mark)),
        ));
        assert!(marked.is_empty());
        assert_eq!(notes[0].mine_result, None);
    }

    #[test]
    fn mine_avoid_cursor_end_stops_before_cutoff_row() {
        let notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
            test_note_at(NoteType::Mine, None, false, 192, 4.0),
        ];
        let mine_ix = [0, 1, 2, 3];

        assert_eq!(mine_avoid_cursor_end(&notes, &mine_ix, 0, 144), 2);
        assert_eq!(mine_avoid_cursor_end(&notes, &mine_ix, 1, 144), 2);
        assert_eq!(mine_avoid_cursor_end(&notes, &mine_ix, 0, 48), 0);
    }

    #[test]
    fn mine_avoid_cursor_end_clamps_cursor_to_mine_index_len() {
        let notes = vec![test_note_at(NoteType::Mine, None, false, 48, 1.0)];
        let mine_ix = [0];

        assert_eq!(mine_avoid_cursor_end(&notes, &mine_ix, 99, 96), 1);
    }

    #[test]
    fn mine_avoidance_predicate_requires_judgable_unresolved_mine() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(mine_can_be_avoided(&note));

        note.mine_result = Some(MineResult::Hit);
        assert!(!mine_can_be_avoided(&note));

        note.mine_result = None;
        note.can_be_judged = false;
        assert!(!mine_can_be_avoided(&note));
    }

    #[test]
    fn mine_hit_offset_window_is_inclusive_and_signed() {
        assert!(mine_hit_offset_in_window(100, 100));
        assert!(mine_hit_offset_in_window(-100, 100));
        assert!(!mine_hit_offset_in_window(101, 100));
        assert!(!mine_hit_offset_in_window(-101, 100));
    }

    #[test]
    fn mine_hit_predicate_requires_unresolved_real_judgable_note() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(mine_can_be_hit(&note));

        note.mine_result = Some(MineResult::Avoided);
        assert!(!mine_can_be_hit(&note));

        note.mine_result = None;
        note.is_fake = true;
        assert!(!mine_can_be_hit(&note));

        note.is_fake = false;
        note.can_be_judged = false;
        assert!(!mine_can_be_hit(&note));
    }

    #[test]
    fn mine_hit_result_marks_only_valid_window_hits() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(!apply_mine_hit_result(&mut note, 101, 100));
        assert_eq!(note.mine_result, None);

        assert!(apply_mine_hit_result(&mut note, -100, 100));
        assert_eq!(note.mine_result, Some(MineResult::Hit));

        assert!(!apply_mine_hit_result(&mut note, 0, 100));
        assert_eq!(note.mine_result, Some(MineResult::Hit));
    }

    #[test]
    fn mine_hit_mark_records_marked_note_metadata() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        note.column = 2;
        let mark = mark_mine_hit_candidate(&mut note, 1_000_000_000, 20_000_000, 25_000_000, 2.0)
            .expect("mine should be inside hit window");

        assert_eq!(note.mine_result, Some(MineResult::Hit));
        assert_eq!(mark.row_index, 48);
        assert_eq!(mark.column, 2);
        assert_eq!(mark.beat, 1.0);
        assert_eq!(mark.note_time_ns, 1_000_000_000);
        assert_eq!(mark.hit_time_ns, 1_020_000_000);
        assert!((mark.time_error_ms - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn mine_hit_mark_rejects_invalid_candidates() {
        let mut late = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert_eq!(
            mark_mine_hit_candidate(&mut late, 1_000_000_000, 26_000_000, 25_000_000, 1.0),
            None
        );
        assert_eq!(late.mine_result, None);

        let mut already_hit = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(
            mark_mine_hit_candidate(&mut already_hit, 1_000_000_000, 0, 25_000_000, 1.0).is_some()
        );
        assert_eq!(
            mark_mine_hit_candidate(&mut already_hit, 1_000_000_000, 0, 25_000_000, 1.0),
            None
        );
        assert_eq!(already_hit.mine_result, Some(MineResult::Hit));
    }

    #[test]
    fn pending_mine_hit_ready_requires_marked_real_judgable_hit() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(!pending_mine_hit_ready(&note));

        note.mine_result = Some(MineResult::Hit);
        assert!(pending_mine_hit_ready(&note));

        note.is_fake = true;
        assert!(!pending_mine_hit_ready(&note));

        note.is_fake = false;
        note.can_be_judged = false;
        assert!(!pending_mine_hit_ready(&note));
    }

    #[test]
    fn pending_mine_hit_event_routes_ready_hit_to_player() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
        ];
        notes[1].column = 5;
        notes[1].mine_result = Some(MineResult::Hit);

        assert_eq!(
            pending_mine_hit_event(&notes, 1, 2, 4),
            Some(PendingMineHitEvent {
                note_index: 1,
                column: 5,
                player: 1,
            })
        );
    }

    #[test]
    fn pending_mine_hit_event_filters_invalid_entries() {
        let mut notes = vec![test_note_at(NoteType::Mine, None, false, 48, 1.0)];
        assert_eq!(pending_mine_hit_event(&notes, 99, 2, 4), None);
        assert_eq!(pending_mine_hit_event(&notes, 0, 2, 4), None);

        notes[0].mine_result = Some(MineResult::Hit);
        notes[0].is_fake = true;
        assert_eq!(pending_mine_hit_event(&notes, 0, 2, 4), None);

        notes[0].is_fake = false;
        notes[0].can_be_judged = false;
        assert_eq!(pending_mine_hit_event(&notes, 0, 2, 4), None);
    }

    #[test]
    fn pending_mine_hit_collection_skips_invalid_entries() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
        ];
        notes[0].mine_result = Some(MineResult::Hit);
        notes[1].mine_result = Some(MineResult::Avoided);
        notes[2].mine_result = Some(MineResult::Hit);
        notes[2].column = 5;
        let pending = [99, 0, 1, 2];
        let mut events = [None; 4];

        let update = collect_pending_mine_hit_events(&notes, &pending, 0, 2, 4, &mut events);

        assert_eq!(
            update,
            PendingMineHitCollectionUpdate {
                next_cursor: 4,
                event_count: 2,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(PendingMineHitEvent {
                note_index: 0,
                column: 0,
                player: 0,
            })
        );
        assert_eq!(
            events[1],
            Some(PendingMineHitEvent {
                note_index: 2,
                column: 5,
                player: 1,
            })
        );
    }

    #[test]
    fn pending_mine_hit_collection_stops_when_event_buffer_fills() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
        ];
        notes[0].mine_result = Some(MineResult::Hit);
        notes[1].mine_result = Some(MineResult::Hit);
        let pending = [0, 1];
        let mut events = [None; 1];

        let first = collect_pending_mine_hit_events(&notes, &pending, 0, 1, 4, &mut events);
        assert_eq!(
            first,
            PendingMineHitCollectionUpdate {
                next_cursor: 1,
                event_count: 1,
                stopped: true,
            }
        );
        assert_eq!(
            events[0],
            Some(PendingMineHitEvent {
                note_index: 0,
                column: 0,
                player: 0,
            })
        );

        let second =
            collect_pending_mine_hit_events(&notes, &pending, first.next_cursor, 1, 4, &mut events);
        assert_eq!(
            second,
            PendingMineHitCollectionUpdate {
                next_cursor: 2,
                event_count: 1,
                stopped: false,
            }
        );
        assert_eq!(
            events[0],
            Some(PendingMineHitEvent {
                note_index: 1,
                column: 0,
                player: 0,
            })
        );
    }

    #[test]
    fn mine_avoid_result_marks_unresolved_judgable_mines() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(apply_mine_avoid_result(&mut note));
        assert_eq!(note.mine_result, Some(MineResult::Avoided));

        assert!(!apply_mine_avoid_result(&mut note));
        note.mine_result = None;
        note.can_be_judged = false;
        assert!(!apply_mine_avoid_result(&mut note));
        assert_eq!(note.mine_result, None);
    }

    #[test]
    fn time_based_mine_avoidance_marks_candidates_and_advances_cursors() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
        ];
        let mine_ix = [0, 1, 2];

        let update =
            apply_time_based_mine_avoidance_for_player(&mut notes, &mine_ix, 0, 144, (0, 3));

        assert_eq!(
            update,
            MineAvoidancePlayerUpdate {
                mine_end: 2,
                next_mine_avoid_cursor: 2,
                avoided_count: 2,
                last_avoided: Some(MineAvoidedEvent {
                    note_index: 1,
                    row_index: 96,
                    column: 0,
                }),
            }
        );
        assert_eq!(notes[0].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[1].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[2].mine_result, None);
    }

    #[test]
    fn time_based_mine_avoidance_reports_only_new_avoids() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
        ];
        notes[0].mine_result = Some(MineResult::Avoided);
        notes[1].mine_result = Some(MineResult::Hit);
        notes[2].can_be_judged = false;
        let mine_ix = [0, 1, 2];

        let update =
            apply_time_based_mine_avoidance_for_player(&mut notes, &mine_ix, 0, 192, (0, 3));

        assert_eq!(update.mine_end, 3);
        assert_eq!(update.next_mine_avoid_cursor, 3);
        assert_eq!(update.avoided_count, 0);
        assert_eq!(update.last_avoided, None);
        assert_eq!(notes[0].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[1].mine_result, Some(MineResult::Hit));
        assert_eq!(notes[2].mine_result, None);
    }

    #[test]
    fn time_based_mine_avoidance_scans_active_players() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
            test_note_at(NoteType::Mine, None, false, 192, 4.0),
        ];
        notes[2].column = 4;
        notes[3].column = 5;
        let mine_ix = vec![vec![0, 1], vec![2, 3]];

        let update = apply_time_based_mine_avoidance_for_players(
            &mut notes,
            &mine_ix,
            &[0, 0],
            &[96, 999],
            &[(0, 2), (2, 4)],
            2,
        );

        assert_eq!(update.players_scanned, 2);
        assert_eq!(
            update.updates[0],
            MineAvoidancePlayerUpdate {
                mine_end: 1,
                next_mine_avoid_cursor: 1,
                avoided_count: 1,
                last_avoided: Some(MineAvoidedEvent {
                    note_index: 0,
                    row_index: 48,
                    column: 0,
                }),
            }
        );
        assert_eq!(
            update.updates[1],
            MineAvoidancePlayerUpdate {
                mine_end: 2,
                next_mine_avoid_cursor: 4,
                avoided_count: 2,
                last_avoided: Some(MineAvoidedEvent {
                    note_index: 3,
                    row_index: 192,
                    column: 5,
                }),
            }
        );
        assert_eq!(notes[0].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[1].mine_result, None);
        assert_eq!(notes[2].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[3].mine_result, Some(MineResult::Avoided));
    }

    #[test]
    fn completed_mine_avoidance_requires_real_unresolved_mine() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(completed_mine_can_be_avoided(&note));

        note.mine_result = Some(MineResult::Hit);
        assert!(!completed_mine_can_be_avoided(&note));

        note.mine_result = None;
        note.is_fake = true;
        assert!(!completed_mine_can_be_avoided(&note));

        note.is_fake = false;
        note.can_be_judged = false;
        assert!(!completed_mine_can_be_avoided(&note));

        note.can_be_judged = true;
        note.note_type = NoteType::Tap;
        assert!(!completed_mine_can_be_avoided(&note));
    }

    #[test]
    fn completed_mine_avoid_result_requires_real_mine() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        assert!(apply_completed_mine_avoid_result(&mut note));
        assert_eq!(note.mine_result, Some(MineResult::Avoided));

        note.mine_result = None;
        note.is_fake = true;
        assert!(!apply_completed_mine_avoid_result(&mut note));
        assert_eq!(note.mine_result, None);

        note.is_fake = false;
        note.note_type = NoteType::Tap;
        assert!(!apply_completed_mine_avoid_result(&mut note));
        assert_eq!(note.mine_result, None);
    }

    #[test]
    fn completed_mine_finalization_marks_unresolved_real_mines() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, true, 144, 3.0),
            test_note_at(NoteType::Tap, None, false, 192, 4.0),
        ];
        notes[1].mine_result = Some(MineResult::Hit);

        let avoided = finalize_completed_mine_avoidance_for_player(&mut notes, (0, 4), 3, 1);

        assert_eq!(avoided, 2);
        assert_eq!(notes[0].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[1].mine_result, Some(MineResult::Hit));
        assert_eq!(notes[2].mine_result, None);
        assert_eq!(notes[3].mine_result, None);
    }

    #[test]
    fn completed_mine_finalization_clamps_range_and_hit_count() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
        ];

        let avoided = finalize_completed_mine_avoidance_for_player(&mut notes, (1, 99), 1, 9);

        assert_eq!(avoided, 0);
        assert_eq!(notes[0].mine_result, None);
        assert_eq!(notes[1].mine_result, Some(MineResult::Avoided));
    }

    #[test]
    fn completed_mine_finalization_handles_player_ranges() {
        let mut notes = vec![
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Mine, None, false, 144, 3.0),
            test_note_at(NoteType::Mine, None, false, 192, 4.0),
        ];
        notes[2].column = 4;
        notes[3].column = 5;

        let update = finalize_completed_mine_avoidance_for_players(
            &mut notes,
            &[(0, 2), (2, 4)],
            &[2, 2],
            &[1, 0],
            2,
        );

        assert_eq!(
            update,
            CompletedMineFinalizationUpdate {
                players_finalized: 2,
                mines_avoided: [1, 2],
            }
        );
        assert_eq!(notes[0].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[1].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[2].mine_result, Some(MineResult::Avoided));
        assert_eq!(notes[3].mine_result, Some(MineResult::Avoided));
    }

    #[test]
    fn step_search_bounds_expand_one_second_plus_one_beat() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(1.0), 48),
            (0, 144)
        );
    }

    #[test]
    fn step_search_bounds_saturate_before_song_start() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(0.0), 0),
            (0, 96)
        );
    }

    #[test]
    fn crossed_mine_held_start_tracks_existing_or_new_hold() {
        let previous = song_time_ns_from_seconds(1.0);
        let pressed_before = song_time_ns_from_seconds(0.9);
        let pressed_after = song_time_ns_from_seconds(1.25);
        let current = song_time_ns_from_seconds(1.5);

        assert_eq!(
            crossed_mine_held_start_time(true, true, None, previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_before), previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_after), previous, current),
            Some(pressed_after)
        );
        assert_eq!(
            crossed_mine_held_start_time(false, true, Some(previous), previous, current),
            None
        );
        assert_eq!(
            crossed_mine_held_start_time(
                true,
                false,
                Some(INVALID_SONG_TIME_NS),
                previous,
                current,
            ),
            None
        );
    }

    #[test]
    fn edge_judge_indices_use_lead_note_only() {
        assert_eq!(collect_edge_judge_indices(0, 7), None);

        let (indices, count) = collect_edge_judge_indices(3, 7).expect("row has notes");
        assert_eq!(count, 1);
        assert_eq!(indices[0], 7);
        assert!(indices[1..].iter().all(|index| *index == usize::MAX));
    }

    #[test]
    fn quantization_index_matches_note_row_subdivision() {
        assert_eq!(quantization_index_from_beat(0.0), QUANT_4TH);
        assert_eq!(quantization_index_from_beat(0.5), QUANT_8TH);
        assert_eq!(quantization_index_from_beat(1.0 / 3.0), QUANT_12TH);
        assert_eq!(quantization_index_from_beat(0.25), QUANT_16TH);
        assert_eq!(quantization_index_from_beat(1.0 / 6.0), QUANT_24TH);
        assert_eq!(quantization_index_from_beat(0.125), QUANT_32ND);
        assert_eq!(quantization_index_from_beat(1.0 / 12.0), QUANT_48TH);
        assert_eq!(quantization_index_from_beat(1.0 / 16.0), QUANT_64TH);
        assert_eq!(quantization_index_from_beat(1.0 / 48.0), QUANT_192ND);
    }

    #[test]
    fn closest_note_breaks_ties_toward_future_note() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 50, 50.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [1_000_000_000_i64, 1_020_000_000_i64];
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            1_010_000_000_i64,
            49,
            0,
            note_indices.len(),
        )
        .expect("expected an equidistant closest note");

        assert_eq!(note_index, 1);
        assert_eq!(err_ns, -10_000_000);
    }

    #[test]
    fn closest_note_prefers_row_distance_over_time_error() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.020),
            song_time_ns_from_seconds(1.028),
        ];
        let current_time_ns = song_time_ns_from_seconds(1.030);
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            current_time_ns,
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the nearer row to win");

        assert_eq!(note_index, 0);
        assert_eq!(err_ns, current_time_ns - note_times_ns[note_index]);
    }

    #[test]
    fn closest_lane_note_search_returns_candidate_and_debug_bounds() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 50, 50.0 / ROWS_PER_BEAT as f32),
            test_note_at(
                NoteType::Tap,
                None,
                false,
                120,
                120.0 / ROWS_PER_BEAT as f32,
            ),
        ];
        let note_indices = [0usize, 1, 2];
        let note_times_ns = [
            timing.get_time_for_beat_ns(notes[0].beat),
            timing.get_time_for_beat_ns(notes[1].beat),
            timing.get_time_for_beat_ns(notes[2].beat),
        ];
        let current_time_ns = note_times_ns[1];

        let search = closest_lane_note_search(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            current_time_ns,
        );

        assert_eq!(search.current_row_index, 50);
        assert_eq!(search.candidate, Some((1, 0)));
        assert!(search.search_start_row <= notes[1].row_index);
        assert!(search.search_end_row > notes[1].row_index);
        assert!(search.search_start_idx <= 1);
        assert!(search.search_end_idx > 1);
    }

    #[test]
    fn closest_lane_note_search_keeps_bounds_when_no_candidate_is_live() {
        let timing = test_timing(144);
        let mut judged_note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged_note.result = Some(test_judgment(JudgeGrade::Fantastic));
        let notes = vec![judged_note];
        let note_indices = [0usize];
        let note_times_ns = [timing.get_time_for_beat_ns(notes[0].beat)];

        let search = closest_lane_note_search(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            note_times_ns[0],
        );

        assert_eq!(search.current_row_index, 48);
        assert_eq!(search.search_start_idx, 0);
        assert_eq!(search.search_end_idx, 1);
        assert_eq!(search.candidate, None);
    }

    #[test]
    fn closest_note_skips_fake_segment_taps_and_judged_mines() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                fakes: vec![FakeSegment {
                    beat: 1.0,
                    length: 0.01,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(144),
        );
        let mut fake_segment_tap = test_note_at(NoteType::Tap, None, true, 48, 1.0);
        fake_segment_tap.can_be_judged = false;
        let mut judged_mine = test_note_at(NoteType::Mine, None, false, 49, 49.0 / 48.0);
        judged_mine.mine_result = Some(MineResult::Hit);
        let notes = vec![
            fake_segment_tap,
            judged_mine,
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / 48.0),
        ];
        let note_indices = [0usize, 1, 2];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.010),
            song_time_ns_from_seconds(1.120),
        ];

        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.030),
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the unjudged tap to remain hittable");

        assert_eq!(note_index, 2);
    }
}
