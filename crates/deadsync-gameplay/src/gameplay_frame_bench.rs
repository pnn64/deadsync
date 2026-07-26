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
