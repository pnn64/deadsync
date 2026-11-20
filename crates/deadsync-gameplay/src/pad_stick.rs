#[derive(Clone, Copy)]
struct SyntheticStepTiming {
    frame_time_ns: SongTimeNs,
    search_time_ns: SongTimeNs,
}

impl<Profile, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
{
    #[inline(always)]
    pub const fn pad_stick_seconds(&self) -> f32 {
        self.setup.config.pad_stick_seconds
    }

    #[inline(always)]
    pub fn set_pad_stick_seconds(&mut self, seconds: f32) {
        if seconds.is_finite() {
            // Native PREFSMAN changes do not rewind the CrossedRows iterator.
            self.setup.config.pad_stick_seconds = seconds;
        }
    }

    /// Install InputFilter's pressed state together with its transition ages.
    /// This does not synthesize press edges or alter per-binding gameplay counts.
    pub fn set_input_snapshot(&mut self, ages: [f32; 32], pressed_actions: u32) {
        if self.progress.replay.mode {
            return;
        }
        self.set_input_age_by_action(ages);
        let mut pressed = 0;
        for action in deadsync_input::ALL_VIRTUAL_ACTIONS {
            if pressed_actions & action.bit() == 0 {
                continue;
            }
            let Some(lane) = lane_from_action(action).and_then(|lane| {
                remap_live_input_lane(
                    self.setup.session.play_style,
                    self.setup.session.player_side,
                    lane,
                )
            }) else {
                continue;
            };
            let col = lane.index();
            if col >= self.setup.num_cols.min(MAX_COLS) {
                continue;
            }
            pressed |= input_lane_bit(col);
            if self.setup.session.play_style.is_routine() && col < self.setup.cols_per_player {
                pressed |= input_lane_bit(col + self.setup.cols_per_player);
            }
        }
        self.control.input_state.physical_pressed_lane_mask = Some(pressed);
    }

    fn pad_stick_pressed_lane_mask(&self) -> LaneMask {
        if !self.progress.replay.mode
            && let Some(pressed) = self.control.input_state.physical_pressed_lane_mask
        {
            return pressed;
        }
        self.control.input_state.pressed_lane_mask()
    }

    /// Install InputFilter ages before updating Players, including bindings
    /// which were already held when gameplay started and retained releases.
    pub fn set_input_age_by_action(&mut self, ages: [f32; 32]) {
        // Replay input has its own recorded timestamps. A viewer's held keys
        // must never supply InputFilter ages to the replay's synthetic steps.
        if self.progress.replay.mode {
            return;
        }
        self.control.input_state.input_age_seconds.fill(0.0);
        for action in deadsync_input::ALL_VIRTUAL_ACTIONS {
            let Some(lane) = lane_from_action(action).and_then(|lane| {
                remap_live_input_lane(
                    self.setup.session.play_style,
                    self.setup.session.player_side,
                    lane,
                )
            }) else {
                continue;
            };
            let col = lane.index();
            if col >= self.setup.num_cols.min(MAX_COLS) {
                continue;
            }
            let age = ages[action.ix()];
            self.control.input_state.input_age_seconds[col] =
                self.control.input_state.input_age_seconds[col].max(age);
            let width = self.setup.cols_per_player;
            if self.setup.session.play_style.is_routine() && col < width {
                self.control.input_state.input_age_seconds[col + width] =
                    self.control.input_state.input_age_seconds[col];
            }
        }
    }

    fn reset_crossed_rows(&mut self, music_time_ns: SongTimeNs) {
        for player in 0..self.setup.num_players.min(MAX_PLAYERS) {
            let (start, end) = self.note_range_for_player(player);
            let timing = &self.timing_runtime.timing_players[player];
            let beat = timing.get_beat_for_time_ns(music_time_ns);
            self.chart_runtime.mine_scan.crossed_note_cursor[player] = start
                + self.chart_runtime.notes[start..end].partition_point(|note| note.beat < beat);
        }
    }

    /// ITGmania Player::CrossedRows visits notes in row/track order, including
    /// mines which an earlier physical press has already judged. Its cursor is
    /// advanced even without input, so later presses never replay a crossed row.
    fn cross_pad_stick_rows(&mut self, music_time_ns: SongTimeNs) {
        if self.progress.replay.mode {
            self.begin_replay_input_snapshot(
                music_time_ns,
                self.setup.config.input_debounce_seconds,
            );
        }
        let music_seconds = song_time_ns_to_seconds(music_time_ns);
        for player in 0..self.setup.num_players.min(MAX_PLAYERS) {
            let timing = &self.timing_runtime.timing_players[player];
            // Player tests the actual position's delay flag, not the shifted one.
            if timing.get_song_position(music_seconds).is_in_delay {
                continue;
            }
            let position = timing.get_song_position(music_seconds - self.pad_stick_seconds());
            let crossed_row = (position.beat * ROWS_PER_BEAT as f32) as i32;
            if crossed_row < 0 {
                continue;
            }
            let (start, end) = self.note_range_for_player(player);
            let mut cursor = self.chart_runtime.mine_scan.crossed_note_cursor[player].max(start);
            while cursor < end {
                let note = &self.chart_runtime.notes[cursor];
                if beat_to_note_row(note.beat) > crossed_row {
                    break;
                }
                let note_index = cursor;
                let column = note.column;
                let note_type = note.note_type;
                cursor += 1;
                // Publish before callbacks, which may update the preference.
                self.chart_runtime.mine_scan.crossed_note_cursor[player] = cursor;
                let hold_head = matches!(note_type, NoteType::Hold | NoteType::Roll);
                let pump = self.setup.session.play_style.is_pump();
                if hold_head {
                    let life = if pump {
                        PUMP_INITIAL_HOLD_LIFE
                    } else {
                        INITIAL_HOLD_LIFE
                    };
                    if let Some(hold) = self.chart_runtime.notes[note_index].hold.as_mut() {
                        hold.life = life;
                    }
                    if let Some(active) = self.hold_runtime.active_holds[column].as_mut()
                        && active.note_index == note_index
                    {
                        active.life = life;
                    }
                }
                if self.live_autoplay_enabled() {
                    self.set_autoplay_cursor(player, cursor);
                    self.judge_autoplay_crossed_note(note_index, music_time_ns);
                    continue;
                }
                if !(note_type == NoteType::Mine || (hold_head && pump)) {
                    continue;
                }
                let stick = self.pad_stick_seconds();
                if stick > 0.0 {
                    // GetSecsHeld uses the last reported raw timestamp; it does
                    // not test the current down bit (released entries can live
                    // through InputFilter's debounce/snapshot cleanup).
                    let age = if self.progress.replay.mode {
                        self.replay_input_age_seconds(column, music_time_ns)
                    } else {
                        self.control.input_state.input_age_seconds[column]
                    };
                    if age >= stick {
                        self.judge_pad_stick_step(column, music_time_ns, stick);
                    }
                } else if self.pad_stick_pressed_lane_mask() & input_lane_bit(column) != 0 {
                    if note_type == NoteType::Mine {
                        self.refresh_roll_life_on_step(column, music_time_ns);
                        if !self.hit_mine(column, note_index, 0) {
                            self.trigger_receptor_step_pulse(column);
                        }
                        self.dispatch_step_callback(column);
                    } else {
                        self.judge_pad_stick_step(column, music_time_ns, 0.0);
                    }
                }
            }
        }
    }

    fn judge_pad_stick_step(&mut self, column: usize, music_time_ns: SongTimeNs, seconds: f32) {
        let player = self.player_for_col(column);
        let rate = normalized_song_rate(self.music_rate());
        let music_seconds = song_time_ns_to_seconds(music_time_ns);
        // Native Step searches at music_position - timestamp.Ago(), but its
        // judgment backdates by timestamp.Ago() * music_rate. Keep both clocks.
        let search_time_ns = song_time_ns_from_seconds(music_seconds - seconds);
        let event_time_ns = song_time_ns_from_seconds(music_seconds - seconds * rate);
        let timing = &self.timing_runtime.timing_players[player];
        let current = timing_row_nearest(timing, timing.get_beat_for_time_ns(search_time_ns));
        let (start, end) = step_search_row_bounds(timing, music_time_ns, current);
        let lane_indices = &mut self.chart_runtime.lane_indices;
        let search = closest_lane_note_search_with_rows_from_cursor(
            &lane_indices.note_indices[column],
            &self.chart_runtime.notes,
            &self.chart_runtime.note_time_cache_ns,
            timing,
            event_time_ns,
            LaneSearchRows {
                current,
                start,
                end,
            },
            &mut lane_indices.note_search_cursors[column],
        );
        self.refresh_roll_life_on_step(column, search_time_ns);
        let timing = SyntheticStepTiming {
            frame_time_ns: music_time_ns,
            search_time_ns,
        };
        if !self.judge_a_tap_with_search(column, event_time_ns, search, Some(timing)) {
            self.trigger_receptor_step_pulse(column);
        }
        self.dispatch_step_callback(column);
    }

    fn start_active_hold_after_step(
        &mut self,
        note_index: usize,
        column: usize,
        hit: NoteHitEval,
        event_time_ns: SongTimeNs,
        synthetic_step: Option<SyntheticStepTiming>,
    ) {
        let note = &self.chart_runtime.notes[note_index];
        let preserve_life = if let Some(step) = synthetic_step
            && (note.note_type == NoteType::Hold
                || (note.note_type == NoteType::Roll
                    && beat_to_note_row(
                        self.timing_runtime.timing_players[self.player_for_col(column)]
                            .get_beat_for_time_ns(step.search_time_ns),
                    ) < beat_to_note_row(note.beat)))
        {
            // CrossedRows has already reset life after this frame's hold
            // integration. Native held Step does not refill an ordinary Hold
            // or an early Roll, nor rewind to the synthetic timestamp.
            if self.hold_runtime.active_holds[column]
                .as_ref()
                .is_some_and(|active| active.note_index == note_index)
            {
                return;
            }
            self.chart_runtime.notes[note_index]
                .hold
                .as_ref()
                .map(|hold| hold.life)
        } else {
            None
        };
        self.start_active_hold_for_hit(
            note_index,
            column,
            hit,
            synthetic_step.map_or(event_time_ns, |step| step.frame_time_ns),
        );
        if let Some(life) = preserve_life {
            let is_pressed = self.pad_stick_pressed_lane_mask() & input_lane_bit(column) != 0;
            if let Some(active) = self.hold_runtime.active_holds[column].as_mut()
                && active.note_index == note_index
            {
                active.life = life;
                active.is_pressed = is_pressed;
            }
            if let Some(hold) = self.chart_runtime.notes[note_index].hold.as_mut() {
                hold.life = life;
            }
        }
    }

    fn judge_autoplay_crossed_note(&mut self, note_index: usize, frame_time_ns: SongTimeNs) {
        let note = &self.chart_runtime.notes[note_index];
        if note.result.is_some()
            || note.is_fake
            || !note.can_be_judged
            || matches!(note.note_type, NoteType::Mine | NoteType::Fake)
        {
            return;
        }
        let column = note.column;
        let note_type = note.note_type;
        let search = LaneNoteSearch {
            current_row_index: note.row_index,
            candidate: Some((note_index, 0)),
            ..LaneNoteSearch::default()
        };
        let note_time_ns = self.chart_runtime.note_time_cache_ns[note_index];
        self.mark_autoplay_used();
        self.refresh_roll_life_on_step(column, frame_time_ns);
        if note_type == NoteType::Lift {
            self.judge_a_lift(column, note_time_ns);
        } else {
            let timing = SyntheticStepTiming {
                frame_time_ns,
                search_time_ns: frame_time_ns,
            };
            self.judge_a_tap_with_search(column, note_time_ns, search, Some(timing));
            self.dispatch_step_callback(column);
        }
    }

    #[inline(always)]
    fn dispatch_step_callback(&mut self, column: usize) {
        if let Some(mut callback) = self.step_callback.take() {
            callback(self, column);
            self.step_callback = Some(callback);
        }
    }
}
