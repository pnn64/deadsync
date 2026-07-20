impl<Profile, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
{
    pub fn update_exit_transition(&mut self, delta_time: f32) -> Option<GameplayAction> {
        let exit = self.control.exit_input.exit_transition?;
        self.boundary.total_elapsed_in_screen += delta_time;
        if exit.started_at.elapsed().as_secs_f32() >= exit_total_seconds(exit.kind) {
            self.control.exit_input.clear_exit();
            return Some(GameplayAction::NavigateNoFade(gameplay_exit_for_kind(
                exit.kind,
            )));
        }
        Some(GameplayAction::None)
    }

    pub fn clear_expired_exit_abort_text(&mut self) {
        if let Some(at) = self.control.exit_input.hold_to_exit_aborted_at
            && at.elapsed().as_secs_f32() >= GIVE_UP_ABORT_TEXT_SECONDS
        {
            self.control.exit_input.clear_aborted_hold();
        }
    }

    pub fn advance_frame_clock(
        &mut self,
        audio_snapshot: GameplayAudioSnapshot,
        delta_time: f32,
        fallback_host_nanos: impl FnOnce() -> u64,
    ) -> GameplayFrameClockUpdate {
        self.clock.audio_clock.set_audio_snapshot(audio_snapshot);

        // Music time driven directly by the audio device clock, interpolated
        // between callbacks for smooth, continuous motion.
        let song_clock = current_song_clock_snapshot(
            audio_snapshot,
            self.music_rate(),
            self.clock.audio_clock.lead_in_seconds(),
            self.clock.offsets.global_offset_seconds(),
        );
        let lead_in = self.clock.audio_clock.positive_lead_in_seconds();
        let previous_music_time_ns = self.clock.song_position.current_music_time_ns;
        let mut music_time_ns = song_clock.song_time_ns;
        let first_update = self.boundary.total_elapsed_in_screen <= f32::EPSILON;
        if first_update {
            const STARTUP_MAX_FORWARD_JUMP_NS: SongTimeNs = 1_000_000_000;
            let jump_ns = music_time_ns.saturating_sub(previous_music_time_ns);
            if jump_ns > STARTUP_MAX_FORWARD_JUMP_NS {
                let previous_music_time = song_time_ns_to_seconds(previous_music_time_ns);
                let music_time_sec = song_time_ns_to_seconds(music_time_ns);
                let jump_s = song_time_ns_delta_seconds(music_time_ns, previous_music_time_ns);
                log::warn!(
                    "Discarding anomalous first-frame music time jump ({jump_s:.3}s): prev={previous_music_time:.3}, now={music_time_sec:.3}, lead_in={lead_in:.3}"
                );
                music_time_ns = previous_music_time_ns;
            }
        }
        let music_time_sec = song_time_ns_to_seconds(music_time_ns);
        self.clock.song_position.current_music_time_ns = music_time_ns;

        let display_diag_host_nanos = if song_clock.valid_at_host_nanos != 0 {
            song_clock.valid_at_host_nanos
        } else {
            fallback_host_nanos()
        };
        let display_music_time_ns = frame_stable_display_music_time_ns(
            &mut self.clock.display_clock,
            display_diag_host_nanos,
            music_time_ns,
            delta_time,
            song_clock.seconds_per_second,
            first_update,
        );
        self.clock.song_position.current_music_time_display =
            song_time_ns_to_seconds(display_music_time_ns);

        GameplayFrameClockUpdate {
            song_clock,
            previous_music_time_ns,
            music_time_ns,
            music_time_sec,
            display_music_time_ns,
        }
    }

    pub fn begin_gameplay_frame(
        &mut self,
        audio_snapshot: GameplayAudioSnapshot,
        delta_time: f32,
        fallback_host_nanos: impl FnOnce() -> u64,
    ) -> GameplayFrameBeginUpdate {
        self.clear_expired_exit_abort_text();
        let clock = self.advance_frame_clock(audio_snapshot, delta_time, fallback_host_nanos);
        let hold_to_exit_completed = self.complete_hold_to_exit_if_ready(clock.music_time_ns);
        if !hold_to_exit_completed {
            self.boundary.total_elapsed_in_screen += delta_time;
        }
        GameplayFrameBeginUpdate {
            clock,
            hold_to_exit_completed,
        }
    }

    pub fn complete_hold_to_exit_if_ready(&mut self, music_time_ns: SongTimeNs) -> bool {
        let (Some(key), Some(start_time)) = (
            self.control.exit_input.hold_to_exit_key,
            self.control.exit_input.hold_to_exit_start,
        ) else {
            return false;
        };
        if start_time.elapsed().as_secs_f32() < hold_to_exit_seconds(key) {
            return false;
        }
        if key == HoldToExitKey::Start && music_time_ns >= self.notes_end_time_ns() {
            self.progress.stage.song_completed_naturally = true;
            self.finalize_completed_mines();
        }
        match key {
            HoldToExitKey::Start => {
                self.begin_exit_transition(ExitTransitionKind::Out);
            }
            HoldToExitKey::Back => {
                self.begin_exit_transition(ExitTransitionKind::Cancel);
            }
        }
        true
    }

    pub fn finish_gameplay_if_ready(&mut self, trace_enabled: bool) -> Option<GameplayAction> {
        // Match ITG's end-of-song ordering: resolve the frame's late taps, hold
        // ends, and misses before leaving gameplay, otherwise the last frame can
        // cut to evaluation before final judgments land.
        if self.clock.song_position.current_music_time_ns >= self.music_end_time_ns() {
            if !self.settle_completion_rows() && trace_enabled {
                log::trace!("Music end time reached with pending score rows; completing gameplay.");
            }
            log::debug!("Music end time reached. Transitioning to evaluation.");
            self.progress.stage.song_completed_naturally = true;
            self.begin_outro_attack_clear();
            self.finalize_completed_mines();
            return Some(GameplayAction::Navigate(GameplayExit::Complete));
        }

        if matches!(
            self.setup.config.default_fail_type,
            GameplayFailType::Immediate
        ) && self.all_joined_players_failed()
        {
            log::debug!("All joined players failed. Transitioning to evaluation.");
            self.progress.stage.song_completed_naturally = false;
            self.push_audio_command(GameplayAudioCommand::StopMusic);
            return Some(GameplayAction::Navigate(GameplayExit::Complete));
        }

        None
    }

    pub fn update_gameplay_frame(
        &mut self,
        delta_time: f32,
        audio_snapshot: GameplayAudioSnapshot,
        assist_sfx_path: &'static str,
        fallback_host_nanos: impl FnOnce() -> u64,
    ) -> GameplayAction {
        if let Some(action) = self.update_exit_transition(delta_time) {
            return action;
        }

        self.latch_column_judgment_health();

        let trace_enabled = log::log_enabled!(log::Level::Trace);
        let frame_trace_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let mut phase_timings = GameplayUpdatePhaseTimings::default();

        let frame_begin =
            self.begin_gameplay_frame(audio_snapshot, delta_time, fallback_host_nanos);
        let frame_clock = frame_begin.clock;
        let song_clock = frame_clock.song_clock;
        let previous_music_time_ns = frame_clock.previous_music_time_ns;
        let music_time_ns = frame_clock.music_time_ns;
        let music_time_sec = frame_clock.music_time_sec;
        let display_music_time_ns = frame_clock.display_music_time_ns;

        if frame_begin.hold_to_exit_completed {
            self.finalize_update_trace(
                delta_time,
                music_time_sec,
                frame_trace_started,
                phase_timings,
            );
            return GameplayAction::None;
        }

        let pre_notes_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.run_pre_notes_phase(
            music_time_ns,
            display_music_time_ns,
            delta_time,
            song_clock.seconds_per_second,
            audio_snapshot.assist_sfx_generation,
            assist_sfx_path,
        );
        if let Some(started) = pre_notes_started {
            phase_timings.pre_notes_us = elapsed_us_since(started);
        }

        let autoplay_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let mut replay_events = [None; MAX_COLS];
        self.run_autoplay_or_replay_phase(music_time_ns, &mut replay_events);
        if let Some(started) = autoplay_started {
            phase_timings.autoplay_us = elapsed_us_since(started);
        }

        let input_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        self.update_offset_adjust_hold(Instant::now());
        process_input_edges(self, trace_enabled, &mut phase_timings, song_clock);
        if let Some(started) = input_started {
            phase_timings.input_edges_us = elapsed_us_since(started);
        }

        self.run_post_input_gameplay_phases(
            previous_music_time_ns,
            music_time_ns,
            music_time_sec,
            delta_time,
            trace_enabled,
            &mut phase_timings,
        );

        if let Some(action) = self.finish_gameplay_if_ready(trace_enabled) {
            self.finalize_update_trace(
                delta_time,
                music_time_sec,
                frame_trace_started,
                phase_timings,
            );
            return action;
        }

        self.finalize_update_trace(
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        GameplayAction::None
    }

    #[inline(always)]
    pub fn music_rate(&self) -> f32 {
        self.clock.music_rate.rate()
    }

    #[inline(always)]
    pub const fn current_beat(&self) -> f32 {
        self.clock.song_position.current_beat
    }

    #[inline(always)]
    pub const fn current_music_time_ns(&self) -> SongTimeNs {
        self.clock.song_position.current_music_time_ns
    }

    #[inline(always)]
    pub fn current_music_time_seconds(&self) -> f32 {
        song_time_ns_to_seconds(self.clock.song_position.current_music_time_ns)
    }

    #[inline(always)]
    pub fn finalize_update_trace(
        &mut self,
        delta_time: f32,
        music_time_sec: f32,
        frame_trace_started: Option<Instant>,
        phase_timings: GameplayUpdatePhaseTimings,
    ) {
        let Some(started) = frame_trace_started else {
            return;
        };
        let total_us = elapsed_us_since(started);
        trace_gameplay_update(self, delta_time, music_time_sec, total_us, phase_timings);
    }

    #[inline(always)]
    pub fn schedule_assist_clap_row(
        &mut self,
        clap_row: usize,
        assist_tick_sfx_path: &'static str,
    ) {
        let Some(music_seconds) =
            assist_clap_music_seconds_for_row(&self.timing_runtime.timing, clap_row)
        else {
            self.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
                assist_tick_sfx_path,
            ));
            return;
        };
        self.push_audio_command(GameplayAudioCommand::PlayAssistTickAtMusicTime {
            path: assist_tick_sfx_path,
            music_seconds,
        });
    }

    #[inline(always)]
    pub fn run_assist_clap(
        &mut self,
        current_row: i32,
        music_time_ns: SongTimeNs,
        slope: f32,
        assist_sfx_generation: u64,
        assist_tick_sfx_path: &'static str,
    ) {
        let song_row = current_row.max(0);
        let timeline_reset = self
            .control
            .assist_clap
            .note_sfx_generation(assist_sfx_generation);

        let assist_enabled = self.control.tick_mode == GameplayTimingTickMode::Assist;
        let future_row = if assist_enabled {
            self.timing_runtime.time_to_beat_caches.assist_future_row(
                &self.timing_runtime.timing,
                self.clock.offsets.global_offset_seconds(),
                self.clock.audio_clock.output_delay_seconds(),
                music_time_ns,
                slope,
                song_row,
            )
        } else {
            song_row
        };
        let update = self.control.assist_clap.schedule_update(
            song_row,
            future_row,
            assist_enabled,
            timeline_reset,
        );
        for ix in update.schedule_start..update.schedule_end {
            let clap_row = self.control.assist_clap.rows[ix];
            self.schedule_assist_clap_row(clap_row, assist_tick_sfx_path);
        }
    }

    #[inline(always)]
    pub fn set_last_judgment(&mut self, player_idx: usize, judgment: Judgment) {
        set_player_last_judgment(
            &mut self.players_runtime.players[player_idx],
            judgment,
            self.boundary.total_elapsed_in_screen,
        );
    }

    #[inline(always)]
    pub fn set_last_mine_judgment(&mut self, player_idx: usize, column: usize, result: MineResult) {
        set_player_last_mine_judgment(
            &mut self.players_runtime.players[player_idx],
            result,
            column,
            self.boundary.total_elapsed_in_screen,
        );
    }

    pub fn hit_mine(
        &mut self,
        column: usize,
        note_index: usize,
        time_error_music_ns: SongTimeNs,
    ) -> bool {
        let player = self.player_for_col(column);
        let rate = normalized_song_rate(self.music_rate());
        let mine_window_music_ns = self.timing_runtime.player_judgment_timing[player]
            .profile_music_ns
            .mine_window_ns;
        let Some(mark) = mark_mine_hit_candidate(
            &mut self.chart_runtime.notes[note_index],
            self.chart_runtime.note_time_cache_ns[note_index],
            time_error_music_ns,
            mine_window_music_ns,
            rate,
        ) else {
            return false;
        };

        self.chart_runtime
            .mine_scan
            .pending_mine_hit_indices
            .push(note_index);
        log::debug!(
            "JUDGE MINE HIT MARKED: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
            mark.row_index,
            mark.column,
            mark.beat,
            song_time_ns_to_seconds(mark.note_time_ns),
            song_time_ns_to_seconds(mark.hit_time_ns),
            mark.time_error_ms,
            rate
        );
        true
    }

    #[inline(always)]
    pub fn try_hit_crossed_mines_while_held(
        &mut self,
        column: usize,
        prev_time_ns: SongTimeNs,
        current_time_ns: SongTimeNs,
    ) -> bool {
        let player = self.player_for_col(column);
        let rate = normalized_song_rate(self.music_rate());
        let mine_window_music_ns = self.timing_runtime.player_judgment_timing[player]
            .profile_music_ns
            .mine_window_ns;
        let notes = &mut self.chart_runtime.notes;
        let mine_note_ix = &self.chart_runtime.mine_scan.mine_note_ix[player];
        let mine_note_time_ns = &self.chart_runtime.mine_scan.mine_note_time_ns[player];
        let pending_mine_hit_indices = &mut self.chart_runtime.mine_scan.pending_mine_hit_indices;

        mark_crossed_held_mine_candidates(
            notes,
            mine_note_ix,
            mine_note_time_ns,
            column,
            prev_time_ns,
            current_time_ns,
            mine_window_music_ns,
            rate,
            |note_index, mark| {
                pending_mine_hit_indices.push(note_index);
                log::debug!(
                    "JUDGE MINE HIT MARKED: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
                    mark.row_index,
                    mark.column,
                    mark.beat,
                    song_time_ns_to_seconds(mark.note_time_ns),
                    song_time_ns_to_seconds(mark.hit_time_ns),
                    mark.time_error_ms,
                    rate
                );
            },
        )
    }

    #[inline(always)]
    pub fn refresh_roll_life_on_step(&mut self, column: usize, event_time_ns: SongTimeNs) {
        refresh_roll_life_for_active_column(
            &mut self.hold_runtime.active_holds,
            &mut self.chart_runtime.notes,
            column,
            event_time_ns,
        );
    }

    #[inline(always)]
    pub fn decay_let_go_hold_life(&mut self) {
        let rate = self.music_rate();
        decay_let_go_hold_life_for_indices(
            &mut self.chart_runtime.notes,
            &mut self.hold_runtime.hold_decay_active,
            &mut self.hold_runtime.decaying_hold_indices,
            self.clock.song_position.current_music_time_ns,
            rate,
        );
    }

    #[inline(always)]
    pub fn queue_missed_hold_resolution(&mut self, note_index: usize) -> bool {
        queue_pending_missed_hold_resolution(
            &mut self.hold_runtime.pending_missed_hold_resolution,
            &mut self.hold_runtime.pending_missed_hold_indices,
            note_index,
        )
    }

    pub fn update_density_graph(
        &mut self,
        current_music_time: f32,
        trace_enabled: bool,
        phase_timings: &mut GameplayUpdatePhaseTimings,
    ) {
        let graph_w = self.display.density_graph.graph_w;
        let graph_h = self.display.density_graph.graph_h;
        let scaled_width = self.display.density_graph.scaled_width;
        self.display.density_graph.u0 = density_graph_u0_for_time(
            DensityGraphWindow {
                first_second: self.display.density_graph.first_second,
                last_second: self.display.density_graph.last_second,
                duration: self.display.density_graph.duration,
                graph_w,
                graph_h,
                scaled_width,
                u_window: self.display.density_graph.u_window,
            },
            current_music_time,
        );
        if graph_w <= 0.0_f32 || graph_h <= 0.0_f32 || scaled_width <= 0.0_f32 {
            return;
        }

        let next_t = self.display.density_graph.life_next_update_elapsed;
        let catch_up_steps = density_graph_life_catch_up_steps(
            self.boundary.total_elapsed_in_screen,
            next_t,
            self.display.density_graph.life_update_rate,
        );
        if catch_up_steps == 0 {
            return;
        }

        let sample_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let rate = self.display.density_graph.life_update_rate;
        self.display.density_graph.life_next_update_elapsed += rate * catch_up_steps as f32;

        if let Some(x) = density_graph_life_sample_x(
            current_music_time,
            self.display.density_graph.first_second,
            self.display.density_graph.last_second,
            self.display.density_graph.duration,
            self.display.density_graph.scaled_width,
        ) {
            for player in 0..self.setup.num_players {
                let life = player_life(&self.players_runtime.players[player]);
                let y = (1.0_f32 - life).clamp(0.0_f32, 1.0_f32) * graph_h;
                let points = &mut self.display.density_graph.life_points[player];
                if push_density_life_point(points, x, y) {
                    self.display.density_graph.life_dirty[player] = true;
                }
            }
        }
        if let Some(started) = sample_started {
            add_elapsed_us(&mut phase_timings.density_sample_us, started);
        }
    }

    #[inline(always)]
    pub fn display_clock_health(&self) -> DisplayClockHealth {
        self.clock.display_clock.health()
    }

    #[inline(always)]
    pub fn display_clock_stutter_diag_trigger_seq(&self) -> u64 {
        self.clock.display_clock.diag_trigger_seq()
    }

    #[inline(always)]
    pub fn collect_display_clock_stutter_diag_events(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<DisplayClockDiagEvent>,
    ) {
        self.clock
            .display_clock
            .collect_diag_events(now_host_nanos, window_ns, out);
    }

    #[inline(always)]
    pub const fn current_beat_display(&self) -> f32 {
        self.clock.song_position.current_beat_display
    }

    #[inline(always)]
    pub const fn current_music_time_display(&self) -> f32 {
        self.clock.song_position.current_music_time_display
    }

    #[inline(always)]
    pub fn timing_for_player(&self, player: usize) -> Option<&TimingData> {
        self.timing_runtime
            .timing_players
            .get(player)
            .map(Arc::as_ref)
    }

    #[inline(always)]
    pub fn timing(&self) -> &TimingData {
        &self.timing_runtime.timing
    }

    #[inline(always)]
    pub fn music_time_for_beat(&self, beat: f32) -> f32 {
        self.timing_runtime.timing.get_time_for_beat(beat)
    }

    #[inline(always)]
    pub fn beat_for_music_time(&self, music_time: f32) -> f32 {
        self.timing_runtime.timing.get_beat_for_time(music_time)
    }

    #[inline(always)]
    pub fn music_time_from_audio_snapshot(&self, audio_snapshot: GameplayAudioSnapshot) -> f32 {
        song_time_ns_to_seconds(
            current_song_clock_snapshot(
                audio_snapshot,
                self.music_rate(),
                self.clock.audio_clock.lead_in_seconds(),
                self.clock.offsets.global_offset_seconds(),
            )
            .song_time_ns,
        )
    }

    #[inline(always)]
    pub fn song(&self) -> &SongData {
        &self.source.song
    }

    #[inline(always)]
    pub fn song_arc(&self) -> Arc<SongData> {
        Arc::clone(&self.source.song)
    }

    #[inline(always)]
    pub fn notes(&self) -> &[Note] {
        &self.chart_runtime.notes
    }

    #[inline(always)]
    pub fn column_judgment_eligible(&self) -> &[bool] {
        &self.chart_runtime.column_judgment_eligible
    }

    #[inline(always)]
    pub fn players(&self) -> &[PlayerRuntime; MAX_PLAYERS] {
        &self.players_runtime.players
    }

    #[inline(always)]
    pub fn player(&self, player_idx: usize) -> Option<&PlayerRuntime> {
        self.players_runtime.players.get(player_idx)
    }

    #[inline(always)]
    pub fn profiles(&self) -> &[Profile; MAX_PLAYERS] {
        &self.profiles_runtime.profiles
    }

    #[inline(always)]
    pub fn profile(&self, player_idx: usize) -> Option<&Profile> {
        self.profiles_runtime.profiles.get(player_idx)
    }

    #[inline(always)]
    pub fn gameplay_chart(&self, player: usize) -> Option<&GameplayChartData> {
        self.source.gameplay_charts.get(player).map(Arc::as_ref)
    }

    #[inline(always)]
    pub fn gameplay_charts(&self) -> &[Arc<GameplayChartData>; MAX_PLAYERS] {
        &self.source.gameplay_charts
    }

    #[inline(always)]
    pub fn chart(&self, player: usize) -> Option<&ChartData> {
        self.source.charts.get(player).map(Arc::as_ref)
    }

    #[inline(always)]
    pub fn charts(&self) -> &[Arc<ChartData>; MAX_PLAYERS] {
        &self.source.charts
    }

    #[inline(always)]
    pub const fn num_players(&self) -> usize {
        self.setup.num_players
    }

    #[inline(always)]
    pub fn set_num_players(&mut self, num_players: usize) {
        self.setup.num_players = num_players;
    }

    #[inline(always)]
    pub const fn num_cols(&self) -> usize {
        self.setup.num_cols
    }

    #[inline(always)]
    pub fn set_num_cols(&mut self, num_cols: usize) {
        self.setup.num_cols = num_cols;
    }

    #[inline(always)]
    pub const fn cols_per_player(&self) -> usize {
        self.setup.cols_per_player
    }

    #[inline(always)]
    pub fn set_cols_per_player(&mut self, cols_per_player: usize) {
        self.setup.cols_per_player = cols_per_player;
    }

    #[inline(always)]
    pub fn set_song_banner_path(&mut self, banner_path: Option<PathBuf>) {
        Arc::make_mut(&mut self.source.song).banner_path = banner_path;
    }

    #[inline(always)]
    pub fn clear_notes(&mut self) {
        self.chart_runtime.notes.clear();
        self.chart_runtime.column_judgment_eligible.clear();
    }

    #[inline(always)]
    pub fn update_player(&mut self, player_idx: usize, update: impl FnOnce(&mut PlayerRuntime)) {
        if let Some(player) = self.players_runtime.players.get_mut(player_idx) {
            update(player);
        }
    }

    #[inline(always)]
    pub fn update_profile(&mut self, player_idx: usize, update: impl FnOnce(&mut Profile)) {
        if let Some(profile) = self.profiles_runtime.profiles.get_mut(player_idx) {
            update(profile);
        }
    }

    #[inline(always)]
    pub fn completed_row_visibility(&self, player: usize) -> CompletedRowVisibility<'_> {
        CompletedRowVisibility::new(
            &self.chart_runtime.row_entries,
            self.chart_runtime
                .row_indices
                .row_map_cache
                .get(player)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        )
    }

    #[inline(always)]
    pub const fn song_lua_visuals(
        &self,
    ) -> &SongLuaRuntimeVisuals<OverlayActor, CapturedActor, StateDelta> {
        &self.mods.song_lua_visuals
    }

    #[inline(always)]
    pub fn song_lua_player_transform(&self, player: usize) -> SongLuaPlayerTransform {
        self.mods
            .song_lua_player_transforms
            .get(player)
            .copied()
            .unwrap_or_default()
    }

    #[inline(always)]
    pub fn set_end_times(&mut self, notes_end_time_ns: SongTimeNs, music_end_time_ns: SongTimeNs) {
        self.clock
            .end_timing
            .set_note_and_music_end_times(notes_end_time_ns, music_end_time_ns);
    }

    pub fn set_music_rate_with_player_judgment_timing(
        &mut self,
        rate: f32,
        player_judgment_timing: [PlayerJudgmentTiming; MAX_PLAYERS],
    ) -> bool {
        if !self.clock.music_rate.set_rate(rate) {
            return false;
        }
        self.timing_runtime.player_judgment_timing = player_judgment_timing;
        let normalized = self.music_rate();
        let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
            &self.chart_runtime.notes,
            &self.chart_runtime.note_time_cache_ns,
            &self.chart_runtime.hold_end_time_cache_ns,
            normalized,
            self.clock.end_timing.audio_end_time_ns(),
        );
        self.clock
            .end_timing
            .set_note_and_music_end_times(notes_end_time_ns, music_end_time_ns);
        true
    }

    fn reset_time_to_beat_caches(&mut self) {
        let GameplayTimingRuntimeState {
            timing,
            timing_players,
            time_to_beat_caches,
            ..
        } = &mut self.timing_runtime;
        let player_refs = std::array::from_fn(|player| timing_players[player].as_ref());
        time_to_beat_caches.reset(timing, &player_refs);
    }

    pub fn refresh_timing_after_offset_change(&mut self) {
        let timing_players: [&_; MAX_PLAYERS] =
            std::array::from_fn(|player| self.timing_runtime.timing_players[player].as_ref());
        refresh_timing_caches_for_offset_change(
            &self.chart_runtime.notes,
            &timing_players,
            self.setup.num_players,
            self.setup.cols_per_player,
            &mut self.chart_runtime.note_time_cache_ns,
            &mut self.chart_runtime.hold_end_time_cache_ns,
            &mut self.chart_runtime.row_entries,
            &self.chart_runtime.mine_scan.mine_note_ix,
            &mut self.chart_runtime.mine_scan.mine_note_time_ns,
        );
        self.reset_time_to_beat_caches();

        let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
            &self.chart_runtime.notes,
            &self.chart_runtime.note_time_cache_ns,
            &self.chart_runtime.hold_end_time_cache_ns,
            self.music_rate(),
            self.clock.end_timing.audio_end_time_ns(),
        );
        self.clock
            .end_timing
            .set_note_and_music_end_times(notes_end_time_ns, music_end_time_ns);
    }

    #[inline(always)]
    pub fn apply_global_offset_delta(&mut self, delta: f32) -> bool {
        let Some(new_offset) =
            offset_delta_target_seconds(self.clock.offsets.global_offset_seconds(), delta)
        else {
            return false;
        };
        mutate_timing_arc(&mut self.timing_runtime.timing, |timing| {
            timing.set_global_offset_seconds(new_offset)
        });
        for (player_idx, timing) in self.timing_runtime.timing_players.iter_mut().enumerate() {
            let effective_offset = new_offset
                + self
                    .clock
                    .offsets
                    .player_global_offset_shift_seconds(player_idx);
            mutate_timing_arc(timing, |timing| {
                timing.set_global_offset_seconds(effective_offset)
            });
        }
        self.refresh_timing_after_offset_change();
        self.clock.offsets.set_global_offset_seconds(new_offset);
        true
    }

    #[inline(always)]
    pub fn apply_song_offset_delta(&mut self, delta: f32) -> bool {
        let Some(new_offset) =
            offset_delta_target_seconds(self.clock.offsets.song_offset_seconds(), delta)
        else {
            return false;
        };

        mutate_timing_arc(&mut self.timing_runtime.timing, |timing| {
            timing.shift_song_offset_seconds(delta)
        });
        for timing in &mut self.timing_runtime.timing_players {
            mutate_timing_arc(timing, |timing| timing.shift_song_offset_seconds(delta));
        }
        self.refresh_timing_after_offset_change();
        self.clock.offsets.set_song_offset_seconds(new_offset);
        true
    }

    #[inline(always)]
    pub fn apply_autosync_offset_correction(&mut self, note_off_by_ns: SongTimeNs) {
        let result = self.apply_autosync_offset_sample(note_off_by_ns);
        match result.correction {
            Some(AutosyncOffsetCorrection::Song(mean)) => {
                let _ = self.apply_song_offset_delta(mean);
            }
            Some(AutosyncOffsetCorrection::Machine(mean)) => {
                let _ = self.apply_global_offset_delta(mean);
            }
            None => {}
        }
    }

    #[inline(always)]
    pub fn set_global_offsets(
        &mut self,
        initial_global_offset_seconds: f32,
        global_offset_seconds: f32,
    ) {
        self.clock.offsets = GameplayOffsetState::new(
            initial_global_offset_seconds,
            [0.0; MAX_PLAYERS],
            self.clock.offsets.song_offset_seconds(),
        );
        self.clock
            .offsets
            .set_global_offset_seconds(global_offset_seconds);
    }

    pub fn set_song_position_for_benchmark(
        &mut self,
        current_beat: f32,
        current_music_time_ns: SongTimeNs,
        current_beat_display: f32,
        current_music_time_display: f32,
    ) {
        self.clock
            .song_position
            .set_music_position(current_beat, current_music_time_ns);
        self.clock
            .song_position
            .set_display_position(current_beat_display, current_music_time_display);
    }

    pub fn stream_segments_for_results(&self, player: usize) -> Vec<StreamSegment> {
        if player >= self.setup.num_players {
            return Vec::new();
        }
        let mini_indicator_segments = self.mini_indicator_stream_segments(player);
        if !mini_indicator_segments.is_empty() {
            return mini_indicator_segments.to_vec();
        }
        let constant_bpm = !self.timing_runtime.timing_players[player].has_bpm_changes();
        let (segments, _, _) = stream_segments_for_note_data(
            &self.source.gameplay_charts[player].notes,
            self.setup.cols_per_player,
            constant_bpm,
        );
        segments
    }

    #[inline(always)]
    pub fn begin_outro_attack_clear(&mut self) {
        begin_outro_attack_visual_clear(
            &mut self.mods.attacks.cleared_for_outro,
            self.setup.num_players,
            &self.mods.attacks.visual,
            &mut self.mods.attacks.outro_visual,
        );
    }

    pub fn refresh_player_attacks(
        &mut self,
        player: usize,
        now: f32,
        delta_time: f32,
        base: AttackBaseEffects,
    ) {
        if player >= self.setup.num_players || player >= MAX_PLAYERS {
            return;
        }

        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now,
                delta_time,
                attacks_cleared_for_outro: self.mods.attacks.cleared_for_outro,
                base_appearance: base.appearance,
                base_visual: base.visual,
                base_scroll: base.scroll,
                base_mini_percent: base.mini_percent,
                attack_windows: &self.mods.attacks.mask_windows[player],
                song_lua_ease_windows: &self.mods.attacks.song_lua_ease_windows[player],
            },
            ActiveAttackRefreshState {
                attack_current_appearance: self.mods.attacks.current_appearance[player],
                active_attack_visual: self.mods.attacks.visual[player],
                active_attack_visibility: self.mods.attacks.visibility[player],
                active_attack_scroll: self.mods.attacks.scroll[player],
                active_attack_mini_percent: self.mods.attacks.mini_percent[player],
                outro_attack_visual: self.mods.attacks.outro_visual[player],
            },
        );

        self.mods.attacks.target_appearance[player] = output.attack_target_appearance;
        self.mods.attacks.speed_appearance[player] = output.attack_speed_appearance;
        self.mods.attacks.current_appearance[player] = output.attack_current_appearance;
        self.mods.attacks.outro_visual[player] = output.outro_attack_visual;
        self.mods.attacks.clear_all[player] = output.active_attack_clear_all;
        self.mods.attacks.chart[player] = output.active_attack_chart;
        self.mods.attacks.accel[player] = output.active_attack_accel;
        self.mods.attacks.visual[player] = output.active_attack_visual;
        self.mods.attacks.appearance[player] = output.active_attack_appearance;
        self.mods.attacks.visibility[player] = output.active_attack_visibility;
        self.mods.attacks.scroll[player] = output.active_attack_scroll;
        self.mods.attacks.perspective[player] = output.active_attack_perspective;
        self.mods.attacks.scroll_speed[player] = output.active_attack_scroll_speed;
        self.mods.attacks.mini_percent[player] = output.active_attack_mini_percent;
        self.mods.song_lua_player_transforms[player] = output.player_transform.resolve();
    }

    #[inline(always)]
    pub fn player_attack_base_cleared(&self, player_idx: usize) -> bool {
        player_idx < self.setup.num_players && self.mods.attacks.clear_all[player_idx]
    }

    #[inline(always)]
    pub fn effective_accel_effects_for_player_with_mask(
        &self,
        player_idx: usize,
        profile_mask_bits: u8,
    ) -> AccelEffects {
        if player_idx >= self.setup.num_players {
            return AccelEffects::default();
        }
        effective_attack_accel_effects(
            self.player_attack_base_cleared(player_idx),
            profile_mask_bits,
            self.mods.attacks.accel[player_idx],
        )
    }

    #[inline(always)]
    pub fn effective_visual_effects_for_player_with_mask(
        &self,
        player_idx: usize,
        profile_mask_bits: u16,
    ) -> VisualEffects {
        if player_idx >= self.setup.num_players {
            return VisualEffects::default();
        }
        effective_attack_visual_effects(
            self.player_attack_base_cleared(player_idx),
            profile_mask_bits,
            self.mods.attacks.visual[player_idx],
        )
    }

    #[inline(always)]
    pub fn effective_appearance_effects_for_player(&self, player_idx: usize) -> AppearanceEffects {
        if player_idx >= self.setup.num_players {
            return AppearanceEffects::default();
        }
        self.mods.attacks.appearance[player_idx]
    }

    #[inline(always)]
    pub fn effective_visibility_effects_for_player(&self, player_idx: usize) -> VisibilityEffects {
        if player_idx >= self.setup.num_players {
            return VisibilityEffects::default();
        }
        effective_attack_visibility_effects(self.mods.attacks.visibility[player_idx])
    }

    #[inline(always)]
    pub fn active_chart_attack_effects_for_player(&self, player_idx: usize) -> ChartAttackEffects {
        if player_idx >= self.setup.num_players {
            return ChartAttackEffects::default();
        }
        self.mods.attacks.chart[player_idx]
    }

    #[inline(always)]
    pub fn effective_scroll_effects_for_player_with_base(
        &self,
        player_idx: usize,
        base_scroll: ScrollEffects,
    ) -> ScrollEffects {
        if player_idx >= self.setup.num_players {
            return ScrollEffects::default();
        }
        effective_attack_scroll_effects(
            self.player_attack_base_cleared(player_idx),
            base_scroll,
            self.mods.attacks.scroll[player_idx],
        )
    }

    #[inline(always)]
    pub fn effective_perspective_effects_for_player_with_base(
        &self,
        player_idx: usize,
        base_perspective: PerspectiveEffects,
    ) -> PerspectiveEffects {
        if player_idx >= self.setup.num_players {
            return PerspectiveEffects::default();
        }
        effective_attack_perspective_effects(
            self.player_attack_base_cleared(player_idx),
            base_perspective,
            self.mods.attacks.perspective[player_idx],
        )
    }

    #[inline(always)]
    pub fn effective_mini_percent_for_player_with_base(
        &self,
        player_idx: usize,
        base_mini_percent: f32,
    ) -> f32 {
        if player_idx >= self.setup.num_players {
            return 0.0;
        }
        effective_mini_percent(
            self.mods.attacks.mini_percent[player_idx],
            base_mini_percent,
            self.player_attack_base_cleared(player_idx),
        )
    }

    #[inline(always)]
    pub fn effective_scroll_speed_for_player_with_base(
        &self,
        player_idx: usize,
        base_scroll_speed: ScrollSpeedSetting,
    ) -> ScrollSpeedSetting {
        if player_idx >= self.setup.num_players {
            return ScrollSpeedSetting::default();
        }
        effective_attack_scroll_speed(
            self.player_attack_base_cleared(player_idx),
            self.mods.attacks.scroll_speed[player_idx],
            base_scroll_speed,
        )
    }

    #[inline(always)]
    pub fn effective_scroll_speed_for_player(&self, player_idx: usize) -> ScrollSpeedSetting {
        self.effective_scroll_speed_for_player_with_base(
            player_idx,
            self.scroll_speed_for_player(player_idx),
        )
    }

    #[inline(always)]
    pub fn mini_indicator_stream_segments(&self, player: usize) -> &[StreamSegment] {
        self.display.mini_indicator.stream_segments(player)
    }

    #[inline(always)]
    pub fn mini_indicator_total_stream_measures(&self, player: usize) -> f32 {
        self.display.mini_indicator.total_stream_measures(player)
    }

    #[inline(always)]
    pub fn mini_indicator_target_score_percent(&self, player: usize) -> f64 {
        self.display.mini_indicator.target_score_percent(player)
    }

    #[inline(always)]
    pub fn mini_indicator_rival_score_percent(&self, player: usize) -> f64 {
        self.display.mini_indicator.rival_score_percent(player)
    }

    #[inline(always)]
    pub fn clear_mini_indicator_stream_segments(&mut self) {
        self.display.mini_indicator.clear_stream_segments();
    }

    #[inline(always)]
    pub fn note_count_stats(&self, player: usize) -> &[NoteCountStat] {
        self.chart_runtime.note_count_stats.player_stats(player)
    }

    #[inline(always)]
    pub fn lane_hold_indices(&self, col: usize) -> &[usize] {
        self.chart_runtime.lane_indices.hold_indices(col)
    }

    #[inline(always)]
    pub fn active_hold(&self, col: usize) -> Option<&ActiveHold> {
        self.hold_runtime
            .active_holds
            .get(col)
            .and_then(Option::as_ref)
    }

    #[inline(always)]
    pub fn active_hold_note_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.hold_runtime
            .active_holds
            .iter()
            .filter_map(|active| active.as_ref().map(|hold| hold.note_index))
    }

    #[inline(always)]
    pub const fn active_color_index(&self) -> i32 {
        self.display.active_color_index
    }

    #[inline(always)]
    pub const fn player_color_index(&self) -> i32 {
        self.display.player_color_index
    }

    #[inline(always)]
    pub fn set_color_indices(&mut self, active_color_index: i32, player_color_index: i32) {
        self.display.active_color_index = active_color_index;
        self.display.player_color_index = player_color_index;
    }

    #[inline(always)]
    pub fn lane_note_row_indices(&self, col: usize) -> &[usize] {
        self.chart_runtime.lane_indices.note_row_indices(col)
    }

    #[inline(always)]
    pub fn tap_row_hold_roll_flags(&self, note_index: usize) -> u8 {
        self.chart_runtime
            .lane_indices
            .tap_row_hold_roll_flags(note_index)
    }

    #[inline(always)]
    pub fn clear_lane_indices(&mut self) {
        self.chart_runtime.lane_indices.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn clear_row_indices(&mut self) {
        self.chart_runtime.row_indices.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn clear_row_entries(&mut self) {
        self.chart_runtime.row_entries.clear();
    }

    #[inline(always)]
    pub fn clear_mine_scan(&mut self) {
        self.chart_runtime.mine_scan.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn clear_note_ranges(&mut self) {
        self.chart_runtime.note_ranges.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn set_note_range(&mut self, player: usize, range: (usize, usize)) {
        self.chart_runtime
            .note_ranges
            .set_range_for_benchmark(player, range);
    }

    #[inline(always)]
    pub fn set_next_tap_miss_cursor(&mut self, player: usize, cursor: usize) {
        self.chart_runtime
            .mine_scan
            .set_next_tap_miss_cursor(player, cursor);
    }

    #[inline(always)]
    pub fn decaying_hold_indices(&self) -> &[usize] {
        &self.hold_runtime.decaying_hold_indices
    }

    #[inline(always)]
    pub fn clear_hold_runtime(&mut self) {
        self.hold_runtime.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn clear_active_holds(&mut self) {
        self.hold_runtime.active_holds.fill(None);
    }

    #[inline(always)]
    pub fn set_active_hold(&mut self, col: usize, active_hold: Option<ActiveHold>) {
        if let Some(slot) = self.hold_runtime.active_holds.get_mut(col) {
            *slot = active_hold;
        }
    }

    #[inline(always)]
    pub fn measure_counter_segments(&self, player: usize) -> &[StreamSegment] {
        self.display.cue_runtime.measure_counter_segments(player)
    }

    #[inline(always)]
    pub fn column_cues(&self, player: usize) -> &[ColumnCue] {
        self.display.cue_runtime.column_cues(player)
    }

    #[inline(always)]
    pub fn crossover_cues(&self, player: usize) -> &[ColumnCue] {
        self.display.cue_runtime.crossover_cues(player)
    }

    #[inline(always)]
    pub fn crossover_cue_entries(&self, player: usize) -> &[Option<f32>] {
        self.display.cue_runtime.crossover_cue_entries(player)
    }

    #[inline(always)]
    pub fn set_column_cues(&mut self, player: usize, cues: Vec<ColumnCue>) {
        self.display
            .cue_runtime
            .set_column_cues_for_benchmark(player, cues);
    }

    #[inline(always)]
    pub fn clear_cue_runtime(&mut self) {
        self.display.cue_runtime.clear_for_benchmark();
    }

    #[inline(always)]
    pub fn hold_judgment(&self, col: usize) -> Option<HoldJudgmentRenderInfo> {
        self.display.hold_feedback.hold_judgment(col)
    }

    #[inline(always)]
    pub fn hold_judgments_for_columns(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<HoldJudgmentRenderInfo>] {
        self.display
            .hold_feedback
            .hold_judgments(col_start, num_cols)
    }

    #[inline(always)]
    pub fn held_miss_judgments_for_columns(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<HeldMissRenderInfo>] {
        self.display
            .hold_feedback
            .held_miss_judgments(col_start, num_cols)
    }

    #[inline(always)]
    pub fn tap_explosions_for_columns(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveTapExplosion>] {
        self.display
            .visual_feedback
            .tap_explosions(col_start, num_cols)
    }

    #[inline(always)]
    pub fn column_flashes_for_columns(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveColumnFlash>] {
        self.display
            .visual_feedback
            .column_flashes(col_start, num_cols)
    }

    #[inline(always)]
    pub fn mine_explosions_for_columns(
        &self,
        col_start: usize,
        num_cols: usize,
    ) -> &[Option<ActiveMineExplosion>] {
        self.display
            .visual_feedback
            .mine_explosions(col_start, num_cols)
    }

    #[inline(always)]
    pub fn last_tap_judgment(&self, col: usize) -> Option<ColumnTapJudgment> {
        self.display.visual_feedback.last_tap_judgment(col)
    }

    #[inline(always)]
    pub fn mine_started_at_screen_s(&self, col: usize) -> Option<f32> {
        self.display.visual_feedback.mine_started_at_screen_s(col)
    }

    #[inline(always)]
    pub fn clear_visual_feedback(&mut self) {
        self.display.visual_feedback.clear();
    }

    #[inline(always)]
    pub fn set_tap_explosion(&mut self, col: usize, explosion: Option<ActiveTapExplosion>) {
        self.display
            .visual_feedback
            .set_tap_explosion_for_benchmark(col, explosion);
    }

    pub fn course_display_carry(&self) -> [CourseDisplayCarry; MAX_PLAYERS] {
        let stages = std::array::from_fn(|player| {
            if player >= self.setup.num_players.min(MAX_PLAYERS) {
                return Default::default();
            }
            player_course_display_stage(
                &self.players_runtime.players[player],
                self.progress.window_counts.canonical(player),
                self.progress.window_counts.ten_ms_blue(player),
                self.progress.window_counts.display_blue(player),
            )
        });
        course_display_carry_for_stages(
            self.progress.course_display.carry(),
            stages,
            self.setup.num_players,
        )
    }

    #[inline(always)]
    pub fn display_carry_for_player(&self, player_idx: usize) -> CourseDisplayCarry {
        self.progress.course_display.carry_for_player(player_idx)
    }

    #[inline(always)]
    pub fn note_range_for_player(&self, player_idx: usize) -> (usize, usize) {
        player_note_range_for_ranges(
            self.chart_runtime.note_ranges.ranges(),
            self.setup.num_players,
            player_idx,
        )
    }

    #[inline(always)]
    pub fn live_window_counts(&self, player_idx: usize) -> WindowCounts {
        self.progress.window_counts.canonical(player_idx)
    }

    #[inline(always)]
    pub fn set_live_window_counts(
        &mut self,
        player_idx: usize,
        canonical: WindowCounts,
        ten_ms_blue: WindowCounts,
        display_blue: WindowCounts,
    ) {
        self.progress.window_counts.set_player_for_benchmark(
            player_idx,
            canonical,
            ten_ms_blue,
            display_blue,
        );
    }

    #[inline(always)]
    pub fn display_totals_for_player(&self, player_idx: usize) -> CourseDisplayTotals {
        self.progress
            .chart_totals
            .display_totals(self.progress.course_display.totals(), player_idx)
    }

    #[inline(always)]
    pub fn course_display_is_course_stage(&self) -> bool {
        self.progress.course_display.is_course_stage()
    }

    #[inline(always)]
    pub fn course_display_timing(&self) -> Option<CourseDisplayTiming> {
        self.progress.course_display.timing()
    }

    #[inline(always)]
    pub fn field_zoom_for_player(&self, player_idx: usize) -> f32 {
        self.display.notefield_motion.field_zoom(player_idx)
    }

    #[inline(always)]
    pub fn notefield_draw_distance_before_targets(&self, player_idx: usize) -> f32 {
        self.display
            .notefield_motion
            .draw_distance_before_targets(player_idx)
    }

    #[inline(always)]
    pub fn notefield_draw_distance_after_targets(&self, player_idx: usize) -> f32 {
        self.display
            .notefield_motion
            .draw_distance_after_targets(player_idx)
    }

    #[inline(always)]
    pub fn notefield_column_scroll_dir(&self, col: usize) -> f32 {
        self.display.notefield_motion.column_scroll_dir(col)
    }

    #[inline(always)]
    pub fn notefield_column_scroll_dir_count(&self) -> usize {
        self.display.notefield_motion.column_scroll_dir_count()
    }

    #[inline(always)]
    pub fn notefield_reverse_scroll(&self, player_idx: usize) -> bool {
        self.display.notefield_motion.reverse_scroll(player_idx)
    }

    #[inline(always)]
    pub fn scroll_speed_for_player(&self, player_idx: usize) -> ScrollSpeedSetting {
        self.display.notefield_motion.scroll_speed(player_idx)
    }

    #[inline(always)]
    pub fn scroll_reference_bpm(&self) -> f32 {
        self.display.notefield_motion.scroll_reference_bpm()
    }

    #[inline(always)]
    pub fn notes_end_time_ns(&self) -> SongTimeNs {
        self.clock.end_timing.notes_end_time_ns()
    }

    #[inline(always)]
    pub fn music_end_time_ns(&self) -> SongTimeNs {
        self.clock.end_timing.music_end_time_ns()
    }

    #[inline(always)]
    pub fn is_in_freeze(&self) -> bool {
        self.display.beat_phase.is_in_freeze()
    }

    #[inline(always)]
    pub fn is_in_delay(&self) -> bool {
        self.display.beat_phase.is_in_delay()
    }

    #[inline(always)]
    pub fn beat_phase_paused(&self) -> bool {
        self.display.beat_phase.paused()
    }

    #[inline(always)]
    pub fn hands_total_for_player(&self, player_idx: usize) -> u32 {
        self.progress
            .chart_totals
            .hands_total
            .get(player_idx)
            .copied()
            .unwrap_or(0)
    }

    pub fn display_judgment_count(&self, player_idx: usize, grade: JudgeGrade) -> u32 {
        if player_idx >= self.setup.num_players {
            return 0;
        }
        player_display_judgment_count(
            &self.players_runtime.players[player_idx],
            self.display_carry_for_player(player_idx),
            grade,
        )
    }

    pub fn display_live_timing_stats(
        &self,
        player_idx: usize,
    ) -> deadsync_rules::timing::LiveTimingSnapshot {
        if player_idx >= self.setup.num_players {
            return deadsync_rules::timing::LiveTimingSnapshot::default();
        }
        player_live_timing_snapshot(&self.players_runtime.players[player_idx])
    }

    pub fn display_window_counts_10ms(&self, player_idx: usize) -> WindowCounts {
        if player_idx >= self.setup.num_players {
            return WindowCounts::default();
        }
        let current = self.progress.window_counts.ten_ms_blue(player_idx);
        display_window_counts_with_carry(
            current,
            self.display_carry_for_player(player_idx),
            DisplayWindowCountsMode::TenMsBlue,
        )
    }

    #[inline(always)]
    pub fn display_score_stage(&self, player_idx: usize) -> ItgScoreStage {
        player_score_stage(&self.players_runtime.players[player_idx])
    }

    pub fn display_window_counts(
        &self,
        player_idx: usize,
        blue_window_ms: Option<f32>,
        player_blue_window_ms: f32,
    ) -> WindowCounts {
        if player_idx >= self.setup.num_players {
            return WindowCounts::default();
        }
        let sources = self.progress.window_counts.sources(player_idx);
        let (start, end) = self.note_range_for_player(player_idx);
        let end = end.min(self.chart_runtime.notes.len());
        let notes = if start < end {
            &self.chart_runtime.notes[start..end]
        } else {
            &[]
        };
        display_window_counts_for_notes(
            sources,
            self.display_carry_for_player(player_idx),
            notes,
            blue_window_ms,
            player_blue_window_ms,
        )
    }

    pub fn live_ex_score_inputs(
        &self,
        player_idx: usize,
        player_blue_window_ms: f32,
    ) -> ExScoreInputs {
        ex_score_inputs_from_display(
            self.display_window_counts(player_idx, None, player_blue_window_ms),
            self.display_window_counts_10ms(player_idx),
            self.display_score_stage(player_idx),
        )
    }

    #[inline(always)]
    pub fn ex_score_data_from_inputs(
        &self,
        player_idx: usize,
        inputs: ExScoreInputs,
    ) -> judgment::ExScoreData {
        ex_score_data_from_display_inputs(
            inputs,
            self.display_carry_for_player(player_idx),
            self.display_totals_for_player(player_idx),
        )
    }

    pub fn display_itg_score_inputs(&self, player_idx: usize) -> Option<ItgScoreInputs> {
        if player_idx >= self.setup.num_players {
            return None;
        }
        Some(itg_score_inputs_from_display(
            self.display_score_stage(player_idx),
            self.display_carry_for_player(player_idx),
            self.display_totals_for_player(player_idx),
        ))
    }

    pub fn display_itg_score_percent(&self, player_idx: usize) -> f64 {
        self.display_itg_score_inputs(player_idx)
            .map_or(0.0, itg_score_percent_from_inputs)
    }

    pub fn display_predictive_itg_score_percent(&self, player_idx: usize) -> f64 {
        self.display_itg_score_inputs(player_idx)
            .map_or(0.0, predictive_itg_score_percent_from_inputs)
    }

    pub fn display_gameplay_itg_score_percent(
        &self,
        player_idx: usize,
        mode: GameplayScoreDisplayMode,
    ) -> f64 {
        self.display_itg_score_inputs(player_idx)
            .map_or(0.0, |inputs| {
                display_itg_score_percent_for_mode(inputs, mode)
            })
    }

    pub fn capture_failed_ex_score_inputs(
        &mut self,
        player_idx: usize,
        player_blue_window_ms: f32,
    ) {
        if player_idx >= self.setup.num_players || player_idx >= MAX_PLAYERS {
            return;
        }
        let live = self.live_ex_score_inputs(player_idx, player_blue_window_ms);
        capture_player_failed_ex_score_inputs(&mut self.players_runtime.players[player_idx], live);
    }

    pub fn display_ex_score_data(
        &self,
        player_idx: usize,
        player_blue_window_ms: f32,
    ) -> judgment::ExScoreData {
        if player_idx >= self.setup.num_players {
            return judgment::ExScoreData::default();
        }
        self.ex_score_data_from_inputs(
            player_idx,
            self.live_ex_score_inputs(player_idx, player_blue_window_ms),
        )
    }

    pub fn display_scored_ex_score_data(
        &self,
        player_idx: usize,
        player_blue_window_ms: f32,
    ) -> judgment::ExScoreData {
        if player_idx >= self.setup.num_players {
            return judgment::ExScoreData::default();
        }
        let live = self.live_ex_score_inputs(player_idx, player_blue_window_ms);
        let inputs =
            player_effective_ex_score_inputs(&self.players_runtime.players[player_idx], live);
        self.ex_score_data_from_inputs(player_idx, inputs)
    }

    pub fn display_ex_score_percent(&self, player_idx: usize, player_blue_window_ms: f32) -> f64 {
        judgment::ex_score_percent(
            &self.display_scored_ex_score_data(player_idx, player_blue_window_ms),
        )
    }

    pub fn display_gameplay_ex_score_percent(
        &self,
        player_idx: usize,
        mode: GameplayScoreDisplayMode,
        player_blue_window_ms: f32,
    ) -> f64 {
        let score = self.display_scored_ex_score_data(player_idx, player_blue_window_ms);
        display_ex_score_percent_for_mode(&score, mode)
    }

    pub fn display_hard_ex_score_percent(
        &self,
        player_idx: usize,
        player_blue_window_ms: f32,
    ) -> f64 {
        judgment::hard_ex_score_percent(
            &self.display_scored_ex_score_data(player_idx, player_blue_window_ms),
        )
    }

    pub fn display_gameplay_hard_ex_score_percent(
        &self,
        player_idx: usize,
        mode: GameplayScoreDisplayMode,
        player_blue_window_ms: f32,
    ) -> f64 {
        let score = self.display_scored_ex_score_data(player_idx, player_blue_window_ms);
        display_hard_ex_score_percent_for_mode(&score, mode)
    }

    #[inline(always)]
    pub fn player_is_dead(&self, player: usize) -> bool {
        player_runtime_is_dead(&self.players_runtime.players[player])
    }

    #[inline(always)]
    pub fn all_joined_players_failed(&self) -> bool {
        all_joined_player_runtimes_failed(&self.players_runtime.players, self.setup.num_players)
    }

    #[inline(always)]
    pub fn course_stage_life_submit_eligible(&self, player_idx: usize) -> bool {
        if player_idx >= self.setup.num_players.min(MAX_PLAYERS) {
            return true;
        }
        player_course_submit_life_eligible(&self.players_runtime.players[player_idx])
    }

    #[inline(always)]
    pub fn disable_score_for_practice(&mut self) {
        self.progress.stage.disable_score();
        self.progress.replay.disable_replay_mode();
    }

    pub fn reset_practice_playback(&mut self, judge_start_music_time: f32) {
        let judge_start_ns = song_time_ns_from_seconds(judge_start_music_time);
        reset_practice_notes_and_rows(
            &mut self.chart_runtime.notes,
            &mut self.chart_runtime.row_entries,
            &self.chart_runtime.note_time_cache_ns,
        );
        self.chart_runtime.column_judgment_eligible.fill(false);
        self.disable_score_for_practice();

        self.progress.stage.reset_for_practice();
        self.display.hold_feedback.clear();
        self.display.visual_feedback.clear();
        self.hold_runtime.active_holds.fill(None);
        self.display.receptor_feedback.reset_for_practice();
        self.hold_runtime.reset_live_state();
        self.reset_live_input_state();
        self.clear_pending_input_edges();
        self.progress.replay.reset_for_restart();
        self.chart_runtime
            .mine_scan
            .pending_mine_hit_indices
            .clear();
        self.control.exit_input.reset();
        self.progress.window_counts.reset();

        let mine_note_time_ns = std::array::from_fn(|player| {
            self.chart_runtime.mine_scan.mine_note_time_ns[player].as_slice()
        });
        let mine_note_ix = std::array::from_fn(|player| {
            self.chart_runtime.mine_scan.mine_note_ix[player].as_slice()
        });
        let cursors = practice_cursors_for_players(
            &self.chart_runtime.note_time_cache_ns,
            self.chart_runtime.note_ranges.ranges(),
            &self.chart_runtime.row_entries,
            &self.chart_runtime.row_indices.row_entry_ranges,
            mine_note_time_ns,
            mine_note_ix,
            self.setup.num_players,
            judge_start_ns,
        );
        for player in 0..self.setup.num_players {
            self.players_runtime.players[player] =
                init_player_runtime_for_practice(judge_start_music_time);

            self.chart_runtime.mine_scan.next_tap_miss_cursor[player] = cursors.note_cursor[player];
            self.control
                .autoplay_runtime
                .set_cursor(player, cursors.note_cursor[player]);
            self.chart_runtime.row_indices.judged_row_cursor[player] = cursors.row_cursor[player];
            self.chart_runtime.mine_scan.next_mine_ix_cursor[player] =
                cursors.mine_ix_cursor[player];
            self.chart_runtime.mine_scan.next_mine_avoid_cursor[player] =
                cursors.mine_avoid_cursor[player];
        }

        let song_row = self.assist_row_no_offset(judge_start_music_time);
        self.control.assist_clap.reset_for_row(song_row);
        self.boundary.total_elapsed_in_screen = 0.0;
    }

    #[inline(always)]
    pub fn runtime_player_side(&self, player: usize) -> GameplayInputPlayerSide {
        self.setup.session.runtime_player_side(player)
    }

    #[inline(always)]
    pub fn default_fa_plus_window_s(&self) -> f32 {
        self.timing_runtime
            .timing_profile
            .fa_plus_window_s
            .unwrap_or(self.timing_runtime.timing_profile.windows_s[0])
    }

    #[inline(always)]
    pub fn timing_profile_windows_s(&self) -> [f32; 5] {
        self.timing_runtime.timing_profile.windows_s
    }

    #[inline(always)]
    pub fn player_largest_tap_window_ns(&self, player_idx: usize) -> SongTimeNs {
        if player_idx >= self.setup.num_players {
            return 0;
        }
        self.timing_runtime.player_judgment_timing[player_idx].largest_tap_window_music_ns
    }

    #[inline(always)]
    pub fn note_hit_eval(
        &self,
        player_idx: usize,
        note_time_ns: SongTimeNs,
        current_time_ns: SongTimeNs,
    ) -> Option<NoteHitEval> {
        if player_idx >= self.setup.num_players {
            return None;
        }
        note_hit_eval_for_timing(
            self.timing_runtime.player_judgment_timing[player_idx],
            note_time_ns,
            current_time_ns,
        )
    }

    #[inline(always)]
    pub fn effective_player_global_offset_seconds(&self, player_idx: usize) -> f32 {
        self.clock
            .offsets
            .effective_player_global_offset_seconds(player_idx)
    }

    #[inline(always)]
    pub fn assist_row_no_offset_ns(&mut self, music_time_ns: SongTimeNs) -> i32 {
        self.timing_runtime.time_to_beat_caches.assist_row_no_offset(
            &self.timing_runtime.timing,
            self.clock.offsets.global_offset_seconds(),
            music_time_ns,
        )
    }

    #[inline(always)]
    pub fn assist_row_no_offset(&mut self, music_time: f32) -> i32 {
        self.assist_row_no_offset_ns(song_time_ns_from_seconds(music_time))
    }

    #[inline(always)]
    pub fn global_offset_seconds(&self) -> f32 {
        self.clock.offsets.global_offset_seconds()
    }

    #[inline(always)]
    pub fn initial_global_offset_seconds(&self) -> f32 {
        self.clock.offsets.initial_global_offset_seconds()
    }

    #[inline(always)]
    pub fn song_offset_seconds(&self) -> f32 {
        self.clock.offsets.song_offset_seconds()
    }

    #[inline(always)]
    pub fn initial_song_offset_seconds(&self) -> f32 {
        self.clock.offsets.initial_song_offset_seconds()
    }

    #[inline(always)]
    pub fn set_replay_capture_enabled(&mut self, enabled: bool) {
        self.progress.replay.capture_enabled = enabled;
    }

    #[inline(always)]
    pub fn replay_capture_enabled(&self) -> bool {
        self.progress.replay.capture_enabled
    }

    #[inline(always)]
    pub fn recorded_replay_edges(&self) -> &[RecordedLaneEdge] {
        &self.progress.replay.edges
    }

    #[inline(always)]
    pub fn clear_recorded_replay_edges(&mut self) {
        self.progress.replay.edges.clear();
    }

    #[inline(always)]
    pub fn autoplay_enabled(&self) -> bool {
        self.progress.stage.autoplay_enabled
    }

    #[inline(always)]
    pub fn autoplay_used(&self) -> bool {
        self.progress.stage.autoplay_used
    }

    #[inline(always)]
    pub fn score_valid_for_player(&self, player: usize) -> bool {
        self.progress
            .stage
            .score_valid
            .get(player)
            .copied()
            .unwrap_or(false)
    }

    #[inline(always)]
    pub fn song_completed_naturally(&self) -> bool {
        self.progress.stage.song_completed_naturally
    }

    #[inline(always)]
    pub fn reset_stage_runtime_for_benchmark(&mut self) {
        self.progress.stage.autoplay_enabled = false;
        self.progress.stage.song_completed_naturally = false;
    }

    #[inline(always)]
    pub fn reset_exit_input(&mut self) {
        self.control.exit_input.reset();
    }

    #[inline(always)]
    pub fn set_autoplay_enabled_for_benchmark(&mut self, enabled: bool) {
        self.progress.stage.autoplay_enabled = enabled;
    }

    #[inline(always)]
    pub fn autosync_mode(&self) -> AutosyncMode {
        self.control.autosync.mode
    }

    #[inline(always)]
    pub fn autosync_standard_deviation(&self) -> f32 {
        self.control.autosync.standard_deviation
    }

    #[inline(always)]
    pub fn autosync_sample_count(&self) -> usize {
        self.control.autosync.offset_sample_count
    }

    #[inline(always)]
    pub fn set_autosync_state_for_benchmark(
        &mut self,
        mode: AutosyncMode,
        standard_deviation: f32,
        sample_count: usize,
    ) {
        self.control.autosync.mode = mode;
        self.control.autosync.standard_deviation = standard_deviation;
        self.control.autosync.offset_sample_count = sample_count.min(AUTOSYNC_OFFSET_SAMPLE_COUNT);
    }

    #[inline(always)]
    pub fn visible_music_time_ns(&self, player: usize) -> SongTimeNs {
        self.clock
            .visible_timing
            .current_music_time_ns
            .get(player)
            .copied()
            .unwrap_or(INVALID_SONG_TIME_NS)
    }

    #[inline(always)]
    pub fn visible_music_time_seconds(&self, player: usize) -> f32 {
        self.clock
            .visible_timing
            .current_music_time
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn visible_beat(&self, player: usize) -> f32 {
        self.clock
            .visible_timing
            .current_beat
            .get(player)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn set_visible_time(
        &mut self,
        player: usize,
        music_time_ns: SongTimeNs,
        music_time_seconds: f32,
        beat: f32,
    ) {
        self.clock
            .visible_timing
            .set_player_time(player, music_time_ns, music_time_seconds, beat);
    }

    #[inline(always)]
    pub fn fill_visible_time_for_benchmark(&mut self, music_time_seconds: f32) {
        let music_time_ns = song_time_ns_from_seconds(music_time_seconds);
        for player in 0..MAX_PLAYERS {
            self.set_visible_time(player, music_time_ns, music_time_seconds, 0.0);
        }
    }

    pub fn set_density_graph_top_for_benchmark(
        &mut self,
        first_second: f32,
        last_second: f32,
        player: usize,
        top_w: f32,
        top_h: f32,
        top_scale_y: f32,
    ) {
        self.display.density_graph.first_second = first_second;
        self.display.density_graph.last_second = last_second;
        self.display.density_graph.duration = (last_second - first_second).max(0.001);
        if player < MAX_PLAYERS {
            self.display.density_graph.top_h = top_h;
            self.display.density_graph.top_w[player] = top_w;
            self.display.density_graph.top_scale_y[player] = top_scale_y;
        }
    }

    #[inline(always)]
    pub fn note_time_cache_ns(&self) -> &[SongTimeNs] {
        &self.chart_runtime.note_time_cache_ns
    }

    #[inline(always)]
    pub fn hold_end_time_cache_ns(&self) -> &[Option<SongTimeNs>] {
        &self.chart_runtime.hold_end_time_cache_ns
    }

    #[inline(always)]
    pub fn note_time_cache_ns_at(&self, index: usize) -> Option<SongTimeNs> {
        self.chart_runtime.note_time_cache_ns.get(index).copied()
    }

    #[inline(always)]
    pub fn hold_end_time_cache_ns_at(&self, index: usize) -> Option<Option<SongTimeNs>> {
        self.chart_runtime
            .hold_end_time_cache_ns
            .get(index)
            .copied()
    }

    #[inline(always)]
    pub fn clear_note_timing_caches(&mut self) {
        self.chart_runtime.note_time_cache_ns.clear();
        self.chart_runtime.hold_end_time_cache_ns.clear();
    }

    #[inline(always)]
    pub fn lane_is_pressed(&self, col: usize) -> bool {
        self.control.input_state.lane_is_pressed(col)
    }

    #[inline(always)]
    pub fn sync_active_hold_pressed_state(&mut self, column: usize, lane_pressed: bool) {
        let live_autoplay = self.live_autoplay_enabled();
        sync_active_hold_pressed_column(
            &mut self.hold_runtime.active_holds,
            column,
            live_autoplay,
            lane_pressed,
        );
    }

    #[inline(always)]
    pub fn set_receptor_glow_timers(&mut self, col: usize, timers: GameplayReceptorGlowTimers) {
        self.display.receptor_feedback.set_glow_timers(col, timers);
    }

    #[inline(always)]
    pub fn trigger_receptor_glow_pulse(&mut self, col: usize) {
        let behavior = self.receptor_glow_behavior_for_col(col);
        self.set_receptor_glow_timers(col, receptor_glow_pulse_timers(behavior));
    }

    #[inline(always)]
    pub fn start_receptor_glow_press(&mut self, col: usize) {
        let behavior = self.receptor_glow_behavior_for_col(col);
        self.set_receptor_glow_timers(col, receptor_glow_press_timers(behavior));
    }

    #[inline(always)]
    pub fn release_receptor_glow(&mut self, col: usize) {
        let behavior = self.receptor_glow_behavior_for_col(col);
        let timers = receptor_glow_release_timers(behavior, self.receptor_glow_press_timer(col));
        self.set_receptor_glow_timers(col, timers);
    }

    #[inline(always)]
    pub fn trigger_receptor_step_pulse(&mut self, col: usize) {
        self.trigger_receptor_step_command(col, None);
    }

    #[inline(always)]
    pub fn trigger_receptor_score_pulse(&mut self, col: usize, window: &'static str) {
        self.trigger_receptor_step_command(col, Some(window));
    }

    #[inline(always)]
    fn trigger_receptor_step_command(&mut self, col: usize, window: Option<&str>) {
        if col >= self.setup.num_cols {
            return;
        }
        self.start_receptor_glow_press(col);
        let behavior = self.receptor_step_behavior_for_col(col, window);
        self.start_receptor_bop(col, behavior);
    }

    #[inline(always)]
    pub fn receptor_glow_press_timer(&self, col: usize) -> f32 {
        self.display
            .receptor_feedback
            .glow_press_timers
            .get(col)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn start_receptor_bop(&mut self, col: usize, behavior: GameplayReceptorStepBehavior) {
        self.display.receptor_feedback.start_bop(col, behavior);
    }

    #[inline(always)]
    pub fn receptor_glow_visual(
        &self,
        col: usize,
        behavior: GameplayReceptorGlowBehavior,
    ) -> Option<(f32, f32)> {
        if col >= self.setup.num_cols {
            return None;
        }
        receptor_glow_visual(
            behavior,
            self.display
                .receptor_feedback
                .receptor_glow_state(col, self.lane_is_pressed(col)),
        )
    }

    #[inline(always)]
    pub fn receptor_glow_visual_for_col(&self, col: usize) -> Option<(f32, f32)> {
        let behavior = self
            .display
            .noteskin_effects
            .receptor_glow_behavior_for_player(self.player_for_col(col));
        self.receptor_glow_visual(col, behavior)
    }

    #[inline(always)]
    pub fn receptor_glow_behavior_for_col(&self, col: usize) -> GameplayReceptorGlowBehavior {
        self.display
            .noteskin_effects
            .receptor_glow_behavior_for_player(self.player_for_col(col))
    }

    #[inline(always)]
    pub fn receptor_step_behavior_for_col(
        &self,
        col: usize,
        window: Option<&str>,
    ) -> GameplayReceptorStepBehavior {
        let player =
            player_index_for_column(self.setup.num_players, self.setup.cols_per_player, col);
        let local_col = local_column_for_field(self.setup.cols_per_player, col);
        self.display
            .noteskin_effects
            .receptor_step_behavior_for_col(player, local_col, window)
    }

    #[inline(always)]
    pub fn receptor_bop_zoom(&self, col: usize) -> f32 {
        self.display.receptor_feedback.bop_zoom(col)
    }

    #[inline(always)]
    pub fn set_receptor_bop_timer_for_benchmark(&mut self, col: usize, timer: f32) {
        self.display
            .receptor_feedback
            .set_bop_timer_for_benchmark(col, timer);
    }

    #[inline(always)]
    pub fn pending_input_is_empty(&self) -> bool {
        self.pending_input.edges.is_empty()
    }

    #[inline(always)]
    pub fn pending_input_len(&self) -> usize {
        self.pending_input.edges.len()
    }

    #[inline(always)]
    pub fn live_input_lane_for_queue(&self, lane: Lane) -> Option<Lane> {
        live_input_lane_for_queue(
            self.progress.stage.autoplay_enabled,
            self.setup.session.play_style,
            self.setup.session.player_side,
            lane,
            self.setup.num_cols,
        )
    }

    #[inline(always)]
    pub fn queue_live_input_edge(
        &mut self,
        lane: Lane,
        build_edge: impl FnOnce(Lane, Instant, SongTimeNs, bool) -> GameplayInputEdge,
    ) -> bool {
        let Some(lane) = self.live_input_lane_for_queue(lane) else {
            return false;
        };
        let queued_at = Instant::now();
        let edge = build_edge(
            lane,
            queued_at,
            INVALID_SONG_TIME_NS,
            self.replay_capture_enabled(),
        );
        self.queue_pending_input_edge(lane.index(), edge)
    }

    #[inline(always)]
    pub fn queue_current_time_input_edge(
        &mut self,
        lane: Lane,
        build_edge: impl FnOnce(Instant) -> GameplayInputEdge,
    ) -> bool {
        let now = Instant::now();
        self.queue_pending_input_edge(lane.index(), build_edge(now))
    }

    #[inline(always)]
    pub fn queue_pending_input_edge(&mut self, lane_idx: usize, edge: GameplayInputEdge) -> bool {
        if lane_idx >= self.num_cols() {
            return false;
        }
        self.push_pending_input_edge(edge);
        if log::log_enabled!(log::Level::Debug) {
            let pending_len = self.pending_input_len();
            if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
                log::debug!(
                    "Gameplay input queue pressure: pending_edges={}, num_cols={}, music_time={:.3}",
                    pending_len,
                    self.num_cols(),
                    self.current_music_time_seconds()
                );
            }
        }
        true
    }

    #[inline(always)]
    pub fn push_pending_input_edge(&mut self, edge: GameplayInputEdge) {
        self.pending_input.edges.push_back(edge);
    }

    #[inline(always)]
    pub fn pop_pending_input_edge(&mut self) -> Option<GameplayInputEdge> {
        self.pending_input.edges.pop_front()
    }

    #[inline(always)]
    pub fn clear_pending_input_edges(&mut self) {
        self.pending_input.edges.clear();
    }

    #[inline(always)]
    pub fn push_recorded_replay_edge(&mut self, edge: RecordedLaneEdge) {
        self.progress.replay.edges.push(edge);
    }

    #[inline(always)]
    pub fn toggle_flash_text(&self) -> Option<(&'static str, f32)> {
        self.display.toggle_flash.visible_text()
    }

    #[inline(always)]
    pub fn exit_transition_active(&self) -> bool {
        self.control.exit_input.exit_transition.is_some()
    }

    #[inline(always)]
    pub fn exit_prompt_state(&self) -> GameplayExitPromptState {
        self.control.exit_input.prompt_state()
    }

    pub fn danger_overlay_rgba(&self, player: usize, hide_lifebar: bool) -> Option<[f32; 4]> {
        if player >= self.setup.num_players || hide_lifebar {
            return None;
        }
        let rgba = self
            .display
            .danger_fx
            .rgba(player, self.boundary.total_elapsed_in_screen);
        if rgba[3] > 0.0 { Some(rgba) } else { None }
    }

    #[inline(always)]
    pub fn lane_pressed(&self, col: usize) -> bool {
        col < self.setup.num_cols && self.lane_is_pressed(col)
    }

    #[inline(always)]
    pub fn player_for_col(&self, col: usize) -> usize {
        player_index_for_column(self.setup.num_players, self.setup.cols_per_player, col)
    }

    #[inline(always)]
    pub const fn player_col_range(&self, player: usize) -> (usize, usize) {
        player_column_range(self.setup.cols_per_player, player)
    }

    #[inline(always)]
    pub fn record_step_calories_for_player(
        &mut self,
        player: usize,
        event_music_time_ns: SongTimeNs,
        weight_pounds: i32,
    ) {
        let (start, end) = self.player_col_range(player);
        let calories = recent_step_calories(
            &self.control.input_state.lane_pressed_since_ns,
            start,
            end,
            event_music_time_ns,
            weight_pounds,
        );
        add_player_step_calories(&mut self.players_runtime.players[player], calories);
    }

    #[inline(always)]
    pub fn clear_offset_adjust_hold_key(&mut self, key: GameplayOffsetAdjustKey) {
        self.control.offset_adjust_hold.clear(key);
    }

    #[inline(always)]
    pub fn start_offset_adjust_hold_key(
        &mut self,
        key: GameplayOffsetAdjustKey,
        at: Instant,
    ) -> f32 {
        self.control.offset_adjust_hold.start(key, at)
    }

    #[inline(always)]
    pub fn tick_offset_adjust_hold_key(
        &mut self,
        key: GameplayOffsetAdjustKey,
        now: Instant,
    ) -> Option<f32> {
        self.control.offset_adjust_hold.tick(key, now)
    }

    #[inline(always)]
    pub fn offset_adjust_target(&self) -> GameplayOffsetAdjustTarget {
        offset_adjust_target(
            self.control.exit_input.shift_held,
            self.progress.course_display.is_course_stage(),
        )
    }

    #[inline(always)]
    pub fn update_offset_adjust_hold(&mut self, now: Instant) {
        for key in [
            GameplayOffsetAdjustKey::Decrease,
            GameplayOffsetAdjustKey::Increase,
        ] {
            let Some(delta) = self.tick_offset_adjust_hold_key(key, now) else {
                continue;
            };
            match self.offset_adjust_target() {
                GameplayOffsetAdjustTarget::Global => {
                    let _ = self.apply_global_offset_delta(delta);
                }
                GameplayOffsetAdjustTarget::Song => {
                    let _ = self.apply_song_offset_delta(delta);
                }
                GameplayOffsetAdjustTarget::None => {}
            }
        }
    }

    #[inline(always)]
    pub fn apply_autosync_offset_sample(
        &mut self,
        note_off_by_ns: SongTimeNs,
    ) -> AutosyncSampleResult {
        self.control.autosync.apply_offset_sample(note_off_by_ns)
    }

    #[inline(always)]
    pub fn autosync_row_hits_enabled(&self, scoring_blocked: bool) -> bool {
        autosync_row_hits_enabled(
            self.progress.replay.mode,
            scoring_blocked,
            self.control.autosync.mode,
            self.progress.course_display.is_course_stage(),
        )
    }

    #[inline(always)]
    pub fn apply_autosync_for_row_hits(&mut self, row_entry_index: usize) {
        if !self.autosync_row_hits_enabled(self.autoplay_blocks_scoring()) {
            return;
        }

        let mut offsets = [0; MAX_COLS];
        let count = collect_autosync_row_hit_offsets(
            &self.chart_runtime.notes,
            &self.chart_runtime.row_entries[row_entry_index],
            &mut offsets,
        );
        for note_off_by_ns in offsets.into_iter().take(count) {
            self.apply_autosync_offset_correction(note_off_by_ns);
        }
    }

    #[inline(always)]
    pub fn timing_tick_status_line(&self) -> Option<&'static str> {
        timing_tick_mode_status_line(self.control.tick_mode)
    }

    #[inline(always)]
    pub fn tick_mode(&self) -> GameplayTimingTickMode {
        self.control.tick_mode
    }

    #[inline(always)]
    pub fn set_tick_mode(&mut self, mode: GameplayTimingTickMode) -> bool {
        if self.control.tick_mode == mode {
            return false;
        }
        self.control.tick_mode = mode;
        true
    }

    pub fn apply_timing_tick_mode_command(
        &mut self,
        mode: GameplayTimingTickMode,
        now_music_time: f32,
    ) {
        if !self.set_tick_mode(mode) {
            return;
        }
        self.push_session_command(GameplaySessionCommand::SetTimingTickMode(mode));

        let song_row = self.assist_row_no_offset(now_music_time);
        self.reset_assist_clap_for_row(song_row);

        log::debug!(
            "Timing ticks set to {} (F7).",
            timing_tick_mode_debug_label(mode)
        );
    }

    pub fn set_live_autoplay_enabled(&mut self, enabled: bool) {
        if !self.set_stage_autoplay_enabled(enabled) {
            return;
        }

        if enabled {
            self.reset_live_input_state();
            self.reset_receptor_feedback_for_autoplay();
            self.clear_pending_input_edges();
            for player in 0..self.setup.num_players {
                self.set_autoplay_cursor_for_enable(
                    player,
                    self.chart_runtime.mine_scan.next_tap_miss_cursor[player],
                    self.note_range_for_player(player),
                );
            }
            log::debug!("Autoplay enabled (F8). Scores for this stage will not be saved.");
            return;
        }

        log::debug!("Autoplay disabled (F8).");
    }

    #[inline(always)]
    pub fn live_autoplay_judgment_offset_music_ns(
        &mut self,
        player_idx: usize,
        window: TimingWindow,
        measured_offset_music_ns: SongTimeNs,
    ) -> SongTimeNs {
        if !self.live_autoplay_enabled() {
            return measured_offset_music_ns;
        }
        let timing_profile = if player_idx < self.setup.num_players {
            self.timing_runtime.player_judgment_timing[player_idx].profile_music_ns
        } else {
            TimingProfileNs::from_profile_scaled(
                &self.timing_runtime.timing_profile,
                self.music_rate(),
            )
        };
        self.control.autoplay_runtime.judgment_offset_music_ns(
            self.live_autoplay_enabled(),
            timing_profile,
            window,
            measured_offset_music_ns,
        )
    }

    #[inline(always)]
    pub fn build_final_note_hit_plan(
        &mut self,
        player_idx: usize,
        hit: NoteHitEval,
        rate: f32,
    ) -> FinalNoteHitPlan {
        let judgment_offset_music_ns = self.live_autoplay_judgment_offset_music_ns(
            player_idx,
            hit.window,
            hit.measured_offset_music_ns,
        );
        final_note_hit_plan(hit, judgment_offset_music_ns, rate)
    }

    #[inline(always)]
    pub fn reset_assist_clap_for_row(&mut self, song_row: i32) {
        self.control.assist_clap.reset_for_row(song_row);
    }

    #[inline(always)]
    pub fn stage_autoplay_enabled(&self) -> bool {
        self.progress.stage.autoplay_enabled
    }

    #[inline(always)]
    pub fn live_autoplay_enabled(&self) -> bool {
        live_autoplay_enabled_from_flags(
            self.progress.stage.autoplay_enabled,
            self.progress.replay.mode,
        )
    }

    #[inline(always)]
    pub fn autoplay_blocks_scoring(&self) -> bool {
        autoplay_blocks_scoring_from_flags(
            self.progress.stage.autoplay_enabled,
            self.progress.replay.mode,
        )
    }

    #[inline(always)]
    pub fn set_stage_autoplay_enabled(&mut self, enabled: bool) -> bool {
        if self.progress.stage.autoplay_enabled == enabled {
            return false;
        }
        self.progress.stage.autoplay_enabled = enabled;
        true
    }

    #[inline(always)]
    pub fn reset_live_input_state(&mut self) {
        self.control.input_state.reset_live_state();
    }

    #[inline(always)]
    pub fn lane_input_counts(&self) -> &[u16; MAX_COLS] {
        self.control.input_state.lane_counts()
    }

    #[inline(always)]
    pub fn input_slot_lane_is_down(
        &self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
    ) -> bool {
        self.control
            .input_state
            .slot_lane_is_down(lane_idx, source, input_slot)
    }

    #[inline(always)]
    pub fn normalized_input_slot_for_lane(
        &self,
        lane_idx: usize,
        input_slot: u32,
        invalid_slot: u32,
    ) -> u32 {
        normalized_input_slot(input_slot, lane_idx as u32, invalid_slot)
    }

    #[inline(always)]
    pub fn input_slot_lane_is_down_normalized(
        &self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
        invalid_slot: u32,
    ) -> bool {
        let input_slot = self.normalized_input_slot_for_lane(lane_idx, input_slot, invalid_slot);
        self.input_slot_lane_is_down(lane_idx, source, input_slot)
    }

    #[inline(always)]
    pub fn update_input_slot(
        &mut self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
        pressed: bool,
    ) -> LaneInputUpdate {
        self.control
            .input_state
            .update_slot(lane_idx, source, input_slot, pressed)
    }

    #[inline(always)]
    pub fn update_input_slot_normalized(
        &mut self,
        lane_idx: usize,
        source: InputSource,
        input_slot: u32,
        pressed: bool,
        invalid_slot: u32,
    ) -> LaneInputUpdate {
        let input_slot = self.normalized_input_slot_for_lane(lane_idx, input_slot, invalid_slot);
        self.update_input_slot(lane_idx, source, input_slot, pressed)
    }

    #[inline(always)]
    pub fn update_lane_input_slot(
        &mut self,
        lane: Lane,
        source: InputSource,
        input_slot: u32,
        pressed: bool,
        invalid_slot: u32,
    ) -> LaneInputUpdate {
        let lane_idx = lane.index();
        let input_slot = self.normalized_input_slot_for_lane(lane_idx, input_slot, invalid_slot);
        let update = self.update_input_slot(lane_idx, source, input_slot, pressed);
        if update.slot_table_full {
            log::debug!(
                "Gameplay active input slot table full; dropping held-state edge for {:?} slot {}",
                source,
                input_slot
            );
        }
        update
    }

    #[inline(always)]
    pub fn press_input_lane(&mut self, lane_idx: usize, event_music_time_ns: SongTimeNs) {
        self.control
            .input_state
            .press_lane(lane_idx, event_music_time_ns);
    }

    #[inline(always)]
    pub fn release_input_lane(&mut self, lane_idx: usize) {
        self.control.input_state.release_lane(lane_idx);
    }

    pub fn current_lane_inputs(&self) -> [bool; MAX_COLS] {
        std::array::from_fn(|col| col < self.setup.num_cols && self.lane_is_pressed(col))
    }

    pub fn held_mine_crossing_start_times(
        &self,
        current_inputs: &[bool; MAX_COLS],
        previous_music_time_ns: SongTimeNs,
        current_music_time_ns: SongTimeNs,
    ) -> [Option<SongTimeNs>; MAX_COLS] {
        std::array::from_fn(|col| {
            if col >= self.setup.num_cols {
                return None;
            }
            crossed_mine_held_start_time(
                current_inputs[col],
                self.control.input_state.prev_inputs[col],
                self.control.input_state.lane_pressed_since_ns[col],
                previous_music_time_ns,
                current_music_time_ns,
            )
        })
    }

    #[inline(always)]
    pub fn set_previous_lane_inputs(&mut self, inputs: [bool; MAX_COLS]) {
        self.control.input_state.prev_inputs = inputs;
    }

    #[inline(always)]
    pub fn reset_receptor_feedback_for_autoplay(&mut self) {
        self.display.receptor_feedback.reset_for_autoplay();
    }

    #[inline(always)]
    pub fn set_autoplay_cursor_for_enable(
        &mut self,
        player: usize,
        next_tap_miss_cursor: usize,
        note_range: (usize, usize),
    ) {
        self.control.autoplay_runtime.set_cursor_for_enable(
            player,
            next_tap_miss_cursor,
            note_range,
        );
    }

    #[inline(always)]
    pub fn autoplay_cursor(&self, player: usize) -> usize {
        self.control.autoplay_runtime.cursor(player)
    }

    #[inline(always)]
    pub fn set_autoplay_cursor(&mut self, player: usize, cursor: usize) {
        self.control.autoplay_runtime.set_cursor(player, cursor);
    }

    #[inline(always)]
    pub fn mark_autoplay_used(&mut self) {
        self.progress.stage.autoplay_used = true;
    }

    #[inline(always)]
    pub fn set_raw_modifier_state(&mut self, shift_held: bool, ctrl_held: bool) {
        self.control.exit_input.shift_held = shift_held;
        self.control.exit_input.ctrl_held = ctrl_held;
    }

    #[inline(always)]
    pub fn set_raw_modifier_key(&mut self, key: GameplayRawModifierKey, pressed: bool) {
        match key {
            GameplayRawModifierKey::Shift => self.control.exit_input.shift_held = pressed,
            GameplayRawModifierKey::Ctrl => self.control.exit_input.ctrl_held = pressed,
        }
    }

    #[inline(always)]
    pub fn raw_key_plan(
        &self,
        input: GameplayRawKeyInput,
        pressed: bool,
        allow_commands: bool,
    ) -> GameplayRawKeyPlan {
        gameplay_raw_key_plan(
            input,
            pressed,
            allow_commands,
            self.control.exit_input.ctrl_held,
            self.control.exit_input.shift_held,
            self.control.autosync.mode,
            self.progress.course_display.is_course_stage(),
            self.control.tick_mode,
            self.progress.stage.autoplay_enabled,
        )
    }

    pub fn apply_raw_key_plan(
        &mut self,
        plan: GameplayRawKeyPlan,
        timestamp: Instant,
        now_music_time: f32,
    ) -> RawKeyAction {
        let action = gameplay_raw_key_action_for_plan(plan);
        match plan {
            GameplayRawKeyPlan::Restart | GameplayRawKeyPlan::Reload => return action,
            GameplayRawKeyPlan::SetAutosyncMode(mode) => self.set_autosync_mode(mode),
            GameplayRawKeyPlan::SetTimingTickMode(mode) => {
                self.apply_timing_tick_mode_command(mode, now_music_time)
            }
            GameplayRawKeyPlan::SetAutoplayEnabled(enabled) => {
                self.set_live_autoplay_enabled(enabled)
            }
            GameplayRawKeyPlan::StartOffsetAdjust { key, target } => {
                let delta = self.start_offset_adjust_hold_key(key, timestamp);
                match target {
                    GameplayOffsetAdjustTarget::Global => {
                        let _ = self.apply_global_offset_delta(delta);
                    }
                    GameplayOffsetAdjustTarget::Song => {
                        let _ = self.apply_song_offset_delta(delta);
                    }
                    GameplayOffsetAdjustTarget::None => {}
                }
            }
            GameplayRawKeyPlan::ClearOffsetAdjust(key) => self.clear_offset_adjust_hold_key(key),
            GameplayRawKeyPlan::None => {}
        }
        action
    }

    pub fn handle_queued_raw_key_input(
        &mut self,
        input: GameplayRawKeyInput,
        modifier_key: Option<GameplayRawModifierKey>,
        pressed: bool,
        timestamp: Instant,
        now_music_time: f32,
        allow_commands: bool,
    ) -> RawKeyAction {
        if let Some(key) = modifier_key {
            self.set_raw_modifier_key(key, pressed);
        }
        let plan = self.raw_key_plan(input, pressed, allow_commands);
        self.apply_raw_key_plan(plan, timestamp, now_music_time)
    }

    #[inline(always)]
    pub fn set_autosync_mode(&mut self, mode: AutosyncMode) {
        self.control.autosync.mode = mode;
    }

    #[inline(always)]
    pub fn drain_audio_commands(&mut self) -> std::vec::Drain<'_, GameplayAudioCommand> {
        self.boundary.commands.drain_audio()
    }

    #[inline(always)]
    pub fn push_audio_command(&mut self, command: GameplayAudioCommand) {
        self.boundary.commands.push_audio(command);
    }

    #[inline(always)]
    pub fn queue_play_music_command(&mut self, path: PathBuf, cut: GameplayMusicCut, rate: f32) {
        self.push_audio_command(GameplayAudioCommand::PlayMusic {
            path,
            cut,
            looping: false,
            rate,
        });
    }

    pub fn set_current_music_time_ns(&mut self, music_time_ns: SongTimeNs) {
        self.reset_time_to_beat_caches();
        self.clock.song_position.current_music_time_ns = music_time_ns;
        let display_time_ns = self.clock.display_clock.reset(music_time_ns);
        self.clock.song_position.current_music_time_display =
            song_time_ns_to_seconds(display_time_ns);
        self.update_song_position_from_time(music_time_ns, display_time_ns);
    }

    pub fn update_song_position_from_time(
        &mut self,
        music_time_ns: SongTimeNs,
        display_time_ns: SongTimeNs,
    ) {
        self.clock.song_position.current_music_time_ns = music_time_ns;
        let beat_info = self
            .timing_runtime
            .time_to_beat_caches
            .song_info(&self.timing_runtime.timing, music_time_ns);
        self.clock.song_position.current_beat = beat_info.beat;
        self.clock.song_position.current_beat_display = self
            .timing_runtime
            .time_to_beat_caches
            .display_beat(&self.timing_runtime.timing, display_time_ns);
        self.display
            .beat_phase
            .set(beat_info.is_in_freeze, beat_info.is_in_delay);

        for player in 0..self.setup.num_players {
            let delay = self.clock.visible_timing.visual_delay_seconds(player);
            let visible_time_ns = visible_notefield_time_ns(music_time_ns, delay);
            let visible_time_seconds = song_time_ns_to_seconds(visible_time_ns);
            self.clock.visible_timing.set_player_time(
                player,
                visible_time_ns,
                visible_time_seconds,
                self.timing_runtime.time_to_beat_caches.visible_beat(
                    player,
                    &self.timing_runtime.timing_players[player],
                    visible_time_ns,
                ),
            );
            self.display
                .cue_runtime
                .update_crossover_cue_anchors(player, visible_time_seconds);
        }
    }

    pub fn start_stage_music(&mut self) {
        let lead_in = self.clock.audio_clock.positive_lead_in_seconds();
        log::debug!("Starting music with a preroll delay of {lead_in:.2}s");
        let start_time = -self.clock.audio_clock.positive_lead_in_seconds();
        self.set_current_music_time_ns(song_time_ns_from_seconds(start_time));
        self.boundary.total_elapsed_in_screen = 0.0;
        self.refresh_seek_dependent_state();

        let Some(music_path) = self.source.charts[0].music_path.clone() else {
            return;
        };
        let rate = normalized_song_rate(self.music_rate());
        self.queue_play_music_command(music_path, stage_music_cut(lead_in), rate);
    }

    pub fn seek_practice_display(&mut self, music_time: f32) {
        self.set_current_music_time_ns(song_time_ns_from_seconds(music_time));
        self.refresh_seek_dependent_state();
    }

    pub fn start_practice_music_at(
        &mut self,
        playback_music_time: f32,
        judge_start_music_time: f32,
    ) {
        self.reset_practice_playback(judge_start_music_time);

        self.clock
            .audio_clock
            .set_lead_in_seconds((-playback_music_time).max(0.0));
        self.set_current_music_time_ns(song_time_ns_from_seconds(playback_music_time));
        self.refresh_seek_dependent_state();

        let Some(music_path) = self.source.charts[0].music_path.clone() else {
            return;
        };
        let rate = normalized_song_rate(self.music_rate());
        self.queue_play_music_command(
            music_path,
            GameplayMusicCut {
                start_sec: f64::from(playback_music_time),
                length_sec: f64::INFINITY,
                ..Default::default()
            },
            rate,
        );
    }

    #[inline(always)]
    pub fn begin_exit_transition(&mut self, kind: ExitTransitionKind) -> bool {
        if !self.control.exit_input.begin_exit(kind, Instant::now()) {
            return false;
        }
        self.push_audio_command(GameplayAudioCommand::StopMusic);
        true
    }

    #[inline(always)]
    pub fn apply_menu_input_plan(&mut self, plan: GameplayMenuInputPlan, timestamp: Instant) {
        match plan {
            GameplayMenuInputPlan::None => {}
            GameplayMenuInputPlan::ArmHold(key) => {
                self.control.exit_input.arm_hold(key, timestamp);
            }
            GameplayMenuInputPlan::AbortHold(_) => self.control.exit_input.abort_hold(timestamp),
            GameplayMenuInputPlan::BeginExit(kind) => {
                self.begin_exit_transition(kind);
            }
        }
    }

    #[inline(always)]
    pub fn handle_gameplay_menu_input(
        &mut self,
        input: GameplayMenuInput,
        pressed: bool,
        timestamp: Instant,
    ) {
        let p2_runtime_player = self.setup.session.p2_runtime_player();
        let p1_menu_active = self.setup.num_players > 1 || !p2_runtime_player;
        let p2_menu_active = self.setup.num_players > 1 || p2_runtime_player;
        let plan = gameplay_menu_input_plan(
            input,
            pressed,
            p1_menu_active,
            p2_menu_active,
            self.setup.config.delayed_back,
            self.control.exit_input.hold_to_exit_key,
        );
        self.apply_menu_input_plan(plan, timestamp);
    }

    #[inline(always)]
    pub fn begin_restart_exit(&mut self) -> bool {
        self.begin_exit_transition(ExitTransitionKind::Cancel)
    }

    #[inline(always)]
    pub fn drain_session_commands(&mut self) -> std::vec::Drain<'_, GameplaySessionCommand> {
        self.boundary.commands.drain_session()
    }

    #[inline(always)]
    pub fn push_session_command(&mut self, command: GameplaySessionCommand) {
        self.boundary.commands.push_session(command);
    }

    #[inline(always)]
    pub const fn total_elapsed_in_screen(&self) -> f32 {
        self.boundary.total_elapsed_in_screen
    }

    #[inline(always)]
    pub fn advance_screen_elapsed(&mut self, delta_time: f32) {
        self.boundary.total_elapsed_in_screen += delta_time;
    }

    #[inline(always)]
    pub fn set_screen_elapsed(&mut self, elapsed: f32) {
        self.boundary.total_elapsed_in_screen = elapsed;
    }

    #[inline(always)]
    pub const fn density_graph_view(&self) -> GameplayDensityGraphView {
        GameplayDensityGraphView {
            first_second: self.display.density_graph.first_second,
            last_second: self.display.density_graph.last_second,
            duration: self.display.density_graph.duration,
            graph_w: self.display.density_graph.graph_w,
            graph_h: self.display.density_graph.graph_h,
            scaled_width: self.display.density_graph.scaled_width,
            u0: self.display.density_graph.u0,
            u_window: self.display.density_graph.u_window,
            top_h: self.display.density_graph.top_h,
            top_w: self.display.density_graph.top_w,
            top_scale_y: self.display.density_graph.top_scale_y,
        }
    }

    #[inline(always)]
    pub const fn density_graph_life_dirty(&self, player: usize) -> bool {
        player < MAX_PLAYERS && self.display.density_graph.life_dirty[player]
    }

    #[inline(always)]
    pub fn set_density_graph_life_dirty(&mut self, player: usize, dirty: bool) {
        if player < MAX_PLAYERS {
            self.display.density_graph.life_dirty[player] = dirty;
        }
    }

    #[inline(always)]
    pub fn density_graph_life_points(&self, player: usize) -> Option<&[[f32; 2]]> {
        self.display
            .density_graph
            .life_points
            .get(player)
            .map(Vec::as_slice)
    }

    #[inline(always)]
    pub fn density_graph_life_points_mut(&mut self, player: usize) -> Option<&mut Vec<[f32; 2]>> {
        self.display.density_graph.life_points.get_mut(player)
    }

    pub fn capacity_trace_snapshot(&self) -> GameplayCapacityTraceSnapshot {
        let mut snapshot = GameplayCapacityTraceSnapshot {
            pending_edges_capacity: self.pending_input.edges.capacity(),
            pending_edges_len: self.pending_input.edges.len(),
            replay_edges_capacity: self.progress.replay.edges.capacity(),
            replay_edges_len: self.progress.replay.edges.len(),
            decaying_hold_capacity: self.hold_runtime.decaying_hold_indices.capacity(),
            decaying_hold_len: self.hold_runtime.decaying_hold_indices.len(),
            num_players: self.setup.num_players,
            ..GameplayCapacityTraceSnapshot::default()
        };
        for player in 0..self.setup.num_players.min(MAX_PLAYERS) {
            snapshot.density_life_capacity[player] =
                self.display.density_graph.life_points[player].capacity();
            snapshot.density_life_len[player] =
                self.display.density_graph.life_points[player].len();
        }
        snapshot
    }
}
