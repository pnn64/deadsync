impl<Profile, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
{
    #[inline(always)]
    pub fn player_blue_window_ms(&self, player_idx: usize) -> f32 {
        let base = self.default_fa_plus_window_s();
        if player_idx >= self.setup.num_players {
            return base * 1000.0;
        }
        blue_fantastic_window_ms_from_profile(base, &self.profiles_runtime.profiles[player_idx])
    }

    pub fn refresh_live_notefield_options(&mut self, current_bpm: f32) {
        for player in 0..self.setup.num_players {
            let scroll = effective_scroll_effects_for_player(self, player);
            self.display.notefield_motion.set_reverse_scroll(
                player,
                scroll.reverse_percent_for_column(0, self.setup.cols_per_player) > 0.5,
            );
            let start = player.saturating_mul(self.setup.cols_per_player);
            let end = (start + self.setup.cols_per_player)
                .min(self.setup.num_cols)
                .min(MAX_COLS);
            for (local_col, col) in (start..end).enumerate() {
                self.display.notefield_motion.set_column_scroll_dir(
                    col,
                    scroll.reverse_scale_for_column(local_col, self.setup.cols_per_player),
                );
            }
        }
        for player in 0..self.setup.num_players {
            let scroll_speed = self.effective_scroll_speed_for_player(player);
            let reference_bpm = self.display.notefield_motion.scroll_reference_bpm();
            let mut dynamic_speed =
                scroll_speed.pixels_per_second(current_bpm, reference_bpm, self.music_rate());
            if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 {
                dynamic_speed = ScrollSpeedSetting::default().pixels_per_second(
                    current_bpm,
                    reference_bpm,
                    self.music_rate(),
                );
            }
            let scroll = effective_scroll_effects_for_player(self, player);
            let visual_mask = effective_visual_mask_for_player(self, player);
            let mini_percent = effective_mini_percent_for_player(self, player);
            let mini = self.profiles_runtime.profiles[player]
                .effective_mini_value_with_visual_mask(visual_mask, mini_percent);
            let mut field_zoom = 1.0 - mini * 0.5;
            if field_zoom.abs() < 0.01 {
                field_zoom = 0.01;
            }

            let perspective = effective_perspective_effects_for_player(self, player);
            let draw_scale = self.profiles_runtime.profiles[player]
                .draw_scale_for_tilt_with_visual_mask(perspective.tilt, visual_mask, mini_percent);
            let draw_distance_before =
                draw_distance_before_targets(self.setup.viewport.height(), draw_scale);
            let draw_distance_after = draw_distance_after_targets(
                self.setup.viewport.height(),
                draw_scale,
                scroll.centered,
            );

            let mut travel_time = scroll_speed.travel_time_seconds(
                draw_distance_before,
                current_bpm,
                reference_bpm,
                self.music_rate(),
            );
            if !travel_time.is_finite() || travel_time <= 0.0 {
                travel_time = draw_distance_before / dynamic_speed.max(f32::EPSILON);
            }
            self.display.notefield_motion.set_player_motion(
                player,
                dynamic_speed,
                field_zoom,
                draw_distance_before,
                draw_distance_after,
                travel_time,
            );
        }
    }

    pub fn refresh_seek_dependent_state(&mut self) {
        refresh_active_attack_masks(self, 0.0);
        let current_bpm = self
            .timing_runtime
            .timing
            .get_bpm_for_beat(self.clock.song_position.current_beat);
        self.refresh_live_notefield_options(current_bpm);
    }

    pub fn set_music_rate(&mut self, rate: f32) -> bool {
        let normalized = normalized_song_rate(rate);
        let timing_profile = self.timing_runtime.timing_profile;
        let player_judgment_timing = std::array::from_fn(|player| {
            build_player_judgment_timing(
                timing_profile,
                &self.profiles_runtime.profiles[player],
                normalized,
            )
        });
        self.set_music_rate_with_player_judgment_timing(rate, player_judgment_timing)
    }

    pub fn error_bar_register_tap(
        &mut self,
        player: usize,
        judgment: &Judgment,
        tap_music_time_s: f32,
    ) {
        let options = self.profiles_runtime.profiles[player].error_bar_options();
        let show_text = options.mask_bits & GAMEPLAY_ERROR_BAR_TEXT != 0;
        let show_monochrome = options.mask_bits & GAMEPLAY_ERROR_BAR_MONOCHROME != 0;
        let show_colorful = options.mask_bits & GAMEPLAY_ERROR_BAR_COLORFUL != 0;
        let show_highlight = options.mask_bits & GAMEPLAY_ERROR_BAR_HIGHLIGHT != 0;
        let show_average = options.mask_bits & GAMEPLAY_ERROR_BAR_AVERAGE != 0;
        let show_text_scalable = options.text_scalable;
        let text_error_bar_threshold_ms = options.text_threshold_ms;
        let show_fa_plus_window = options.show_fa_plus_window;
        let blue_fantastic_window_s = self.player_blue_window_ms(player) / 1000.0;
        let error_bar_trim = options.trim;
        let error_bar_multi_tick = options.multi_tick;
        let error_ms_display = options.error_ms_display;
        let short_avg_enabled = options.short_average_enabled;
        let short_avg_intensity = options.short_average_intensity;
        let long_avg_enabled = options.long_average_enabled;
        let long_avg_threshold_s = options.long_average_threshold_ms as f32 / 1000.0;
        let long_avg_intensity = options.long_average_intensity;
        let long_avg_min_samples = options.long_average_min_samples as usize;
        let average_interval_ms = options.average_interval_ms;
        let Some(window) = judgment.window else {
            return;
        };

        let now = self.boundary.total_elapsed_in_screen;
        let offset_s = judgment.time_error_ms / 1000.0;
        let p = &mut self.players_runtime.players[player];

        if error_ms_display {
            p.offset_indicator_text = Some(OffsetIndicatorText {
                started_at: now,
                offset_ms: judgment.time_error_ms,
                window,
            });
        }

        if show_text {
            let threshold_s = if show_text_scalable {
                text_error_bar_threshold_ms as f32 / 1000.0
            } else if show_fa_plus_window {
                blue_fantastic_window_s
            } else {
                self.timing_runtime.timing_profile.windows_s[0]
            };
            if offset_s.abs() > threshold_s {
                p.error_bar_text = Some(ErrorBarText {
                    started_at: now,
                    early: offset_s < 0.0,
                    offset_ms: judgment.time_error_ms.abs(),
                    scaled: show_text_scalable,
                    scale_start_ms: text_error_bar_threshold_ms as f32,
                });
            } else {
                p.error_bar_text = None;
            }
        } else {
            p.error_bar_text = None;
        }

        if !(show_monochrome || show_colorful || show_highlight || show_average) {
            return;
        }

        let max_window_ix = gameplay_error_bar_trim_max_window_ix(error_bar_trim);
        let max_offset_s = self.timing_runtime.timing_profile.windows_s[max_window_ix];
        let clamped_offset_s = if max_offset_s.is_finite() && max_offset_s > 0.0 {
            offset_s.clamp(-max_offset_s, max_offset_s)
        } else {
            offset_s
        };

        let tick = ErrorBarTick {
            started_at: now,
            offset_s: clamped_offset_s,
            window,
        };

        if show_monochrome {
            error_bar_push_tick(
                &mut p.error_bar_mono_ticks,
                &mut p.error_bar_mono_next,
                error_bar_multi_tick,
                tick,
            );
        }

        if show_colorful || show_highlight {
            error_bar_push_tick(
                &mut p.error_bar_color_ticks,
                &mut p.error_bar_color_next,
                error_bar_multi_tick,
                tick,
            );
            p.error_bar_color_bar_started_at = Some(now);
        }

        if show_highlight {
            let is_top = if show_fa_plus_window {
                window == TimingWindow::W0
            } else {
                window == TimingWindow::W1
            };
            let flash_window = if offset_s.abs() > max_offset_s {
                match max_window_ix {
                    0 => TimingWindow::W1,
                    1 => TimingWindow::W2,
                    2 => TimingWindow::W3,
                    3 => TimingWindow::W4,
                    _ => TimingWindow::W5,
                }
            } else {
                window
            };
            let wi = error_bar_window_ix(flash_window);
            if is_top {
                p.error_bar_color_flash_early[wi] = Some(now);
                p.error_bar_color_flash_late[wi] = Some(now);
            } else if offset_s < 0.0 {
                p.error_bar_color_flash_early[wi] = Some(now);
            } else {
                p.error_bar_color_flash_late[wi] = Some(now);
            }
        }

        if show_average {
            if short_avg_enabled {
                let (avg_raw, avg_count) = error_bar_average_offset_s(
                    &mut p.error_bar_avg_samples,
                    tap_music_time_s,
                    offset_s,
                    average_interval_ms,
                );
                let mut avg = avg_raw * short_avg_intensity;
                if max_offset_s.is_finite() && max_offset_s > 0.0 {
                    avg = avg.clamp(-max_offset_s, max_offset_s);
                }
                if avg_count == 1 {
                    avg *= 0.75;
                }
                error_bar_push_tick(
                    &mut p.error_bar_avg_ticks,
                    &mut p.error_bar_avg_next,
                    error_bar_multi_tick,
                    ErrorBarTick {
                        started_at: now,
                        offset_s: avg,
                        window,
                    },
                );
                p.error_bar_avg_bar_started_at = Some(now);
            }

            if long_avg_enabled {
                let (long_mean, long_len) = error_bar_long_term_offset_s(
                    &mut p.error_bar_long_avg_samples,
                    &mut p.error_bar_long_avg_total,
                    tap_music_time_s,
                    offset_s,
                    average_interval_ms,
                );
                if long_len >= long_avg_min_samples
                    && long_mean.abs() * long_avg_intensity >= long_avg_threshold_s
                {
                    p.error_bar_long_avg_tick = Some(ErrorBarTick {
                        started_at: now,
                        offset_s: long_mean,
                        window,
                    });
                    p.error_bar_long_avg_visible = true;
                } else {
                    p.error_bar_long_avg_visible = false;
                }
            } else {
                p.error_bar_long_avg_visible = false;
            }
        }
    }

    #[inline(always)]
    pub fn render_provisional_early_rescore_feedback(
        &mut self,
        player: usize,
        column: usize,
        judgment: &Judgment,
        current_time: f32,
        hide_early_dw_judgments: bool,
        hide_early_dw_flash: bool,
        hide_early_dw_column_flash: bool,
    ) {
        if !hide_early_dw_judgments {
            self.set_last_judgment(player, *judgment);
            self.error_bar_register_tap(player, judgment, current_time);
        }

        if !hide_early_dw_flash {
            self.trigger_receptor_glow_pulse(column);
            self.spawn_tap_explosion_for_grade(column, judgment.grade, false);
        }

        if !hide_early_dw_column_flash {
            self.trigger_column_flash_for_judgment(player, column, judgment);
        }
    }

    pub fn apply_pending_mine_hits(&mut self) {
        if self
            .chart_runtime
            .mine_scan
            .pending_mine_hit_indices
            .is_empty()
        {
            return;
        }

        let pending = std::mem::take(&mut self.chart_runtime.mine_scan.pending_mine_hit_indices);
        let scoring_blocked = self.autoplay_blocks_scoring();
        let current_music_time = self.current_music_time_seconds();

        let mut cursor = 0usize;
        let mut events = [None; 8];
        loop {
            let update = collect_pending_mine_hit_events(
                &self.chart_runtime.notes,
                &pending,
                cursor,
                self.setup.num_players,
                self.setup.cols_per_player,
                &mut events,
            );
            cursor = update.next_cursor;

            for event in events.iter().take(update.event_count).flatten() {
                let column = event.column;
                let player = event.player;

                let side_effect_plan = mine_hit_side_effect_plan(scoring_blocked);
                if side_effect_plan.apply_life_change {
                    apply_life_change(
                        &mut self.players_runtime.players[player],
                        current_music_time,
                        side_effect_plan.life_delta,
                    );
                }
                if side_effect_plan.capture_failed_ex_score_inputs {
                    self.capture_failed_ex_score_inputs(player, self.player_blue_window_ms(player));
                }
                let mut player_state = mine_hit_player_state(&self.players_runtime.players[player]);
                let player_update = apply_mine_hit_player_state(
                    &mut player_state,
                    scoring_blocked,
                    self.player_is_dead(player),
                );
                apply_mine_hit_player_update(
                    &mut self.players_runtime.players[player],
                    player_state,
                    player_update,
                );

                self.display.receptor_feedback.clear_lift_glow(column);
                self.trigger_mine_explosion(column);
                self.set_last_mine_judgment(player, column, MineResult::Hit);
                if player_update.counted_for_score {
                    update_itg_grade_totals(&mut self.players_runtime.players[player]);
                }
            }
            if !update.stopped || update.event_count == 0 {
                break;
            }
        }
    }

    #[inline(always)]
    pub fn tap_judgment_uses_bright_explosion(
        &self,
        player_idx: usize,
        judgment: &Judgment,
    ) -> bool {
        let Some(profile) = self.profiles_runtime.profiles.get(player_idx) else {
            return false;
        };
        tap_judgment_uses_bright_explosion_from_profile(profile, judgment)
    }

    #[inline(always)]
    pub fn column_flash_enabled_for_player(
        &self,
        player_idx: usize,
        grade: JudgeGrade,
        blue_fantastic: bool,
    ) -> bool {
        let Some(profile) = self.profiles_runtime.profiles.get(player_idx) else {
            return false;
        };
        column_flash_enabled_for_options(
            column_flash_options_from_profile(profile),
            grade,
            blue_fantastic,
        )
    }

    #[inline(always)]
    pub fn tap_explosion_enabled_for_player(&self, player_idx: usize, window_key: &str) -> bool {
        let Some(profile) = self.profiles_runtime.profiles.get(player_idx) else {
            return false;
        };
        tap_explosion_enabled_for_options(tap_explosion_options_from_profile(profile), window_key)
    }

    #[inline(always)]
    pub fn record_step_calories_for_lane(
        &mut self,
        lane_idx: usize,
        event_music_time_ns: SongTimeNs,
    ) {
        let player = self.player_for_col(lane_idx);
        let weight_pounds = self.profiles_runtime.profiles[player].calculated_weight_pounds();
        self.record_step_calories_for_player(player, event_music_time_ns, weight_pounds);
    }

    #[inline(always)]
    pub fn trigger_column_flash(&mut self, column: usize, grade: JudgeGrade, blue_fantastic: bool) {
        if column >= self.display.visual_feedback.column_flashes.len() {
            return;
        }
        // Record the judgment unconditionally for feedback consumers (SMX panel lighting),
        // before the on-screen column-flash mask gate below.
        self.display.visual_feedback.last_tap_judgments[column] = Some(ColumnTapJudgment {
            grade,
            blue_fantastic,
            at_screen_s: self.boundary.total_elapsed_in_screen,
        });
        let player = self.player_for_col(column);
        if !self.column_flash_enabled_for_player(player, grade, blue_fantastic) {
            return;
        }
        self.display.visual_feedback.column_flashes[column] = Some(ActiveColumnFlash {
            grade,
            blue_fantastic,
            started_at_screen_s: self.boundary.total_elapsed_in_screen,
        });
    }

    #[inline(always)]
    pub fn trigger_column_flash_for_grade(&mut self, column: usize, grade: JudgeGrade) {
        self.trigger_column_flash(column, grade, false);
    }

    #[inline(always)]
    pub fn trigger_column_flash_for_judgment(
        &mut self,
        player_idx: usize,
        column: usize,
        judgment: &Judgment,
    ) {
        let blue_fantastic = judgment.grade == JudgeGrade::Fantastic
            && !self.tap_judgment_uses_bright_explosion(player_idx, judgment);
        self.trigger_column_flash(column, judgment.grade, blue_fantastic);
    }

    #[inline(always)]
    pub fn spawn_tap_explosion_for_grade(
        &mut self,
        column: usize,
        grade: JudgeGrade,
        bright: bool,
    ) {
        let Some(window_key) = grade_to_window(grade) else {
            return;
        };
        self.spawn_tap_explosion(column, window_key, bright);
    }

    #[inline(always)]
    pub fn trigger_hold_explosion(&mut self, column: usize) {
        // Hold success uses the noteskin's `HeldCommand` (matching ITGMania), which
        // is plumbed through the parser as the "Held" pseudo-window.
        self.spawn_tap_explosion(column, "Held", false);
    }

    pub fn handle_hold_let_go(
        &mut self,
        column: usize,
        note_index: usize,
        let_go_time_ns: SongTimeNs,
    ) {
        let player = self.player_for_col(column);
        let scoring_blocked = self.autoplay_blocks_scoring();
        let note_type = self.chart_runtime.notes[note_index].note_type;
        let player_dead = self.player_is_dead(player);
        let Some(update) = apply_hold_let_go_update(
            self.chart_runtime.notes[note_index].hold.as_mut(),
            &mut self.hold_runtime.hold_decay_active,
            &mut self.hold_runtime.decaying_hold_indices,
            note_index,
            note_type,
            let_go_time_ns,
            scoring_blocked,
            player_dead,
        ) else {
            return;
        };
        let mut player_state = hold_resolution_player_state(&self.players_runtime.players[player]);
        let player_update =
            apply_hold_let_go_player_state(&mut player_state, update.stats_update, scoring_blocked);
        apply_hold_resolution_player_state(&mut self.players_runtime.players[player], player_state);
        if update.effects.show_judgment {
            self.display.hold_feedback.hold_judgments[column] = Some(hold_judgment_render_info(
                update.result,
                self.boundary.total_elapsed_in_screen,
            ));
        }
        if player_update.apply_life_change {
            let current_music_time = self.current_music_time_seconds();
            apply_life_change(
                &mut self.players_runtime.players[player],
                current_music_time,
                player_update.life_delta,
            );
        }
        if player_update.capture_failed_ex_score_inputs {
            self.capture_failed_ex_score_inputs(player, self.player_blue_window_ms(player));
        }
        if hold_resolution_updates_grade_totals(
            update.result,
            player_update.stats_update,
            self.player_is_dead(player),
        ) {
            update_itg_grade_totals(&mut self.players_runtime.players[player]);
        }
        apply_combo_update(
            &mut self.players_runtime.players[player],
            player_update.combo_update,
        );
        if update.effects.reset_receptor_glow {
            self.display.receptor_feedback.clear_lift_glow(column);
        }
    }

    pub fn handle_hold_success(&mut self, column: usize, note_index: usize) {
        let player = self.player_for_col(column);
        let scoring_blocked = self.autoplay_blocks_scoring();
        let note_type = self.chart_runtime.notes[note_index].note_type;
        let player_dead = self.player_is_dead(player);
        let Some(update) = apply_hold_success_update(
            self.chart_runtime.notes[note_index].hold.as_mut(),
            &mut self.hold_runtime.hold_decay_active,
            note_index,
            note_type,
            scoring_blocked,
            player_dead,
        ) else {
            return;
        };
        let mut player_state = hold_resolution_player_state(&self.players_runtime.players[player]);
        let player_update = apply_hold_success_player_state(
            &mut player_state,
            update.stats_update,
            scoring_blocked,
        );
        apply_hold_resolution_player_state(&mut self.players_runtime.players[player], player_state);
        if player_update.apply_life_change {
            let current_music_time = self.current_music_time_seconds();
            apply_life_change(
                &mut self.players_runtime.players[player],
                current_music_time,
                player_update.life_delta,
            );
        }
        if player_update.capture_failed_ex_score_inputs {
            self.capture_failed_ex_score_inputs(player, self.player_blue_window_ms(player));
        }
        if hold_resolution_updates_grade_totals(
            update.result,
            player_update.stats_update,
            self.player_is_dead(player),
        ) {
            update_itg_grade_totals(&mut self.players_runtime.players[player]);
        }
        apply_combo_update(
            &mut self.players_runtime.players[player],
            player_update.combo_update,
        );
        if update.effects.trigger_hold_explosion {
            self.trigger_hold_explosion(column);
        }
        if update.effects.show_judgment {
            self.display.hold_feedback.hold_judgments[column] = Some(hold_judgment_render_info(
                update.result,
                self.boundary.total_elapsed_in_screen,
            ));
        }
    }

    #[inline(always)]
    fn resolve_active_hold(&mut self, column: usize, resolution: ActiveHoldResolution) {
        match resolution {
            ActiveHoldResolution::LetGo {
                note_index,
                time_ns,
            } => self.handle_hold_let_go(column, note_index, time_ns),
            ActiveHoldResolution::Success { note_index } => {
                self.handle_hold_success(column, note_index)
            }
        }
    }

    #[inline(always)]
    pub fn settle_due_autoplay_active_holds(&mut self, cutoff_time_ns: SongTimeNs) {
        let mut events = [None; MAX_COLS];
        let update = collect_due_autoplay_active_hold_resolutions(
            &mut self.hold_runtime.active_holds,
            self.setup.num_cols,
            cutoff_time_ns,
            &mut events,
        );
        for event in events.iter().take(update.event_count).flatten() {
            self.resolve_active_hold(event.column, event.resolution);
        }
    }

    #[inline(always)]
    pub fn start_active_hold(
        &mut self,
        column: usize,
        note_index: usize,
        start_time_ns: SongTimeNs,
        end_time_ns: SongTimeNs,
        current_time_ns: SongTimeNs,
    ) {
        if column >= self.setup.num_cols {
            return;
        }
        let player = self.player_for_col(column);
        let rate = self.music_rate();
        // A fast same-column hold jack can hit the next head early while the
        // previous hold is still alive. ITG stores hold state per TapNote; settle
        // the previous non-overlapping hold before replacing this column slot.
        if let Some(event) = settle_replaced_active_hold_column(
            &mut self.hold_runtime.active_holds,
            &mut self.chart_runtime.notes,
            column,
            note_index,
            start_time_ns,
            &self.timing_runtime.timing_players[player],
            rate,
        ) {
            self.resolve_active_hold(event.column, event.resolution);
        }
        start_active_hold_column(
            &mut self.hold_runtime.active_holds,
            &mut self.chart_runtime.notes,
            column,
            note_index,
            start_time_ns,
            end_time_ns,
            current_time_ns,
        );
    }

    #[inline(always)]
    pub fn integrate_active_hold_to_time(&mut self, column: usize, target_time_ns: SongTimeNs) {
        if column >= self.setup.num_cols || song_time_ns_invalid(target_time_ns) {
            return;
        }

        let player = self.player_for_col(column);
        let rate = self.music_rate();
        if let Some(resolution) = integrate_active_hold_column(
            &mut self.hold_runtime.active_holds,
            &mut self.chart_runtime.notes,
            column,
            &self.timing_runtime.timing_players[player],
            target_time_ns,
            rate,
        ) {
            self.resolve_active_hold(column, resolution);
        }
    }

    pub fn update_active_holds(&mut self, inputs: &[bool; MAX_COLS], current_time_ns: SongTimeNs) {
        let timing_players: [&_; MAX_PLAYERS] =
            std::array::from_fn(|player| self.timing_runtime.timing_players[player].as_ref());
        let live_autoplay = self.live_autoplay_enabled();
        let rate = self.music_rate();
        let mut events = [None; MAX_COLS];
        let update = update_active_hold_columns(
            &mut self.hold_runtime.active_holds,
            &mut self.chart_runtime.notes,
            inputs,
            self.setup.num_cols,
            self.setup.cols_per_player,
            self.setup.num_players,
            &timing_players,
            current_time_ns,
            rate,
            live_autoplay,
            &mut events,
        );
        for event in events.iter().take(update.event_count).flatten() {
            self.resolve_active_hold(event.column, event.resolution);
        }
    }

    #[inline(always)]
    pub fn resolve_pending_missed_holds(&mut self, current_time_ns: SongTimeNs) {
        let mut score_missed_holds_rolls_by_column = [false; MAX_COLS];
        for col in 0..self.setup.num_cols.min(MAX_COLS) {
            score_missed_holds_rolls_by_column[col] =
                self.progress.stage.score_missed_holds_rolls[self.player_for_col(col)];
        }
        let mut events = [None; 8];
        loop {
            let update = collect_pending_missed_hold_resolutions(
                &self.chart_runtime.notes,
                &self.chart_runtime.hold_end_time_cache_ns,
                &mut self.hold_runtime.pending_missed_hold_resolution,
                &mut self.hold_runtime.pending_missed_hold_indices,
                current_time_ns,
                &score_missed_holds_rolls_by_column[..self.setup.num_cols.min(MAX_COLS)],
                &mut events,
            );
            for event in events.iter().take(update.event_count).flatten() {
                let event = *event;
                match event.resolution {
                    PendingMissedHoldResolution::None => {}
                    PendingMissedHoldResolution::ShowMissedFeedback => {
                        let column = event.column;
                        self.display.hold_feedback.hold_judgments[column] =
                            Some(HoldJudgmentRenderInfo {
                                result: HoldResult::Missed,
                                started_at_screen_s: self.boundary.total_elapsed_in_screen,
                            });
                    }
                    PendingMissedHoldResolution::ScoreLetGo => {
                        self.handle_hold_let_go(event.column, event.note_index, event.end_time_ns);
                    }
                }
            }
            if update.finished || update.event_count == 0 {
                break;
            }
        }
    }

    #[inline(always)]
    pub fn spawn_tap_explosion(&mut self, column: usize, window_key: &'static str, bright: bool) {
        let player = self.player_for_col(column);
        if !self.tap_explosion_enabled_for_player(player, window_key) {
            return;
        }
        let local_col = local_column_for_field(self.setup.cols_per_player, column);
        let spawn_duration = self
            .display
            .noteskin_effects
            .tap_explosion_duration(player, local_col, window_key, bright);
        if let Some(duration) = spawn_duration {
            self.display.visual_feedback.tap_explosions[column] = Some(ActiveTapExplosion {
                window: window_key,
                bright,
                elapsed: 0.0,
                duration,
                start_beat: self.clock.song_position.current_beat,
            });
        }
    }

    #[inline(always)]
    pub fn trigger_mine_explosion(&mut self, column: usize) {
        let player = self.player_for_col(column);
        let duration = self
            .display
            .noteskin_effects
            .mine_explosion_duration(player);
        self.display.visual_feedback.mine_explosions[column] = Some(ActiveMineExplosion {
            elapsed: 0.0,
            duration,
            started_at_screen_s: self.boundary.total_elapsed_in_screen,
        });
        if self.setup.config.mine_hit_sound {
            self.push_audio_command(GameplayAudioCommand::PlayPreloadedSfx(
                "assets/sounds/boom.ogg",
            ));
        }
    }

    #[inline(always)]
    pub fn trigger_tap_judgment_explosion(
        &mut self,
        player_idx: usize,
        column: usize,
        judgment: &Judgment,
    ) {
        self.trigger_column_flash_for_judgment(player_idx, column, judgment);
        let Some(window_key) = grade_to_window(judgment.grade) else {
            return;
        };
        let bright = self.tap_judgment_uses_bright_explosion(player_idx, judgment);
        self.spawn_tap_explosion(column, window_key, bright);
    }

    #[inline(always)]
    pub fn trigger_completed_row_tap_explosions(&mut self, player_idx: usize, row_index: usize) {
        let Some(plan) = ({
            let Some(row_entry) = row_entry_for_cached_row(
                &self.chart_runtime.row_entries,
                &self.chart_runtime.row_indices.row_map_cache[player_idx],
                row_index,
            ) else {
                return;
            };
            completed_row_tap_feedback_plan(&self.chart_runtime.notes, row_entry)
        }) else {
            return;
        };

        for &note_index in &plan.note_indices[..plan.note_count] {
            let note = &self.chart_runtime.notes[note_index];
            let column = note.column;
            let beat = note.beat;
            if song_lua_hides_note_visual(self, player_idx, column, beat) {
                if let Some(window_key) = plan.receptor_window {
                    self.trigger_receptor_score_pulse(column, window_key);
                }
                continue;
            }
            self.trigger_tap_judgment_explosion(player_idx, column, &plan.judgment);
        }
    }

    #[inline(always)]
    pub fn register_provisional_early_result(&mut self, note_index: usize, judgment: Judgment) {
        apply_provisional_early_note_result(
            &mut self.chart_runtime.notes,
            &mut self.chart_runtime.row_entries,
            &self.chart_runtime.row_indices.note_row_entry_indices,
            note_index,
            judgment,
        );
    }

    #[inline(always)]
    pub fn set_final_note_result(&mut self, note_index: usize, judgment: Judgment) {
        let update = apply_final_note_result_to_rows(
            &mut self.chart_runtime.notes,
            &mut self.chart_runtime.row_entries,
            &self.chart_runtime.row_indices.note_row_entry_indices,
            note_index,
            judgment,
            MAX_COLS,
        );
        let effects = update.effects;
        if let Some(column) = effects.trigger_miss_flash_column {
            self.trigger_column_flash_for_grade(column, judgment.grade);
        }
        if let Some(column) = effects.held_miss_column {
            self.display.hold_feedback.held_miss_judgments[column] =
                Some(held_miss_render_info(self.boundary.total_elapsed_in_screen));
        }
    }

    #[inline(always)]
    pub fn update_danger_fx(&mut self) {
        let now = self.boundary.total_elapsed_in_screen;
        for player in 0..self.setup.num_players {
            if self.profiles_runtime.profiles[player].hide_lifebar() {
                self.display.danger_fx.reset_player(player);
                continue;
            }

            let health = player_health_state(&self.players_runtime.players[player]);
            let hide_danger = self.profiles_runtime.profiles[player].hide_danger();
            self.display
                .danger_fx
                .update_player(player, health, now, hide_danger);
        }
    }

    #[inline(always)]
    pub fn tick_visual_effects(&mut self, delta_time: f32) {
        let lane_counts = *self.lane_input_counts();
        self.display.receptor_feedback.tick(
            &self.display.noteskin_effects,
            self.setup.num_cols,
            self.setup.num_players,
            self.setup.cols_per_player,
            &lane_counts,
            delta_time,
        );
        self.display.toggle_flash.tick(delta_time);
        for player in 0..self.setup.num_players {
            tick_player_combo_milestones(&mut self.players_runtime.players[player], delta_time);
        }
        for slot in &mut self.display.visual_feedback.tap_explosions {
            tick_tap_explosion_slot(slot, delta_time);
        }
        for slot in &mut self.display.visual_feedback.mine_explosions {
            tick_mine_explosion_slot(slot, delta_time);
        }
        for slot in &mut self.display.visual_feedback.column_flashes {
            if let Some(active) = slot
                && column_flash_expired_at(*active, self.boundary.total_elapsed_in_screen)
            {
                *slot = None;
            }
        }
        for slot in &mut self.display.hold_feedback.hold_judgments {
            if let Some(render_info) = slot
                && hold_judgment_expired_at(*render_info, self.boundary.total_elapsed_in_screen)
            {
                *slot = None;
            }
        }
        for slot in &mut self.display.hold_feedback.held_miss_judgments {
            if let Some(render_info) = slot
                && held_miss_judgment_expired_at(
                    *render_info,
                    self.boundary.total_elapsed_in_screen,
                )
            {
                *slot = None;
            }
        }
    }

    pub fn finalize_completed_mines(&mut self) {
        let mines_hit: [u32; MAX_PLAYERS] = std::array::from_fn(|player| {
            self.players_runtime
                .players
                .get(player)
                .map(player_mines_hit)
                .unwrap_or(0)
        });
        let update = finalize_completed_mine_avoidance_for_players(
            &mut self.chart_runtime.notes,
            self.chart_runtime.note_ranges.ranges(),
            &self.progress.chart_totals.mines_total,
            &mines_hit,
            self.setup.num_players,
        );
        for player in 0..update.players_finalized {
            set_player_mines_avoided(
                &mut self.players_runtime.players[player],
                update.mines_avoided[player],
            );
        }
    }

    #[inline(always)]
    pub fn score_rows_finalized(&self) -> bool {
        score_rows_finalized_for_players(
            &self.chart_runtime.row_entries,
            &self.chart_runtime.row_indices.row_entry_ranges,
            self.setup.num_players,
        )
    }

    #[inline(always)]
    fn missed_note_cutoff_rows(
        &mut self,
        music_time_ns: SongTimeNs,
    ) -> [usize; MAX_PLAYERS] {
        let music_rate = self.music_rate();
        let num_players = self.setup.num_players;
        let GameplayTimingRuntimeState {
            timing_players,
            time_to_beat_caches,
            timing_profile,
            ..
        } = &mut self.timing_runtime;
        let player_refs = std::array::from_fn(|player| timing_players[player].as_ref());
        time_to_beat_caches.missed_note_cutoff_rows(
            timing_profile,
            &player_refs,
            music_rate,
            music_time_ns,
            num_players,
        )
    }

    #[inline(always)]
    pub fn apply_time_based_mine_avoidance(&mut self, music_time_ns: SongTimeNs) {
        let music_time_sec = song_time_ns_to_seconds(music_time_ns);
        let log_mine_avoid = log::log_enabled!(log::Level::Trace);
        let cutoff_rows = self.missed_note_cutoff_rows(music_time_ns);
        let player_updates = apply_time_based_mine_avoidance_for_players(
            &mut self.chart_runtime.notes,
            &self.chart_runtime.mine_scan.mine_note_ix,
            &self.chart_runtime.mine_scan.next_mine_ix_cursor,
            &cutoff_rows,
            self.chart_runtime.note_ranges.ranges(),
            self.setup.num_players,
        );
        for player in 0..player_updates.players_scanned {
            let update = player_updates.updates[player];
            if let Some(event) = update.last_avoided {
                if log_mine_avoid {
                    let row_index = event.row_index;
                    let column = event.column;
                    log::trace!(
                        "MINE AVOIDED: Row {row_index}, Col {column}, Time: {music_time_sec:.2}s"
                    );
                }
                self.set_last_mine_judgment(player, event.column, MineResult::Avoided);
            }
            if update.avoided_count > 0 {
                add_player_mines_avoided(
                    &mut self.players_runtime.players[player],
                    update.avoided_count,
                );
            }
            self.chart_runtime.mine_scan.next_mine_ix_cursor[player] = update.mine_end;
            self.chart_runtime.mine_scan.next_mine_avoid_cursor[player] =
                update.next_mine_avoid_cursor;
        }
    }

    #[inline(always)]
    pub fn apply_time_based_tap_misses(&mut self, music_time_ns: SongTimeNs) {
        let rate = normalized_song_rate(self.music_rate());
        let music_time_sec = song_time_ns_to_seconds(music_time_ns);
        let cutoff_rows = self.missed_note_cutoff_rows(music_time_ns);
        let mut miss_events = [None; 16];
        loop {
            let update = collect_time_based_tap_misses_for_players(
                &mut self.chart_runtime.notes,
                &self.chart_runtime.note_time_cache_ns,
                &self.hold_runtime.tap_miss_held_window,
                &mut self.hold_runtime.hold_decay_active,
                &mut self.hold_runtime.decaying_hold_indices,
                &mut self.chart_runtime.mine_scan.next_tap_miss_cursor,
                self.chart_runtime.note_ranges.ranges(),
                &cutoff_rows,
                music_time_ns,
                rate,
                &self.progress.stage.score_missed_holds_rolls,
                self.setup.num_players,
                &mut miss_events,
            );
            for player_event in miss_events.iter().take(update.event_count).flatten() {
                let player = player_event.player;
                let event = player_event.event;
                if event.queue_missed_hold_resolution {
                    self.queue_missed_hold_resolution(event.note_index);
                }
                self.set_final_note_result(event.note_index, event.judgment);
                let judgment_time_error_ms = event.judgment.time_error_ms;
                if log::log_enabled!(log::Level::Debug) {
                    let note_time = song_time_ns_to_seconds(event.note_time_ns);
                    let song_offset_s = self.clock.offsets.song_offset_seconds();
                    let global_offset_s = self.effective_player_global_offset_seconds(player);
                    let lead_in_s = self.clock.audio_clock.positive_lead_in_seconds();
                    let stream_pos_s = self.clock.audio_clock.stream_position_seconds();
                    let expected_stream_for_note_s =
                        note_time / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let expected_stream_for_miss_s =
                        music_time_sec / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
                    let stream_delta_miss_ms = (stream_pos_s - expected_stream_for_miss_s) * 1000.0;

                    log::debug!(
                        concat!(
                            "TIMING MISS: row={}, col={}, beat={:.3}, ",
                            "song_offset_s={:.4}, global_offset_s={:.4}, ",
                            "note_time_s={:.6}, miss_time_s={:.6}, ",
                            "offset_ms={:.2}, miss_because_held={}, rate={:.3}, lead_in_s={:.4}, ",
                            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
                            "stream_miss_s={:.6}, stream_delta_miss_ms={:.2}"
                        ),
                        event.row_index,
                        event.column,
                        event.beat,
                        song_offset_s,
                        global_offset_s,
                        note_time,
                        music_time_sec,
                        judgment_time_error_ms,
                        event.miss_because_held,
                        rate,
                        lead_in_s,
                        stream_pos_s,
                        expected_stream_for_note_s,
                        stream_delta_note_ms,
                        expected_stream_for_miss_s,
                        stream_delta_miss_ms,
                    );
                }
                log::debug!("MISSED (time-based): Row {}", event.row_index);
            }
            if !update.stopped || update.event_count == 0 {
                break;
            }
        }
    }

    fn latch_column_judgment_health(&mut self) {
        self.players_runtime.column_judgments_active = std::array::from_fn(|player| {
            player < self.setup.num_players
                && !player_runtime_is_dead(&self.players_runtime.players[player])
        });
    }

    pub fn finalize_row_judgment(
        &mut self,
        player: usize,
        row_index: usize,
        row_entry_index: usize,
        skip_life_change: bool,
    ) {
        let (col_start, col_end) = self.player_col_range(player);
        let Some(plan) = row_finalization_plan_for_entry(
            &self.chart_runtime.notes,
            &self.chart_runtime.row_entries[row_entry_index],
            self.autoplay_blocks_scoring(),
            skip_life_change,
        ) else {
            return;
        };
        self.apply_autosync_for_row_hits(row_entry_index);
        let final_judgment = plan.judgment;
        self.chart_runtime.row_entries[row_entry_index].final_outcome = Some(plan.outcome);
        if self.players_runtime.column_judgments_active[player] {
            let row = &self.chart_runtime.row_entries[row_entry_index];
            let note_indices = row.nonmine_note_indices;
            let note_count = usize::from(row.nonmine_note_count);
            for &note_index in &note_indices[..note_count] {
                if let Some(eligible) = self
                    .chart_runtime
                    .column_judgment_eligible
                    .get_mut(note_index)
                {
                    *eligible = true;
                }
            }
        }
        record_player_live_timing_stats(&mut self.players_runtime.players[player], &final_judgment);
        if plan.record_display_window_counts {
            self.progress.window_counts.record_judgment(
                player,
                &final_judgment,
                self.player_blue_window_ms(player),
            );
        }
        let current_music_time = self.current_music_time_seconds();
        if plan.apply_player_state {
            let p = &mut self.players_runtime.players[player];
            let player_dead = player_runtime_is_dead(p);
            let carried_holds_down = carried_holds_down_at_row(
                &self.chart_runtime.notes,
                &self.hold_runtime.active_holds,
                (col_start, col_end),
                row_index,
            );
            let mut row_state = row_finalization_player_state(p);
            let update = apply_row_finalization_player_state(
                &mut row_state,
                &final_judgment,
                plan.note_count,
                carried_holds_down,
                player_dead,
            );
            set_row_finalization_player_state(p, row_state);
            if update.update_grade_totals {
                update_itg_grade_totals(p);
            }
            if plan.apply_life_change {
                apply_life_change(p, current_music_time, plan.life_delta);
            }
            apply_combo_update(p, update.combo_update);
        }
        if plan.show_final_visual {
            // Arrow Cloud's gameplay HUD uses the row-final JudgmentMessage for
            // offset/error-bar visuals, not individual note hits inside a chord.
            self.set_last_judgment(player, final_judgment);
            self.error_bar_register_tap(player, &final_judgment, self.current_music_time_seconds());
        }
        if plan.capture_failed_ex_score_inputs {
            self.capture_failed_ex_score_inputs(player, self.player_blue_window_ms(player));
        }
    }

    pub fn update_judged_rows(&mut self) {
        let lookahead_time_ns = judged_row_lookahead_time_ns(
            self.clock.song_position.current_music_time_ns,
            &self.timing_runtime.timing_profile,
            self.music_rate(),
        );
        for player in 0..self.setup.num_players {
            let (row_start, row_end) = self.chart_runtime.row_indices.row_entry_ranges[player];
            let row_count = row_end;
            let mut scan_start =
                self.chart_runtime.row_indices.judged_row_cursor[player].max(row_start);
            let mut events = [None; 8];
            loop {
                let update = collect_ready_judged_row_events(
                    &self.chart_runtime.row_entries,
                    (row_start, row_end),
                    scan_start,
                    lookahead_time_ns,
                    &mut events,
                );
                scan_start = update.next_scan_start;
                for event in events.iter().take(update.event_count).flatten() {
                    self.finalize_row_judgment(
                        player,
                        event.row_index,
                        event.row_entry_index,
                        event.skip_life_change,
                    );
                }
                if !update.stopped || update.event_count == 0 {
                    break;
                }
            }
            self.chart_runtime.row_indices.judged_row_cursor[player] =
                advance_judged_row_cursor_for_entries(
                    &self.chart_runtime.row_entries,
                    (row_start, row_count),
                    self.chart_runtime.row_indices.judged_row_cursor[player],
                    lookahead_time_ns,
                );
        }
    }

    #[inline(always)]
    pub fn settle_completion_rows(&mut self) -> bool {
        self.update_judged_rows();
        self.score_rows_finalized()
    }

    #[inline(always)]
    pub fn start_active_hold_for_hit(
        &mut self,
        note_index: usize,
        column: usize,
        hit: NoteHitEval,
        current_time_ns: SongTimeNs,
    ) {
        let hold_end_time_ns = self
            .chart_runtime
            .hold_end_time_cache_ns
            .get(note_index)
            .copied()
            .flatten();
        if let Some(plan) = hit_active_hold_start(
            self.chart_runtime.notes[note_index].note_type,
            note_index,
            column,
            hit.note_time_ns,
            hold_end_time_ns,
            current_time_ns,
        ) {
            self.start_active_hold(
                plan.column,
                plan.note_index,
                plan.start_time_ns,
                plan.end_time_ns,
                plan.current_time_ns,
            );
        }
    }

    pub fn run_autoplay(&mut self, now_music_time_ns: SongTimeNs) {
        if !self.stage_autoplay_enabled() {
            return;
        }

        for player in 0..self.setup.num_players {
            let note_range = self.note_range_for_player(player);
            let mut cursor = self.autoplay_cursor(player).max(note_range.0);
            loop {
                let mut events = [None; MAX_COLS];
                let update = collect_next_autoplay_row_events(
                    &self.chart_runtime.notes,
                    &self.chart_runtime.note_time_cache_ns,
                    note_range,
                    cursor,
                    self.setup.num_cols,
                    now_music_time_ns,
                    &mut events,
                );
                cursor = update.cursor;
                if !update.row_ready {
                    break;
                }
                // Finalize any already-ended autoplay holds before a new warped
                // row on the same lane can replace the active hold slot.
                self.settle_due_autoplay_active_holds(update.row_time_ns);
                for event in events.iter().take(update.event_count).flatten() {
                    self.mark_autoplay_used();
                    match event.action {
                        AutoplayNoteAction::Lift => {
                            let _ = self.judge_a_lift(event.column, update.row_time_ns);
                        }
                        AutoplayNoteAction::Tap => {
                            let _ = self.judge_a_tap(event.column, update.row_time_ns);
                        }
                    }
                }
            }
            self.set_autoplay_cursor(player, cursor);
        }

        let mut roll_cols = [usize::MAX; MAX_COLS];
        let roll_count = collect_active_autoplay_roll_columns(
            &self.hold_runtime.active_holds,
            self.setup.num_cols,
            &mut roll_cols,
        );
        for col in roll_cols.into_iter().take(roll_count) {
            self.refresh_roll_life_on_step(col, self.clock.song_position.current_music_time_ns);
        }
    }

    pub fn collect_ready_replay_edges(
        &mut self,
        events: &mut [Option<RecordedLaneEdge>; MAX_COLS],
    ) -> usize {
        if !self.stage_autoplay_enabled() || !self.progress.replay.mode {
            return 0;
        }
        self.progress.replay.input.collect_ready(
            self.clock.song_position.current_music_time_ns,
            self.setup.num_cols,
            events,
        )
    }

    pub fn run_autoplay_or_replay_phase(
        &mut self,
        now_music_time_ns: SongTimeNs,
        replay_events: &mut [Option<RecordedLaneEdge>; MAX_COLS],
    ) {
        if self.progress.replay.mode {
            let event_count = self.collect_ready_replay_edges(replay_events);
            for edge in replay_events.iter().take(event_count).flatten().copied() {
                handle_replay_edge(self, edge);
                self.mark_autoplay_used();
            }
        } else {
            self.run_autoplay(now_music_time_ns);
        }
    }

    pub fn run_pre_notes_phase(
        &mut self,
        music_time_ns: SongTimeNs,
        display_music_time_ns: SongTimeNs,
        delta_time: f32,
        seconds_per_second: f32,
        assist_sfx_generation: u64,
        assist_tick_sfx_path: &'static str,
    ) {
        self.update_song_position_from_time(music_time_ns, display_music_time_ns);
        let song_row = self.assist_row_no_offset_ns(music_time_ns);
        self.run_assist_clap(
            song_row,
            music_time_ns,
            seconds_per_second,
            assist_sfx_generation,
            assist_tick_sfx_path,
        );
        refresh_active_attack_masks(self, delta_time);
        let current_bpm = self
            .timing_runtime
            .timing
            .get_bpm_for_beat(self.clock.song_position.current_beat);
        self.refresh_live_notefield_options(current_bpm);
    }

    pub fn run_post_input_gameplay_phases(
        &mut self,
        previous_music_time_ns: SongTimeNs,
        music_time_ns: SongTimeNs,
        music_time_sec: f32,
        delta_time: f32,
        trace_enabled: bool,
        phase_timings: &mut GameplayUpdatePhaseTimings,
    ) {
        let held_mines_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let current_inputs = self.current_lane_inputs();
        if !self.live_autoplay_enabled() {
            for (col, crossed_from_ns) in self
                .held_mine_crossing_start_times(
                    &current_inputs,
                    previous_music_time_ns,
                    music_time_ns,
                )
                .into_iter()
                .enumerate()
            {
                if let Some(crossed_from_ns) = crossed_from_ns {
                    let _ =
                        self.try_hit_crossed_mines_while_held(col, crossed_from_ns, music_time_ns);
                }
            }
        }
        self.track_held_miss_windows(&current_inputs, music_time_ns);
        self.set_previous_lane_inputs(current_inputs);
        if let Some(started) = held_mines_started {
            phase_timings.held_mines_us = elapsed_us_since(started);
        }

        let active_holds_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.update_active_holds(&current_inputs, music_time_ns);
        if let Some(started) = active_holds_started {
            phase_timings.active_holds_us = elapsed_us_since(started);
        }
        self.apply_pending_mine_hits();

        let hold_decay_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.decay_let_go_hold_life();
        self.resolve_pending_missed_holds(music_time_ns);
        if let Some(started) = hold_decay_started {
            phase_timings.hold_decay_us = elapsed_us_since(started);
        }

        let visuals_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.tick_visual_effects(delta_time);
        if let Some(started) = visuals_started {
            phase_timings.visuals_us = elapsed_us_since(started);
        }

        let judged_rows_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        // ITGmania resolves already-complete rows before it promotes overdue
        // mines/taps to avoids/misses.
        self.update_judged_rows();
        if let Some(started) = judged_rows_started {
            phase_timings.judged_rows_us = elapsed_us_since(started);
        }

        let mine_avoid_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.apply_time_based_mine_avoidance(music_time_ns);
        if let Some(started) = mine_avoid_started {
            phase_timings.mine_avoid_us = elapsed_us_since(started);
        }

        let tap_miss_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.apply_time_based_tap_misses(music_time_ns);
        if let Some(started) = tap_miss_started {
            phase_timings.tap_miss_us = elapsed_us_since(started);
        }

        let density_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.update_density_graph(music_time_sec, trace_enabled, phase_timings);
        if let Some(started) = density_started {
            phase_timings.density_us = elapsed_us_since(started);
        }

        let danger_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.update_danger_fx();
        if let Some(started) = danger_started {
            phase_timings.danger_us = elapsed_us_since(started);
        }
    }

    #[inline(always)]
    fn closest_lane_note_search_cached(
        &mut self,
        column: usize,
        player: usize,
        current_time_ns: SongTimeNs,
    ) -> LaneNoteSearch {
        let rows = self.timing_runtime.time_to_beat_caches.lane_search_rows(
            player,
            &self.timing_runtime.timing_players[player],
            current_time_ns,
        );
        closest_lane_note_search_with_rows(
            &self.chart_runtime.lane_indices.note_indices[column],
            &self.chart_runtime.notes,
            &self.chart_runtime.note_time_cache_ns,
            &self.timing_runtime.timing_players[player],
            current_time_ns,
            rows,
        )
    }

    #[inline(always)]
    pub fn judge_a_tap(&mut self, column: usize, current_time_ns: SongTimeNs) -> bool {
        let rate = normalized_song_rate(self.music_rate());
        let timing_hit_log = timing_hit_log_enabled();
        let input_log = gameplay_input_log_enabled();
        let player = self.player_for_col(column);
        let rescore_early_hits = self.profiles_runtime.profiles[player].rescore_early_hits();
        let hide_early_dw_judgments =
            self.profiles_runtime.profiles[player].hide_early_dw_judgments();
        let hide_early_dw_flash = self.profiles_runtime.profiles[player].hide_early_dw_flash();
        let hide_early_dw_column_flash =
            self.profiles_runtime.profiles[player].hide_early_dw_column_flash();
        let scoring_blocked = self.autoplay_blocks_scoring();
        let search = self.closest_lane_note_search_cached(column, player, current_time_ns);
        let lane_notes = &self.chart_runtime.lane_indices.note_indices[column];
        let current_row_index = search.current_row_index;
        if let Some((note_index, _)) = search.candidate {
            let note_row_index = self.chart_runtime.notes[note_index].row_index;
            let note_type = self.chart_runtime.notes[note_index].note_type;
            let time_error_music_ns =
                current_time_ns.saturating_sub(self.chart_runtime.note_time_cache_ns[note_index]);

            if matches!(note_type, NoteType::Mine) {
                if self.chart_runtime.notes[note_index].is_fake {
                    log_tap_judge_candidate(
                        input_log,
                        "fake_mine_ignored",
                        player,
                        column,
                        current_row_index,
                        current_time_ns,
                        note_index,
                        &self.chart_runtime.notes[note_index],
                        self.chart_runtime.note_time_cache_ns[note_index],
                        rate,
                    );
                    return false;
                }
                let hit = self.hit_mine(column, note_index, time_error_music_ns);
                log_tap_judge_candidate(
                    input_log,
                    if hit {
                        "mine_hit"
                    } else {
                        "mine_outside_window"
                    },
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                return hit;
            }
            if !lane_edge_matches_note_type(true, note_type) {
                log_tap_judge_candidate(
                    input_log,
                    "note_type_mismatch",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                return false;
            }

            let Some(hit) = self.note_hit_eval(
                player,
                self.chart_runtime.note_time_cache_ns[note_index],
                current_time_ns,
            ) else {
                log_tap_judge_candidate(
                    input_log,
                    "outside_tap_window",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                return false;
            };
            let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
                (
                    self.clock.offsets.song_offset_seconds(),
                    self.effective_player_global_offset_seconds(player),
                    self.clock.audio_clock.positive_lead_in_seconds(),
                    self.clock.audio_clock.stream_position_seconds(),
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            if self.chart_runtime.notes[note_index].is_fake {
                log_tap_judge_candidate(
                    input_log,
                    "fake_hit",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                let hit_plan = self.build_final_note_hit_plan(player, hit, rate);
                let judgment = hit_plan.judgment;
                self.set_final_note_result(note_index, judgment);
                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    hit.grade,
                    note_row_index,
                    self.chart_runtime.notes[note_index].column,
                    self.chart_runtime.notes[note_index].beat,
                    song_offset_s,
                    global_offset_s,
                    hit.note_time_ns,
                    hit_plan.judgment_event_time_ns,
                    self.current_music_time_seconds(),
                    rate,
                    lead_in_s,
                );
                self.trigger_receptor_glow_pulse(column);
                return true;
            }
            let Some(row_entry) = row_entry_for_cached_row(
                &self.chart_runtime.row_entries,
                &self.chart_runtime.row_indices.row_map_cache[player],
                note_row_index,
            ) else {
                debug_assert!(false, "missing row cache for row {note_row_index}");
                return false;
            };
            let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
            let row_note_count = usize::from(row_entry.unresolved_nonlift_count);

            if rescore_early_hits
                && let Some(early_rescore_decision) = early_rescore_hit_decision(
                    row_rescore_track_count,
                    hit,
                    self.chart_runtime.notes[note_index].early_result.is_some(),
                )
            {
                let note_col = self.chart_runtime.notes[note_index].column;

                if matches!(
                    early_rescore_decision,
                    EarlyRescoreHitDecision::Provisional
                        | EarlyRescoreHitDecision::DuplicateProvisional
                ) {
                    log_tap_judge_candidate(
                        input_log,
                        match early_rescore_decision {
                            EarlyRescoreHitDecision::DuplicateProvisional => {
                                "provisional_early_duplicate"
                            }
                            _ => "provisional_early_hit",
                        },
                        player,
                        column,
                        current_row_index,
                        current_time_ns,
                        note_index,
                        &self.chart_runtime.notes[note_index],
                        self.chart_runtime.note_time_cache_ns[note_index],
                        rate,
                    );
                    if early_rescore_decision == EarlyRescoreHitDecision::Provisional {
                        let plan = provisional_early_hit_plan(hit, rate, scoring_blocked);
                        let judgment = plan.judgment;
                        self.register_provisional_early_result(note_index, judgment);
                        let current_music_time = self.current_music_time_seconds();
                        if plan.apply_life_change {
                            apply_life_change(
                                &mut self.players_runtime.players[player],
                                current_music_time,
                                plan.life_delta,
                            );
                        }
                        if plan.capture_failed_ex_score_inputs {
                            self.capture_failed_ex_score_inputs(
                                player,
                                self.player_blue_window_ms(player),
                            );
                        }
                        self.render_provisional_early_rescore_feedback(
                            player,
                            note_col,
                            &judgment,
                            song_time_ns_to_seconds(current_time_ns),
                            hide_early_dw_judgments,
                            hide_early_dw_flash,
                            hide_early_dw_column_flash,
                        );
                        // Zmod parity: provisional early W4/W5 (with Rescore Early Hits enabled)
                        // should immediately drive EarlyHit-style visuals, but the later finalized
                        // W4/W5 should not produce a second bad popup/tick.
                        log_timing_hit_detail(
                            timing_hit_log,
                            stream_pos_s,
                            hit.grade,
                            note_row_index,
                            note_col,
                            self.chart_runtime.notes[note_index].beat,
                            song_offset_s,
                            global_offset_s,
                            hit.note_time_ns,
                            current_time_ns,
                            self.current_music_time_seconds(),
                            rate,
                            lead_in_s,
                        );

                        self.start_active_hold_for_hit(note_index, note_col, hit, current_time_ns);
                    }
                    return true;
                }

                if early_rescore_decision == EarlyRescoreHitDecision::IgnoreBadRehit {
                    log_tap_judge_candidate(
                        input_log,
                        "provisional_bad_rehit_ignored",
                        player,
                        column,
                        current_row_index,
                        current_time_ns,
                        note_index,
                        &self.chart_runtime.notes[note_index],
                        self.chart_runtime.note_time_cache_ns[note_index],
                        rate,
                    );
                    return true;
                }

                log_tap_judge_candidate(
                    input_log,
                    "hit",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                let hit_plan = self.build_final_note_hit_plan(player, hit, rate);
                let judgment = hit_plan.judgment;
                self.set_final_note_result(note_index, judgment);

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    hit.grade,
                    note_row_index,
                    note_col,
                    self.chart_runtime.notes[note_index].beat,
                    song_offset_s,
                    global_offset_s,
                    hit.note_time_ns,
                    hit_plan.judgment_event_time_ns,
                    self.current_music_time_seconds(),
                    rate,
                    lead_in_s,
                );

                self.trigger_completed_row_tap_explosions(player, note_row_index);
                if let Some(window_key) = hit_plan.receptor_window {
                    self.trigger_receptor_score_pulse(note_col, window_key);
                }
                self.start_active_hold_for_hit(note_index, note_col, hit, current_time_ns);
                return true;
            }

            let Some((judge_indices, judge_count)) =
                collect_edge_judge_indices(row_note_count, note_index)
            else {
                log_tap_judge_candidate(
                    input_log,
                    "no_row_judge_indices",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    note_index,
                    &self.chart_runtime.notes[note_index],
                    self.chart_runtime.note_time_cache_ns[note_index],
                    rate,
                );
                return false;
            };

            for &idx in &judge_indices[..judge_count] {
                let note_col = self.chart_runtime.notes[idx].column;
                let Some(hit) = self.note_hit_eval(
                    player,
                    self.chart_runtime.note_time_cache_ns[idx],
                    current_time_ns,
                ) else {
                    log_tap_judge_candidate(
                        input_log,
                        "row_sibling_outside_tap_window",
                        player,
                        column,
                        current_row_index,
                        current_time_ns,
                        idx,
                        &self.chart_runtime.notes[idx],
                        self.chart_runtime.note_time_cache_ns[idx],
                        rate,
                    );
                    continue;
                };
                log_tap_judge_candidate(
                    input_log,
                    "hit",
                    player,
                    column,
                    current_row_index,
                    current_time_ns,
                    idx,
                    &self.chart_runtime.notes[idx],
                    self.chart_runtime.note_time_cache_ns[idx],
                    rate,
                );
                let hit_plan = self.build_final_note_hit_plan(player, hit, rate);
                let judgment = hit_plan.judgment;
                self.set_final_note_result(idx, judgment);

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    hit.grade,
                    note_row_index,
                    note_col,
                    self.chart_runtime.notes[idx].beat,
                    song_offset_s,
                    global_offset_s,
                    hit.note_time_ns,
                    hit_plan.judgment_event_time_ns,
                    self.current_music_time_seconds(),
                    rate,
                    lead_in_s,
                );

                self.trigger_completed_row_tap_explosions(player, note_row_index);
                if let Some(window_key) = hit_plan.receptor_window {
                    self.trigger_receptor_score_pulse(note_col, window_key);
                }
                self.start_active_hold_for_hit(idx, note_col, hit, current_time_ns);
            }
            return true;
        }
        if input_log {
            log::debug!(
                concat!(
                    "GAMEPLAY TAP JUDGE: reason=no_candidate, player={}, lane={}, ",
                    "current_row={}, search_rows={}..{}, search_indices={}..{}, ",
                    "lane_notes={}, event_time_s={:.6}, current_time_s={:.6}"
                ),
                player,
                column,
                current_row_index,
                search.search_start_row,
                search.search_end_row,
                search.search_start_idx,
                search.search_end_idx,
                lane_notes.len(),
                song_time_ns_to_seconds(current_time_ns),
                self.current_music_time_seconds(),
            );
        }
        false
    }

    /// Judge lift notes on button release. Mirrors tap judging's per-note path but
    /// only matches NoteType::Lift.
    pub fn judge_a_lift(&mut self, column: usize, current_time_ns: SongTimeNs) -> bool {
        let rate = normalized_song_rate(self.music_rate());
        let timing_hit_log = timing_hit_log_enabled();
        let player = self.player_for_col(column);
        let rescore_early_hits = self.profiles_runtime.profiles[player].rescore_early_hits();
        let hide_early_dw_judgments =
            self.profiles_runtime.profiles[player].hide_early_dw_judgments();
        let hide_early_dw_flash = self.profiles_runtime.profiles[player].hide_early_dw_flash();
        let hide_early_dw_column_flash =
            self.profiles_runtime.profiles[player].hide_early_dw_column_flash();
        let scoring_blocked = self.autoplay_blocks_scoring();
        let search = self.closest_lane_note_search_cached(column, player, current_time_ns);
        let Some((note_index, _)) = search.candidate else {
            return false;
        };
        if !lane_edge_matches_note_type(false, self.chart_runtime.notes[note_index].note_type) {
            return false;
        }

        let Some(hit) = self.note_hit_eval(
            player,
            self.chart_runtime.note_time_cache_ns[note_index],
            current_time_ns,
        ) else {
            return false;
        };
        let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
            (
                self.clock.offsets.song_offset_seconds(),
                self.effective_player_global_offset_seconds(player),
                self.clock.audio_clock.positive_lead_in_seconds(),
                self.clock.audio_clock.stream_position_seconds(),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let note_col = self.chart_runtime.notes[note_index].column;
        let note_row_index = self.chart_runtime.notes[note_index].row_index;
        let note_beat = self.chart_runtime.notes[note_index].beat;

        if rescore_early_hits {
            let Some(row_entry) = row_entry_for_cached_row(
                &self.chart_runtime.row_entries,
                &self.chart_runtime.row_indices.row_map_cache[player],
                note_row_index,
            ) else {
                debug_assert!(false, "missing row cache for row {note_row_index}");
                return false;
            };
            let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
            if let Some(early_rescore_decision) = early_rescore_hit_decision(
                row_rescore_track_count,
                hit,
                self.chart_runtime.notes[note_index].early_result.is_some(),
            ) {
                if early_rescore_decision == EarlyRescoreHitDecision::Provisional {
                    let plan = provisional_early_hit_plan(hit, rate, scoring_blocked);
                    let judgment = plan.judgment;
                    self.register_provisional_early_result(note_index, judgment);
                    let current_music_time = self.current_music_time_seconds();
                    if plan.apply_life_change {
                        apply_life_change(
                            &mut self.players_runtime.players[player],
                            current_music_time,
                            plan.life_delta,
                        );
                    }
                    if plan.capture_failed_ex_score_inputs {
                        self.capture_failed_ex_score_inputs(
                            player,
                            self.player_blue_window_ms(player),
                        );
                    }
                    self.render_provisional_early_rescore_feedback(
                        player,
                        note_col,
                        &judgment,
                        song_time_ns_to_seconds(current_time_ns),
                        hide_early_dw_judgments,
                        hide_early_dw_flash,
                        hide_early_dw_column_flash,
                    );

                    log_timing_hit_detail(
                        timing_hit_log,
                        stream_pos_s,
                        hit.grade,
                        note_row_index,
                        note_col,
                        note_beat,
                        song_offset_s,
                        global_offset_s,
                        hit.note_time_ns,
                        current_time_ns,
                        self.current_music_time_seconds(),
                        rate,
                        lead_in_s,
                    );
                    return true;
                }

                if matches!(
                    early_rescore_decision,
                    EarlyRescoreHitDecision::DuplicateProvisional
                        | EarlyRescoreHitDecision::IgnoreBadRehit
                ) {
                    return true;
                }
            }
        }

        let hit_plan = self.build_final_note_hit_plan(player, hit, rate);
        let judgment = hit_plan.judgment;
        self.set_final_note_result(note_index, judgment);

        log_timing_hit_detail(
            timing_hit_log,
            stream_pos_s,
            hit.grade,
            note_row_index,
            note_col,
            note_beat,
            song_offset_s,
            global_offset_s,
            hit.note_time_ns,
            hit_plan.judgment_event_time_ns,
            self.current_music_time_seconds(),
            rate,
            lead_in_s,
        );

        self.trigger_completed_row_tap_explosions(player, note_row_index);
        if let Some(window_key) = hit_plan.receptor_window {
            self.trigger_receptor_score_pulse(note_col, window_key);
        }
        true
    }

    pub fn track_held_miss_windows(
        &mut self,
        inputs: &[bool; MAX_COLS],
        music_time_ns: SongTimeNs,
    ) {
        let mut largest_windows_ns = [0; MAX_PLAYERS];
        for player in 0..self.setup.num_players.min(MAX_PLAYERS) {
            largest_windows_ns[player] = self.player_largest_tap_window_ns(player);
        }
        track_held_miss_windows_for_players(
            &self.chart_runtime.notes,
            &self.chart_runtime.note_time_cache_ns,
            &mut self.hold_runtime.tap_miss_held_window,
            self.chart_runtime.note_ranges.ranges(),
            &self.chart_runtime.mine_scan.next_tap_miss_cursor,
            &largest_windows_ns,
            self.setup.num_players,
            self.setup.cols_per_player,
            inputs,
            music_time_ns,
        );
    }
}
