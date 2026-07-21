mod runtime_regression_tests {
    use super::*;
    use deadsync_chart::notes::ParsedNote;
    use deadsync_chart::{
        ArrowStats, GameplayChartData, SongData, StaminaCounts, SyncPref, TechCounts,
    };
    use deadsync_core::song_time::SongTimeNs;
    use deadsync_input::{InputEvent, VirtualAction};
    use deadsync_rules::note::{HoldData, HoldResult, MineResult};
    use deadsync_rules::timing::{DelaySegment, TimingData, TimingSegments};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    #[derive(Clone)]
    struct TestProfile {
        remove_mask_bits: u8,
        turn_option: GameplayTurnOption,
        attack_mode: GameplayAttackMode,
        column_flash_options: ColumnFlashOptions,
        tap_explosion_options: TapExplosionOptions,
        error_bar_options: GameplayErrorBarOptions,
        fantastic_options: FantasticWindowOptions,
        fantastic_feedback_options: FantasticFeedbackOptions,
        noteskin_name: &'static str,
        mini_percent: f32,
    }

    impl Default for TestProfile {
        fn default() -> Self {
            Self {
                remove_mask_bits: 0,
                turn_option: GameplayTurnOption::None,
                attack_mode: GameplayAttackMode::On,
                column_flash_options: ColumnFlashOptions::default(),
                tap_explosion_options: TapExplosionOptions::default(),
                error_bar_options: GameplayErrorBarOptions::default(),
                fantastic_options: FantasticWindowOptions {
                    base_fa_plus_s: 0.0,
                    custom_fantastic_window_s: None,
                    fa_plus_10ms_blue_window: false,
                },
                fantastic_feedback_options: FantasticFeedbackOptions::default(),
                noteskin_name: DEFAULT_NOTESKIN_NAME,
                mini_percent: 0.0,
            }
        }
    }

    fn all_tap_explosion_options() -> TapExplosionOptions {
        TapExplosionOptions {
            fantastic: true,
            excellent: true,
            great: true,
            decent: true,
            way_off: true,
            miss: true,
            held: true,
            holding: true,
        }
    }

    fn error_bar_profile(options: GameplayErrorBarOptions) -> TestProfile {
        TestProfile {
            error_bar_options: options,
            ..TestProfile::default()
        }
    }

    impl GameplayProfileData for TestProfile {
        fn insert_mask_bits(&self) -> u8 {
            0
        }

        fn remove_mask_bits(&self) -> u8 {
            self.remove_mask_bits
        }

        fn holds_mask_bits(&self) -> u8 {
            0
        }

        fn appearance_mask_bits(&self) -> u8 {
            0
        }

        fn visual_mask_bits(&self) -> u16 {
            0
        }

        fn turn_option(&self) -> GameplayTurnOption {
            self.turn_option
        }

        fn attack_mode(&self) -> GameplayAttackMode {
            self.attack_mode
        }

        fn perspective_effects(&self) -> PerspectiveEffects {
            PerspectiveEffects::default()
        }

        fn scroll_effects(&self) -> ScrollEffects {
            ScrollEffects::default()
        }

        fn mini_indicator_options(&self) -> GameplayMiniIndicatorOptions {
            GameplayMiniIndicatorOptions::default()
        }

        fn target_score(&self) -> GameplayTargetScoreSetting {
            GameplayTargetScoreSetting::default()
        }

        fn timing_disabled_windows(&self) -> [bool; 5] {
            [false; 5]
        }

        fn column_flash_options(&self) -> ColumnFlashOptions {
            self.column_flash_options
        }

        fn tap_explosion_options(&self) -> TapExplosionOptions {
            self.tap_explosion_options
        }

        fn fantastic_options(&self, base_fa_plus_s: f32) -> FantasticWindowOptions {
            FantasticWindowOptions {
                base_fa_plus_s,
                ..self.fantastic_options
            }
        }

        fn fantastic_feedback_options(&self) -> FantasticFeedbackOptions {
            self.fantastic_feedback_options
        }

        fn error_bar_options(&self) -> GameplayErrorBarOptions {
            self.error_bar_options
        }

        fn measure_counter_threshold(&self) -> Option<usize> {
            None
        }

        fn step_statistics_density_graph(&self) -> bool {
            false
        }

        fn note_field_offset_x(&self) -> f32 {
            0.0
        }

        fn noteskin_name(&self) -> String {
            self.noteskin_name.to_string()
        }

        fn mini_percent(&self) -> f32 {
            self.mini_percent
        }

        fn global_offset_shift_ms(&self) -> i32 {
            0
        }

        fn visual_delay_ms(&self) -> i32 {
            0
        }

        fn reverse_scroll(&self) -> bool {
            false
        }

        fn column_cues(&self) -> bool {
            false
        }

        fn crossover_cues(&self) -> bool {
            false
        }

        fn crossover_cue_duration_ms(&self) -> u16 {
            0
        }

        fn crossover_cue_quantization(&self) -> u8 {
            0
        }

        fn crossover_cue_brackets(&self) -> bool {
            false
        }

        fn nps_graph_at_top(&self) -> bool {
            false
        }

        fn carry_combo_between_songs(&self) -> bool {
            false
        }

        fn calculated_weight_pounds(&self) -> i32 {
            0
        }

        fn hide_lifebar(&self) -> bool {
            false
        }

        fn hide_danger(&self) -> bool {
            false
        }

        fn rescore_early_hits(&self) -> bool {
            false
        }

        fn hide_early_dw_judgments(&self) -> bool {
            false
        }

        fn hide_early_dw_flash(&self) -> bool {
            false
        }

        fn hide_early_dw_column_flash(&self) -> bool {
            false
        }
    }

    struct NoSongLuaRuntime;

    impl SongLuaRuntimeBuilder<(), (), ()> for NoSongLuaRuntime {
        fn build_song_lua_runtime(
            self,
            params: SongLuaRuntimeWindowBuild<'_>,
        ) -> SongLuaRuntimeBuildOutput<(), (), ()> {
            (
                std::array::from_fn(|_| Vec::new()),
                std::array::from_fn(|_| Vec::new()),
                build_song_lua_runtime_visuals(
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    [(); MAX_PLAYERS],
                    std::array::from_fn(|_| Vec::new()),
                    (),
                    Vec::new(),
                    [false; MAX_PLAYERS],
                    std::array::from_fn(|_| Vec::new()),
                    std::array::from_fn(|_| Vec::new()),
                    params.screen_width,
                    params.screen_height,
                ),
            )
        }
    }

    type State = GameplayRuntimeState<TestProfile, (), (), ()>;

    fn assert_valid_hot_state_for_tests(state: &State, delta_time: f32, music_time_sec: f32) {
        debug_assert!(
            delta_time.is_finite() && delta_time >= 0.0,
            "invalid delta_time={delta_time}"
        );
        debug_assert!(
            music_time_sec.is_finite(),
            "invalid music_time_sec={music_time_sec}"
        );
        debug_assert!(
            state.setup.num_players > 0 && state.setup.num_players <= MAX_PLAYERS,
            "invalid num_players={}",
            state.setup.num_players
        );
        debug_assert!(
            state.setup.num_cols > 0 && state.setup.num_cols <= MAX_COLS,
            "invalid num_cols={}",
            state.setup.num_cols
        );
        debug_assert!(
            state.setup.cols_per_player > 0 && state.setup.cols_per_player <= MAX_COLS,
            "invalid cols_per_player={}",
            state.setup.cols_per_player
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.chart_runtime.note_time_cache_ns.len()
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.chart_runtime.hold_end_time_cache_ns.len()
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.chart_runtime.column_judgment_eligible.len()
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.chart_runtime.lane_indices.note_itg_rows.len()
        );
        debug_assert!(
            state
                .chart_runtime
                .notes
                .iter()
                .zip(&state.chart_runtime.lane_indices.note_itg_rows)
                .all(|(note, &row)| row == beat_to_note_row(note.beat))
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.hold_runtime.hold_decay_active.len()
        );
        debug_assert_eq!(
            state.chart_runtime.notes.len(),
            state.chart_runtime.row_indices.note_row_entry_indices.len()
        );
        for player in 0..state.setup.num_players {
            let (start, end) = state.chart_runtime.note_ranges.range(player);
            debug_assert!(start <= end && end <= state.chart_runtime.notes.len());
            let (row_start, row_end) = state.chart_runtime.row_indices.row_entry_ranges[player];
            debug_assert!(row_start <= row_end && row_end <= state.chart_runtime.row_entries.len());
            debug_assert!(
                state.chart_runtime.row_indices.judged_row_cursor[player] >= row_start
                    && state.chart_runtime.row_indices.judged_row_cursor[player] <= row_end
            );
            debug_assert!(
                state.chart_runtime.mine_scan.next_tap_miss_cursor[player] >= start
                    && state.chart_runtime.mine_scan.next_tap_miss_cursor[player] <= end
            );
            debug_assert!(
                state.chart_runtime.mine_scan.next_mine_avoid_cursor[player] >= start
                    && state.chart_runtime.mine_scan.next_mine_avoid_cursor[player] <= end
            );
            debug_assert_eq!(
                state.chart_runtime.mine_scan.mine_note_ix[player].len(),
                state.chart_runtime.mine_scan.mine_note_time_ns[player].len()
            );
            debug_assert!(
                state.chart_runtime.mine_scan.next_mine_ix_cursor[player]
                    <= state.chart_runtime.mine_scan.mine_note_ix[player].len()
            );
        }
        for player in 0..state.setup.num_players {
            let (start, end) = state.chart_runtime.note_ranges.range(player);
            debug_assert!(
                state.chart_runtime.mine_scan.mine_note_time_ns[player]
                    .windows(2)
                    .all(|pair| pair[0] <= pair[1])
            );
            for &note_index in &state.chart_runtime.mine_scan.mine_note_ix[player] {
                debug_assert!(note_index >= start && note_index < end);
                debug_assert!(matches!(
                    state.chart_runtime.notes[note_index].note_type,
                    NoteType::Mine
                ));
            }
        }
        for note in &state.chart_runtime.notes {
            if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                let player = state.player_for_col(note.column);
                debug_assert!(
                    row_entry_for_cached_row(
                        &state.chart_runtime.row_entries,
                        &state.chart_runtime.row_indices.row_map_cache[player],
                        note.row_index
                    )
                    .is_some()
                );
            }
        }
        for (row_entry_index, row_entry) in state.chart_runtime.row_entries.iter().enumerate() {
            let first_note_index = row_entry.note_indices()[0];
            let player = state.player_for_col(state.chart_runtime.notes[first_note_index].column);
            debug_assert!(
                row_entry_index >= state.chart_runtime.row_indices.row_entry_ranges[player].0
                    && row_entry_index < state.chart_runtime.row_indices.row_entry_ranges[player].1
            );
            debug_assert_eq!(
                state.chart_runtime.row_indices.row_map_cache[player]
                    .get(row_entry.row_index)
                    .copied(),
                Some(row_entry_index as u32)
            );
            for &note_index in row_entry.note_indices() {
                debug_assert!(note_index < state.chart_runtime.notes.len());
                debug_assert_eq!(
                    state.chart_runtime.row_indices.note_row_entry_indices[note_index],
                    row_entry_index as u32
                );
                let note = &state.chart_runtime.notes[note_index];
                debug_assert_eq!(note.row_index, row_entry.row_index);
                debug_assert!(note.can_be_judged);
                debug_assert!(!note.is_fake);
                debug_assert!(!matches!(note.note_type, NoteType::Mine));
            }
        }
        for col in 0..state.setup.num_cols {
            debug_assert!(
                state
                    .display
                    .notefield_motion
                    .column_scroll_dir(col)
                    .is_finite()
            );
            debug_assert!(
                state.chart_runtime.lane_indices.note_indices[col]
                    .windows(2)
                    .all(|pair| {
                        let left = pair[0];
                        let right = pair[1];
                        left < right
                            && state.chart_runtime.note_time_cache_ns[left]
                                <= state.chart_runtime.note_time_cache_ns[right]
                    })
            );
            for &note_index in &state.chart_runtime.lane_indices.note_indices[col] {
                debug_assert!(note_index < state.chart_runtime.notes.len());
                debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
            }
            debug_assert_eq!(
                state.chart_runtime.lane_indices.note_row_indices[col].len(),
                state.chart_runtime.lane_indices.note_indices[col].len()
            );
            debug_assert!(
                state.chart_runtime.lane_indices.note_row_indices[col]
                    .windows(2)
                    .all(|pair| {
                        let left = pair[0];
                        let right = pair[1];
                        (beat_to_note_row(state.chart_runtime.notes[left].beat), left)
                            <= (
                                beat_to_note_row(state.chart_runtime.notes[right].beat),
                                right,
                            )
                    })
            );
            for &note_index in &state.chart_runtime.lane_indices.note_row_indices[col] {
                debug_assert!(note_index < state.chart_runtime.notes.len());
                debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
            }
            debug_assert!(
                state.chart_runtime.lane_indices.hold_indices[col]
                    .windows(2)
                    .all(|pair| {
                        let left = pair[0];
                        let right = pair[1];
                        left < right
                            && state.chart_runtime.note_time_cache_ns[left]
                                <= state.chart_runtime.note_time_cache_ns[right]
                    })
            );
            for &note_index in &state.chart_runtime.lane_indices.hold_indices[col] {
                debug_assert!(note_index < state.chart_runtime.notes.len());
                debug_assert_eq!(state.chart_runtime.notes[note_index].column, col);
                debug_assert!(matches!(
                    state.chart_runtime.notes[note_index].note_type,
                    NoteType::Hold | NoteType::Roll
                ));
            }
        }
        for col in state.setup.num_cols..MAX_COLS {
            debug_assert!(state.chart_runtime.lane_indices.note_indices[col].is_empty());
            debug_assert!(state.chart_runtime.lane_indices.note_row_indices[col].is_empty());
            debug_assert!(state.chart_runtime.lane_indices.hold_indices[col].is_empty());
        }
        let mut lane_positions = [0usize; MAX_COLS];
        for (note_index, note) in state.chart_runtime.notes.iter().enumerate() {
            if note.column >= state.setup.num_cols {
                continue;
            }
            let lane_pos = lane_positions[note.column];
            debug_assert_eq!(
                state.chart_runtime.lane_indices.note_indices[note.column]
                    .get(lane_pos)
                    .copied(),
                Some(note_index)
            );
            lane_positions[note.column] += 1;
        }
        for col in 0..state.setup.num_cols {
            debug_assert_eq!(
                lane_positions[col],
                state.chart_runtime.lane_indices.note_indices[col].len()
            );
        }
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        Note {
            beat: row_index as f32 / ROWS_PER_BEAT as f32,
            quantization_idx: 0,
            column,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn note_with_judgment(
        column: usize,
        row_index: usize,
        note_type: NoteType,
        grade: JudgeGrade,
        time_error_ms: f32,
    ) -> Note {
        let mut note = test_note(column, row_index, note_type);
        note.result = Some(Judgment {
            time_error_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(time_error_ms, 1.0),
            grade,
            window: None,
            miss_because_held: false,
        });
        note
    }

    fn test_hold(column: usize, row_index: usize, end_row_index: usize) -> Note {
        let mut note = test_note(column, row_index, NoteType::Hold);
        note.hold = Some(HoldData {
            end_row_index,
            end_beat: end_row_index as f32 / ROWS_PER_BEAT as f32,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row_index,
            last_held_beat: row_index as f32 / ROWS_PER_BEAT as f32,
        });
        note
    }

    fn test_roll(column: usize, row_index: usize, end_row_index: usize) -> Note {
        let mut note = test_hold(column, row_index, end_row_index);
        note.note_type = NoteType::Roll;
        note
    }

    fn test_row_entry_with_times(
        notes: &[Note],
        note_time_cache_ns: &[SongTimeNs],
        row_index: usize,
        nonmine_note_indices: Vec<usize>,
    ) -> RowEntry {
        let mut row_note_indices = [usize::MAX; MAX_COLS];
        let nonmine_note_count = nonmine_note_indices.len() as u8;
        for (i, note_index) in nonmine_note_indices.into_iter().enumerate() {
            row_note_indices[i] = note_index;
        }
        build_row_entry(
            row_index,
            row_note_indices,
            nonmine_note_count,
            notes,
            note_time_cache_ns,
        )
    }

    fn enable_tap_explosion_durations(state: &mut State) {
        for player in 0..MAX_PLAYERS {
            for col in 0..MAX_COLS {
                for window in TAP_EXPLOSION_WINDOWS {
                    state.display.noteskin_effects.set_tap_explosion_duration(
                        player,
                        col,
                        window,
                        false,
                        Some(0.5),
                    );
                    state.display.noteskin_effects.set_tap_explosion_duration(
                        player,
                        col,
                        window,
                        true,
                        Some(0.5),
                    );
                }
            }
        }
    }

    fn set_single_judged_tap(
        state: &mut State,
        column: usize,
        row_index: usize,
        grade: JudgeGrade,
        time_error_ms: f32,
    ) {
        state.chart_runtime.notes = vec![note_with_judgment(
            column,
            row_index,
            NoteType::Tap,
            grade,
            time_error_ms,
        )];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
    }

    fn test_input_edge_at(
        lane: Lane,
        pressed: bool,
        event_music_time_ns: SongTimeNs,
    ) -> GameplayInputEdge {
        let now = std::time::Instant::now();
        GameplayInputEdge {
            lane,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            record_replay: false,
            captured_at: now,
            captured_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
            queued_at: now,
            event_music_time_ns,
        }
    }

    fn test_input_event(action: VirtualAction) -> InputEvent {
        test_input_event_with_source(action, true, InputSource::Keyboard)
    }

    fn test_input_event_with_source(
        action: VirtualAction,
        pressed: bool,
        source: InputSource,
    ) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn regression_chart() -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "gameplay-regression".to_string(),
            stats: ArrowStats {
                total_arrows: 2,
                total_steps: 2,
                ..ArrowStats::default()
            },
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 2.0,
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
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
        }
    }

    fn test_chart(
        stats: ArrowStats,
        timing_segments: TimingSegments,
        chart_attacks: Option<&str>,
    ) -> ChartData {
        let bpms = timing_segments
            .bpms
            .iter()
            .map(|(_, bpm)| *bpm)
            .collect::<Vec<_>>();
        let min_bpm = bpms.iter().copied().reduce(f32::min).unwrap_or(120.0);
        let max_bpm = bpms.iter().copied().reduce(f32::max).unwrap_or(min_bpm);
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "score-validity-test".to_string(),
            stats,
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
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
            has_chart_attacks: chart_attacks.is_some_and(|attacks| !attacks.trim().is_empty()),
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: f64::from(min_bpm),
            max_bpm: f64::from(max_bpm),
        }
    }

    fn regression_song() -> SongData {
        SongData {
            simfile_path: PathBuf::from("songs/Tests/gameplay-regression.ssc"),
            title: "Gameplay Regression".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: "Tests".to_string(),
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
            display_bpm: "150".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
            normalized_bpms: "0.000=150.000".to_string(),
            music_length_seconds: 60.0,
            first_second: 0.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            charts: vec![regression_chart()],
        }
    }

    fn regression_payload_with_segments(
        timing_segments: TimingSegments,
        last_row: usize,
    ) -> GameplayChartData {
        let row_to_beat = test_row_to_beat(last_row);
        let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
        GameplayChartData {
            notes: Vec::new(),
            parsed_notes: vec![
                ParsedNote {
                    row_index: 48,
                    column: 0,
                    note_type: NoteType::Tap,
                    tail_row_index: None,
                },
                ParsedNote {
                    row_index: 96,
                    column: 1,
                    note_type: NoteType::Tap,
                    tail_row_index: None,
                },
            ],
            row_to_beat,
            timing_segments,
            timing,
            chart_attacks: None,
        }
    }

    fn regression_state() -> State {
        regression_state_with_profiles(std::array::from_fn(|_| TestProfile::default()))
    }

    fn regression_state_with_segments(timing_segments: TimingSegments, last_row: usize) -> State {
        regression_state_with_segments_and_profiles(
            timing_segments,
            last_row,
            std::array::from_fn(|_| TestProfile::default()),
        )
    }

    fn regression_state_with_profiles(profiles: [TestProfile; MAX_PLAYERS]) -> State {
        regression_state_with_segments_and_profiles(TimingSegments::default(), 96, profiles)
    }

    fn regression_state_with_session(session: GameplaySession) -> State {
        regression_state_with_session_and_config(session, GameplayConfig::default())
    }

    fn regression_state_with_session_and_config(
        session: GameplaySession,
        config: GameplayConfig,
    ) -> State {
        regression_state_with_session_profiles_and_scroll(
            session,
            config,
            std::array::from_fn(|_| TestProfile::default()),
            [ScrollSpeedSetting::default(); MAX_PLAYERS],
        )
    }

    fn regression_state_with_session_profiles_and_scroll(
        session: GameplaySession,
        config: GameplayConfig,
        profiles: [TestProfile; MAX_PLAYERS],
        scroll_speeds: [ScrollSpeedSetting; MAX_PLAYERS],
    ) -> State {
        let song = Arc::new(regression_song());
        let chart = Arc::new(song.charts[0].clone());
        let gameplay_chart = Arc::new(regression_payload_with_segments(
            TimingSegments::default(),
            96,
        ));
        init_gameplay_runtime(
            song,
            [chart.clone(), chart],
            [gameplay_chart.clone(), gameplay_chart],
            GameplayViewport::default(),
            session,
            config,
            SyncPref::Default,
            GameplayMiniIndicatorData::default(),
            GameplayNoteskinData::default(),
            NoSongLuaRuntime,
            empty_crossover_annotations,
            5,
            1.0,
            scroll_speeds,
            profiles,
            None,
            None,
            None,
            None,
            None,
            None,
            [0; MAX_PLAYERS],
        )
    }

    fn regression_state_with_segments_and_profiles(
        timing_segments: TimingSegments,
        last_row: usize,
        profiles: [TestProfile; MAX_PLAYERS],
    ) -> State {
        let song = Arc::new(regression_song());
        let chart = Arc::new(song.charts[0].clone());
        let gameplay_chart = Arc::new(regression_payload_with_segments(timing_segments, last_row));
        init_gameplay_runtime(
            song,
            [chart.clone(), chart],
            [gameplay_chart.clone(), gameplay_chart],
            GameplayViewport::default(),
            GameplaySession::default(),
            GameplayConfig::default(),
            SyncPref::Default,
            GameplayMiniIndicatorData::default(),
            GameplayNoteskinData::default(),
            NoSongLuaRuntime,
            empty_crossover_annotations,
            5,
            1.0,
            [ScrollSpeedSetting::default(); MAX_PLAYERS],
            profiles,
            None,
            None,
            None,
            None,
            None,
            None,
            [0; MAX_PLAYERS],
        )
    }

    fn set_regression_mine(
        state: &mut State,
        note_index: usize,
        column: usize,
        row_index: usize,
        time_ns: SongTimeNs,
    ) {
        state.chart_runtime.notes[note_index] = test_note(column, row_index, NoteType::Mine);
        state.chart_runtime.note_time_cache_ns[note_index] = time_ns;
        state.chart_runtime.mine_scan.mine_note_ix[0] = vec![note_index];
        state.chart_runtime.mine_scan.mine_note_time_ns[0] = vec![time_ns];
        state.chart_runtime.mine_scan.next_mine_ix_cursor[0] = 0;
        state.chart_runtime.mine_scan.next_mine_avoid_cursor[0] = note_index;
        state.progress.chart_totals.mines_total[0] = 1;
    }

    #[test]
    fn regression_state_passes_hot_state_audit() {
        let state = regression_state();
        assert_valid_hot_state_for_tests(
            &state,
            0.0,
            state.clock.song_position.current_music_time_display,
        );
    }

    fn set_state_timing(state: &mut State, timing: Arc<TimingData>) {
        state.timing_runtime.timing = Arc::clone(&timing);
        state.timing_runtime.timing_players[0] = Arc::clone(&timing);
        state.timing_runtime.timing_players[1] = timing;
        state.reset_time_to_beat_caches();
    }

    fn song_lua_test_timing() -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(16 * 48),
        )
    }

    fn song_lua_beat_mod(
        start: f32,
        limit: f32,
        span_mode: SongLuaRuntimeSpanMode,
        mods: &str,
    ) -> SongLuaRuntimeModWindow {
        SongLuaRuntimeModWindow {
            player: Some(1),
            unit: SongLuaRuntimeTimeUnit::Beat,
            start,
            limit,
            span_mode,
            mods: mods.to_string(),
        }
    }

    fn song_lua_beat_ease(
        start: f32,
        limit: f32,
        target: SongLuaRuntimeEaseTargetOwned,
        from: f32,
        to: f32,
        sustain: Option<f32>,
    ) -> SongLuaRuntimeEaseWindow {
        SongLuaRuntimeEaseWindow {
            player: Some(1),
            unit: SongLuaRuntimeTimeUnit::Beat,
            start,
            limit,
            span_mode: SongLuaRuntimeSpanMode::Len,
            target,
            from,
            to,
            easing: Some("linear".to_string()),
            sustain,
            opt1: None,
            opt2: None,
        }
    }

    #[test]
    fn delayed_rows_do_not_time_miss_or_avoid_until_delay_finishes() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            delays: vec![DelaySegment {
                beat: 1.0,
                duration: 2.0,
            }],
            ..TimingSegments::default()
        };
        let note_time_ns = song_time_ns_from_seconds(1.0);

        let mut tap_state =
            regression_state_with_segments(timing_segments.clone(), ROWS_PER_BEAT as usize * 4);
        tap_state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        let miss_distance_ns = max_step_distance_ns(
            &tap_state.timing_runtime.timing_profile,
            tap_state.music_rate(),
        );
        let inside_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        tap_state.apply_time_based_tap_misses(inside_delay_music_time);
        assert!(tap_state.chart_runtime.notes[0].result.is_none());
        assert_eq!(tap_state.chart_runtime.mine_scan.next_tap_miss_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(miss_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        tap_state.apply_time_based_tap_misses(after_delay_music_time);
        assert_eq!(
            tap_state.chart_runtime.notes[0]
                .result
                .as_ref()
                .map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );

        let mut mine_state =
            regression_state_with_segments(timing_segments, ROWS_PER_BEAT as usize * 4);
        set_regression_mine(&mut mine_state, 0, 0, ROWS_PER_BEAT as usize, note_time_ns);
        let mine_distance_ns = max_step_distance_ns(
            &mine_state.timing_runtime.timing_profile,
            mine_state.music_rate(),
        );
        let inside_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(0.5));
        mine_state.apply_time_based_mine_avoidance(inside_delay_music_time);
        assert_eq!(mine_state.chart_runtime.notes[0].mine_result, None);
        assert_eq!(mine_state.chart_runtime.mine_scan.next_mine_ix_cursor[0], 0);

        let after_delay_music_time = note_time_ns
            .saturating_add(mine_distance_ns)
            .saturating_add(song_time_ns_from_seconds(2.1));
        mine_state.apply_time_based_mine_avoidance(after_delay_music_time);
        assert_eq!(
            mine_state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Avoided)
        );
    }

    #[test]
    fn completed_song_counts_last_mine_as_avoided_at_end_cutoff() {
        let timing = Arc::new(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        ));
        let mut state = regression_state();
        set_state_timing(&mut state, Arc::clone(&timing));

        let mine_row = ROWS_PER_BEAT as usize;
        let mine_time_ns = timing.get_time_for_beat_ns(1.0);
        set_regression_mine(&mut state, 0, 0, mine_row, mine_time_ns);
        let end_time_ns = mine_time_ns.saturating_add(max_step_distance_ns(
            &state.timing_runtime.timing_profile,
            state.music_rate(),
        ));

        state.apply_time_based_mine_avoidance(end_time_ns);
        assert_eq!(state.players_runtime.players[0].mines_avoided, 0);
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);

        state.finalize_completed_mines();
        assert_eq!(state.players_runtime.players[0].mines_avoided, 1);
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Avoided)
        );
    }

    #[test]
    fn completed_song_finalizes_last_tap_miss_before_eval() {
        let mut state = regression_state();
        let (note_start, note_end) = state.note_range_for_player(0);
        let first_note = note_start;
        let last_note = note_end - 1;
        let first_row_entry =
            state.chart_runtime.row_indices.note_row_entry_indices[first_note] as usize;
        let last_row_entry =
            state.chart_runtime.row_indices.note_row_entry_indices[last_note] as usize;
        let miss_ix = judgment::judge_grade_ix(JudgeGrade::Miss);

        state.set_final_note_result(
            first_note,
            Judgment {
                time_error_ms: 0.0,
                time_error_music_ns: 0,
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
        );
        state.clock.song_position.current_music_time_ns =
            state.chart_runtime.note_time_cache_ns[first_note].saturating_add(
                max_step_distance_ns(&state.timing_runtime.timing_profile, state.music_rate()),
            );
        assert!(!state.settle_completion_rows());
        assert!(
            state.chart_runtime.row_entries[first_row_entry]
                .final_outcome
                .is_some()
        );
        assert!(
            state.chart_runtime.row_entries[last_row_entry]
                .final_outcome
                .is_none()
        );

        let miss_time_ns = state.chart_runtime.note_time_cache_ns[last_note]
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        state.clock.song_position.current_music_time_ns = miss_time_ns;
        state.update_judged_rows();
        state.apply_time_based_tap_misses(miss_time_ns);
        assert_eq!(
            state.chart_runtime.notes[last_note]
                .result
                .as_ref()
                .map(|j| j.grade),
            Some(JudgeGrade::Miss)
        );
        assert_eq!(state.players_runtime.players[0].judgment_counts[miss_ix], 0);

        assert!(state.settle_completion_rows());
        assert_eq!(
            state.chart_runtime.row_entries[last_row_entry].final_outcome,
            Some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Miss,
            })
        );
        assert_eq!(state.players_runtime.players[0].judgment_counts[miss_ix], 1);
    }

    #[test]
    fn column_judgments_include_fatal_frame_then_stop() {
        let mut state = regression_state();
        let rows = [0, 48, 96];
        state.chart_runtime.notes = vec![
            note_with_judgment(0, rows[0], NoteType::Tap, JudgeGrade::Miss, 0.0),
            note_with_judgment(0, rows[1], NoteType::Tap, JudgeGrade::Fantastic, 4.0),
            note_with_judgment(0, rows[2], NoteType::Tap, JudgeGrade::Excellent, 18.0),
        ];
        state.chart_runtime.note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
        ];
        state.chart_runtime.column_judgment_eligible = vec![false; 3];
        state.chart_runtime.row_entries = rows
            .iter()
            .enumerate()
            .map(|(note_index, &row_index)| {
                test_row_entry_with_times(
                    &state.chart_runtime.notes,
                    &state.chart_runtime.note_time_cache_ns,
                    row_index,
                    vec![note_index],
                )
            })
            .collect();
        state.players_runtime.players[0].life = 0.001;

        state.latch_column_judgment_health();
        state.finalize_row_judgment(0, rows[0], 0, false);
        assert!(state.players_runtime.players[0].is_failing);

        // HealthState_Dead is published after the update's JudgmentMessages,
        // so another judgment finalized in the fatal update is still tracked.
        state.finalize_row_judgment(0, rows[1], 1, false);
        state.latch_column_judgment_health();
        state.finalize_row_judgment(0, rows[2], 2, false);

        assert_eq!(
            state.chart_runtime.column_judgment_eligible,
            [true, true, false]
        );
        assert_eq!(
            state.players_runtime.players[0]
                .judgment_counts
                .iter()
                .sum::<u32>(),
            3
        );
    }

    #[test]
    fn crossed_held_mine_hits_even_when_frame_offset_exceeds_mine_window() {
        let mut state = regression_state();
        set_regression_mine(&mut state, 0, 0, 48, song_time_ns_from_seconds(1.0));

        assert!(state.try_hit_crossed_mines_while_held(
            0,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.2),
        ));
        assert_eq!(
            state.chart_runtime.notes[0].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(
            state.chart_runtime.mine_scan.pending_mine_hit_indices,
            vec![0]
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 0);

        state.apply_pending_mine_hits();

        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 1);
    }

    #[test]
    fn mine_hit_side_effects_wait_until_after_active_holds() {
        let mut state = regression_state();
        let hold_end_ns = song_time_ns_from_seconds(1.0);
        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        set_regression_mine(&mut state, 1, 1, ROWS_PER_BEAT as usize, hold_end_ns);
        state.players_runtime.players[0].life = 0.04;
        state.hold_runtime.active_holds[0] = Some(ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: hold_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: MAX_HOLD_LIFE,
            last_update_time_ns: 0,
        });

        assert!(state.hit_mine(1, 1, 0));
        assert_eq!(
            state.chart_runtime.notes[1].mine_result,
            Some(MineResult::Hit)
        );
        assert_eq!(state.players_runtime.players[0].mines_hit, 0);
        assert!(!state.players_runtime.players[0].is_failing);

        let inputs = std::array::from_fn(|col| col == 0);
        state.update_active_holds(&inputs, hold_end_ns);
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(state.players_runtime.players[0].holds_held_for_score, 1);
        assert!(!state.players_runtime.players[0].is_failing);

        state.apply_pending_mine_hits();
        assert_eq!(state.players_runtime.players[0].mines_hit, 1);
        assert_eq!(state.players_runtime.players[0].mines_hit_for_score, 0);
        assert!(state.players_runtime.players[0].is_failing);
    }

    #[test]
    fn scored_missed_hold_resolves_let_go_at_hold_end() {
        let mut state = regression_state();
        let note_time_ns = song_time_ns_from_seconds(1.0);
        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.progress.stage.score_missed_holds_rolls[0] = true;
        state.chart_runtime.notes[0] = test_hold(0, 48, 96);
        state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        state.apply_time_based_tap_misses(miss_time_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .result
                .as_ref()
                .map(|judgment| judgment.grade),
            Some(JudgeGrade::Miss)
        );
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|h| h.result),
            None
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);

        state.resolve_pending_missed_holds(hold_end_ns.saturating_sub(1));
        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|h| h.result),
            None
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);

        state.resolve_pending_missed_holds(hold_end_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::LetGo)
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 1);
        assert_eq!(
            state.display.hold_feedback.hold_judgments[0]
                .as_ref()
                .map(|info| info.result),
            Some(HoldResult::LetGo)
        );
    }

    #[test]
    fn unscored_missed_hold_emits_missed_feedback_at_hold_end() {
        let mut state = regression_state();
        let note_time_ns = song_time_ns_from_seconds(1.0);
        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.progress.stage.score_missed_holds_rolls[0] = false;
        state.chart_runtime.notes[0] = test_hold(0, 48, 96);
        state.chart_runtime.note_time_cache_ns[0] = note_time_ns;
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[1].can_be_judged = false;

        let miss_time_ns = note_time_ns
            .saturating_add(max_step_distance_ns(
                &state.timing_runtime.timing_profile,
                state.music_rate(),
            ))
            .saturating_add(song_time_ns_from_seconds(0.1));
        state.apply_time_based_tap_misses(miss_time_ns);

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Missed)
        );
        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_none());

        state.resolve_pending_missed_holds(hold_end_ns);

        assert_eq!(state.players_runtime.players[0].holds_let_go_for_score, 0);
        assert_eq!(
            state.display.hold_feedback.hold_judgments[0]
                .as_ref()
                .map(|info| info.result),
            Some(HoldResult::Missed)
        );
    }

    #[test]
    fn crossed_held_mine_new_press_excludes_rows_before_press() {
        let mut state = regression_state();
        set_regression_mine(&mut state, 0, 0, 48, song_time_ns_from_seconds(1.0));
        let crossed_from_ns = crossed_mine_held_start_time(
            true,
            false,
            Some(song_time_ns_from_seconds(1.1)),
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.2),
        )
        .expect("new press should produce a crossed-row start");

        assert!(!state.try_hit_crossed_mines_while_held(
            0,
            crossed_from_ns,
            song_time_ns_from_seconds(1.2),
        ));
        assert_eq!(state.chart_runtime.notes[0].mine_result, None);
    }

    #[test]
    fn set_music_rate_rebuilds_judgment_and_end_times() {
        let mut state = regression_state();
        let baseline_great_ns = state.timing_runtime.player_judgment_timing[0]
            .profile_music_ns
            .windows_ns[2];
        let baseline_notes_end = state.notes_end_time_ns();
        let baseline_music_end = state.music_end_time_ns();

        assert!(state.set_music_rate(1.5));
        assert!((state.music_rate() - 1.5).abs() < 1e-6);

        let scaled_great_ns = state.timing_runtime.player_judgment_timing[0]
            .profile_music_ns
            .windows_ns[2];
        assert!(
            scaled_great_ns > baseline_great_ns,
            "music-rate=1.5 should widen the W3 window in song-time ns ({} vs {})",
            scaled_great_ns,
            baseline_great_ns,
        );
        assert!(
            state.notes_end_time_ns() > baseline_notes_end,
            "music-rate=1.5 should also widen the late-resolution slack on the note end time \
             ({} vs {})",
            state.notes_end_time_ns(),
            baseline_notes_end,
        );
        assert_eq!(state.music_end_time_ns(), baseline_music_end);

        assert!(!state.set_music_rate(1.5));
        assert!(state.set_music_rate(f32::NAN));
        assert!((state.music_rate() - 1.0).abs() < 1e-6);
        assert!(state.set_music_rate(1.5));
        assert!(state.set_music_rate(-2.0));
        assert!((state.music_rate() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn active_hold_let_go_visual_row_uses_frame_target() {
        let mut state = regression_state();
        let timing = Arc::new(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        ));
        set_state_timing(&mut state, timing);

        let hold_end_ns = song_time_ns_from_seconds(2.0);
        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize * 2);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(hold_end_ns);
        state.chart_runtime.notes[0]
            .hold
            .as_mut()
            .expect("test hold")
            .life = 0.25;
        state.hold_runtime.active_holds[0] = Some(ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: hold_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: false,
            life: 0.25,
            last_update_time_ns: 0,
        });

        let target_ns = song_time_ns_from_seconds(0.2);
        state.integrate_active_hold_to_time(0, target_ns);

        let hold = state.chart_runtime.notes[0]
            .hold
            .as_ref()
            .expect("test hold");
        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert!(state.hold_runtime.active_holds[0].is_none());
        assert!((hold.last_held_beat - 0.2).abs() <= 1e-6);
        assert!(hold.last_held_beat > TIMING_WINDOW_SECONDS_HOLD * 0.25 + f32::EPSILON);
    }

    #[test]
    fn early_next_hold_start_settles_previous_same_column_hold() {
        let mut state = regression_state();
        let previous_end_ns = song_time_ns_from_seconds(1.0);
        let next_start_ns = song_time_ns_from_seconds(1.09375);
        let next_end_ns = song_time_ns_from_seconds(1.375);

        state.chart_runtime.notes[0] = test_hold(0, 0, ROWS_PER_BEAT as usize);
        state.chart_runtime.notes[1] =
            test_hold(0, ROWS_PER_BEAT as usize + 12, ROWS_PER_BEAT as usize * 2);
        state.chart_runtime.hold_end_time_cache_ns[0] = Some(previous_end_ns);
        state.chart_runtime.hold_end_time_cache_ns[1] = Some(next_end_ns);
        state.hold_runtime.active_holds[0] = Some(ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: previous_end_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: MAX_HOLD_LIFE,
            last_update_time_ns: song_time_ns_from_seconds(0.95),
        });

        state.start_active_hold(
            0,
            1,
            next_start_ns,
            next_end_ns,
            song_time_ns_from_seconds(0.95),
        );

        assert_eq!(
            state.chart_runtime.notes[0]
                .hold
                .as_ref()
                .and_then(|hold| hold.result),
            Some(HoldResult::Held)
        );
        assert_eq!(
            state.hold_runtime.active_holds[0]
                .as_ref()
                .map(|active| active.note_index),
            Some(1)
        );
    }

    #[test]
    fn roll_step_refreshes_before_event_time_decay() {
        let mut state = regression_state();
        let mut roll = test_roll(0, 0, ROWS_PER_BEAT as usize * 4);
        roll.result = Some(Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        });
        state.chart_runtime.notes[0] = roll;

        let event_time_ns = song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_ROLL + 0.01);
        state.hold_runtime.active_holds[0] = Some(ActiveHold {
            note_index: 0,
            start_time_ns: 0,
            end_time_ns: song_time_ns_from_seconds(2.0),
            note_type: NoteType::Roll,
            let_go: false,
            is_pressed: false,
            life: MAX_HOLD_LIFE,
            last_update_time_ns: 0,
        });
        state
            .pending_input
            .edges
            .push_back(test_input_edge_at(Lane::Left, true, event_time_ns));

        let now = std::time::Instant::now();
        let clock = SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: now,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        let active = state.hold_runtime.active_holds[0]
            .as_ref()
            .expect("roll should remain active after the body step");
        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, event_time_ns);
        let hold = state.chart_runtime.notes[0]
            .hold
            .as_ref()
            .expect("roll hold data");
        assert_eq!(hold.result, None);
        assert_eq!(hold.life, MAX_HOLD_LIFE);
    }

    #[test]
    fn live_input_resolves_invalid_edge_time_from_song_clock() {
        let mut state = regression_state();
        let event_time_ns = song_time_ns_from_seconds(12.345);
        let edge = test_input_edge_at(Lane::Left, true, INVALID_SONG_TIME_NS);
        let captured_at = edge.captured_at;
        state.pending_input.edges.push_back(edge);

        let clock = SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: captured_at,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        assert_eq!(
            state.control.input_state.lane_pressed_since_ns[0],
            Some(event_time_ns)
        );
    }

    #[test]
    fn empty_live_press_steps_receptor() {
        let mut state = regression_state();
        let event_time_ns = song_time_ns_from_seconds(12.345);
        state
            .pending_input
            .edges
            .push_back(test_input_edge_at(Lane::Left, true, event_time_ns));

        let now = std::time::Instant::now();
        let clock = SongClockSnapshot {
            song_time_ns: event_time_ns,
            seconds_per_second: 1.0,
            mapped_audio: true,
            valid_at: now,
            valid_at_host_nanos: 0,
            timing_diag_enabled: false,
            timing_diag_callback_gap_ns: 0,
        };
        let mut phase_timings = GameplayUpdatePhaseTimings::default();
        process_input_edges(&mut state, false, &mut phase_timings, clock);

        assert!(state.display.receptor_feedback.bop_timers[0] > 0.0);
    }

    #[test]
    fn jump_row_finalization_uses_row_judgment_for_error_bar_hud() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_TEXT,
            error_ms_display: true,
            text_threshold_ms: 10,
            ..GameplayErrorBarOptions::default()
        });

        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        let row_index = 48usize;
        state.chart_runtime.notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
        ];
        state.chart_runtime.note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0, 1],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0, 0];
        state.chart_runtime.row_indices.judged_row_cursor = [0; MAX_PLAYERS];
        state.clock.song_position.current_music_time_ns = song_time_ns_from_seconds(1.096);
        state.boundary.total_elapsed_in_screen = 12.0;

        state.set_final_note_result(
            0,
            Judgment {
                time_error_ms: -12.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(-12.0, 1.0),
                grade: JudgeGrade::Great,
                window: Some(TimingWindow::W3),
                miss_because_held: false,
            },
        );
        state.set_final_note_result(
            1,
            Judgment {
                time_error_ms: 96.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(96.0, 1.0),
                grade: JudgeGrade::Decent,
                window: Some(TimingWindow::W4),
                miss_because_held: false,
            },
        );

        assert!(
            state.players_runtime.players[0]
                .offset_indicator_text
                .is_none()
        );
        assert!(state.players_runtime.players[0].error_bar_text.is_none());

        state.finalize_row_judgment(0, row_index, 0, false);

        let offset = state.players_runtime.players[0]
            .offset_indicator_text
            .expect("row-final judgment should drive the offset indicator");
        assert_eq!(offset.started_at, 12.0);
        assert_eq!(offset.offset_ms, 96.0);
        assert_eq!(offset.window, TimingWindow::W4);

        let early_late = state.players_runtime.players[0]
            .error_bar_text
            .expect("row-final judgment should drive the early/late text");
        assert_eq!(early_late.started_at, 12.0);
        assert!(!early_late.early);
        assert_eq!(early_late.offset_ms, 96.0);
        assert!(!early_late.scaled);

        let last = state.players_runtime.players[0]
            .last_judgment
            .as_ref()
            .expect("row-final judgment should update the judgment sprite");
        assert_eq!(last.judgment.grade, JudgeGrade::Decent);
        assert_eq!(last.judgment.time_error_ms, 96.0);
        assert_eq!(last.started_at_screen_s, 12.0);
    }

    #[test]
    fn error_bar_text_uses_10ms_blue_fantastic_threshold() {
        let mut p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_TEXT,
            text_threshold_ms: 10,
            show_fa_plus_window: true,
            ..GameplayErrorBarOptions::default()
        });
        p1.fantastic_options.fa_plus_10ms_blue_window = true;
        p1.fantastic_feedback_options.show_fa_plus_window = true;
        p1.fantastic_feedback_options.fa_plus_10ms_blue_window = true;

        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 8.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(8.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(state.players_runtime.players[0].error_bar_text.is_none());

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 12.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players_runtime.players[0]
            .error_bar_text
            .expect("12ms should exceed Arrow Cloud's 10ms blue window");
        assert_eq!(text.started_at, 4.0);
        assert!(!text.early);
        assert_eq!(text.offset_ms, 12.0);
        assert!(!text.scaled);
    }

    #[test]
    fn text_error_bar_scalable_mode_surfaces_default_window_fantastics() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_TEXT,
            text_scalable: true,
            text_threshold_ms: 10,
            ..GameplayErrorBarOptions::default()
        });

        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 10.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(10.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players_runtime.players[0].error_bar_text.is_none(),
            "10ms exactly should remain hidden"
        );

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: -10.1,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(-10.1, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players_runtime.players[0]
            .error_bar_text
            .expect(">10ms should show even inside the default Fantastic window");
        assert_eq!(text.started_at, 4.0);
        assert!(text.early);
        assert_eq!(text.offset_ms, 10.1);
        assert!(text.scaled);
        assert_eq!(text.scale_start_ms, 10.0);
    }

    #[test]
    fn text_error_bar_scalable_mode_uses_custom_threshold() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_TEXT,
            text_scalable: true,
            text_threshold_ms: 17,
            ..GameplayErrorBarOptions::default()
        });

        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 16.9,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(16.9, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players_runtime.players[0].error_bar_text.is_none(),
            "custom threshold should hide hits at or below the selected ms value"
        );

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 17.1,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(17.1, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players_runtime.players[0]
            .error_bar_text
            .expect("> custom threshold should show inside the default Fantastic window");
        assert!(!text.early);
        assert_eq!(text.offset_ms, 17.1);
        assert!(text.scaled);
        assert_eq!(text.scale_start_ms, 17.0);
    }

    #[test]
    fn text_error_bar_window_mode_preserves_default_threshold() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_TEXT,
            text_threshold_ms: 10,
            ..GameplayErrorBarOptions::default()
        });

        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 12.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            1.0,
        );
        assert!(
            state.players_runtime.players[0].error_bar_text.is_none(),
            "legacy Text mode should keep the active-window threshold"
        );

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 24.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(24.0, 1.0),
                grade: JudgeGrade::Excellent,
                window: Some(TimingWindow::W2),
                miss_because_held: false,
            },
            1.1,
        );

        let text = state.players_runtime.players[0]
            .error_bar_text
            .expect("legacy Text mode should still show hits outside the active window");
        assert_eq!(text.started_at, 4.0);
        assert!(!text.early);
        assert_eq!(text.offset_ms, 24.0);
        assert!(!text.scaled);
    }

    #[test]
    fn average_error_bar_can_show_long_term_only() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_AVERAGE,
            short_average_enabled: false,
            long_average_enabled: true,
            long_average_threshold_ms: 1,
            long_average_intensity: 1.0,
            long_average_min_samples: 4,
            average_interval_ms: 400,
            ..GameplayErrorBarOptions::default()
        });
        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            state.error_bar_register_tap(
                0,
                &Judgment {
                    time_error_ms: 10.0,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(10.0, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        let player = &state.players_runtime.players[0];
        assert!(player.error_bar_avg_bar_started_at.is_none());
        assert!(player.error_bar_avg_ticks.iter().all(Option::is_none));
        assert!(player.error_bar_long_avg_visible);
        let long_tick = player
            .error_bar_long_avg_tick
            .expect("long-only Average should still emit the blue tick");
        assert!((long_tick.offset_s - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn long_error_bar_threshold_accounts_for_intensity() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_AVERAGE,
            short_average_enabled: false,
            long_average_enabled: true,
            long_average_threshold_ms: 4,
            long_average_intensity: 2.0,
            long_average_min_samples: 4,
            average_interval_ms: 400,
            ..GameplayErrorBarOptions::default()
        });
        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            state.error_bar_register_tap(
                0,
                &Judgment {
                    time_error_ms: 2.5,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(2.5, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        let player = &state.players_runtime.players[0];
        assert!(
            player.error_bar_long_avg_visible,
            "intensity should scale the long-term mean past the threshold"
        );
        let long_tick = player
            .error_bar_long_avg_tick
            .expect("blue long-term tick should be emitted once intensity-scaled");
        assert!((long_tick.offset_s - 0.0025).abs() <= 1e-6);
    }

    #[test]
    fn long_error_bar_stays_hidden_below_intensity_scaled_threshold() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_AVERAGE,
            short_average_enabled: false,
            long_average_enabled: true,
            long_average_threshold_ms: 4,
            long_average_intensity: 1.0,
            long_average_min_samples: 4,
            average_interval_ms: 400,
            ..GameplayErrorBarOptions::default()
        });
        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 4.0;

        for i in 0..4 {
            state.error_bar_register_tap(
                0,
                &Judgment {
                    time_error_ms: 2.5,
                    time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(2.5, 1.0),
                    grade: JudgeGrade::Fantastic,
                    window: Some(TimingWindow::W1),
                    miss_because_held: false,
                },
                i as f32 * 0.1,
            );
        }

        assert!(!state.players_runtime.players[0].error_bar_long_avg_visible);
    }

    #[test]
    fn short_average_error_bar_applies_single_sample_correction_after_clamp() {
        let p1 = error_bar_profile(GameplayErrorBarOptions {
            mask_bits: GAMEPLAY_ERROR_BAR_AVERAGE,
            trim: GameplayErrorBarTrim::Off,
            short_average_enabled: true,
            short_average_intensity: 2.0,
            long_average_enabled: false,
            average_interval_ms: 400,
            ..GameplayErrorBarOptions::default()
        });
        let mut state = regression_state_with_profiles([p1, TestProfile::default()]);
        state.boundary.total_elapsed_in_screen = 1.0;
        let max_offset_s = state.timing_runtime.timing_profile.windows_s[4];

        state.error_bar_register_tap(
            0,
            &Judgment {
                time_error_ms: 500.0,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(500.0, 1.0),
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W1),
                miss_because_held: false,
            },
            0.0,
        );

        let tick = state.players_runtime.players[0]
            .error_bar_avg_ticks
            .iter()
            .flatten()
            .next()
            .copied()
            .expect("a single tap should register a short average tick");
        let expected = max_offset_s * 0.75;
        assert!(
            (tick.offset_s - expected).abs() <= 1e-6,
            "single-sample offset {} should equal clamp(offset * intensity) * 0.75 = {}",
            tick.offset_s,
            expected
        );
    }

    #[test]
    fn autosync_row_hits_use_music_time_offsets_at_rate() {
        let mut state = regression_state();
        let row_index = 48usize;
        let autosync_offset_ns = song_time_ns_from_seconds(0.015);

        assert!(state.set_music_rate(1.5));
        state.control.autosync.mode = AutosyncMode::Song;
        state.chart_runtime.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.chart_runtime.notes[0].result = Some(Judgment {
            time_error_ms: -10.0,
            time_error_music_ns: -autosync_offset_ns,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.control.autosync.offset_samples = [autosync_offset_ns; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        state.control.autosync.offset_sample_count = AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        state.apply_autosync_for_row_hits(0);

        assert!((state.song_offset_seconds() - 0.015).abs() <= 1e-6);
        assert_eq!(state.control.autosync.offset_sample_count, 0);
    }

    #[test]
    fn hold_judgment_cleanup_uses_screen_time_boundary() {
        let mut state = regression_state();
        state.boundary.total_elapsed_in_screen = 5.0;
        state.display.hold_feedback.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.201,
        });
        state.tick_visual_effects(0.0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_some());

        state.display.hold_feedback.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.2,
        });
        state.tick_visual_effects(0.0);
        assert!(state.display.hold_feedback.hold_judgments[0].is_none());
    }

    #[test]
    fn held_miss_feedback_records_column_and_cleans_up() {
        let mut state = regression_state();
        state.boundary.total_elapsed_in_screen = 5.0;
        state.chart_runtime.notes = vec![test_note(2, 48, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            48,
            vec![0],
        )];
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];

        state.set_final_note_result(
            0,
            Judgment {
                time_error_ms: 180.0,
                time_error_music_ns: song_time_ns_from_seconds(0.18),
                grade: JudgeGrade::Miss,
                window: None,
                miss_because_held: true,
            },
        );

        assert!(state.display.hold_feedback.held_miss_judgments[0].is_none());
        assert!(state.display.hold_feedback.held_miss_judgments[1].is_none());
        assert_eq!(
            state.display.hold_feedback.held_miss_judgments[2]
                .as_ref()
                .map(|info| info.started_at_screen_s),
            Some(5.0)
        );

        state.display.hold_feedback.held_miss_judgments[2] = Some(HeldMissRenderInfo {
            started_at_screen_s: 5.0 - HELD_MISS_TOTAL_DURATION,
        });
        state.tick_visual_effects(0.0);
        assert!(state.display.hold_feedback.held_miss_judgments[2].is_none());
    }

    #[test]
    fn mine_judgment_feedback_records_result_column_and_time() {
        let mut state = regression_state();
        state.boundary.total_elapsed_in_screen = 9.25;

        state.set_last_mine_judgment(0, 2, MineResult::Avoided);

        let info = state.players_runtime.players[0]
            .last_mine_judgment
            .expect("mine judgment should be recorded");
        assert_eq!(info.result, MineResult::Avoided);
        assert_eq!(info.column, 2);
        assert_eq!(info.started_at_screen_s, 9.25);
    }

    #[test]
    fn course_display_carry_captures_current_life() {
        let mut state = regression_state();
        state.players_runtime.players[0].life = 0.32;

        let carry = state.course_display_carry();

        assert!((carry[0].life - 0.32).abs() <= f32::EPSILON);
    }

    #[test]
    fn autoplay_rows_do_not_record_ex_counts() {
        let mut state = regression_state();
        let row_index = 48usize;
        state.chart_runtime.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];
        state.progress.stage.autoplay_enabled = true;

        state.set_final_note_result(
            0,
            Judgment {
                time_error_ms: 0.0,
                time_error_music_ns: 0,
                grade: JudgeGrade::Fantastic,
                window: Some(TimingWindow::W0),
                miss_because_held: false,
            },
        );
        state.finalize_row_judgment(0, row_index, 0, false);

        assert_eq!(state.live_window_counts(0).w0, 0);
        let blue_window_ms = state.player_blue_window_ms(0);
        assert_eq!(state.display_ex_score_percent(0, blue_window_ms), 0.0);
        assert_eq!(state.display_itg_score_percent(0), 0.0);
        assert!(state.players_runtime.players[0].last_judgment.is_some());
    }

    #[test]
    fn begin_restart_exit_arms_cancel_transition_like_back_out() {
        let mut state = regression_state();
        assert!(state.control.exit_input.exit_transition.is_none());

        state.begin_restart_exit();

        let exit = state
            .control
            .exit_input
            .exit_transition
            .expect("begin_restart_exit should arm an exit_transition");
        assert_eq!(
            exit.kind,
            ExitTransitionKind::Cancel,
            "restart should reuse the fast Cancel out-fade for SL/zmod parity"
        );
        assert_eq!(
            state.drain_audio_commands().collect::<Vec<_>>(),
            vec![GameplayAudioCommand::StopMusic]
        );
    }

    #[test]
    fn gameplay_menu_input_uses_p2_buttons_for_double_p2() {
        let mut state = regression_state_with_session(GameplaySession {
            play_style: GameplayInputPlayStyle::Double,
            player_side: GameplayInputPlayerSide::P2,
            joined_sides: [false, true],
            ..GameplaySession::default()
        });

        handle_core_input(&mut state, &test_input_event(VirtualAction::p1_start));
        assert_eq!(state.control.exit_input.hold_to_exit_key, None);

        handle_core_input(&mut state, &test_input_event(VirtualAction::p2_start));
        assert_eq!(
            state.control.exit_input.hold_to_exit_key,
            Some(HoldToExitKey::Start)
        );
        assert!(state.control.exit_input.hold_to_exit_start.is_some());
    }

    #[test]
    fn gameplay_menu_input_uses_p2_buttons_for_versus() {
        let session = GameplaySession {
            play_style: GameplayInputPlayStyle::Versus,
            player_side: GameplayInputPlayerSide::P1,
            joined_sides: [true, true],
            ..GameplaySession::default()
        };

        let mut start_state = regression_state_with_session(session.clone());
        assert_eq!(start_state.setup.num_players, 2);
        handle_core_input(&mut start_state, &test_input_event(VirtualAction::p2_start));
        assert_eq!(
            start_state.control.exit_input.hold_to_exit_key,
            Some(HoldToExitKey::Start)
        );
        assert!(start_state.control.exit_input.hold_to_exit_start.is_some());

        let mut back_state = regression_state_with_session(session);
        assert_eq!(back_state.setup.num_players, 2);
        handle_core_input(&mut back_state, &test_input_event(VirtualAction::p2_back));
        assert_eq!(
            back_state.control.exit_input.hold_to_exit_key,
            Some(HoldToExitKey::Back)
        );
        assert!(back_state.control.exit_input.hold_to_exit_start.is_some());
    }

    #[test]
    fn gameplay_init_uses_p2_modifiers_for_double_p2() {
        let p1 = TestProfile {
            noteskin_name: "p1-runtime",
            mini_percent: 11.0,
            turn_option: GameplayTurnOption::Mirror,
            ..TestProfile::default()
        };
        let p2 = TestProfile {
            noteskin_name: "p2-runtime",
            mini_percent: 22.0,
            turn_option: GameplayTurnOption::Left,
            ..TestProfile::default()
        };
        let state = regression_state_with_session_profiles_and_scroll(
            GameplaySession {
                play_style: GameplayInputPlayStyle::Double,
                player_side: GameplayInputPlayerSide::P2,
                joined_sides: [false, true],
                ..GameplaySession::default()
            },
            GameplayConfig::default(),
            [p1, p2],
            [
                ScrollSpeedSetting::XMod(1.5),
                ScrollSpeedSetting::CMod(777.0),
            ],
        );

        assert_eq!(state.setup.num_players, 1);
        assert_eq!(
            state.scroll_speed_for_player(0),
            ScrollSpeedSetting::CMod(777.0)
        );
        assert_eq!(
            state.profiles_runtime.profiles[0].noteskin_name,
            "p2-runtime"
        );
        assert_eq!(state.profiles_runtime.profiles[0].mini_percent, 22.0);
        assert_eq!(
            state.profiles_runtime.profiles[0].turn_option,
            GameplayTurnOption::Left
        );
        assert_eq!(state.display.player_color_index, 3);
    }

    #[test]
    fn gameplay_lane_input_keeps_back_hold_active() {
        let mut state = regression_state_with_session(GameplaySession {
            play_style: GameplayInputPlayStyle::Single,
            player_side: GameplayInputPlayerSide::P1,
            joined_sides: [true, false],
            ..GameplaySession::default()
        });

        handle_core_input(&mut state, &test_input_event(VirtualAction::p1_back));
        let hold_start = state.control.exit_input.hold_to_exit_start;

        handle_core_input(
            &mut state,
            &test_input_event_with_source(VirtualAction::p1_left, true, InputSource::Gamepad),
        );
        handle_core_input(
            &mut state,
            &test_input_event_with_source(VirtualAction::p1_left, false, InputSource::Gamepad),
        );

        assert_eq!(
            state.control.exit_input.hold_to_exit_key,
            Some(HoldToExitKey::Back)
        );
        assert_eq!(state.control.exit_input.hold_to_exit_start, hold_start);
        assert_eq!(state.control.exit_input.hold_to_exit_aborted_at, None);
    }

    #[test]
    fn delayed_back_false_exits_song_on_first_press() {
        let mut config = GameplayConfig::default();
        config.delayed_back = false;
        let mut state =
            regression_state_with_session_and_config(GameplaySession::default(), config);

        handle_core_input(&mut state, &test_input_event(VirtualAction::p1_back));

        let exit = state.control.exit_input.exit_transition;
        assert!(
            exit.is_some(),
            "exit_transition should be armed immediately when delayed_back is false"
        );
        assert_eq!(
            exit.unwrap().kind,
            ExitTransitionKind::Cancel,
            "BACK should trigger a Cancel exit transition"
        );
        assert_eq!(
            state.control.exit_input.hold_to_exit_key, None,
            "hold_to_exit_key should remain unset in instant-back mode"
        );
    }

    #[test]
    fn delayed_back_true_preserves_hold_arming() {
        let mut config = GameplayConfig::default();
        config.delayed_back = true;
        let mut state =
            regression_state_with_session_and_config(GameplaySession::default(), config);

        handle_core_input(&mut state, &test_input_event(VirtualAction::p1_back));

        assert_eq!(
            state.control.exit_input.hold_to_exit_key,
            Some(HoldToExitKey::Back)
        );
        assert!(state.control.exit_input.hold_to_exit_start.is_some());
        assert!(
            state.control.exit_input.exit_transition.is_none(),
            "exit_transition should not fire until the hold elapses"
        );
    }

    #[test]
    fn begin_restart_exit_is_idempotent_when_already_exiting() {
        let mut state = regression_state();

        state.begin_exit_transition(ExitTransitionKind::Out);
        let original = state
            .control
            .exit_input
            .exit_transition
            .expect("primed exit");

        state.begin_restart_exit();
        let after = state
            .control
            .exit_input
            .exit_transition
            .expect("still exiting");
        assert_eq!(
            after.kind, original.kind,
            "begin_restart_exit must not overwrite an in-flight exit transition"
        );
    }

    #[test]
    fn positive_song_offset_delta_moves_notes_earlier_like_global_offset() {
        let mut song_state = regression_state();
        let mut global_state = regression_state();

        let song_offset_before = song_state.song_offset_seconds();
        let global_offset_before = global_state.global_offset_seconds();
        let song_before = song_state.chart_runtime.note_time_cache_ns[0];
        let global_before = global_state.chart_runtime.note_time_cache_ns[0];

        assert!(song_state.apply_song_offset_delta(0.010));
        assert!(global_state.apply_global_offset_delta(0.010));

        let song_after = song_state.chart_runtime.note_time_cache_ns[0];
        let global_after = global_state.chart_runtime.note_time_cache_ns[0];
        let expected_delta_ns = song_time_ns_from_seconds(0.010);
        let song_delta_ns = song_before - song_after;
        let global_delta_ns = global_before - global_after;

        assert!((song_state.song_offset_seconds() - (song_offset_before + 0.010)).abs() <= 1e-6);
        assert!(
            (global_state.global_offset_seconds() - (global_offset_before + 0.010)).abs() <= 1e-6
        );
        assert!((song_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((global_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((song_delta_ns - global_delta_ns).abs() <= 1);
    }

    #[test]
    fn global_offset_delta_preserves_player_shift() {
        let mut state = regression_state();
        let shift = 0.015_f32;

        state
            .clock
            .offsets
            .set_player_global_offset_shift_seconds(0, shift);
        let effective_offset = state
            .clock
            .offsets
            .effective_player_global_offset_seconds(0);
        Arc::make_mut(&mut state.timing_runtime.timing_players[0])
            .set_global_offset_seconds(effective_offset);
        state.refresh_timing_after_offset_change();

        let machine_before = state.global_offset_seconds();
        let effective_before = state.effective_player_global_offset_seconds(0);
        let note_before = state.chart_runtime.note_time_cache_ns[0];

        assert!((effective_before - (machine_before + shift)).abs() <= 1e-6);
        assert!(state.apply_global_offset_delta(0.010));

        let effective_after = state.effective_player_global_offset_seconds(0);
        let note_after = state.chart_runtime.note_time_cache_ns[0];

        assert!((state.global_offset_seconds() - (machine_before + 0.010)).abs() <= 1e-6);
        assert!((effective_after - (state.global_offset_seconds() + shift)).abs() <= 1e-6);
        assert_eq!(note_before - note_after, song_time_ns_from_seconds(0.010));
    }

    #[test]
    fn synced_raw_modifier_makes_first_offset_key_use_global_offset() {
        let mut state = regression_state();

        let song_before = state.song_offset_seconds();
        let global_before = state.global_offset_seconds();

        state.set_raw_modifier_state(true, false);
        let _ = state.handle_queued_raw_key_input(
            GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
            None,
            true,
            Instant::now(),
            0.0,
            true,
        );

        assert!((state.song_offset_seconds() - song_before).abs() <= 1e-6);
        assert!(
            (state.global_offset_seconds() - (global_before + OFFSET_ADJUST_STEP_SECONDS)).abs()
                <= 1e-6
        );
    }

    #[test]
    fn timing_tick_key_queues_session_command() {
        let mut state = regression_state();

        let action = state.handle_queued_raw_key_input(
            GameplayRawKeyInput::TimingTick,
            None,
            true,
            Instant::now(),
            0.0,
            true,
        );

        assert!(matches!(action, RawKeyAction::None));
        assert_eq!(state.timing_tick_status_line(), Some("Assist Tick"));
        assert_eq!(
            state.drain_session_commands().collect::<Vec<_>>(),
            vec![GameplaySessionCommand::SetTimingTickMode(
                GameplayTimingTickMode::Assist
            )]
        );
    }

    #[test]
    fn tap_explosion_mask_disables_selected_tap_window() {
        let column = 1usize;
        let enabled_profile = TestProfile {
            tap_explosion_options: all_tap_explosion_options(),
            ..TestProfile::default()
        };
        let mut enabled =
            regression_state_with_profiles(std::array::from_fn(|_| enabled_profile.clone()));
        enable_tap_explosion_durations(&mut enabled);
        enabled.spawn_tap_explosion_for_grade(column, JudgeGrade::Great, false);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let disabled_profile = TestProfile {
            tap_explosion_options: TapExplosionOptions {
                great: false,
                ..all_tap_explosion_options()
            },
            ..TestProfile::default()
        };
        let mut disabled =
            regression_state_with_profiles(std::array::from_fn(|_| disabled_profile.clone()));
        enable_tap_explosion_durations(&mut disabled);
        disabled.spawn_tap_explosion_for_grade(column, JudgeGrade::Great, false);
        assert!(disabled.display.visual_feedback.tap_explosions[column].is_none());
    }

    #[test]
    fn column_flash_mask_gates_completed_row_flash() {
        let row_index = 48usize;
        let column = 1usize;
        let build_state = |options| {
            let profile = TestProfile {
                column_flash_options: options,
                ..TestProfile::default()
            };
            let mut state =
                regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
            set_single_judged_tap(&mut state, column, row_index, JudgeGrade::Great, 0.0);
            state
        };

        let mut disabled = build_state(ColumnFlashOptions {
            enabled: true,
            excellent: true,
            ..ColumnFlashOptions::default()
        });
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());
        let judged = disabled.display.visual_feedback.last_tap_judgments[column]
            .expect("masked column flash should still record the ungated tap judgment");
        assert_eq!(judged.grade, JudgeGrade::Great);

        let mut enabled = build_state(ColumnFlashOptions {
            enabled: true,
            great: true,
            ..ColumnFlashOptions::default()
        });
        enabled.trigger_completed_row_tap_explosions(0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("Great should trigger column flash");
        assert_eq!(flash.grade, JudgeGrade::Great);
    }

    #[test]
    fn mine_hit_records_screen_time_and_refreshes_on_rehit() {
        let column = 1usize;
        let mut state = regression_state();
        state.setup.config.mine_hit_sound = false;

        state.boundary.total_elapsed_in_screen = 1.0;
        state.trigger_mine_explosion(column);
        let first = state.display.visual_feedback.mine_explosions[column]
            .as_ref()
            .expect("mine hit should set an explosion");
        assert_eq!(first.started_at_screen_s, 1.0);

        state.boundary.total_elapsed_in_screen = 1.5;
        state.trigger_mine_explosion(column);
        let second = state.display.visual_feedback.mine_explosions[column]
            .as_ref()
            .expect("re-hit should keep an explosion");
        assert_eq!(second.started_at_screen_s, 1.5);
    }

    fn fantastic_row_state(
        options: ColumnFlashOptions,
        time_error_ms: f32,
        window: TimingWindow,
    ) -> (State, usize, usize) {
        let row_index = 48usize;
        let column = 1usize;
        let profile = TestProfile {
            column_flash_options: options,
            fantastic_feedback_options: FantasticFeedbackOptions {
                show_fa_plus_window: true,
                ..FantasticFeedbackOptions::default()
            },
            ..TestProfile::default()
        };
        let mut state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        set_single_judged_tap(
            &mut state,
            column,
            row_index,
            JudgeGrade::Fantastic,
            time_error_ms,
        );
        state.chart_runtime.notes[0]
            .result
            .as_mut()
            .expect("test note should carry a judgment")
            .window = Some(window);
        (state, row_index, column)
    }

    #[test]
    fn white_fantastic_column_flash_uses_only_white_mask() {
        let (mut disabled, row_index, column) = fantastic_row_state(
            ColumnFlashOptions {
                enabled: true,
                blue_fantastic: true,
                ..ColumnFlashOptions::default()
            },
            18.0,
            TimingWindow::W1,
        );
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            ColumnFlashOptions {
                enabled: true,
                white_fantastic: true,
                ..ColumnFlashOptions::default()
            },
            18.0,
            TimingWindow::W1,
        );
        enabled.trigger_completed_row_tap_explosions(0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("white Fantastic should flash");
        assert_eq!(flash.grade, JudgeGrade::Fantastic);
        assert!(!flash.blue_fantastic);
    }

    #[test]
    fn blue_fantastic_column_flash_uses_only_blue_mask() {
        let (mut disabled, row_index, column) = fantastic_row_state(
            ColumnFlashOptions {
                enabled: true,
                white_fantastic: true,
                ..ColumnFlashOptions::default()
            },
            4.0,
            TimingWindow::W0,
        );
        disabled.trigger_completed_row_tap_explosions(0, row_index);
        assert!(disabled.display.visual_feedback.column_flashes[column].is_none());

        let (mut enabled, row_index, column) = fantastic_row_state(
            ColumnFlashOptions {
                enabled: true,
                blue_fantastic: true,
                ..ColumnFlashOptions::default()
            },
            4.0,
            TimingWindow::W0,
        );
        enabled.trigger_completed_row_tap_explosions(0, row_index);
        let flash = enabled.display.visual_feedback.column_flashes[column]
            .expect("blue Fantastic should flash");
        assert_eq!(flash.grade, JudgeGrade::Fantastic);
        assert!(flash.blue_fantastic);
    }

    #[test]
    fn early_dw_column_flash_hide_is_independent_from_notefield_flash() {
        let column = 1usize;
        let judgment = Judgment {
            time_error_ms: -120.0,
            time_error_music_ns: song_time_ns_from_seconds(-0.12),
            grade: JudgeGrade::Decent,
            window: Some(TimingWindow::W4),
            miss_because_held: false,
        };
        let profile = TestProfile {
            column_flash_options: ColumnFlashOptions {
                enabled: true,
                decent: true,
                ..ColumnFlashOptions::default()
            },
            tap_explosion_options: all_tap_explosion_options(),
            ..TestProfile::default()
        };
        let build_state = || {
            let mut state =
                regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
            enable_tap_explosion_durations(&mut state);
            state
        };

        let mut hide_notefield = build_state();
        hide_notefield.render_provisional_early_rescore_feedback(
            0, column, &judgment, 1.0, true, true, false,
        );
        assert!(hide_notefield.display.visual_feedback.tap_explosions[column].is_none());
        assert!(hide_notefield.display.visual_feedback.column_flashes[column].is_some());

        let mut hide_column = build_state();
        hide_column.render_provisional_early_rescore_feedback(
            0, column, &judgment, 1.0, true, false, true,
        );
        assert!(hide_column.display.visual_feedback.tap_explosions[column].is_some());
        assert!(hide_column.display.visual_feedback.column_flashes[column].is_none());
    }

    #[test]
    fn tap_explosion_mask_disables_held_success_flash() {
        let column = 1usize;
        let enabled_profile = TestProfile {
            tap_explosion_options: all_tap_explosion_options(),
            ..TestProfile::default()
        };
        let mut enabled =
            regression_state_with_profiles(std::array::from_fn(|_| enabled_profile.clone()));
        enable_tap_explosion_durations(&mut enabled);
        enabled.trigger_hold_explosion(column);
        assert!(enabled.display.visual_feedback.tap_explosions[column].is_some());

        let disabled_profile = TestProfile {
            tap_explosion_options: TapExplosionOptions {
                held: false,
                ..all_tap_explosion_options()
            },
            ..TestProfile::default()
        };
        let mut disabled =
            regression_state_with_profiles(std::array::from_fn(|_| disabled_profile.clone()));
        enable_tap_explosion_durations(&mut disabled);
        disabled.trigger_hold_explosion(column);
        assert!(disabled.display.visual_feedback.tap_explosions[column].is_none());
    }

    #[test]
    fn white_fantastic_row_uses_bright_tap_explosion() {
        let profile = TestProfile {
            tap_explosion_options: all_tap_explosion_options(),
            fantastic_feedback_options: FantasticFeedbackOptions {
                show_fa_plus_window: true,
                ..FantasticFeedbackOptions::default()
            },
            ..TestProfile::default()
        };
        let mut state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        enable_tap_explosion_durations(&mut state);
        let row_index = 48usize;
        let column = 1usize;
        set_single_judged_tap(&mut state, column, row_index, JudgeGrade::Fantastic, 18.0);
        state.chart_runtime.notes[0].result.as_mut().unwrap().window = Some(TimingWindow::W1);

        state.trigger_completed_row_tap_explosions(0, row_index);

        let active = state.display.visual_feedback.tap_explosions[column]
            .expect("white Fantastic should flash");
        assert_eq!(active.window, "W1");
        assert!(active.bright);
    }

    #[test]
    fn blue_fantastic_row_uses_dim_tap_explosion() {
        let profile = TestProfile {
            tap_explosion_options: all_tap_explosion_options(),
            fantastic_feedback_options: FantasticFeedbackOptions {
                show_fa_plus_window: true,
                ..FantasticFeedbackOptions::default()
            },
            ..TestProfile::default()
        };
        let mut state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        enable_tap_explosion_durations(&mut state);
        let row_index = 48usize;
        let column = 1usize;
        set_single_judged_tap(&mut state, column, row_index, JudgeGrade::Fantastic, 4.0);
        state.chart_runtime.notes[0].result.as_mut().unwrap().window = Some(TimingWindow::W0);

        state.trigger_completed_row_tap_explosions(0, row_index);

        let active = state.display.visual_feedback.tap_explosions[column]
            .expect("blue Fantastic should flash");
        assert_eq!(active.window, "W1");
        assert!(!active.bright);
    }

    #[test]
    fn ten_ms_blue_window_uses_bright_tap_explosion_above_10ms() {
        let profile = TestProfile {
            fantastic_feedback_options: FantasticFeedbackOptions {
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: true,
                ..FantasticFeedbackOptions::default()
            },
            ..TestProfile::default()
        };
        let state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        let judgment = Judgment {
            time_error_ms: 12.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            miss_because_held: false,
        };

        assert!(state.tap_judgment_uses_bright_explosion(0, &judgment));
    }

    #[test]
    fn split_15_10ms_keeps_dim_tap_explosion_above_10ms() {
        let profile = TestProfile {
            fantastic_feedback_options: FantasticFeedbackOptions {
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: true,
                split_15_10ms: true,
                ..FantasticFeedbackOptions::default()
            },
            ..TestProfile::default()
        };
        let state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        let judgment = Judgment {
            time_error_ms: 12.0,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(12.0, 1.0),
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W0),
            miss_because_held: false,
        };

        assert!(!state.tap_judgment_uses_bright_explosion(0, &judgment));
    }

    #[test]
    fn synthetic_receptor_step_survives_until_lift() {
        let mut state = regression_state();
        let column = 0usize;

        state.trigger_receptor_step_pulse(column);
        let started_press = state.display.receptor_feedback.glow_press_timers[column];
        if started_press <= f32::EPSILON {
            assert!(state.display.receptor_feedback.bop_timers[column] > 0.0);
            return;
        }
        state.tick_visual_effects(0.01);

        if started_press > 0.01 {
            assert!(state.display.receptor_feedback.glow_press_timers[column] > 0.0);
            assert!(state.display.receptor_feedback.glow_press_timers[column] < started_press);
        }
        state.tick_visual_effects(started_press.max(0.01));

        assert_eq!(
            state.display.receptor_feedback.glow_press_timers[column],
            0.0
        );
        assert!(state.display.receptor_feedback.glow_lift_timers[column] > 0.0);
    }

    #[test]
    fn hidden_song_lua_tap_steps_receptor_without_core_flash() {
        let mut state = regression_state();
        let row_index = 48usize;
        let column = 1usize;
        enable_tap_explosion_durations(&mut state);
        state.trigger_receptor_step_pulse(0);
        let supports_press_tween =
            state.display.receptor_feedback.glow_press_timers[0] > f32::EPSILON;
        state.display.receptor_feedback.glow_press_timers.fill(0.0);
        state.display.receptor_feedback.glow_lift_timers.fill(0.0);
        state.display.receptor_feedback.bop_timers.fill(0.0);
        set_single_judged_tap(&mut state, column, row_index, JudgeGrade::Great, 0.0);
        state.mods.song_lua_visuals.note_hides[0].push(SongLuaNoteHideWindowRuntime {
            column,
            start_beat: 0.0,
            end_beat: 2.0,
        });

        state.trigger_completed_row_tap_explosions(0, row_index);

        assert!(state.display.visual_feedback.tap_explosions[column].is_none());
        assert_eq!(state.display.receptor_feedback.bop_timers[column], 0.0);
        assert_eq!(
            state.display.receptor_feedback.bop_behaviors[column].duration,
            0.0
        );
        if supports_press_tween {
            assert!(state.display.receptor_feedback.glow_press_timers[column] > 0.0);
            assert_eq!(
                state.display.receptor_feedback.glow_lift_timers[column],
                0.0
            );
        }
    }

    #[test]
    fn visible_tap_hit_uses_score_receptor_command_with_core_flash() {
        let profile = TestProfile {
            tap_explosion_options: all_tap_explosion_options(),
            ..TestProfile::default()
        };
        let mut state = regression_state_with_profiles(std::array::from_fn(|_| profile.clone()));
        let row_index = 48usize;
        let column = 1usize;
        let note_time = song_time_ns_from_seconds(1.0);
        enable_tap_explosion_durations(&mut state);
        state.chart_runtime.notes = vec![test_note(column, row_index, NoteType::Tap)];
        state.chart_runtime.note_time_cache_ns = vec![note_time];
        state.chart_runtime.hold_end_time_cache_ns = vec![Some(note_time)];
        for col in 0..MAX_COLS {
            state.chart_runtime.lane_indices.note_indices[col].clear();
            state.chart_runtime.lane_indices.note_row_indices[col].clear();
            state.chart_runtime.lane_indices.hold_indices[col].clear();
        }
        state.chart_runtime.lane_indices.note_indices[column].push(0);
        state.chart_runtime.lane_indices.note_row_indices[column].push(0);
        state.chart_runtime.row_indices.note_row_entry_indices = vec![0];
        state.chart_runtime.row_entries = vec![test_row_entry_with_times(
            &state.chart_runtime.notes,
            &state.chart_runtime.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.chart_runtime.row_indices.row_entry_ranges = [(0, 1), (0, 0)];
        state.chart_runtime.row_indices.row_map_cache =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        state.chart_runtime.row_indices.row_map_cache[0][row_index] = 0;

        assert!(state.judge_a_tap(column, note_time));
        assert!(state.display.visual_feedback.tap_explosions[column].is_some());
        assert_eq!(state.display.receptor_feedback.bop_timers[column], 0.0);
        assert_eq!(
            state.display.receptor_feedback.bop_behaviors[column].duration,
            0.0
        );
    }

    #[test]
    fn score_valid_rejects_nohands_when_chart_has_hands() {
        let profile = TestProfile {
            remove_mask_bits: REMOVE_MASK_BIT_NO_HANDS,
            ..TestProfile::default()
        };
        let chart = test_chart(
            ArrowStats {
                hands: 4,
                ..ArrowStats::default()
            },
            TimingSegments::default(),
            None,
        );

        assert!(
            !score_invalid_reason_lines_for_chart(
                &chart,
                &profile,
                ScrollSpeedSetting::default(),
                1.0
            )
            .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_turn_options_rankable() {
        let profile = TestProfile {
            turn_option: GameplayTurnOption::Mirror,
            ..TestProfile::default()
        };
        let chart = test_chart(ArrowStats::default(), TimingSegments::default(), None);

        assert!(
            score_invalid_reason_lines_for_chart(
                &chart,
                &profile,
                ScrollSpeedSetting::default(),
                1.0
            )
            .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_cmod_rankable_on_timing_changes() {
        let profile = TestProfile::default();
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments {
                bpms: vec![(0.0, 120.0), (32.0, 128.5)],
                ..TimingSegments::default()
            },
            None,
        );

        assert!(
            score_invalid_reason_lines_for_chart(
                &chart,
                &profile,
                ScrollSpeedSetting::CMod(600.0),
                1.0
            )
            .is_empty()
        );
    }

    #[test]
    fn score_valid_rejects_disabled_chart_attacks() {
        let profile = TestProfile {
            attack_mode: GameplayAttackMode::Off,
            ..TestProfile::default()
        };
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments::default(),
            Some("TIME=1.0:LEN=2.0:MODS=mirror"),
        );

        assert!(
            !score_invalid_reason_lines_for_chart(
                &chart,
                &profile,
                ScrollSpeedSetting::default(),
                1.0
            )
            .is_empty()
        );
    }

    #[test]
    fn chart_attack_sudden_offset_approaches_instead_of_snapping() {
        let mut state = regression_state();
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some(
                "TIME=0.000:LEN=3.000:MODS=*1000 sudden,*1000 -125% suddenoffset\
                 :TIME=0.083:LEN=3.000:MODS=*2.4 150% suddenoffset",
            ),
            GameplayAttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 0.01;
        refresh_active_attack_masks(&mut state, 0.01);
        let start = state.effective_appearance_effects_for_player(0);
        assert!((start.sudden - 1.0).abs() <= 1e-6);
        assert!((start.sudden_offset + 1.25).abs() <= 1e-6);

        state.clock.visible_timing.current_music_time[0] = 0.10;
        refresh_active_attack_masks(&mut state, 0.09);
        let mid = state.effective_appearance_effects_for_player(0);
        assert!(mid.sudden_offset > -1.25);
        assert!(mid.sudden_offset < 1.5);

        state.clock.visible_timing.current_music_time[0] = 1.10;
        refresh_active_attack_masks(&mut state, 1.0);
        let late = state.effective_appearance_effects_for_player(0);
        assert!(late.sudden_offset > mid.sudden_offset);
        assert!(late.sudden_offset < 1.5);
    }

    #[test]
    fn chart_attack_runtime_mods_stop_after_len() {
        let mut state = regression_state();
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=1.000:MODS=50% drunk"),
            GameplayAttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let visual = effective_visual_effects_for_player(&state, 0);
        assert!(visual.drunk.abs() <= 0.000_1);
    }

    #[test]
    fn outro_attack_clear_keeps_player_rotationz_eases_alive() {
        let mut state = regression_state();
        let timing = song_lua_test_timing();
        let eases = vec![song_lua_beat_ease(
            1.0,
            1.0,
            SongLuaRuntimeEaseTargetOwned::Player(SongLuaEaseMaskTarget::PlayerRotationZ),
            0.0,
            5.0,
            Some(4.0),
        )];
        let (windows, unsupported) =
            build_song_lua_ease_windows_for_player(&eases, &timing, 0, 0.0, &[], |_| {});
        assert_eq!(unsupported, 0);
        state.mods.attacks.song_lua_ease_windows[0] = windows;
        state.clock.visible_timing.current_music_time[0] = 2.5;

        state.begin_outro_attack_clear();
        refresh_active_attack_masks(&mut state, 0.0);

        assert!((state.mods.song_lua_player_transforms[0].rotation_z - 5.0).abs() <= 0.0001);
    }

    #[test]
    fn song_lua_active_reset_cuts_overlapping_ease_tail() {
        let mut state = regression_state();
        let timing = song_lua_test_timing();
        let constants = build_song_lua_constant_windows_for_player(
            &[],
            &[song_lua_beat_mod(
                0.0,
                999.0,
                SongLuaRuntimeSpanMode::Len,
                "*1 0 Stealth, *1 0 PulseOuter",
            )],
            &timing,
            0,
            0.0,
        );
        let eases = vec![
            song_lua_beat_ease(
                4.0,
                2.0,
                SongLuaRuntimeEaseTargetOwned::Mod("Stealth".to_string()),
                0.0,
                45.0,
                None,
            ),
            song_lua_beat_ease(
                4.0,
                2.0,
                SongLuaRuntimeEaseTargetOwned::Mod("PulseOuter".to_string()),
                0.0,
                80.0,
                None,
            ),
            song_lua_beat_ease(
                4.0,
                2.0,
                SongLuaRuntimeEaseTargetOwned::Mod("PulsePeriod".to_string()),
                0.0,
                -80.0,
                None,
            ),
        ];
        let (windows, unsupported) =
            build_song_lua_ease_windows_for_player(&eases, &timing, 0, 0.0, &constants, |_| {});

        assert_eq!(unsupported, 0);
        let stealth = windows
            .iter()
            .find(|window| matches!(window.target, SongLuaEaseMaskTarget::AppearanceStealth))
            .unwrap();
        let pulse_outer = windows
            .iter()
            .find(|window| matches!(window.target, SongLuaEaseMaskTarget::VisualPulseOuter))
            .unwrap();
        let pulse_period = windows
            .iter()
            .find(|window| matches!(window.target, SongLuaEaseMaskTarget::VisualPulsePeriod))
            .unwrap();

        assert_eq!(stealth.sustain_end_second, 6.0);
        assert_eq!(pulse_outer.sustain_end_second, 6.0);
        assert_eq!(pulse_period.sustain_end_second, f32::MAX);
        assert!(song_lua_ease_window_value(stealth, 5.0).is_some());
        assert!(song_lua_ease_window_value(pulse_outer, 5.0).is_some());
        assert!(song_lua_ease_window_value(stealth, 6.25).is_none());
        assert!(song_lua_ease_window_value(pulse_outer, 6.25).is_none());

        state.mods.attacks.mask_windows[0] = constants;
        state.mods.attacks.song_lua_ease_windows[0] = windows;
        state.clock.visible_timing.current_music_time[0] = 5.99;
        refresh_active_attack_masks(&mut state, 0.0);
        let eased_stealth = state.effective_appearance_effects_for_player(0).stealth;
        assert!(eased_stealth > 0.4);
        assert!(effective_visual_effects_for_player(&state, 0).pulse_outer > 0.0);

        state.clock.visible_timing.current_music_time[0] = 6.016;
        refresh_active_attack_masks(&mut state, 0.026);
        let fading_stealth = state.effective_appearance_effects_for_player(0).stealth;
        assert!(fading_stealth > 0.0);
        assert!(fading_stealth < eased_stealth);

        state.clock.visible_timing.current_music_time[0] = 7.0;
        refresh_active_attack_masks(&mut state, 0.984);
        assert!(
            state
                .effective_appearance_effects_for_player(0)
                .stealth
                .abs()
                <= 0.000_1
        );
        assert!(
            effective_visual_effects_for_player(&state, 0)
                .pulse_outer
                .abs()
                <= 0.000_1
        );
    }

    #[test]
    fn song_lua_constant_mods_persist_after_attack_window() {
        let mut state = regression_state();
        let timing = song_lua_test_timing();
        state.mods.attacks.mask_windows[0] = build_song_lua_constant_windows_for_player(
            &[],
            &[song_lua_beat_mod(
                0.0,
                1.0,
                SongLuaRuntimeSpanMode::Len,
                "*100 314 confusionoffset",
            )],
            &timing,
            0,
            0.0,
        );

        state.clock.visible_timing.current_music_time[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let visual = effective_visual_effects_for_player(&state, 0);
        assert!((visual.confusion_offset - 3.14).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_constant_visual_scroll_and_mini_mods_approach() {
        let mut state = regression_state();
        let timing = song_lua_test_timing();
        state.mods.attacks.mask_windows[0] = build_song_lua_constant_windows_for_player(
            &[],
            &[song_lua_beat_mod(
                0.0,
                3.0,
                SongLuaRuntimeSpanMode::Len,
                "*10 50% flip, *10 10% reverse, *10 -100% mini",
            )],
            &timing,
            0,
            0.0,
        );

        state.clock.visible_timing.current_music_time[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let visual = effective_visual_effects_for_player(&state, 0);
        let scroll = effective_scroll_effects_for_player(&state, 0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(visual.flip > 0.0);
        assert!(visual.flip < 0.5);
        assert!((scroll.reverse - 0.1).abs() <= 0.000_1);
        assert!(mini < 0.0);
        assert!(mini > -100.0);

        state.clock.visible_timing.current_music_time[0] = 1.016;
        refresh_active_attack_masks(&mut state, 1.0);
        let visual = effective_visual_effects_for_player(&state, 0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((visual.flip - 0.5).abs() <= 0.000_1);
        assert!((mini + 100.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_constant_mini_layers_on_profile_mini() {
        let mut state = regression_state_with_profiles(std::array::from_fn(|player| TestProfile {
            mini_percent: if player == 0 { 50.0 } else { 0.0 },
            ..TestProfile::default()
        }));
        let timing = song_lua_test_timing();
        state.mods.attacks.mask_windows[0] = build_song_lua_constant_windows_for_player(
            &[],
            &[song_lua_beat_mod(
                0.0,
                3.0,
                SongLuaRuntimeSpanMode::Len,
                "*10 -100% mini",
            )],
            &timing,
            0,
            0.0,
        );

        state.clock.visible_timing.current_music_time[0] = 0.016;
        refresh_active_attack_masks(&mut state, 0.016);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini - 34.0).abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = 1.016;
        refresh_active_attack_masks(&mut state, 1.0);
        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini + 50.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_eased_mini_layers_on_profile_mini() {
        let mut state = regression_state_with_profiles(std::array::from_fn(|player| TestProfile {
            mini_percent: if player == 0 { 50.0 } else { 0.0 },
            ..TestProfile::default()
        }));
        let timing = song_lua_test_timing();
        let eases = vec![song_lua_beat_ease(
            0.0,
            4.0,
            SongLuaRuntimeEaseTargetOwned::Mod("mini".to_string()),
            0.0,
            -100.0,
            None,
        )];
        let (windows, unsupported) =
            build_song_lua_ease_windows_for_player(&eases, &timing, 0, 0.0, &[], |_| {});
        assert_eq!(unsupported, 0);
        state.mods.attacks.song_lua_ease_windows[0] = windows;

        state.clock.visible_timing.current_music_time[0] = 2.0;
        refresh_active_attack_masks(&mut state, 0.0);

        let mini = effective_mini_percent_for_player(&state, 0);
        assert!(mini.abs() <= 0.000_1);
    }

    #[test]
    fn chart_attack_mini_overrides_profile_mini() {
        let mut state = regression_state_with_profiles(std::array::from_fn(|player| TestProfile {
            mini_percent: if player == 0 { 50.0 } else { 0.0 },
            ..TestProfile::default()
        }));
        state.mods.attacks.mask_windows[0] = build_attack_mask_windows_for_player(
            Some("TIME=0.000:LEN=3.000:MODS=*1000 25% mini"),
            GameplayAttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.clock.visible_timing.current_music_time[0] = 1.0;
        refresh_active_attack_masks(&mut state, 1.0);

        let mini = effective_mini_percent_for_player(&state, 0);
        assert!((mini - 25.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_active_reset_overrides_ended_constant_mods() {
        let mut state = regression_state();
        let timing = song_lua_test_timing();
        state.mods.attacks.mask_windows[0] = build_song_lua_constant_windows_for_player(
            &[],
            &[
                song_lua_beat_mod(
                    0.0,
                    9999.0,
                    SongLuaRuntimeSpanMode::End,
                    "*1000 no invert, *1000 no flip",
                ),
                song_lua_beat_mod(0.25, 0.25, SongLuaRuntimeSpanMode::Len, "*1000 invert"),
                song_lua_beat_mod(0.5, 0.25, SongLuaRuntimeSpanMode::Len, "*1000 flip"),
            ],
            &timing,
            0,
            0.0,
        );

        state.clock.visible_timing.current_music_time[0] = 0.6;
        refresh_active_attack_masks(&mut state, 0.0);
        let visual = effective_visual_effects_for_player(&state, 0);
        assert!((visual.flip - 1.0).abs() <= 0.000_1);
        assert!(visual.invert.abs() <= 0.000_1);

        state.clock.visible_timing.current_music_time[0] = 1.1;
        refresh_active_attack_masks(&mut state, 0.0);
        let reset = effective_visual_effects_for_player(&state, 0);
        assert!(reset.flip.abs() <= 0.000_1);
        assert!(reset.invert.abs() <= 0.000_1);
    }

    #[test]
    fn outro_attack_clear_phases_out_song_lua_visual_mods() {
        let mut state = regression_state();
        state.mods.attacks.visual[0].confusion_offset = Some(-12.56);
        state.mods.attacks.visual[0].tipsy = Some(0.75);
        state.mods.attacks.visibility[0].dark = Some(1.0);

        state.begin_outro_attack_clear();
        refresh_active_attack_masks(&mut state, 0.5);

        let visual = effective_visual_effects_for_player(&state, 0);
        let visibility = state.effective_visibility_effects_for_player(0);
        assert!(visual.confusion_offset > -12.56);
        assert!(visual.confusion_offset < -12.0);
        assert!(visual.tipsy > 0.0);
        assert!(visual.tipsy < 0.75);
        assert!((visibility.dark - 1.0).abs() <= 0.0001);

        refresh_active_attack_masks(&mut state, 20.0);

        let cleared = effective_visual_effects_for_player(&state, 0);
        let visibility = state.effective_visibility_effects_for_player(0);
        assert!(cleared.confusion_offset.abs() <= 0.0001);
        assert!(cleared.tipsy.abs() <= 0.0001);
        assert!(state.mods.attacks.visual[0].confusion_offset.is_none());
        assert!(state.mods.attacks.visual[0].tipsy.is_none());
        assert!((visibility.dark - 1.0).abs() <= 0.0001);
    }
}
