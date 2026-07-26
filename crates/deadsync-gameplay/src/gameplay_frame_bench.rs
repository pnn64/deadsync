#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayFrameHotPathBenchOutput {
    pub checksum: u64,
    pub samples: usize,
}

pub struct ActiveColumnScanBench {
    num_cols: usize,
    lane_counts: [u16; MAX_COLS],
    prev_inputs: [bool; MAX_COLS],
    lane_pressed_since_ns: [Option<SongTimeNs>; MAX_COLS],
}

#[derive(Clone)]
pub struct IdleAttackRefreshBench {
    state: ActiveAttackRefreshState,
}

#[derive(Clone)]
pub struct DisabledAssistClapBench {
    state: GameplayAssistClapState,
}

#[derive(Clone)]
pub struct CrossoverCueCursorBench {
    cues: Vec<ColumnCue>,
    cursor: usize,
}

impl Default for CrossoverCueCursorBench {
    fn default() -> Self {
        Self {
            cues: (0..8_192)
                .map(|index| ColumnCue {
                    start_time: index as f32 * 0.125,
                    duration: 0.2,
                    columns: Vec::new(),
                })
                .collect(),
            cursor: 0,
        }
    }
}

impl CrossoverCueCursorBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let current_time = frame as f32 / 120.0;
        self.cursor = self
            .cues
            .partition_point(|cue| cue.start_time <= current_time);
        let active = active_column_cue_range(&self.cues, current_time);
        record_crossover_cues(self.cursor, active)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let current_time = frame as f32 / 120.0;
        self.cursor = column_cue_cursor_from_hint(&self.cues, current_time, self.cursor);
        let active = active_column_cue_range_from_cursor(&self.cues, current_time, self.cursor);
        record_crossover_cues(self.cursor, active)
    }
}

#[inline(always)]
fn record_crossover_cues(
    cursor: usize,
    active: core::ops::Range<usize>,
) -> GameplayFrameHotPathBenchOutput {
    GameplayFrameHotPathBenchOutput {
        checksum: (cursor as u64).rotate_left(17)
            ^ (active.start as u64).rotate_left(7)
            ^ active.end as u64,
        samples: active.len(),
    }
}

impl Default for DisabledAssistClapBench {
    fn default() -> Self {
        Self {
            state: GameplayAssistClapState::new((0..8_192).map(|index| index * 12).collect()),
        }
    }
}

impl DisabledAssistClapBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let song_row = ((frame * 7) % (8_192 * 12)) as i32;
        let timeline_reset = self.state.note_sfx_generation(1);
        let update = self
            .state
            .schedule_update(song_row, song_row, false, timeline_reset);
        GameplayFrameHotPathBenchOutput {
            checksum: update.cursor as u64,
            samples: 1,
        }
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let song_row = ((frame * 7) % (8_192 * 12)) as i32;
        self.state.note_disabled(1);
        GameplayFrameHotPathBenchOutput {
            checksum: (song_row as usize / 12 + 1) as u64,
            samples: 1,
        }
    }
}

impl Default for IdleAttackRefreshBench {
    fn default() -> Self {
        let mut visual = VisualOverrides {
            drunk: Some(0.75),
            ..VisualOverrides::default()
        };
        visual.confusion_offset_cols[2] = Some(0.4);
        Self {
            state: ActiveAttackRefreshState {
                attack_current_appearance: AppearanceEffects {
                    hidden: 0.8,
                    sudden: 0.35,
                    stealth: 0.5,
                    ..AppearanceEffects::default()
                },
                active_attack_visual: visual,
                active_attack_visibility: VisibilityOverrides {
                    dark: Some(1.0),
                    ..VisibilityOverrides::default()
                },
                active_attack_scroll: ScrollOverrides {
                    reverse: Some(0.6),
                    ..ScrollOverrides::default()
                },
                active_attack_mini_percent: Some(40.0),
                outro_attack_visual: VisualOverrides::default(),
            },
        }
    }
}

impl IdleAttackRefreshBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let output = refresh_active_attack_player_full(self.input(frame), self.state);
        self.record(output)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let output = refresh_active_attack_player(self.input(frame), self.state);
        self.record(output)
    }

    fn input(&self, frame: usize) -> ActiveAttackRefreshInput<'static> {
        ActiveAttackRefreshInput {
            now: frame as f32 / 120.0,
            delta_time: 1.0 / 120.0,
            attacks_cleared_for_outro: false,
            base_appearance: AppearanceEffects {
                hidden: 0.1,
                sudden: 0.2,
                stealth: 0.05,
                ..AppearanceEffects::default()
            },
            base_visual: VisualEffects {
                drunk: 0.25,
                ..VisualEffects::default()
            },
            base_scroll: ScrollEffects {
                reverse: 0.2,
                ..ScrollEffects::default()
            },
            base_mini_percent: 15.0,
            attack_windows: &[],
            song_lua_ease_windows: &[],
        }
    }

    fn record(&mut self, output: ActiveAttackRefreshOutput) -> GameplayFrameHotPathBenchOutput {
        self.state.attack_current_appearance = output.attack_current_appearance;
        self.state.active_attack_visual = output.active_attack_visual;
        self.state.active_attack_visibility = output.active_attack_visibility;
        self.state.active_attack_scroll = output.active_attack_scroll;
        self.state.active_attack_mini_percent = output.active_attack_mini_percent;
        self.state.outro_attack_visual = output.outro_attack_visual;
        let values = [
            output.attack_target_appearance.hidden,
            output.attack_speed_appearance.stealth,
            output.attack_current_appearance.hidden,
            output.attack_current_appearance.sudden,
            output.attack_current_appearance.stealth,
            output.active_attack_visual.drunk.unwrap_or_default(),
            output.active_attack_scroll.reverse.unwrap_or_default(),
            output.active_attack_mini_percent.unwrap_or_default(),
        ];
        let checksum = values.into_iter().fold(0_u64, |checksum, value| {
            checksum.rotate_left(7) ^ u64::from(value.to_bits())
        });
        GameplayFrameHotPathBenchOutput {
            checksum,
            samples: values.len(),
        }
    }
}

impl Default for ActiveColumnScanBench {
    fn default() -> Self {
        let mut lane_counts = [0; MAX_COLS];
        lane_counts[0] = 1;
        lane_counts[2] = 2;
        let mut prev_inputs = [false; MAX_COLS];
        prev_inputs[0] = true;
        let mut lane_pressed_since_ns = [None; MAX_COLS];
        lane_pressed_since_ns[2] = Some(1_000_000_000);
        Self {
            num_cols: 4,
            lane_counts,
            prev_inputs,
            lane_pressed_since_ns,
        }
    }
}

impl ActiveColumnScanBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let previous_time_ns = 1_000_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        let current_time_ns = previous_time_ns.saturating_add(8_333_333);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        let current_inputs: [bool; MAX_COLS] = std::array::from_fn(|col| {
            let pressed = col < self.num_cols && self.lane_counts[col] != 0;
            record_pressed_input(&mut output, col, pressed);
            pressed
        });
        let starts: [Option<SongTimeNs>; MAX_COLS] = std::array::from_fn(|col| {
            if col >= self.num_cols {
                return None;
            }
            crossed_mine_held_start_time(
                current_inputs[col],
                self.prev_inputs[col],
                self.lane_pressed_since_ns[col],
                previous_time_ns,
                current_time_ns,
            )
        });
        for (col, start) in starts.into_iter().enumerate() {
            record_crossing(&mut output, col, start);
        }
        output
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let previous_time_ns = 1_000_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        let current_time_ns = previous_time_ns.saturating_add(8_333_333);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        let mut current_inputs = [false; MAX_COLS];
        for (col, input) in current_inputs
            .iter_mut()
            .enumerate()
            .take(self.num_cols.min(MAX_COLS))
        {
            *input = self.lane_counts[col] != 0;
            record_pressed_input(&mut output, col, *input);
        }
        for col in 0..self.num_cols.min(MAX_COLS) {
            let start = crossed_mine_held_start_time(
                current_inputs[col],
                self.prev_inputs[col],
                self.lane_pressed_since_ns[col],
                previous_time_ns,
                current_time_ns,
            );
            record_crossing(&mut output, col, start);
        }
        output
    }
}

#[inline(always)]
fn record_pressed_input(output: &mut GameplayFrameHotPathBenchOutput, col: usize, pressed: bool) {
    if pressed {
        output.checksum = output.checksum.rotate_left(5) ^ (col as u64 + 1);
        output.samples += 1;
    }
}

#[inline(always)]
fn record_crossing(
    output: &mut GameplayFrameHotPathBenchOutput,
    col: usize,
    start: Option<SongTimeNs>,
) {
    if let Some(start) = start {
        output.checksum = output.checksum.rotate_left(7) ^ start as u64 ^ ((col as u64 + 1) << 48);
        output.samples += 1;
    }
}

#[derive(Clone)]
pub struct LiveNotefieldOptionsBench {
    base_scroll: [ScrollEffects; MAX_PLAYERS],
    attack_scroll: [ScrollOverrides; MAX_PLAYERS],
    reverse: [bool; MAX_PLAYERS],
    column_dirs: [f32; MAX_COLS],
    motion: [[f32; 3]; MAX_PLAYERS],
}

impl Default for LiveNotefieldOptionsBench {
    fn default() -> Self {
        Self {
            base_scroll: [
                ScrollEffects {
                    reverse: 0.25,
                    split: 0.0,
                    alternate: 0.5,
                    cross: 0.0,
                    centered: 0.2,
                },
                ScrollEffects {
                    reverse: 0.75,
                    split: 0.4,
                    alternate: 0.0,
                    cross: 0.3,
                    centered: 0.6,
                },
            ],
            attack_scroll: [
                ScrollOverrides {
                    reverse: Some(0.8),
                    centered: Some(0.35),
                    ..ScrollOverrides::default()
                },
                ScrollOverrides {
                    alternate: Some(0.65),
                    cross: Some(0.2),
                    ..ScrollOverrides::default()
                },
            ],
            reverse: [false; MAX_PLAYERS],
            column_dirs: [1.0; MAX_COLS],
            motion: [[0.0; 3]; MAX_PLAYERS],
        }
    }
}

impl LiveNotefieldOptionsBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        const COLS_PER_PLAYER: usize = 4;
        for player in 0..MAX_PLAYERS {
            let scroll = resolve_bench_scroll(
                self.base_scroll[player],
                self.attack_scroll[player],
                player,
                frame,
            );
            self.reverse[player] = scroll.reverse_percent_for_column(0, COLS_PER_PLAYER) > 0.5;
            let start = player * COLS_PER_PLAYER;
            for local_col in 0..COLS_PER_PLAYER {
                self.column_dirs[start + local_col] =
                    scroll.reverse_scale_for_column(local_col, COLS_PER_PLAYER);
            }
        }
        for player in 0..MAX_PLAYERS {
            let scroll = resolve_bench_scroll(
                self.base_scroll[player],
                self.attack_scroll[player],
                player,
                frame,
            );
            self.motion[player] = bench_motion(frame, player, scroll);
        }
        live_options_output(self)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        const COLS_PER_PLAYER: usize = 4;
        for player in 0..MAX_PLAYERS {
            let scroll = resolve_bench_scroll(
                self.base_scroll[player],
                self.attack_scroll[player],
                player,
                frame,
            );
            self.reverse[player] = scroll.reverse_percent_for_column(0, COLS_PER_PLAYER) > 0.5;
            let start = player * COLS_PER_PLAYER;
            for local_col in 0..COLS_PER_PLAYER {
                self.column_dirs[start + local_col] =
                    scroll.reverse_scale_for_column(local_col, COLS_PER_PLAYER);
            }
            self.motion[player] = bench_motion(frame, player, scroll);
        }
        live_options_output(self)
    }
}

#[inline(never)]
fn resolve_bench_scroll(
    base: ScrollEffects,
    attack: ScrollOverrides,
    player: usize,
    frame: usize,
) -> ScrollEffects {
    effective_attack_scroll_effects((frame + player) % 257 == 0, base, attack)
}

#[inline(always)]
fn bench_motion(frame: usize, player: usize, scroll: ScrollEffects) -> [f32; 3] {
    let bpm = 90.0 + ((frame * 7 + player * 31) % 181) as f32;
    let speed = bpm * (1.5 + player as f32 * 0.25);
    let draw_before = (720.0 * (1.0 + player as f32 * 0.1)).max(1.0);
    let draw_after = 320.0 + 400.0 * scroll.centered.clamp(0.0, 1.0);
    [speed, draw_after, draw_before / speed]
}

fn live_options_output(bench: &LiveNotefieldOptionsBench) -> GameplayFrameHotPathBenchOutput {
    let mut output = GameplayFrameHotPathBenchOutput::default();
    for reverse in bench.reverse {
        output.checksum = output.checksum.rotate_left(3) ^ u64::from(reverse);
        output.samples += 1;
    }
    for direction in bench.column_dirs {
        output.checksum = output.checksum.rotate_left(5) ^ u64::from(direction.to_bits());
        output.samples += 1;
    }
    for values in bench.motion {
        for value in values {
            output.checksum = output.checksum.rotate_left(7) ^ u64::from(value.to_bits());
            output.samples += 1;
        }
    }
    output
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OptionalFrameWorkBench;

impl OptionalFrameWorkBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let replay_mode = frame % 1_009 == 0;
        let offset_adjust_active = frame % 257 == 0;
        let mut replay_events = [None; MAX_COLS];
        let replay_count = bench_collect_replay_edges(replay_mode, frame, &mut replay_events);
        let offset_tick = bench_tick_offset_adjust(offset_adjust_active, std::time::Instant::now());
        optional_work_output(replay_count, offset_tick)
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let replay_mode = frame % 1_009 == 0;
        let offset_adjust_active = frame % 257 == 0;
        let replay_count = if replay_mode {
            let mut replay_events = [None; MAX_COLS];
            bench_collect_replay_edges(true, frame, &mut replay_events)
        } else {
            0
        };
        let offset_tick =
            offset_adjust_active && bench_tick_offset_adjust(true, std::time::Instant::now());
        optional_work_output(replay_count, offset_tick)
    }
}

#[inline(never)]
fn bench_collect_replay_edges(
    replay_mode: bool,
    frame: usize,
    events: &mut [Option<RecordedLaneEdge>; MAX_COLS],
) -> usize {
    if !replay_mode {
        return 0;
    }
    events[0] = Some(RecordedLaneEdge {
        lane_index: 0,
        pressed: frame % 2 == 0,
        source: InputSource::Keyboard,
        event_music_time_ns: frame as SongTimeNs,
    });
    usize::from(events[0].is_some())
}

#[inline(never)]
fn bench_tick_offset_adjust(active: bool, now: std::time::Instant) -> bool {
    if !active {
        return false;
    }
    std::hint::black_box(now);
    true
}

#[inline(always)]
fn optional_work_output(replay_count: usize, offset_tick: bool) -> GameplayFrameHotPathBenchOutput {
    GameplayFrameHotPathBenchOutput {
        checksum: (replay_count as u64) << 1 | u64::from(offset_tick),
        samples: replay_count + usize::from(offset_tick),
    }
}

#[derive(Clone)]
pub struct IdleHoldPhaseBench {
    active_holds: [Option<usize>; MAX_COLS],
    decaying_hold_indices: Vec<usize>,
    pending_missed_hold_indices: Vec<usize>,
    num_cols: usize,
}

impl Default for IdleHoldPhaseBench {
    fn default() -> Self {
        Self {
            active_holds: [None; MAX_COLS],
            decaying_hold_indices: Vec::with_capacity(4),
            pending_missed_hold_indices: Vec::with_capacity(4),
            num_cols: 4,
        }
    }
}

impl IdleHoldPhaseBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        self.run_frame(false)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        self.run_frame(true)
    }

    fn prepare_frame(&mut self, frame: usize) {
        self.active_holds.fill(None);
        self.decaying_hold_indices.clear();
        self.pending_missed_hold_indices.clear();
        if frame % 67 == 0 {
            self.active_holds[frame / 67 % self.num_cols] = Some(frame);
        }
        if frame % 131 == 0 {
            self.decaying_hold_indices.push(frame % 4_096);
        }
        if frame % 193 == 0 {
            self.pending_missed_hold_indices.push(frame % 4_096);
        }
    }

    fn run_frame(&self, guarded: bool) -> GameplayFrameHotPathBenchOutput {
        let mut output = GameplayFrameHotPathBenchOutput::default();
        let active_idle = self.active_holds[..self.num_cols]
            .iter()
            .all(Option::is_none);
        if !guarded || !active_idle {
            let timing_players = [11_u64, 29_u64];
            let mut events = [None; MAX_COLS];
            for (column, active) in self.active_holds[..self.num_cols].iter().enumerate() {
                if let Some(note_index) = active {
                    events[output.samples] = Some((column, *note_index));
                    output.checksum = output.checksum.rotate_left(5)
                        ^ timing_players[column / self.num_cols]
                        ^ *note_index as u64;
                    output.samples += 1;
                }
            }
            std::hint::black_box(events);
        }

        if !guarded || !self.decaying_hold_indices.is_empty() {
            for &note_index in &self.decaying_hold_indices {
                output.checksum = output.checksum.rotate_left(7) ^ note_index as u64 ^ 0xD3CA_u64;
                output.samples += 1;
            }
        }

        if !guarded || !self.pending_missed_hold_indices.is_empty() {
            let mut score_missed_by_column = [false; MAX_COLS];
            for score in score_missed_by_column.iter_mut().take(self.num_cols) {
                *score = true;
            }
            let mut events = [None; MAX_COLS];
            for (event, &note_index) in events.iter_mut().zip(&self.pending_missed_hold_indices) {
                *event = Some(note_index);
                output.checksum = output.checksum.rotate_left(11)
                    ^ note_index as u64
                    ^ u64::from(score_missed_by_column[note_index % self.num_cols]);
                output.samples += 1;
            }
            std::hint::black_box(events);
        }
        output
    }
}

#[derive(Clone)]
pub struct IdleLaneScanBench {
    notes: Vec<Note>,
    note_times_ns: Vec<SongTimeNs>,
    held_window: Vec<bool>,
}

impl Default for IdleLaneScanBench {
    fn default() -> Self {
        let notes = (0..24)
            .map(|index| Note {
                beat: index as f32 * 0.25,
                quantization_idx: 0,
                column: index % 4,
                note_type: NoteType::Tap,
                row_index: index * 12,
                result: None,
                early_result: None,
                hold: None,
                mine_result: None,
                is_fake: false,
                can_be_judged: true,
            })
            .collect::<Vec<_>>();
        let note_times_ns = (0..notes.len())
            .map(|index| 900_000_000 + index as SongTimeNs * 10_000_000)
            .collect();
        Self {
            held_window: vec![false; notes.len()],
            notes,
            note_times_ns,
        }
    }
}

impl IdleLaneScanBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.run_frame(frame, false)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.run_frame(frame, true)
    }

    fn run_frame(
        &mut self,
        frame: usize,
        guard_idle: bool,
    ) -> GameplayFrameHotPathBenchOutput {
        let mut current_inputs = [false; MAX_COLS];
        current_inputs[0] = frame % 257 == 0;
        let current_inputs = std::hint::black_box(current_inputs);
        let mut output = GameplayFrameHotPathBenchOutput::default();

        if !guard_idle || current_inputs[..4].contains(&true) {
            let mut previous_inputs = [false; MAX_COLS];
            previous_inputs[0] = true;
            let pressed_since_ns = [None; MAX_COLS];
            for column in 0..4 {
                if crossed_mine_held_start_time(
                    current_inputs[column],
                    previous_inputs[column],
                    pressed_since_ns[column],
                    1_000_000_000,
                    1_008_333_333,
                )
                .is_some()
                {
                    output.checksum ^= 1_u64 << column;
                }
            }
            let _ = track_held_miss_windows_for_players(
                &self.notes,
                &self.note_times_ns,
                &mut self.held_window,
                &[(0, self.notes.len()), (0, 0)],
                &[0; MAX_PLAYERS],
                &[180_000_000, 0],
                1,
                4,
                &current_inputs,
                1_008_333_333,
            );
        }

        std::hint::black_box(&self.held_window);
        output.checksum = output.checksum.rotate_left(5)
            ^ u64::from(self.held_window.first().copied().unwrap_or(false))
            ^ u64::from(self.held_window.last().copied().unwrap_or(false)).rotate_left(1);
        output.samples += usize::from(self.held_window.iter().any(|held| *held));
        output
    }
}

#[derive(Clone)]
pub struct SharedMissCutoffBench {
    timing: TimingData,
    timing_players: [TimingData; MAX_PLAYERS],
    profile: TimingProfile,
    caches: GameplayTimeToBeatCaches,
}

impl Default for SharedMissCutoffBench {
    fn default() -> Self {
        let row_to_beat = (0..=2_048)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect::<Vec<_>>();
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0), (16.0, 180.0), (32.0, 90.0)],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let timing_players = [timing.clone(), timing.clone()];
        let player_refs = [&timing_players[0], &timing_players[1]];
        let caches = GameplayTimeToBeatCaches::new(&timing, &player_refs);
        Self {
            timing,
            timing_players,
            profile: TimingProfile::default_itg_with_fa_plus(),
            caches,
        }
    }
}

impl SharedMissCutoffBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.run_frame(frame, false)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.run_frame(frame, true)
    }

    fn run_frame(
        &mut self,
        frame: usize,
        share_cutoff: bool,
    ) -> GameplayFrameHotPathBenchOutput {
        let music_time_ns = 500_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        let player_refs = [&self.timing_players[0], &self.timing_players[1]];
        let rows = self.caches.missed_note_cutoff_rows(
            &self.profile,
            &player_refs,
            1.0,
            music_time_ns,
            2,
        );
        let second_rows = if share_cutoff {
            rows
        } else {
            self.caches.missed_note_cutoff_rows(
                &self.profile,
                &player_refs,
                1.0,
                music_time_ns,
                2,
            )
        };
        let mut output = GameplayFrameHotPathBenchOutput::default();
        for cutoff in rows.into_iter().chain(second_rows) {
            output.checksum = output.checksum.rotate_left(7) ^ cutoff as u64;
            output.samples += usize::from(cutoff != 0);
        }
        std::hint::black_box(&self.timing);
        output
    }
}
