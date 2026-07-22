#[inline(always)]
pub const fn column_flash_duration(grade: JudgeGrade) -> f32 {
    match grade {
        JudgeGrade::Miss => COLUMN_FLASH_MISS_DURATION,
        JudgeGrade::Fantastic
        | JudgeGrade::Excellent
        | JudgeGrade::Great
        | JudgeGrade::Decent
        | JudgeGrade::WayOff => COLUMN_FLASH_JUDGMENT_DURATION,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnFlashOptions {
    pub enabled: bool,
    pub blue_fantastic: bool,
    pub white_fantastic: bool,
    pub excellent: bool,
    pub great: bool,
    pub decent: bool,
    pub way_off: bool,
    pub miss: bool,
}

#[inline(always)]
pub fn column_flash_options_from_profile<Profile: GameplayProfileData>(
    profile: &Profile,
) -> ColumnFlashOptions {
    profile.column_flash_options()
}

#[inline(always)]
pub const fn column_flash_enabled_for_options(
    options: ColumnFlashOptions,
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> bool {
    if !options.enabled {
        return false;
    }
    match grade {
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                options.blue_fantastic
            } else {
                options.white_fantastic
            }
        }
        JudgeGrade::Excellent => options.excellent,
        JudgeGrade::Great => options.great,
        JudgeGrade::Decent => options.decent,
        JudgeGrade::WayOff => options.way_off,
        JudgeGrade::Miss => options.miss,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveTapExplosion {
    pub window: &'static str,
    pub bright: bool,
    pub elapsed: f32,
    pub duration: f32,
    pub start_beat: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveColumnFlash {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ColumnTapJudgment {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub at_screen_s: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveMineExplosion {
    pub elapsed: f32,
    pub duration: f32,
    pub started_at_screen_s: f32,
}

#[inline(always)]
pub fn tick_tap_explosion_slot(slot: &mut Option<ActiveTapExplosion>, delta_time: f32) {
    if let Some(active) = slot {
        active.elapsed += delta_time;
        if active.duration <= 0.0 || active.elapsed >= active.duration {
            *slot = None;
        }
    }
}

#[inline(always)]
pub fn tick_mine_explosion_slot(slot: &mut Option<ActiveMineExplosion>, delta_time: f32) {
    if let Some(active) = slot {
        active.elapsed += delta_time;
        if active.duration <= 0.0 || active.elapsed >= active.duration {
            *slot = None;
        }
    }
}

#[inline(always)]
pub fn column_flash_expired_at(flash: ActiveColumnFlash, screen_time_s: f32) -> bool {
    screen_time_s - flash.started_at_screen_s >= column_flash_duration(flash.grade)
}

#[inline(always)]
pub fn hold_judgment_expired_at(render_info: HoldJudgmentRenderInfo, screen_time_s: f32) -> bool {
    screen_time_s - render_info.started_at_screen_s >= HOLD_JUDGMENT_TOTAL_DURATION
}

#[inline(always)]
pub fn held_miss_judgment_expired_at(render_info: HeldMissRenderInfo, screen_time_s: f32) -> bool {
    screen_time_s - render_info.started_at_screen_s >= HELD_MISS_TOTAL_DURATION
}

pub const MINE_EXPLOSION_DURATION: f32 = 0.6;
pub const RECEPTOR_STEP_WINDOW_COUNT: usize = 7;
pub const RECEPTOR_STEP_WINDOWS: [Option<&str>; RECEPTOR_STEP_WINDOW_COUNT] = [
    None,
    Some("W1"),
    Some("W2"),
    Some("W3"),
    Some("W4"),
    Some("W5"),
    Some("Miss"),
];
pub const TAP_EXPLOSION_WINDOW_COUNT: usize = 7;
pub const TAP_EXPLOSION_WINDOWS: [&str; TAP_EXPLOSION_WINDOW_COUNT] =
    ["W1", "W2", "W3", "W4", "W5", "Miss", "Held"];

#[inline(always)]
pub const fn grade_to_window(grade: JudgeGrade) -> Option<&'static str> {
    match grade {
        JudgeGrade::Fantastic => Some("W1"),
        JudgeGrade::Excellent => Some("W2"),
        JudgeGrade::Great => Some("W3"),
        JudgeGrade::Decent => Some("W4"),
        JudgeGrade::WayOff => Some("W5"),
        JudgeGrade::Miss => Some("Miss"),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FantasticFeedbackOptions {
    pub show_fa_plus_window: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub custom_fantastic_window: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FantasticWindowOptions {
    pub base_fa_plus_s: f32,
    pub custom_fantastic_window_s: Option<f32>,
    pub fa_plus_10ms_blue_window: bool,
}

#[inline(always)]
pub fn fantastic_window_options<Profile: GameplayProfileData>(
    base_fa_plus_s: f32,
    profile: &Profile,
) -> FantasticWindowOptions {
    profile.fantastic_options(base_fa_plus_s)
}

#[inline(always)]
pub fn fantastic_window_seconds(options: FantasticWindowOptions) -> f32 {
    options
        .custom_fantastic_window_s
        .unwrap_or(options.base_fa_plus_s)
}

#[inline(always)]
pub fn blue_fantastic_window_ms(options: FantasticWindowOptions) -> f32 {
    if let Some(custom_s) = options.custom_fantastic_window_s {
        return custom_s * 1000.0;
    }
    if options.fa_plus_10ms_blue_window {
        return 10.0;
    }
    options.base_fa_plus_s * 1000.0
}

#[inline(always)]
pub fn blue_fantastic_window_ms_from_profile<Profile: GameplayProfileData>(
    base_fa_plus_s: f32,
    profile: &Profile,
) -> f32 {
    blue_fantastic_window_ms(fantastic_window_options(base_fa_plus_s, profile))
}

#[derive(Clone, Copy, Debug)]
#[derive(Default)]
pub struct PlayerJudgmentTiming {
    pub profile_music_ns: TimingProfileNs,
    pub disabled_windows: [bool; 5],
    pub largest_tap_window_music_ns: SongTimeNs,
}


#[inline(always)]
pub fn build_player_judgment_timing_for_options(
    mut timing_profile: TimingProfile,
    fantastic_options: FantasticWindowOptions,
    disabled_windows: [bool; 5],
    music_rate: f32,
) -> PlayerJudgmentTiming {
    timing_profile.fa_plus_window_s = Some(fantastic_window_seconds(fantastic_options));
    let profile_music_ns = TimingProfileNs::from_profile_scaled(&timing_profile, music_rate);
    let largest_tap_window_music_ns =
        largest_enabled_tap_window_ns(&profile_music_ns, &disabled_windows)
            .unwrap_or(profile_music_ns.windows_ns[2]);

    PlayerJudgmentTiming {
        profile_music_ns,
        disabled_windows,
        largest_tap_window_music_ns,
    }
}

#[inline(always)]
pub fn build_player_judgment_timing<Profile: GameplayProfileData>(
    timing_profile: TimingProfile,
    player_profile: &Profile,
    music_rate: f32,
) -> PlayerJudgmentTiming {
    let base_fa_plus_s = timing_profile
        .fa_plus_window_s
        .unwrap_or(timing_profile.windows_s[0]);
    build_player_judgment_timing_for_options(
        timing_profile,
        player_profile.fantastic_options(base_fa_plus_s),
        player_profile.timing_disabled_windows(),
        music_rate,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteHitEval {
    pub note_time_ns: SongTimeNs,
    pub measured_offset_music_ns: SongTimeNs,
    pub grade: JudgeGrade,
    pub window: TimingWindow,
}

#[inline(always)]
pub fn note_hit_eval_for_timing(
    timing: PlayerJudgmentTiming,
    note_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> Option<NoteHitEval> {
    let measured_offset_music_ns = current_time_ns.saturating_sub(note_time_ns);
    if i128::from(measured_offset_music_ns).abs() > i128::from(timing.largest_tap_window_music_ns) {
        return None;
    }
    let (grade, window) = classify_offset_ns_with_disabled_windows(
        measured_offset_music_ns,
        &timing.profile_music_ns,
        &timing.disabled_windows,
    )?;
    Some(NoteHitEval {
        note_time_ns,
        measured_offset_music_ns,
        grade,
        window,
    })
}

#[inline(always)]
pub fn note_hit_judgment(
    hit: NoteHitEval,
    judgment_offset_music_ns: SongTimeNs,
    rate: f32,
) -> Judgment {
    Judgment {
        time_error_ms: judgment::judgment_time_error_ms_from_music_ns(
            judgment_offset_music_ns,
            rate,
        ),
        time_error_music_ns: judgment_offset_music_ns,
        grade: hit.grade,
        window: Some(hit.window),
        miss_because_held: false,
    }
}

#[inline(always)]
pub fn final_note_hit_judgment(
    hit: NoteHitEval,
    judgment_offset_music_ns: SongTimeNs,
    rate: f32,
) -> (Judgment, SongTimeNs) {
    let plan = final_note_hit_plan(hit, judgment_offset_music_ns, rate);
    (plan.judgment, plan.judgment_event_time_ns)
}

#[derive(Clone, Copy, Debug)]
pub struct FinalNoteHitPlan {
    pub judgment: Judgment,
    pub judgment_event_time_ns: SongTimeNs,
    pub receptor_window: Option<&'static str>,
}

#[inline(always)]
pub fn final_note_hit_plan(
    hit: NoteHitEval,
    judgment_offset_music_ns: SongTimeNs,
    rate: f32,
) -> FinalNoteHitPlan {
    let judgment = note_hit_judgment(hit, judgment_offset_music_ns, rate);
    FinalNoteHitPlan {
        judgment,
        judgment_event_time_ns: hit.note_time_ns.saturating_add(judgment_offset_music_ns),
        receptor_window: grade_to_window(judgment.grade),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitActiveHoldStart {
    pub column: usize,
    pub note_index: usize,
    pub start_time_ns: SongTimeNs,
    pub end_time_ns: SongTimeNs,
    pub current_time_ns: SongTimeNs,
}

#[inline(always)]
pub fn hit_active_hold_start(
    note_type: NoteType,
    note_index: usize,
    column: usize,
    hit_note_time_ns: SongTimeNs,
    hold_end_time_ns: Option<SongTimeNs>,
    current_time_ns: SongTimeNs,
) -> Option<HitActiveHoldStart> {
    if !matches!(note_type, NoteType::Hold | NoteType::Roll) {
        return None;
    }
    Some(HitActiveHoldStart {
        column,
        note_index,
        start_time_ns: hit_note_time_ns,
        end_time_ns: hold_end_time_ns?,
        current_time_ns,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ProvisionalEarlyHitPlan {
    pub judgment: Judgment,
    pub life_delta: f32,
    pub apply_life_change: bool,
    pub capture_failed_ex_score_inputs: bool,
}

#[inline(always)]
pub fn provisional_early_hit_plan(
    hit: NoteHitEval,
    rate: f32,
    scoring_blocked: bool,
) -> ProvisionalEarlyHitPlan {
    let apply_scoring_effects = !scoring_blocked;
    ProvisionalEarlyHitPlan {
        judgment: note_hit_judgment(hit, hit.measured_offset_music_ns, rate),
        life_delta: deadsync_rules::life::judge_life_delta(hit.grade),
        apply_life_change: apply_scoring_effects,
        capture_failed_ex_score_inputs: apply_scoring_effects,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EarlyRescoreHitDecision {
    Provisional,
    DuplicateProvisional,
    IgnoreBadRehit,
    FinalSingleTrackHit,
}

#[inline(always)]
pub const fn early_rescore_hit_decision(
    row_rescore_track_count: usize,
    hit: NoteHitEval,
    has_provisional_early_result: bool,
) -> Option<EarlyRescoreHitDecision> {
    if row_rescore_track_count != 1 {
        return None;
    }
    let is_early = hit.measured_offset_music_ns < 0;
    let is_bad = matches!(hit.grade, JudgeGrade::Decent | JudgeGrade::WayOff);
    if is_early && is_bad {
        return Some(if has_provisional_early_result {
            EarlyRescoreHitDecision::DuplicateProvisional
        } else {
            EarlyRescoreHitDecision::Provisional
        });
    }
    if has_provisional_early_result
        && !matches!(
            hit.grade,
            JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
        )
    {
        return Some(EarlyRescoreHitDecision::IgnoreBadRehit);
    }
    Some(EarlyRescoreHitDecision::FinalSingleTrackHit)
}

#[inline(always)]
pub fn timing_hit_log_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[inline(always)]
pub fn gameplay_input_log_enabled() -> bool {
    log::log_enabled!(log::Level::Debug)
}

pub fn process_input_edges<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
    song_clock: SongClockSnapshot,
) where
    Profile: GameplayProfileData,
{
    if state.pending_input_is_empty() {
        return;
    }

    let input_log = gameplay_input_log_enabled();
    while let Some(mut edge) = state.pop_pending_input_edge() {
        let lane_idx = edge.lane.index();
        if lane_idx >= state.setup.num_cols {
            if input_log {
                log::debug!(
                    "GAMEPLAY INPUT EDGE DROP: reason=lane_out_of_range lane={} num_cols={} source={:?} slot={} pressed={}",
                    lane_idx,
                    state.setup.num_cols,
                    edge.source,
                    edge.input_slot,
                    edge.pressed,
                );
            }
            continue;
        }
        let mut event_time_source = "precomputed";
        let mut resolved_from_song_clock = false;
        if song_time_ns_invalid(edge.event_music_time_ns) {
            edge.event_music_time_ns = music_time_ns_from_song_clock(
                song_clock,
                edge.captured_at,
                edge.captured_host_nanos,
            );
            event_time_source = "song_clock";
            resolved_from_song_clock = true;
        }
        if song_time_ns_invalid(edge.event_music_time_ns) {
            if input_log {
                log::debug!(
                    "GAMEPLAY INPUT EDGE DROP: reason=invalid_song_time lane={} source={:?} slot={} pressed={} captured_host_nanos={} pending={}",
                    lane_idx,
                    edge.source,
                    edge.input_slot,
                    edge.pressed,
                    edge.captured_host_nanos,
                    state.pending_input_len(),
                );
            }
            continue;
        }
        let lane_was_down = state.lane_is_pressed(lane_idx);
        let slot_was_down = state.input_slot_lane_is_down_normalized(
            lane_idx,
            edge.source,
            edge.input_slot,
            INPUT_SLOT_INVALID,
        );
        let edge_judges_tap = lane_edge_judges_tap(edge.pressed, slot_was_down);
        let edge_judges_lift = lane_edge_judges_lift(edge.pressed, slot_was_down);
        if resolved_from_song_clock
            && !song_clock.mapped_audio
            && record_unmapped_input_clock_warning(edge.event_music_time_ns)
        {
            log::warn!(
                "GAMEPLAY INPUT CLOCK WARNING: reason=audio_map_unavailable lane={} source={:?} slot={} pressed={} edge_time_s={:.6} song_clock_time_s={:.6} captured_host_nanos={} current_time_s={:.6}",
                lane_idx,
                edge.source,
                edge.input_slot,
                edge.pressed,
                song_time_ns_to_seconds(edge.event_music_time_ns),
                song_time_ns_to_seconds(song_clock.song_time_ns),
                edge.captured_host_nanos,
                state.current_music_time_seconds(),
            );
        }
        if input_log {
            let event_music_time = song_time_ns_to_seconds(edge.event_music_time_ns);
            if resolved_from_song_clock && !song_clock.mapped_audio {
                log::debug!(
                    "GAMEPLAY INPUT CLOCK FALLBACK: reason=audio_map_unavailable lane={} source={:?} slot={} pressed={} edge_time_s={:.6} song_clock_time_s={:.6} captured_host_nanos={}",
                    lane_idx,
                    edge.source,
                    edge.input_slot,
                    edge.pressed,
                    event_music_time,
                    song_time_ns_to_seconds(song_clock.song_time_ns),
                    edge.captured_host_nanos,
                );
            }
            let processed_at = Instant::now();
            let latency = gameplay_input_latency_sample(
                edge.captured_at,
                edge.stored_at,
                edge.emitted_at,
                edge.queued_at,
                processed_at,
            );
            log::debug!(
                concat!(
                    "GAMEPLAY INPUT EDGE: lane={} source={:?} slot={} pressed={} ",
                    "lane_was_down={} slot_was_down={} judges_tap={} judges_lift={} ",
                    "time_source={} song_clock_mapped={} edge_time_s={:.6} current_time_s={:.6} ",
                    "capture_queue_us={} queue_process_us={} capture_process_us={} pending={}"
                ),
                lane_idx,
                edge.source,
                edge.input_slot,
                edge.pressed,
                lane_was_down,
                slot_was_down,
                edge_judges_tap,
                edge_judges_lift,
                event_time_source,
                song_clock.mapped_audio,
                event_music_time,
                state.current_music_time_seconds(),
                latency.capture_to_queue_us,
                latency.queue_to_process_us,
                latency.capture_to_process_us,
                state.pending_input_len(),
            );
        }
        if edge_judges_tap {
            state.refresh_roll_life_on_step(lane_idx, edge.event_music_time_ns);
        }
        state.integrate_active_hold_to_time(lane_idx, edge.event_music_time_ns);
        if edge.record_replay {
            state.push_recorded_replay_edge(RecordedLaneEdge {
                lane_index: lane_idx as u8,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time_ns: edge.event_music_time_ns,
            });
        }
        if trace_enabled {
            let processed_at = Instant::now();
            let latency = gameplay_input_latency_sample(
                edge.captured_at,
                edge.stored_at,
                edge.emitted_at,
                edge.queued_at,
                processed_at,
            );
            state
                .control
                .update_trace
                .summary
                .record_input_latency(latency);
            if latency.capture_to_process_us >= GAMEPLAY_INPUT_LATENCY_WARN_US {
                log::debug!(
                    "Gameplay input latency spike: lane={} pressed={} source={:?} capture_store_us={} store_emit_us={} emit_queue_us={} queue_process_us={} capture_queue_us={} capture_process_us={} pending={} now_t={:.3} edge_t={:.3}",
                    lane_idx,
                    edge.pressed,
                    edge.source,
                    latency.capture_to_store_us,
                    latency.store_to_emit_us,
                    latency.emit_to_queue_us,
                    latency.queue_to_process_us,
                    latency.capture_to_queue_us,
                    latency.capture_to_process_us,
                    state.pending_input_len() + 1,
                    state.current_music_time_seconds(),
                    song_time_ns_to_seconds(edge.event_music_time_ns),
                );
            }
        }

        let state_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let lane_update = state.update_lane_input_slot(
            edge.lane,
            edge.source,
            edge.input_slot,
            edge.pressed,
            INPUT_SLOT_INVALID,
        );
        debug_assert_eq!(lane_update.slot_was_down, slot_was_down);
        if let Some(started) = state_started {
            add_elapsed_us(&mut phase_timings.input_state_us, started);
        }

        let press_started =
            lane_press_started(edge.pressed, lane_update.was_down, lane_update.is_down);
        let release_finished =
            lane_release_finished(edge.pressed, lane_update.was_down, lane_update.is_down);
        state.sync_active_hold_pressed_state(lane_idx, lane_update.is_down);

        if press_started {
            state.press_input_lane(lane_idx, edge.event_music_time_ns);
            state.record_step_calories_for_lane(lane_idx, edge.event_music_time_ns);
            if trace_enabled {
                let started = Instant::now();
                state.start_receptor_glow_press(lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                state.start_receptor_glow_press(lane_idx);
            }
        } else if release_finished {
            state.release_input_lane(lane_idx);
            if trace_enabled {
                let started = Instant::now();
                state.release_receptor_glow(lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                state.release_receptor_glow(lane_idx);
            }
        }

        if edge_judges_tap {
            let event_music_time_ns = edge.event_music_time_ns;
            let hit_note = if trace_enabled {
                let started = Instant::now();
                let hit_note = state.judge_a_tap(lane_idx, event_music_time_ns);
                add_elapsed_us(&mut phase_timings.input_judge_us, started);
                hit_note
            } else {
                state.judge_a_tap(lane_idx, event_music_time_ns)
            };
            if trace_enabled {
                let started = Instant::now();
                state.refresh_roll_life_on_step(lane_idx, event_music_time_ns);
                add_elapsed_us(&mut phase_timings.input_roll_us, started);
            } else {
                state.refresh_roll_life_on_step(lane_idx, event_music_time_ns);
            }
            if hit_note {
                if state.tick_mode() == GameplayTimingTickMode::Hit {
                    state.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
                        ASSIST_TICK_SFX_PATH,
                    ));
                }
            } else {
                state.trigger_receptor_step_pulse(lane_idx);
            }
        } else if edge_judges_lift {
            let hit_lift = state.judge_a_lift(lane_idx, edge.event_music_time_ns);
            if hit_lift && state.tick_mode() == GameplayTimingTickMode::Hit {
                state.push_audio_command(GameplayAudioCommand::PlayPreloadedAssistTick(
                    ASSIST_TICK_SFX_PATH,
                ));
            }
        }
    }
}

#[inline(always)]
pub fn handle_replay_edge<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    edge: RecordedLaneEdge,
) where
    Profile: GameplayProfileData,
{
    let col = edge.lane_index as usize;
    let Some(lane) = lane_from_column(col) else {
        return;
    };
    state.queue_current_time_input_edge(lane, |now| GameplayInputEdge {
        lane,
        input_slot: INPUT_SLOT_INVALID,
        pressed: edge.pressed,
        source: edge.source,
        record_replay: false,
        captured_at: now,
        captured_host_nanos: 0,
        stored_at: now,
        emitted_at: now,
        queued_at: now,
        event_music_time_ns: edge.event_music_time_ns,
    });
}

pub fn update_core<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
) -> GameplayAction
where
    Profile: GameplayProfileData,
{
    state.update_gameplay_frame(
        delta_time,
        audio_snapshot,
        ASSIST_TICK_SFX_PATH,
        fallback_host_nanos,
    )
}

#[inline(always)]
fn gameplay_menu_input(action: VirtualAction) -> Option<GameplayMenuInput> {
    match action {
        VirtualAction::p1_start => Some(GameplayMenuInput::P1Start),
        VirtualAction::p2_start => Some(GameplayMenuInput::P2Start),
        VirtualAction::p1_back => Some(GameplayMenuInput::P1Back),
        VirtualAction::p2_back => Some(GameplayMenuInput::P2Back),
        _ => None,
    }
}

fn queue_live_input_event<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    ev: &InputEvent,
) where
    Profile: GameplayProfileData,
{
    let Some(lane) = lane_from_action(ev.action) else {
        return;
    };
    state.queue_live_input_edge(
        lane,
        |lane, queued_at, event_music_time_ns, record_replay| GameplayInputEdge {
            lane,
            input_slot: ev.input_slot,
            pressed: ev.pressed,
            source: ev.source,
            record_replay,
            captured_at: ev.timestamp,
            captured_host_nanos: ev.timestamp_host_nanos,
            stored_at: ev.stored_at,
            emitted_at: ev.emitted_at,
            queued_at,
            event_music_time_ns,
        },
    );
}

pub fn handle_core_input<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    ev: &InputEvent,
) -> GameplayAction
where
    Profile: GameplayProfileData,
{
    if state.exit_transition_active() {
        return GameplayAction::None;
    }
    if lane_from_action(ev.action).is_some() {
        queue_live_input_event(state, ev);
        return GameplayAction::None;
    }
    if let Some(input) = gameplay_menu_input(ev.action) {
        state.handle_gameplay_menu_input(input, ev.pressed, ev.timestamp);
    }
    GameplayAction::None
}

#[inline(always)]
pub fn log_tap_judge_candidate(
    enabled: bool,
    reason: &str,
    player: usize,
    column: usize,
    current_row_index: usize,
    current_time_ns: SongTimeNs,
    note_index: usize,
    note: &Note,
    note_time_ns: SongTimeNs,
    rate: f32,
) {
    if !enabled {
        return;
    }
    let offset_music_ns = current_time_ns.saturating_sub(note_time_ns);
    log::debug!(
        concat!(
            "GAMEPLAY TAP JUDGE: reason={}, player={}, lane={}, note_index={}, ",
            "note_col={}, note_type={:?}, note_row={}, current_row={}, beat={:.3}, ",
            "quant={}, fake={}, can_be_judged={}, result_set={}, early_result_set={}, ",
            "note_time_s={:.6}, event_time_s={:.6}, offset_ms={:.2}, rate={:.3}"
        ),
        reason,
        player,
        column,
        note_index,
        note.column,
        note.note_type,
        note.row_index,
        current_row_index,
        note.beat,
        note.quantization_idx,
        note.is_fake,
        note.can_be_judged,
        note.result.is_some(),
        note.early_result.is_some(),
        song_time_ns_to_seconds(note_time_ns),
        song_time_ns_to_seconds(current_time_ns),
        judgment::judgment_time_error_ms_from_music_ns(offset_music_ns, rate),
        rate,
    );
}

#[inline(always)]
pub fn log_timing_hit_detail(
    enabled: bool,
    stream_pos_s: f32,
    grade: JudgeGrade,
    row_index: usize,
    col: usize,
    beat: f32,
    song_offset_s: f32,
    global_offset_s: f32,
    note_time_ns: SongTimeNs,
    event_time_ns: SongTimeNs,
    music_now_s: f32,
    rate: f32,
    lead_in_s: f32,
) {
    if !enabled {
        return;
    }
    let note_time_s = song_time_ns_to_seconds(note_time_ns);
    let event_time_s = song_time_ns_to_seconds(event_time_ns);
    let expected_stream_for_note_s =
        note_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let expected_stream_for_hit_s =
        event_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
    let stream_delta_hit_ms = (stream_pos_s - expected_stream_for_hit_s) * 1000.0;
    log::debug!(
        concat!(
            "TIMING HIT: grade={:?}, row={}, col={}, beat={:.3}, ",
            "song_offset_s={:.4}, global_offset_s={:.4}, ",
            "note_time_s={:.6}, event_time_s={:.6}, music_now_s={:.6}, ",
            "offset_ms={:.2}, rate={:.3}, lead_in_s={:.4}, ",
            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
            "stream_hit_s={:.6}, stream_delta_hit_ms={:.2}"
        ),
        grade,
        row_index,
        col,
        beat,
        song_offset_s,
        global_offset_s,
        note_time_s,
        event_time_s,
        music_now_s,
        ((event_time_s - note_time_s) / rate) * 1000.0,
        rate,
        lead_in_s,
        stream_pos_s,
        expected_stream_for_note_s,
        stream_delta_note_ms,
        expected_stream_for_hit_s,
        stream_delta_hit_ms,
    );
}

#[inline(always)]
pub fn tap_judgment_uses_bright_explosion_from_profile<Profile: GameplayProfileData>(
    profile: &Profile,
    judgment: &Judgment,
) -> bool {
    tap_judgment_uses_bright_explosion_for_options(profile.fantastic_feedback_options(), judgment)
}

pub fn tap_judgment_uses_bright_explosion_for_options(
    options: FantasticFeedbackOptions,
    judgment: &Judgment,
) -> bool {
    if !options.show_fa_plus_window || judgment.grade != JudgeGrade::Fantastic {
        return false;
    }
    if options.fa_plus_10ms_blue_window
        && !options.split_15_10ms
        && !options.custom_fantastic_window
    {
        return judgment.time_error_ms.abs() > FA_PLUS_W010_MS;
    }
    judgment.window == Some(TimingWindow::W1)
}

