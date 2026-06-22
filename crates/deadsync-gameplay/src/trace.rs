#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayUpdatePhaseTimings {
    pub pre_notes_us: u32,
    pub autoplay_us: u32,
    pub input_edges_us: u32,
    pub input_queue_us: u32,
    pub input_state_us: u32,
    pub input_glow_us: u32,
    pub input_judge_us: u32,
    pub input_roll_us: u32,
    pub held_mines_us: u32,
    pub active_holds_us: u32,
    pub hold_decay_us: u32,
    pub visuals_us: u32,
    pub spawn_arrows_us: u32,
    pub mine_avoid_us: u32,
    pub tap_miss_us: u32,
    pub cull_us: u32,
    pub judged_rows_us: u32,
    pub density_us: u32,
    pub density_sample_us: u32,
    pub danger_us: u32,
    pub untracked_us: u32,
}

#[inline(always)]
pub fn gameplay_update_hot_phase(phases: &GameplayUpdatePhaseTimings) -> (&'static str, u32) {
    let mut best = ("pre_notes", phases.pre_notes_us);
    if phases.autoplay_us > best.1 {
        best = ("autoplay", phases.autoplay_us);
    }
    if phases.input_edges_us > best.1 {
        best = ("input_edges", phases.input_edges_us);
    }
    if phases.held_mines_us > best.1 {
        best = ("held_mines", phases.held_mines_us);
    }
    if phases.active_holds_us > best.1 {
        best = ("active_holds", phases.active_holds_us);
    }
    if phases.hold_decay_us > best.1 {
        best = ("hold_decay", phases.hold_decay_us);
    }
    if phases.visuals_us > best.1 {
        best = ("visuals", phases.visuals_us);
    }
    if phases.spawn_arrows_us > best.1 {
        best = ("spawn_arrows", phases.spawn_arrows_us);
    }
    if phases.mine_avoid_us > best.1 {
        best = ("mine_avoid", phases.mine_avoid_us);
    }
    if phases.tap_miss_us > best.1 {
        best = ("tap_miss", phases.tap_miss_us);
    }
    if phases.cull_us > best.1 {
        best = ("cull", phases.cull_us);
    }
    if phases.judged_rows_us > best.1 {
        best = ("judged_rows", phases.judged_rows_us);
    }
    if phases.density_us > best.1 {
        best = ("density", phases.density_us);
    }
    if phases.danger_us > best.1 {
        best = ("danger", phases.danger_us);
    }
    if phases.untracked_us > best.1 {
        best = ("untracked", phases.untracked_us);
    }
    best
}

#[inline(always)]
pub fn accumulate_gameplay_update_phase_max(
    dst: &mut GameplayUpdatePhaseTimings,
    src: &GameplayUpdatePhaseTimings,
) {
    dst.pre_notes_us = dst.pre_notes_us.max(src.pre_notes_us);
    dst.autoplay_us = dst.autoplay_us.max(src.autoplay_us);
    dst.input_edges_us = dst.input_edges_us.max(src.input_edges_us);
    dst.input_queue_us = dst.input_queue_us.max(src.input_queue_us);
    dst.input_state_us = dst.input_state_us.max(src.input_state_us);
    dst.input_glow_us = dst.input_glow_us.max(src.input_glow_us);
    dst.input_judge_us = dst.input_judge_us.max(src.input_judge_us);
    dst.input_roll_us = dst.input_roll_us.max(src.input_roll_us);
    dst.held_mines_us = dst.held_mines_us.max(src.held_mines_us);
    dst.active_holds_us = dst.active_holds_us.max(src.active_holds_us);
    dst.hold_decay_us = dst.hold_decay_us.max(src.hold_decay_us);
    dst.visuals_us = dst.visuals_us.max(src.visuals_us);
    dst.spawn_arrows_us = dst.spawn_arrows_us.max(src.spawn_arrows_us);
    dst.mine_avoid_us = dst.mine_avoid_us.max(src.mine_avoid_us);
    dst.tap_miss_us = dst.tap_miss_us.max(src.tap_miss_us);
    dst.cull_us = dst.cull_us.max(src.cull_us);
    dst.judged_rows_us = dst.judged_rows_us.max(src.judged_rows_us);
    dst.density_us = dst.density_us.max(src.density_us);
    dst.density_sample_us = dst.density_sample_us.max(src.density_sample_us);
    dst.danger_us = dst.danger_us.max(src.danger_us);
    dst.untracked_us = dst.untracked_us.max(src.untracked_us);
}

#[inline(always)]
pub fn gameplay_update_tracked_phase_total_us(phases: &GameplayUpdatePhaseTimings) -> u32 {
    phases
        .pre_notes_us
        .saturating_add(phases.autoplay_us)
        .saturating_add(phases.input_edges_us)
        .saturating_add(phases.held_mines_us)
        .saturating_add(phases.active_holds_us)
        .saturating_add(phases.hold_decay_us)
        .saturating_add(phases.visuals_us)
        .saturating_add(phases.spawn_arrows_us)
        .saturating_add(phases.mine_avoid_us)
        .saturating_add(phases.tap_miss_us)
        .saturating_add(phases.cull_us)
        .saturating_add(phases.judged_rows_us)
        .saturating_add(phases.density_us)
        .saturating_add(phases.danger_us)
}

#[inline(always)]
pub const fn gameplay_trace_frame_is_slow(total_us: u32, hot_phase_us: u32) -> bool {
    total_us >= GAMEPLAY_TRACE_SLOW_FRAME_US || hot_phase_us >= GAMEPLAY_TRACE_PHASE_SPIKE_US
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayUpdateTraceSummary {
    pub frame_counter: u64,
    pub elapsed_s: f32,
    pub frames: u32,
    pub slow_frames: u32,
    pub max_total_us: u32,
    pub max_phase: GameplayUpdatePhaseTimings,
    pub input_latency: GameplayInputLatencyTrace,
    pub peak_pending_edges: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayUpdateTraceFrame {
    pub frame_counter: u64,
    pub phases: GameplayUpdatePhaseTimings,
    pub hot_phase_name: &'static str,
    pub hot_phase_us: u32,
    pub slow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayCapacityTraceKind {
    PendingEdges,
    ReplayEdges,
    DecayingHoldIndices,
    DensityGraphLifePoints(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayCapacityTraceEvent {
    pub kind: GameplayCapacityTraceKind,
    pub old_capacity: usize,
    pub new_capacity: usize,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayCapacityTraceSnapshot {
    pub pending_edges_capacity: usize,
    pub pending_edges_len: usize,
    pub replay_edges_capacity: usize,
    pub replay_edges_len: usize,
    pub decaying_hold_capacity: usize,
    pub decaying_hold_len: usize,
    pub density_life_capacity: [usize; MAX_PLAYERS],
    pub density_life_len: [usize; MAX_PLAYERS],
    pub num_players: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameplayUpdateTraceState {
    pub summary: GameplayUpdateTraceSummary,
    pending_edges_capacity: usize,
    replay_edges_capacity: usize,
    decaying_hold_capacity: usize,
    density_life_capacity: [usize; MAX_PLAYERS],
}

impl Default for GameplayUpdateTraceState {
    fn default() -> Self {
        Self {
            summary: GameplayUpdateTraceSummary::default(),
            pending_edges_capacity: 0,
            replay_edges_capacity: 0,
            decaying_hold_capacity: 0,
            density_life_capacity: [0; MAX_PLAYERS],
        }
    }
}

impl GameplayUpdateTraceState {
    #[inline(always)]
    pub fn from_capacity_snapshot(snapshot: &GameplayCapacityTraceSnapshot) -> Self {
        let mut trace = Self {
            pending_edges_capacity: snapshot.pending_edges_capacity,
            replay_edges_capacity: snapshot.replay_edges_capacity,
            decaying_hold_capacity: snapshot.decaying_hold_capacity,
            ..Self::default()
        };
        let players = snapshot.num_players.min(MAX_PLAYERS);
        trace.density_life_capacity[..players]
            .copy_from_slice(&snapshot.density_life_capacity[..players]);
        trace
    }

    pub fn collect_capacity_growth(
        &mut self,
        snapshot: &GameplayCapacityTraceSnapshot,
        out: &mut [Option<GameplayCapacityTraceEvent>],
    ) -> usize {
        let mut count = 0;
        count += record_capacity_growth(
            &mut self.pending_edges_capacity,
            snapshot.pending_edges_capacity,
            snapshot.pending_edges_len,
            GameplayCapacityTraceKind::PendingEdges,
            out.get_mut(count),
        );
        count += record_capacity_growth(
            &mut self.replay_edges_capacity,
            snapshot.replay_edges_capacity,
            snapshot.replay_edges_len,
            GameplayCapacityTraceKind::ReplayEdges,
            out.get_mut(count),
        );
        count += record_capacity_growth(
            &mut self.decaying_hold_capacity,
            snapshot.decaying_hold_capacity,
            snapshot.decaying_hold_len,
            GameplayCapacityTraceKind::DecayingHoldIndices,
            out.get_mut(count),
        );

        for player in 0..snapshot.num_players.min(MAX_PLAYERS) {
            count += record_capacity_growth(
                &mut self.density_life_capacity[player],
                snapshot.density_life_capacity[player],
                snapshot.density_life_len[player],
                GameplayCapacityTraceKind::DensityGraphLifePoints(player),
                out.get_mut(count),
            );
        }
        count
    }
}

fn trace_capacity_growth<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
) where
    Profile: GameplayProfileData,
{
    let frame = state.control.update_trace.summary.frame_counter;
    let snapshot = state.capacity_trace_snapshot();
    let mut events = [None; 3 + MAX_PLAYERS];
    let event_count = state
        .control
        .update_trace
        .collect_capacity_growth(&snapshot, &mut events);
    for event in events.iter().take(event_count).flatten() {
        match event.kind {
            GameplayCapacityTraceKind::PendingEdges => log::debug!(
                "Gameplay vec growth frame={frame}: pending_edges capacity {} -> {} (len={})",
                event.old_capacity,
                event.new_capacity,
                event.len
            ),
            GameplayCapacityTraceKind::ReplayEdges => log::debug!(
                "Gameplay vec growth frame={frame}: replay_edges capacity {} -> {} (len={})",
                event.old_capacity,
                event.new_capacity,
                event.len
            ),
            GameplayCapacityTraceKind::DecayingHoldIndices => log::debug!(
                "Gameplay vec growth frame={frame}: decaying_hold_indices capacity {} -> {} (len={})",
                event.old_capacity,
                event.new_capacity,
                event.len
            ),
            GameplayCapacityTraceKind::DensityGraphLifePoints(player) => log::debug!(
                "Gameplay vec growth frame={frame}: density_graph_life_points[{player}] capacity {} -> {} (len={})",
                event.old_capacity,
                event.new_capacity,
                event.len
            ),
        }
    }
}

pub fn trace_gameplay_update<Profile, OverlayActor, CapturedActor, StateDelta>(
    state: &mut GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>,
    delta_time: f32,
    music_time_sec: f32,
    total_us: u32,
    phases: GameplayUpdatePhaseTimings,
) where
    Profile: GameplayProfileData,
{
    let pending_len = state.pending_input_len();
    let replay_edges_len = state.recorded_replay_edges().len();
    let decaying_len = state.decaying_hold_indices().len();
    let frame =
        state
            .control
            .update_trace
            .summary
            .record_frame(delta_time, total_us, phases, pending_len);
    let frame_counter = frame.frame_counter;
    let phases = frame.phases;

    if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
        log::debug!(
            "Gameplay input backlog: frame={}, pending_edges={}, replay_edges={}",
            frame_counter,
            pending_len,
            replay_edges_len
        );
    }

    if frame.slow {
        log::debug!(
            "Gameplay slow frame={} t={:.3}s total={:.3}ms hot={}({:.3}ms) pending={} decays={} phases_ms=[pre:{:.3} auto:{:.3} input:{:.3} held:{:.3} holds:{:.3} decay:{:.3} vis:{:.3} spawn:{:.3} mine:{:.3} tmiss:{:.3} cull:{:.3} judged:{:.3} density:{:.3} danger:{:.3} other:{:.3}] input_sub_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] density_sub_ms=[sample:{:.3}]",
            frame_counter,
            music_time_sec,
            total_us as f32 / 1000.0,
            frame.hot_phase_name,
            frame.hot_phase_us as f32 / 1000.0,
            pending_len,
            decaying_len,
            phases.pre_notes_us as f32 / 1000.0,
            phases.autoplay_us as f32 / 1000.0,
            phases.input_edges_us as f32 / 1000.0,
            phases.held_mines_us as f32 / 1000.0,
            phases.active_holds_us as f32 / 1000.0,
            phases.hold_decay_us as f32 / 1000.0,
            phases.visuals_us as f32 / 1000.0,
            phases.spawn_arrows_us as f32 / 1000.0,
            phases.mine_avoid_us as f32 / 1000.0,
            phases.tap_miss_us as f32 / 1000.0,
            phases.cull_us as f32 / 1000.0,
            phases.judged_rows_us as f32 / 1000.0,
            phases.density_us as f32 / 1000.0,
            phases.danger_us as f32 / 1000.0,
            phases.untracked_us as f32 / 1000.0,
            phases.input_queue_us as f32 / 1000.0,
            phases.input_state_us as f32 / 1000.0,
            phases.input_glow_us as f32 / 1000.0,
            phases.input_judge_us as f32 / 1000.0,
            phases.input_roll_us as f32 / 1000.0,
            phases.density_sample_us as f32 / 1000.0
        );
    }

    if log::log_enabled!(log::Level::Trace)
        && state.control.update_trace.summary.should_log_summary()
    {
        let summary = state.control.update_trace.summary;
        let summary_frames = summary.frames;
        let summary_slow_frames = summary.slow_frames;
        let summary_max_total_us = summary.max_total_us;
        let summary_max_phase = summary.max_phase;
        let summary_input_latency = summary.input_latency;
        let summary_peak_pending_edges = summary.peak_pending_edges;
        let (summary_hot_name, summary_hot_us) = gameplay_update_hot_phase(&summary_max_phase);
        log::trace!(
            "Gameplay trace summary: frames={} slow={} max_total={:.3}ms max_hot={}({:.3}ms) peak_pending={} input_sub_max_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] input_latency_us=[samples:{} cap_store_avg:{:.1} cap_store_max:{} store_emit_avg:{:.1} store_emit_max:{} emit_queue_avg:{:.1} emit_queue_max:{} queue_proc_avg:{:.1} queue_proc_max:{} cap_proc_avg:{:.1} cap_proc_max:{}] density_sub_max_ms=[sample:{:.3}] other_max={:.3}",
            summary_frames,
            summary_slow_frames,
            summary_max_total_us as f32 / 1000.0,
            summary_hot_name,
            summary_hot_us as f32 / 1000.0,
            summary_peak_pending_edges,
            summary_max_phase.input_queue_us as f32 / 1000.0,
            summary_max_phase.input_state_us as f32 / 1000.0,
            summary_max_phase.input_glow_us as f32 / 1000.0,
            summary_max_phase.input_judge_us as f32 / 1000.0,
            summary_max_phase.input_roll_us as f32 / 1000.0,
            summary_input_latency.samples,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_store_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_store_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.store_to_emit_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.store_to_emit_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.emit_to_queue_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.emit_to_queue_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.queue_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.queue_to_process_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_process_max_us,
            summary_max_phase.density_sample_us as f32 / 1000.0,
            summary_max_phase.untracked_us as f32 / 1000.0
        );
        state.control.update_trace.summary.reset_interval();
    }

    trace_capacity_growth(state);
}

#[inline(always)]
fn record_capacity_growth(
    old: &mut usize,
    new_capacity: usize,
    len: usize,
    kind: GameplayCapacityTraceKind,
    slot: Option<&mut Option<GameplayCapacityTraceEvent>>,
) -> usize {
    if new_capacity <= *old {
        return 0;
    }
    let old_capacity = *old;
    *old = new_capacity;
    if let Some(slot) = slot {
        *slot = Some(GameplayCapacityTraceEvent {
            kind,
            old_capacity,
            new_capacity,
            len,
        });
    }
    1
}

impl GameplayUpdateTraceSummary {
    #[inline(always)]
    pub fn record_frame(
        &mut self,
        delta_time: f32,
        total_us: u32,
        mut phases: GameplayUpdatePhaseTimings,
        pending_edges: usize,
    ) -> GameplayUpdateTraceFrame {
        phases.untracked_us =
            total_us.saturating_sub(gameplay_update_tracked_phase_total_us(&phases));
        self.frame_counter = self.frame_counter.wrapping_add(1);
        self.elapsed_s += delta_time.max(0.0);
        self.frames = self.frames.saturating_add(1);
        self.max_total_us = self.max_total_us.max(total_us);
        accumulate_gameplay_update_phase_max(&mut self.max_phase, &phases);
        self.peak_pending_edges = self.peak_pending_edges.max(pending_edges);

        let (hot_phase_name, hot_phase_us) = gameplay_update_hot_phase(&phases);
        let slow = gameplay_trace_frame_is_slow(total_us, hot_phase_us);
        if slow {
            self.slow_frames = self.slow_frames.saturating_add(1);
        }

        GameplayUpdateTraceFrame {
            frame_counter: self.frame_counter,
            phases,
            hot_phase_name,
            hot_phase_us,
            slow,
        }
    }

    #[inline(always)]
    pub fn record_input_latency(&mut self, sample: GameplayInputLatencySample) {
        self.input_latency.record_sample(sample);
    }

    #[inline(always)]
    pub fn should_log_summary(&self) -> bool {
        self.elapsed_s >= GAMEPLAY_TRACE_SUMMARY_INTERVAL_S
    }

    #[inline(always)]
    pub fn reset_interval(&mut self) {
        let frame_counter = self.frame_counter;
        *self = Self {
            frame_counter,
            ..Self::default()
        };
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayInputLatencyTrace {
    pub samples: u32,
    pub capture_to_store_total_us: u64,
    pub store_to_emit_total_us: u64,
    pub emit_to_queue_total_us: u64,
    pub capture_to_process_total_us: u64,
    pub queue_to_process_total_us: u64,
    pub capture_to_store_max_us: u32,
    pub store_to_emit_max_us: u32,
    pub emit_to_queue_max_us: u32,
    pub capture_to_process_max_us: u32,
    pub queue_to_process_max_us: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayInputLatencySample {
    pub capture_to_store_us: u32,
    pub store_to_emit_us: u32,
    pub emit_to_queue_us: u32,
    pub capture_to_queue_us: u32,
    pub capture_to_process_us: u32,
    pub queue_to_process_us: u32,
}

impl GameplayInputLatencyTrace {
    #[inline(always)]
    pub fn record(
        &mut self,
        capture_to_store_us: u32,
        store_to_emit_us: u32,
        emit_to_queue_us: u32,
        capture_to_process_us: u32,
        queue_to_process_us: u32,
    ) {
        self.samples = self.samples.saturating_add(1);
        self.capture_to_store_total_us = self
            .capture_to_store_total_us
            .saturating_add(u64::from(capture_to_store_us));
        self.store_to_emit_total_us = self
            .store_to_emit_total_us
            .saturating_add(u64::from(store_to_emit_us));
        self.emit_to_queue_total_us = self
            .emit_to_queue_total_us
            .saturating_add(u64::from(emit_to_queue_us));
        self.capture_to_process_total_us = self
            .capture_to_process_total_us
            .saturating_add(u64::from(capture_to_process_us));
        self.queue_to_process_total_us = self
            .queue_to_process_total_us
            .saturating_add(u64::from(queue_to_process_us));
        self.capture_to_store_max_us = self.capture_to_store_max_us.max(capture_to_store_us);
        self.store_to_emit_max_us = self.store_to_emit_max_us.max(store_to_emit_us);
        self.emit_to_queue_max_us = self.emit_to_queue_max_us.max(emit_to_queue_us);
        self.capture_to_process_max_us = self.capture_to_process_max_us.max(capture_to_process_us);
        self.queue_to_process_max_us = self.queue_to_process_max_us.max(queue_to_process_us);
    }

    #[inline(always)]
    pub fn record_sample(&mut self, sample: GameplayInputLatencySample) {
        self.record(
            sample.capture_to_store_us,
            sample.store_to_emit_us,
            sample.emit_to_queue_us,
            sample.capture_to_process_us,
            sample.queue_to_process_us,
        );
    }

    #[inline(always)]
    pub fn avg_us(total_us: u64, samples: u32) -> f32 {
        if samples == 0 {
            0.0
        } else {
            total_us as f32 / samples as f32
        }
    }
}

#[inline(always)]
pub fn gameplay_input_latency_sample(
    captured_at: Instant,
    stored_at: Instant,
    emitted_at: Instant,
    queued_at: Instant,
    processed_at: Instant,
) -> GameplayInputLatencySample {
    GameplayInputLatencySample {
        capture_to_store_us: saturating_elapsed_us_between(stored_at, captured_at),
        store_to_emit_us: saturating_elapsed_us_between(emitted_at, stored_at),
        emit_to_queue_us: saturating_elapsed_us_between(queued_at, emitted_at),
        capture_to_queue_us: saturating_elapsed_us_between(queued_at, captured_at),
        capture_to_process_us: saturating_elapsed_us_between(processed_at, captured_at),
        queue_to_process_us: saturating_elapsed_us_between(processed_at, queued_at),
    }
}

#[inline(always)]
pub fn saturating_elapsed_us_between(later: Instant, earlier: Instant) -> u32 {
    let elapsed = later
        .checked_duration_since(earlier)
        .unwrap_or(Duration::ZERO)
        .as_micros();
    if elapsed > u128::from(u32::MAX) {
        u32::MAX
    } else {
        elapsed as u32
    }
}

#[inline(always)]
pub fn elapsed_us_since(started: Instant) -> u32 {
    saturating_elapsed_us_between(Instant::now(), started)
}

#[inline(always)]
pub fn add_elapsed_us(dst: &mut u32, started: Instant) {
    *dst = dst.saturating_add(elapsed_us_since(started));
}

