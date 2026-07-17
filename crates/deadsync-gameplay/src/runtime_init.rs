pub fn init_gameplay_runtime<
    Profile,
    BuildSongLuaRuntime,
    OverlayActor,
    CapturedActor,
    StateDelta,
>(
    song: Arc<SongData>,
    charts: [Arc<ChartData>; MAX_PLAYERS],
    gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    viewport: GameplayViewport,
    session: GameplaySession,
    config: GameplayConfig,
    pack_sync_pref: SyncPref,
    mini_indicator_data: GameplayMiniIndicatorData,
    noteskin_data: GameplayNoteskinData,
    build_song_lua_runtime: BuildSongLuaRuntime,
    build_crossover_annotations: CrossoverAnnotationBuilder,
    active_color_index: i32,
    music_rate: f32,
    mut scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    mut player_profiles: [Profile; MAX_PLAYERS],
    replay_edges: Option<Vec<ReplayInputEdge>>,
    replay_offsets: Option<ReplayOffsetSnapshot>,
    lead_in_timing: Option<LeadInTiming>,
    course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    course_display_timing: Option<CourseDisplayTiming>,
    mut combo_carry: [u32; MAX_PLAYERS],
) -> GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
    BuildSongLuaRuntime: SongLuaRuntimeBuilder<OverlayActor, CapturedActor, StateDelta>,
{
    log::debug!("Initializing Gameplay Screen...");
    let init_started = Instant::now();
    let rate = normalized_song_rate(music_rate);

    let play_style = session.play_style;
    let p2_runtime_player = session.p2_runtime_player();
    let cols_per_player = play_style.cols_per_player();
    let num_players = play_style.player_count();
    let num_cols = play_style.total_cols();
    let replay_edges = replay_edges.unwrap_or_default();
    let mut charts = charts;
    let mut gameplay_charts = gameplay_charts;
    if p2_runtime_player {
        scroll_speed[0] = scroll_speed[1];
        player_profiles[0] = player_profiles[1].clone();
        charts[0] = charts[1].clone();
        gameplay_charts[0] = gameplay_charts[1].clone();
        combo_carry[0] = combo_carry[1];
    }
    let player_color_index = if p2_runtime_player {
        active_color_index - 2
    } else {
        active_color_index
    };

    let GameplayNoteskinData {
        effects: noteskin_effects,
    } = noteskin_data;

    let field_zoom: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 1.0;
        }
        let visual_mask = player_profiles[player].visual_mask_bits();
        let mini_value = effective_mini_value_with_visual_mask(
            &player_profiles[player],
            visual_mask,
            player_profiles[player].mini_percent(),
        );
        let mut z = 1.0 - mini_value * 0.5;
        if z.abs() < 0.01 {
            z = 0.01;
        }
        z
    });

    let pack_sync_offset_seconds = if config.machine_pack_ini_offsets {
        sync_pref_offset(pack_sync_pref, config.machine_default_sync_pref)
    } else {
        0.0
    };
    let player_global_offset_shift_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if !config.machine_allow_per_player_global_offsets || player >= num_players {
            return 0.0;
        }
        player_profiles[player]
            .global_offset_shift_ms()
            .clamp(-100, 100) as f32
            / 1000.0
    });
    let mut timing_base = gameplay_charts[0].timing.clone();
    timing_base.shift_song_offset_seconds(pack_sync_offset_seconds);
    timing_base.set_global_offset_seconds(config.global_offset_seconds);
    let timing = Arc::new(timing_base);
    let mut timing_players: [Arc<TimingData>; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut t = gameplay_charts[player].timing.clone();
        t.shift_song_offset_seconds(pack_sync_offset_seconds);
        t.set_global_offset_seconds(
            config.global_offset_seconds + player_global_offset_shift_seconds[player],
        );
        Arc::new(t)
    });
    if num_players == 1 {
        timing_players[1] = timing_players[0].clone();
    }
    let replay_offsets = replay_offsets.unwrap_or(ReplayOffsetSnapshot {
        beat0_time_ns: timing_players[0].get_time_for_beat_ns(0.0),
    });
    let replay_beat0_times = std::array::from_fn(|player| {
        timing_players[player.min(MAX_PLAYERS - 1)].get_time_for_beat_ns(0.0)
    });
    let replay_input = build_replay_input_edges(
        &replay_edges,
        num_players,
        cols_per_player,
        num_cols,
        replay_offsets.beat0_time_ns,
        replay_beat0_times,
    );
    let replay_mode = !replay_input.is_empty();
    if replay_mode {
        log::debug!(
            "Gameplay replay mode enabled: {} recorded edges loaded.",
            replay_input.len(),
        );
    }
    let replay_input = GameplayReplayInputState::new(replay_input);
    let timing_player_refs = std::array::from_fn(|player| timing_players[player].as_ref());
    let time_to_beat_caches = GameplayTimeToBeatCaches::new(&timing, &timing_player_refs);
    let setup_ms = init_started.elapsed().as_secs_f64() * 1000.0;

    let note_build_started = Instant::now();
    let notes_cap: usize = (0..num_players)
        .map(|player| gameplay_charts[player].parsed_notes.len())
        .sum();
    let mut notes: Vec<Note> = Vec::with_capacity(notes_cap);
    let mut note_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut holds_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut rolls_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut mines_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut max_row_index = 0usize;

    for player in 0..num_players {
        let timing_player = &timing_players[player];
        let parsed_notes = &gameplay_charts[player].parsed_notes;
        let start = notes.len();
        let col_offset = player.saturating_mul(cols_per_player);
        for parsed in parsed_notes {
            let row_index = parsed.row_index;
            max_row_index = max_row_index.max(row_index);

            let Some(beat) = timing_player.get_beat_for_row(row_index) else {
                continue;
            };
            let explicit_fake_tap = matches!(parsed.note_type, NoteType::Fake);
            let fake_by_segment = timing_player.is_fake_at_beat(beat);
            let is_fake = explicit_fake_tap || fake_by_segment;
            let note_type = if explicit_fake_tap {
                NoteType::Tap
            } else {
                parsed.note_type
            };

            // Pre-calculate judgability to avoid binary searches during gameplay
            let judgable_by_timing = timing_player.is_judgable_at_beat(beat);
            let can_be_judged = !is_fake && judgable_by_timing;

            if can_be_judged {
                match note_type {
                    NoteType::Hold => {
                        holds_total[player] = holds_total[player].saturating_add(1);
                    }
                    NoteType::Roll => {
                        rolls_total[player] = rolls_total[player].saturating_add(1);
                    }
                    NoteType::Mine => {
                        mines_total[player] = mines_total[player].saturating_add(1);
                    }
                    NoteType::Tap | NoteType::Lift => {}
                    NoteType::Fake => {}
                }
            }

            let hold = match (note_type, parsed.tail_row_index) {
                (NoteType::Hold | NoteType::Roll, Some(tail_row)) => timing_player
                    .get_beat_for_row(tail_row)
                    .map(|end_beat| HoldData {
                        end_row_index: tail_row,
                        end_beat,
                        result: None,
                        life: INITIAL_HOLD_LIFE,
                        let_go_started_at: None,
                        let_go_starting_life: 0.0,
                        last_held_row_index: row_index,
                        last_held_beat: beat,
                    }),
                _ => None,
            };

            let quantization_idx = quantization_index_from_beat(beat);
            notes.push(Note {
                beat,
                quantization_idx,
                column: parsed.column.saturating_add(col_offset),
                note_type,
                row_index,
                result: None,
                early_result: None,
                hold,
                mine_result: None,
                is_fake,
                can_be_judged,
            });
        }
        let end = notes.len();
        note_ranges[player] = (start, end);
    }
    let note_build_ms = note_build_started.elapsed().as_secs_f64() * 1000.0;

    let transform_started = Instant::now();
    let uncommon_effects = std::array::from_fn(|player| {
        if player < num_players {
            chart_effects_from_profile(&player_profiles[player])
        } else {
            ChartAttackEffects::default()
        }
    });
    let timing_player_refs: [&TimingData; MAX_PLAYERS] =
        std::array::from_fn(|player| timing_players[player].as_ref());
    apply_uncommon_chart_transforms(
        &mut notes,
        &mut note_ranges,
        cols_per_player,
        num_players,
        &uncommon_effects,
        &timing_player_refs,
    );

    let song_seed = turn_seed_for_song(&song);
    let mut attack_song_length_seconds = song.precise_last_second();
    if !attack_song_length_seconds.is_finite() || attack_song_length_seconds <= 0.0 {
        attack_song_length_seconds = song.total_length_seconds.max(0) as f32;
    }
    let player_turn_options: [GameplayTurnOption; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player < num_players {
            player_profiles[player].turn_option()
        } else {
            GameplayTurnOption::None
        }
    });
    apply_turn_options(
        &mut notes,
        note_ranges,
        cols_per_player,
        num_players,
        player_turn_options,
        song_seed,
    );
    let player_attack_modes: [GameplayAttackMode; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player < num_players {
            player_profiles[player].attack_mode()
        } else {
            GameplayAttackMode::Off
        }
    });
    apply_chart_attacks_transforms(
        &mut notes,
        &mut note_ranges,
        &gameplay_charts,
        cols_per_player,
        num_players,
        &player_attack_modes,
        &timing_players,
        song_seed,
        attack_song_length_seconds,
    );

    let mut score_valid = [true; MAX_PLAYERS];
    let mut score_missed_holds_rolls = [false; MAX_PLAYERS];
    for player in 0..num_players {
        let invalid_reasons = score_invalid_reason_lines_for_chart(
            &charts[player],
            &player_profiles[player],
            scroll_speed[player],
            rate,
        );
        score_valid[player] = invalid_reasons.is_empty();
        if !score_valid[player] {
            log::debug!(
                "Score validity disabled for player {} ({}): {}.",
                player + 1,
                charts[player].short_hash,
                invalid_reasons.join("; ")
            );
        }
        score_missed_holds_rolls[player] =
            judgment::score_missed_holds_and_rolls(&charts[player].chart_type);
    }

    let chart_layout_changed = (0..num_players)
        .any(|player| player_changes_chart(&gameplay_charts[player], &player_profiles[player]));
    let mut total_steps = [0u32; MAX_PLAYERS];
    let mut hands_total = [0u32; MAX_PLAYERS];
    let mut possible_grade_points = [0i32; MAX_PLAYERS];
    if chart_layout_changed {
        holds_total = [0; MAX_PLAYERS];
        rolls_total = [0; MAX_PLAYERS];
        mines_total = [0; MAX_PLAYERS];
        for player in 0..num_players {
            let totals = recompute_player_totals(&notes, note_ranges[player]);
            total_steps[player] = totals.steps;
            holds_total[player] = totals.holds;
            rolls_total[player] = totals.rolls;
            mines_total[player] = totals.mines;
            hands_total[player] = totals.hands;
            possible_grade_points[player] = judgment::max_grade_points(
                totals.steps,
                holds_total[player],
                rolls_total[player],
                charts[player].possible_grade_points,
            );
        }
    } else {
        for player in 0..num_players {
            total_steps[player] = charts[player].stats.total_steps;
            holds_total[player] = charts[player].holds_total;
            rolls_total[player] = charts[player].rolls_total;
            mines_total[player] = charts[player].mines_total;
            hands_total[player] = charts[player].stats.hands;
            possible_grade_points[player] = charts[player].possible_grade_points;
        }
    }
    if num_players == 1 {
        possible_grade_points[1] = possible_grade_points[0];
        holds_total[1] = holds_total[0];
        rolls_total[1] = rolls_total[0];
        mines_total[1] = mines_total[0];
        total_steps[1] = total_steps[0];
        hands_total[1] = hands_total[0];
        score_valid[1] = score_valid[0];
        score_missed_holds_rolls[1] = score_missed_holds_rolls[0];
        note_ranges[1] = note_ranges[0];
    }
    let note_count_stats: [Vec<NoteCountStat>; MAX_PLAYERS] =
        std::array::from_fn(|player| build_note_count_stats(&notes, note_ranges[player]));
    let transform_ms = transform_started.elapsed().as_secs_f64() * 1000.0;

    let note_player_for_col =
        |col: usize| -> usize { player_index_for_column(num_players, cols_per_player, col) };

    let cache_build_started = Instant::now();
    let mut note_time_cache_ns = Vec::with_capacity(notes.len());
    let mut hold_end_time_cache_ns = Vec::with_capacity(notes.len());
    for note in &notes {
        let timing_player = &timing_players[note_player_for_col(note.column)];
        let note_time_ns = timing_player.get_time_for_beat_ns(note.beat);
        note_time_cache_ns.push(note_time_ns);
        if let Some(hold) = note.hold.as_ref() {
            let end_time_ns = timing_player.get_time_for_beat_ns(hold.end_beat);
            hold_end_time_cache_ns.push(Some(end_time_ns));
        } else {
            hold_end_time_cache_ns.push(None);
        }
    }

    log::debug!("Parsed {} notes from chart data.", notes.len());

    let mut row_entries: Vec<RowEntry> = Vec::with_capacity(notes.len() / 2);
    let mut row_entry_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] =
        std::array::from_fn(|_| vec![u32::MAX; max_row_index + 1]);
    let mut note_row_entry_indices = vec![u32::MAX; notes.len()];
    let mut tap_row_hold_roll_flags = vec![0u8; notes.len()];
    for player in 0..num_players {
        let row_range_start = row_entries.len();
        let (note_start, note_end) = note_ranges[player];
        let mut cursor = note_start;
        while cursor < note_end {
            let row_index = notes[cursor].row_index;
            let row_start = cursor;
            let mut row_flags = 0u8;
            let mut nonmine_note_indices = [usize::MAX; MAX_COLS];
            let mut nonmine_note_count = 0u8;
            while cursor < note_end && notes[cursor].row_index == row_index {
                let note = &notes[cursor];
                match note.note_type {
                    NoteType::Hold => row_flags |= 0b01,
                    NoteType::Roll => row_flags |= 0b10,
                    _ => {}
                }
                if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                    let count = usize::from(nonmine_note_count);
                    debug_assert!(count < MAX_COLS);
                    nonmine_note_indices[count] = cursor;
                    nonmine_note_count += 1;
                }
                cursor += 1;
            }
            if nonmine_note_count != 0 {
                let row_entry_index = row_entries.len() as u32;
                row_map_cache[player][row_index] = row_entry_index;
                for &note_index in &nonmine_note_indices[..usize::from(nonmine_note_count)] {
                    note_row_entry_indices[note_index] = row_entry_index;
                }
                row_entries.push(build_row_entry(
                    row_index,
                    nonmine_note_indices,
                    nonmine_note_count,
                    &notes,
                    &note_time_cache_ns,
                ));
            }
            tap_row_hold_roll_flags[row_start..cursor].fill(row_flags);
        }
        row_entry_ranges[player] = (row_range_start, row_entries.len());
    }
    let cache_build_ms = cache_build_started.elapsed().as_secs_f64() * 1000.0;

    let timing_prep_started = Instant::now();
    let first_second = notes
        .iter()
        .zip(&note_time_cache_ns)
        .filter_map(|(n, &t_ns)| n.can_be_judged.then_some(song_time_ns_to_seconds(t_ns)))
        .reduce(f32::min)
        .unwrap_or(0.0);
    // ITGmania's ScreenGameplay::StartPlayingSong uses theme metrics
    // MinSecondsToStep / MinSecondsToMusic. Simply Love scales both by
    // MusicRate, so we apply the same here to keep real-world lead-in time
    // consistent across rates.
    let lead_in_timing = lead_in_timing.unwrap_or_default();
    let min_time_to_notes = lead_in_timing.min_seconds_to_step.max(0.0) * rate;
    let min_time_to_music = lead_in_timing.min_seconds_to_music.max(0.0) * rate;
    let mut start_delay = min_time_to_notes - first_second;
    if start_delay < min_time_to_music {
        start_delay = min_time_to_music;
    }
    if start_delay < 0.0 {
        start_delay = 0.0;
    }

    let first_note_beat = timing.get_beat_for_time(first_second);
    let initial_bpm = timing.get_bpm_for_beat(first_note_beat);

    let mut reference_bpm =
        reference_bpm_from_display_tag(charts[0].display_bpm.as_ref(), &song.display_bpm)
            .unwrap_or_else(|| {
                let mut actual_max = timing.get_capped_max_bpm(Some(M_MOD_HIGH_CAP));
                if !actual_max.is_finite() || actual_max <= 0.0 {
                    actual_max = initial_bpm.max(120.0);
                }
                actual_max
            });
    if !reference_bpm.is_finite() || reference_bpm <= 0.0 {
        reference_bpm = initial_bpm.max(120.0);
    }

    let pixels_per_second: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut pps = scroll_speed[player].pixels_per_second(initial_bpm, reference_bpm, rate);
        if !pps.is_finite() || pps <= 0.0 {
            pps = ScrollSpeedSetting::default().pixels_per_second(initial_bpm, reference_bpm, rate);
        }
        pps
    });
    let initial_draw_scale: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 1.0;
        }
        let profile = &player_profiles[player];
        player_draw_scale_for_tilt_with_visual_mask(
            profile.perspective_effects().tilt,
            profile,
            profile.visual_mask_bits(),
            0.0,
        )
    });
    let draw_distance_before_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_before_targets(viewport.height(), 1.0);
        }
        draw_distance_before_targets(viewport.height(), initial_draw_scale[player])
    });
    let draw_distance_after_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return draw_distance_after_targets(viewport.height(), 1.0, 0.0);
        }
        let centered_percent = if player_profiles[player].scroll_effects().centered > 0.5 {
            1.0
        } else {
            0.0
        };
        draw_distance_after_targets(
            viewport.height(),
            initial_draw_scale[player],
            centered_percent,
        )
    });

    let travel_time: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut tt = scroll_speed[player].travel_time_seconds(
            draw_distance_before_targets[player],
            initial_bpm,
            reference_bpm,
            rate,
        );
        if !tt.is_finite() || tt <= 0.0 {
            tt = draw_distance_before_targets[player] / pixels_per_second[player];
        }
        tt
    });

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let player_judgment_timing = std::array::from_fn(|player| {
        build_player_judgment_timing(timing_profile, &player_profiles[player], rate)
    });
    let audio_end_time_ns = song_audio_end_time_ns(&song);
    let (notes_end_time_ns, music_end_time_ns) = compute_end_times_ns(
        &notes,
        &note_time_cache_ns,
        &hold_end_time_cache_ns,
        rate,
        audio_end_time_ns,
    );
    let notes_len = notes.len();
    let mut column_scroll_dirs = [1.0_f32; MAX_COLS];
    for (player, player_profile) in player_profiles.iter().enumerate().take(num_players) {
        let start = player * cols_per_player;
        let end = (start + cols_per_player).min(num_cols).min(MAX_COLS);
        let local_dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: player_profile.scroll_effects().reverse > 0.5,
                split: player_profile.scroll_effects().split > 0.5,
                alternate: player_profile.scroll_effects().alternate > 0.5,
                cross: player_profile.scroll_effects().cross > 0.5,
            },
            cols_per_player,
        );
        for (offset, column_scroll_dir) in column_scroll_dirs[start..end].iter_mut().enumerate() {
            *column_scroll_dir = local_dirs[offset];
        }
    }

    let note_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| note_ranges[player].0);
    let row_entry_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| row_entry_ranges[player].0);
    let mut mine_note_ix: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    let mut mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let mut mine_ix = Vec::with_capacity(mines_total[player] as usize);
        let mut mine_times_ns = Vec::with_capacity(mines_total[player] as usize);
        for note_idx in start..end {
            if matches!(notes[note_idx].note_type, NoteType::Mine) {
                mine_ix.push(note_idx);
                mine_times_ns.push(note_time_cache_ns[note_idx]);
            }
        }
        mine_note_ix[player] = mine_ix;
        mine_note_time_ns[player] = mine_times_ns;
    }
    let mut lane_note_counts = [0usize; MAX_COLS];
    let mut lane_hold_counts = [0usize; MAX_COLS];
    let mut replay_cells = 0usize;
    for note in &notes {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_counts[col] = lane_note_counts[col].saturating_add(1);
            if note_has_displayable_hold(note) {
                lane_hold_counts[col] = lane_hold_counts[col].saturating_add(1);
            }
        }
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            replay_cells = replay_cells.saturating_add(1);
        }
    }
    let mut lane_note_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_note_counts[col]));
    let mut lane_hold_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_hold_counts[col]));
    for (note_index, note) in notes.iter().enumerate() {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_indices[col].push(note_index);
            if note_has_displayable_hold(note) {
                lane_hold_indices[col].push(note_index);
            }
        }
    }
    let mut lane_note_row_indices = lane_note_indices.clone();
    for indices in lane_note_row_indices
        .iter_mut()
        .take(num_cols.min(MAX_COLS))
    {
        indices.sort_unstable_by_key(|&note_index| {
            (beat_to_note_row(notes[note_index].beat), note_index)
        });
    }
    let pending_edges_capacity = input_queue_cap(num_cols);
    let replay_seconds = (song_time_ns_to_seconds(music_end_time_ns) + start_delay)
        .max(song_time_ns_to_seconds(notes_end_time_ns) + start_delay);
    let replay_capture_enabled = !replay_mode && config.machine_enable_replays;
    let replay_edges_capacity = [
        0,
        replay_edge_cap(num_cols, replay_cells, replay_mode, replay_seconds),
    ][replay_capture_enabled as usize];
    let decaying_hold_capacity = (0..num_players).fold(0usize, |acc, player| {
        acc.saturating_add(holds_total[player] as usize + rolls_total[player] as usize)
    });
    let timing_prep_ms = timing_prep_started.elapsed().as_secs_f64() * 1000.0;

    let hud_prep_started = Instant::now();
    let global_visual_delay_seconds = config.visual_delay_seconds;
    let player_visual_delay_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 0.0;
        }
        let ms = player_profiles[player].visual_delay_ms().clamp(-100, 100);
        ms as f32 / 1000.0
    });
    let init_music_time = -start_delay;
    let init_music_time_ns = song_time_ns_from_seconds(init_music_time);
    let init_beat = timing.get_beat_for_time_ns(init_music_time_ns);
    let current_music_time_visible_ns: [SongTimeNs; MAX_PLAYERS] = std::array::from_fn(|player| {
        let delay = global_visual_delay_seconds + player_visual_delay_seconds[player];
        visible_notefield_time_ns(init_music_time_ns, delay)
    });
    let current_music_time_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        song_time_ns_to_seconds(current_music_time_visible_ns[player])
    });
    let current_beat_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        timing_players[player].get_beat_for_time_ns(current_music_time_visible_ns[player])
    });
    let (song_lua_mask_windows, song_lua_ease_windows, song_lua_visuals) = build_song_lua_runtime
        .build_song_lua_runtime(build_song_lua_runtime_window_build(
            song.title.as_str(),
            &timing_players,
            num_players,
            &player_profiles,
            config.global_offset_seconds,
            viewport,
            &session,
            config.center_1player_notefield,
            &player_global_offset_shift_seconds,
        ));
    let attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return Vec::new();
        }
        let attack_mode = player_profiles[player].attack_mode();
        let mut windows = if attack_mode == GameplayAttackMode::Off {
            Vec::new()
        } else {
            build_attack_mask_windows_for_player(
                gameplay_charts[player].chart_attacks.as_deref(),
                attack_mode,
                player,
                song_seed,
                attack_song_length_seconds,
            )
        };
        windows.extend(song_lua_mask_windows[player].iter().copied());
        windows
    });
    let reverse_scroll: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return false;
        }
        player_profiles[player].reverse_scroll()
    });
    let mut column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        if !player_profiles[player].column_cues() {
            continue;
        }
        let col_start = player.saturating_mul(cols_per_player);
        let col_end = (col_start + cols_per_player).min(num_cols);
        column_cues[player] = build_column_cues_for_player(
            &notes,
            note_ranges[player],
            &note_time_cache_ns,
            col_start,
            col_end,
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        let (first, second) = column_cues.split_at_mut(1);
        second[0].clone_from(&first[0]);
    }

    let mut crossover_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        if !player_profiles[player].crossover_cues() {
            continue;
        }
        let col_start = player.saturating_mul(cols_per_player);
        crossover_cues[player] = build_crossover_cues_for_player_annotations(
            build_crossover_annotations,
            &notes,
            note_ranges[player],
            &gameplay_charts[player].timing_segments,
            &timing_players[player],
            cols_per_player,
            col_start,
            player_profiles[player].crossover_cue_duration_ms(),
            player_profiles[player].crossover_cue_quantization(),
            player_profiles[player].crossover_cue_brackets(),
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        let (first, second) = crossover_cues.split_at_mut(1);
        second[0].clone_from(&first[0]);
    }

    let measure_densities: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !needs_stream_data(&player_profiles[p]) {
            return Vec::new();
        }
        measure_densities(&gameplay_charts[p].notes, cols_per_player)
    });

    let measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players {
            return Vec::new();
        }
        measure_counter_segments_for_densities(
            &measure_densities[p],
            player_profiles[p].measure_counter_threshold(),
        )
    });

    let mut mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut mini_indicator_total_stream_measures = [0.0_f32; MAX_PLAYERS];
    let mut mini_indicator_target_score_percent = [89.0_f64; MAX_PLAYERS];
    let mut mini_indicator_rival_score_percent = [0.0_f64; MAX_PLAYERS];

    for p in 0..num_players {
        if mini_indicator_mode(&player_profiles[p]) == GameplayMiniIndicatorMode::None {
            continue;
        }
        let constant_bpm = !timing_players[p].has_bpm_changes();
        let (stream_segments, total_stream, _total_break) =
            zmod_stream_totals_for_densities(&measure_densities[p], constant_bpm);
        mini_indicator_total_stream_measures[p] = total_stream.max(0.0);
        mini_indicator_stream_segments[p] = stream_segments;

        let personal_best = mini_indicator_data.personal_best_percent[p];
        let machine_best = mini_indicator_data.machine_best_percent[p];

        mini_indicator_target_score_percent[p] = resolve_target_score_percent(
            player_profiles[p].target_score(),
            personal_best,
            machine_best,
        );

        mini_indicator_rival_score_percent[p] = machine_best
            .unwrap_or(0.0)
            .max(personal_best.unwrap_or(0.0));
    }

    let hud_prep_ms = hud_prep_started.elapsed().as_secs_f64() * 1000.0;

    let graph_prep_started = Instant::now();
    let wants_density_graph = player_profiles
        .iter()
        .take(num_players)
        .any(GameplayProfileData::step_statistics_density_graph);
    let gameplay_play_style = step_stats_play_style(play_style);
    let wide = viewport.is_wide();
    let density_graph_enabled = wide && wants_density_graph;
    let sw = viewport.width();
    let sh = viewport.height().max(1.0_f32);
    let density_graph_graph_h = if density_graph_enabled {
        105.0_f32
    } else {
        0.0_f32
    };
    let density_graph_graph_w = if density_graph_enabled {
        step_stats_density_graph_width(
            gameplay_play_style,
            cols_per_player,
            num_players,
            sw,
            sh,
            wide,
            config.center_1player_notefield,
        )
    } else {
        0.0_f32
    };
    let density_graph_first_second = timing.get_time_for_beat(0.0).min(0.0_f32);
    let density_graph_last_second = song.precise_last_second();
    let density_graph_duration =
        (density_graph_last_second - density_graph_first_second).max(0.001_f32);

    const DENSITY_GRAPH_MAX_SECONDS: f32 = 4.0 * 60.0;
    let density_graph_scaled_width =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (density_graph_graph_w * (density_graph_duration / DENSITY_GRAPH_MAX_SECONDS))
                .round()
                .max(density_graph_graph_w)
        } else {
            density_graph_graph_w
        };
    let density_graph_u_window =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (DENSITY_GRAPH_MAX_SECONDS / density_graph_duration).clamp(0.0_f32, 1.0_f32)
        } else {
            1.0_f32
        };
    let density_graph_u0 = 0.0_f32;
    let density_graph_top_h = 30.0_f32;
    let density_graph_top_w: [f32; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !player_profiles[p].nps_graph_at_top() {
            return 0.0;
        }
        step_stats_upper_density_graph_width(gameplay_play_style)
    });
    let density_graph_top_scale_y: [f32; MAX_PLAYERS] = {
        let mut scale = [1.0_f32; MAX_PLAYERS];
        if num_players == 2
            && player_profiles[0].nps_graph_at_top()
            && player_profiles[1].nps_graph_at_top()
        {
            let p1_peak = charts[0].max_nps as f32;
            let p2_peak = charts[1].max_nps as f32;
            if p1_peak.is_finite() && p2_peak.is_finite() && p1_peak > 0.0 && p2_peak > 0.0 {
                if p1_peak < p2_peak {
                    scale[0] = (p1_peak / p2_peak).clamp(0.0, 1.0);
                } else if p2_peak < p1_peak {
                    scale[1] = (p2_peak / p1_peak).clamp(0.0, 1.0);
                }
            }
        }
        scale
    };
    let mut density_graph_life_update_rate = 0.25_f32;
    if density_graph_enabled && !timing.has_bpm_changes() {
        let bpm = timing.first_bpm();
        if bpm.is_finite() && bpm >= 60.0_f32 {
            let interval_8th = (60.0_f32 / bpm) * 0.5_f32;
            if interval_8th.is_finite() && interval_8th > 0.0_f32 {
                density_graph_life_update_rate =
                    interval_8th * (density_graph_life_update_rate / interval_8th).ceil();
            }
        }
    }
    if !density_graph_life_update_rate.is_finite() || density_graph_life_update_rate <= 0.0_f32 {
        density_graph_life_update_rate = 0.25_f32;
    }
    let density_graph_life_next_update_elapsed = 0.0_f32;
    let density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if density_graph_enabled && p < num_players {
            Vec::with_capacity(1024)
        } else {
            Vec::new()
        }
    });
    let density_graph_life_dirty: [bool; MAX_PLAYERS] = [false; MAX_PLAYERS];
    let graph_prep_ms = graph_prep_started.elapsed().as_secs_f64() * 1000.0;

    let finalize_started = Instant::now();
    let mut players = std::array::from_fn(|_| init_player_runtime());
    let in_course_stage = course_display_totals.is_some();
    for p in 0..num_players {
        let course_carry = course_display_carry.as_ref().map(|carry| carry[p]);
        players[p] = init_player_runtime_for_song(
            init_music_time,
            in_course_stage,
            course_carry,
            player_profiles[p].carry_combo_between_songs(),
            replay_mode,
            combo_carry[p],
        );
    }
    let assist_clap_rows = build_assist_clap_rows(&notes, note_ranges[0]);
    let song_offset_seconds = song.offset;
    let base_attack_appearance = std::array::from_fn(|player| {
        if player < num_players {
            base_appearance_effects(&player_profiles[player])
        } else {
            AppearanceEffects::default()
        }
    });
    let tick_mode = session.tick_mode;

    let mut state = GameplayRuntimeState {
        source: GameplaySourceRuntimeState {
            song,
            charts,
            gameplay_charts,
        },
        setup: GameplaySetupRuntimeState {
            num_cols,
            cols_per_player,
            num_players,
            viewport,
            session,
            config,
        },
        boundary: GameplayBoundaryRuntimeState::new(8, 2),
        timing_runtime: GameplayTimingRuntimeState {
            timing,
            timing_players,
            time_to_beat_caches,
            timing_profile,
            player_judgment_timing,
        },
        chart_runtime: GameplayChartRuntimeState {
            notes,
            column_judgment_eligible: vec![false; notes_len],
            note_ranges: GameplayNoteRangeState::new(note_ranges),
            note_count_stats: GameplayNoteCountStatsState::new(note_count_stats),
            lane_indices: GameplayLaneIndexState::new(
                lane_note_indices,
                lane_note_row_indices,
                lane_hold_indices,
                tap_row_hold_roll_flags,
            ),
            row_indices: GameplayRowIndexState::new(
                row_entry_ranges,
                row_entry_range_start,
                row_map_cache,
                note_row_entry_indices,
            ),
            note_time_cache_ns,
            hold_end_time_cache_ns,
            mine_scan: GameplayMineScanState::new(
                note_range_start,
                mine_note_ix,
                mine_note_time_ns,
            ),
            row_entries,
        },
        clock: GameplayClockRuntimeState {
            audio_clock: GameplayAudioClockState::new(start_delay, 0.0, 0.0),
            song_position: GameplaySongPositionState::new(
                init_beat,
                song_time_ns_from_seconds(init_music_time),
                init_beat,
                init_music_time,
            ),
            display_clock: GameplayDisplayClockState::new(song_time_ns_from_seconds(
                init_music_time,
            )),
            end_timing: GameplayEndTimingState::new(
                notes_end_time_ns,
                music_end_time_ns,
                audio_end_time_ns,
            ),
            music_rate: GameplayMusicRateState::new(rate),
            offsets: GameplayOffsetState::new(
                config.global_offset_seconds,
                player_global_offset_shift_seconds,
                song_offset_seconds,
            ),
            visible_timing: GameplayVisibleTimingState {
                global_visual_delay_seconds,
                player_visual_delay_seconds,
                current_music_time_ns: current_music_time_visible_ns,
                current_music_time: current_music_time_visible,
                current_beat: current_beat_visible,
            },
        },
        hold_runtime: GameplayHoldRuntimeState::new(notes_len, decaying_hold_capacity),
        players_runtime: GameplayPlayersRuntimeState {
            players,
            column_judgments_active: [true; MAX_PLAYERS],
        },
        display: GameplayDisplayRuntimeState {
            cue_runtime: GameplayCueRuntimeState::new(
                measure_counter_segments,
                column_cues,
                crossover_cues,
            ),
            mini_indicator: GameplayMiniIndicatorRuntimeState::new(
                mini_indicator_stream_segments,
                mini_indicator_total_stream_measures,
                mini_indicator_target_score_percent,
                mini_indicator_rival_score_percent,
            ),
            hold_feedback: GameplayHoldFeedbackState::default(),
            beat_phase: GameplayBeatPhaseState::default(),
            noteskin_effects,
            active_color_index,
            player_color_index,
            notefield_motion: GameplayNotefieldMotionState::new(
                scroll_speed,
                reference_bpm,
                field_zoom,
                pixels_per_second,
                travel_time,
                draw_distance_before_targets,
                draw_distance_after_targets,
                reverse_scroll,
                column_scroll_dirs,
            ),
            receptor_feedback: GameplayReceptorFeedbackState::default(),
            visual_feedback: GameplayVisualFeedbackState::default(),
            danger_fx: GameplayDangerFxState::default(),
            density_graph: GameplayDensityGraphState {
                first_second: density_graph_first_second,
                last_second: density_graph_last_second,
                duration: density_graph_duration,
                graph_w: density_graph_graph_w,
                graph_h: density_graph_graph_h,
                scaled_width: density_graph_scaled_width,
                u0: density_graph_u0,
                u_window: density_graph_u_window,
                life_update_rate: density_graph_life_update_rate,
                life_next_update_elapsed: density_graph_life_next_update_elapsed,
                life_points: density_graph_life_points,
                life_dirty: density_graph_life_dirty,
                top_h: density_graph_top_h,
                top_w: density_graph_top_w,
                top_scale_y: density_graph_top_scale_y,
            },
            toggle_flash: GameplayToggleFlashState::default(),
        },
        progress: GameplayProgressRuntimeState {
            chart_totals: GameplayChartTotalsState::new(
                possible_grade_points,
                total_steps,
                holds_total,
                rolls_total,
                mines_total,
                hands_total,
            ),
            stage: GameplayStageRuntimeState::new(
                replay_mode,
                replay_mode,
                score_valid,
                score_missed_holds_rolls,
            ),
            replay: GameplayReplayRuntimeState::new(
                replay_input,
                replay_capture_enabled,
                replay_edges_capacity,
            ),
            course_display: GameplayCourseDisplayState::new(
                course_display_carry,
                course_display_totals,
                course_display_timing,
            ),
            window_counts: GameplayWindowCountsState::default(),
        },
        profiles_runtime: GameplayProfilesRuntimeState {
            profiles: player_profiles,
        },
        mods: GameplayModRuntimeState {
            song_lua_visuals,
            song_lua_player_transforms: song_lua_player_transforms_default(),
            attacks: GameplayAttackRuntimeState {
                mask_windows: attack_mask_windows,
                song_lua_ease_windows,
                current_appearance: base_attack_appearance,
                target_appearance: base_attack_appearance,
                speed_appearance: [AppearanceEffects::approach_speeds(); MAX_PLAYERS],
                appearance: base_attack_appearance,
                ..GameplayAttackRuntimeState::default()
            },
        },
        control: GameplayControlRuntimeState {
            exit_input: GameplayExitInputState::default(),
            offset_adjust_hold: GameplayOffsetAdjustHoldState::default(),
            input_state: GameplayInputState::default(),
            autoplay_runtime: GameplayAutoplayRuntimeState::new(
                song_seed ^ 0xA17F_0FF5_EED5_1EED,
                note_range_start,
            ),
            autosync: GameplayAutosyncRuntimeState::default(),
            tick_mode,
            assist_clap: GameplayAssistClapState::new(assist_clap_rows),
            update_trace: GameplayUpdateTraceState::default(),
        },
        pending_input: GameplayPendingInputState::with_capacity(pending_edges_capacity),
    };
    state.control.update_trace =
        GameplayUpdateTraceState::from_capacity_snapshot(&state.capacity_trace_snapshot());
    state.refresh_seek_dependent_state();
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = init_started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 50.0 {
        log::info!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.source.song.title,
            state.chart_runtime.notes.len(),
            state.setup.num_players,
            density_graph_enabled,
        );
    } else {
        log::debug!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.source.song.title,
            state.chart_runtime.notes.len(),
            state.setup.num_players,
            density_graph_enabled,
        );
    }
    state
}
