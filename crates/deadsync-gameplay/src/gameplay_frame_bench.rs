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
