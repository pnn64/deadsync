#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayFrameHotPathBenchOutput {
    pub checksum: u64,
    pub samples: usize,
}

#[derive(Clone)]
pub struct DisabledComboMilestoneBench {
    player: PlayerRuntime,
}

impl DisabledComboMilestoneBench {
    pub fn legacy() -> Self {
        Self {
            player: init_player_runtime(),
        }
    }

    pub fn gated() -> Self {
        Self {
            player: init_player_runtime_with_caps(PlayerBufferCaps::EMPTY),
        }
    }

    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let update = disabled_combo_milestone_update(&mut self.player, frame);
        apply_combo_update(&mut self.player, update, true);
        tick_player_combo_milestones(&mut self.player, 1.0 / 120.0);
        disabled_combo_milestone_output(&self.player, frame)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let update = disabled_combo_milestone_update(&mut self.player, frame);
        apply_combo_update(&mut self.player, update, false);
        disabled_combo_milestone_output(&self.player, frame)
    }
}

#[inline(always)]
fn disabled_combo_milestone_update(player: &mut PlayerRuntime, frame: usize) -> ComboUpdate {
    let combo_broken = frame % 1_009 == 0;
    if combo_broken {
        player.current_combo_window_counts.w1 = 7;
    }
    ComboUpdate {
        combo_broken,
        hit_hundred_milestone: frame % 120 == 0,
        hit_thousand_milestone: frame % 1_200 == 0,
    }
}

#[inline(always)]
fn disabled_combo_milestone_output(
    player: &PlayerRuntime,
    frame: usize,
) -> GameplayFrameHotPathBenchOutput {
    GameplayFrameHotPathBenchOutput {
        checksum: (frame as u64).rotate_left(7)
            ^ u64::from(player.current_combo_window_counts.w1),
        samples: usize::from(player.current_combo_window_counts.w1 != 0),
    }
}

#[derive(Clone)]
pub struct SongLuaNoteHideIndexBench {
    legacy_windows: Vec<SongLuaNoteHideWindowRuntime>,
    indexed_windows: SongLuaNoteHideWindows,
}

impl Default for SongLuaNoteHideIndexBench {
    fn default() -> Self {
        let legacy_windows = (0..256)
            .rev()
            .map(|index| SongLuaNoteHideWindowRuntime {
                column: index % MAX_COLS,
                start_beat: (index / MAX_COLS) as f32 * 4.0,
                end_beat: (index / MAX_COLS) as f32 * 4.0 + 2.0,
            })
            .collect::<Vec<_>>();
        Self {
            indexed_windows: SongLuaNoteHideWindows::new(legacy_windows.clone()),
            legacy_windows,
        }
    }
}

impl SongLuaNoteHideIndexBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, |column, beat| {
            legacy_song_lua_note_hidden(&self.legacy_windows, column, beat)
        })
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, |column, beat| {
            song_lua_note_hidden(&self.indexed_windows, column, beat)
        })
    }

    fn frame(
        &self,
        frame: usize,
        mut hidden: impl FnMut(usize, f32) -> bool,
    ) -> GameplayFrameHotPathBenchOutput {
        let mut output = GameplayFrameHotPathBenchOutput::default();
        for sample in 0..96 {
            let column = (frame + sample) % MAX_COLS;
            let beat = ((frame * 3 + sample * 17) % 256) as f32 * 0.5;
            let is_hidden = hidden(column, beat);
            output.checksum = output.checksum.rotate_left(5)
                ^ u64::from(beat.to_bits())
                ^ column as u64
                ^ u64::from(is_hidden);
            output.samples += usize::from(is_hidden);
        }
        output
    }
}

#[inline(always)]
fn legacy_song_lua_note_hidden(
    windows: &[SongLuaNoteHideWindowRuntime],
    local_col: usize,
    beat: f32,
) -> bool {
    const EPS: f32 = 1.0e-4;
    windows.iter().any(|window| {
        window.column == local_col
            && beat + EPS >= window.start_beat
            && beat <= window.end_beat + EPS
    })
}

#[derive(Clone)]
pub struct InputLaneSearchCursorBench {
    notes: Vec<Note>,
    note_indices: Vec<usize>,
    note_times_ns: Vec<SongTimeNs>,
    timing: TimingData,
    cursor: LaneNoteWindowCursor,
}

impl Default for InputLaneSearchCursorBench {
    fn default() -> Self {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..TimingSegments::default()
            },
            &[],
        );
        let notes = (0..8_192)
            .map(|index| {
                let beat = index as f32 * 0.5;
                Note {
                    beat,
                    quantization_idx: 0,
                    column: 0,
                    note_type: NoteType::Tap,
                    row_index: index * 24,
                    result: None,
                    early_result: None,
                    hold: None,
                    mine_result: None,
                    is_fake: false,
                    can_be_judged: true,
                }
            })
            .collect::<Vec<_>>();
        let note_times_ns = notes
            .iter()
            .map(|note| timing.get_time_for_beat_ns(note.beat))
            .collect();
        Self {
            note_indices: (0..notes.len()).collect(),
            notes,
            note_times_ns,
            timing,
            cursor: LaneNoteWindowCursor::default(),
        }
    }
}

impl InputLaneSearchCursorBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let (current_time_ns, rows) = input_search_frame(frame);
        record_lane_search(closest_lane_note_search_with_rows(
            &self.note_indices,
            &self.notes,
            &self.note_times_ns,
            &self.timing,
            current_time_ns,
            rows,
        ))
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let (current_time_ns, rows) = input_search_frame(frame);
        record_lane_search(closest_lane_note_search_with_rows_from_cursor(
            &self.note_indices,
            &self.notes,
            &self.note_times_ns,
            &self.timing,
            current_time_ns,
            rows,
            &mut self.cursor,
        ))
    }
}

#[inline(always)]
fn input_search_frame(frame: usize) -> (SongTimeNs, LaneSearchRows) {
    let current = if frame.is_multiple_of(1_009) {
        (frame * 97) % 180_000
    } else {
        frame * 2
    };
    let rows = LaneSearchRows {
        current,
        start: current.saturating_sub(96),
        end: current.saturating_add(97),
    };
    let current_time_ns =
        (current as SongTimeNs).saturating_mul(1_000_000_000) / (ROWS_PER_BEAT as SongTimeNs * 2);
    (current_time_ns, rows)
}

#[inline(always)]
fn record_lane_search(search: LaneNoteSearch) -> GameplayFrameHotPathBenchOutput {
    let mut output = GameplayFrameHotPathBenchOutput {
        checksum: (search.search_start_idx as u64).rotate_left(11)
            ^ (search.search_end_idx as u64).rotate_left(23),
        samples: search
            .search_end_idx
            .saturating_sub(search.search_start_idx),
    };
    if let Some((note_index, error_ns)) = search.candidate {
        output.checksum ^= (note_index as u64).rotate_left(37) ^ error_ns as u64;
        output.samples += 1;
    }
    output
}

#[derive(Clone)]
pub struct InputQueueDrainBench {
    deque: VecDeque<GameplayInputEdge>,
    vector: Vec<GameplayInputEdge>,
    edges: [GameplayInputEdge; 8],
}

impl Default for InputQueueDrainBench {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            deque: VecDeque::with_capacity(8),
            vector: Vec::with_capacity(8),
            edges: std::array::from_fn(|index| GameplayInputEdge {
                lane: lane_from_column(index).expect("benchmark lane"),
                input_slot: index as u32,
                pressed: index % 2 == 0,
                source: InputSource::Keyboard,
                record_replay: false,
                captured_at: now,
                captured_host_nanos: index as u64,
                stored_at: now,
                emitted_at: now,
                queued_at: now,
                event_music_time_ns: index as SongTimeNs * 8_333_333,
            }),
        }
    }
}

impl InputQueueDrainBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.deque.extend(self.edges.iter().copied());
        let mut output = GameplayFrameHotPathBenchOutput {
            checksum: frame as u64,
            samples: 0,
        };
        while let Some(edge) = self.deque.pop_front() {
            record_queued_input_edge(&mut output, edge);
        }
        output
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.vector.extend_from_slice(&self.edges);
        let mut output = GameplayFrameHotPathBenchOutput {
            checksum: frame as u64,
            samples: 0,
        };
        for edge in self.vector.drain(..) {
            record_queued_input_edge(&mut output, edge);
        }
        output
    }
}

#[inline(always)]
fn record_queued_input_edge(output: &mut GameplayFrameHotPathBenchOutput, edge: GameplayInputEdge) {
    output.checksum = output.checksum.rotate_left(7)
        ^ (edge.lane.index() as u64).rotate_left(11)
        ^ u64::from(edge.input_slot).rotate_left(19)
        ^ u64::from(edge.pressed).rotate_left(31)
        ^ edge.captured_host_nanos.rotate_left(43)
        ^ edge.event_music_time_ns as u64;
    output.samples += 1;
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

#[derive(Clone)]
pub struct RegularCueCursorBench {
    cues: Vec<ColumnCue>,
    cursor: usize,
}

impl Default for RegularCueCursorBench {
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

impl RegularCueCursorBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let current_time = frame as f32 / 120.0;
        let end = self
            .cues
            .partition_point(|cue| cue.start_time <= current_time);
        self.cursor = end;
        record_crossover_cues(self.cursor, end.saturating_sub(1)..end)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let current_time = frame as f32 / 120.0;
        self.cursor = column_cue_cursor_from_hint(&self.cues, current_time, self.cursor);
        record_crossover_cues(self.cursor, self.cursor.saturating_sub(1)..self.cursor)
    }
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
        let scrolls: [ScrollEffects; MAX_PLAYERS] = std::array::from_fn(|player| {
            resolve_bench_scroll(
                self.base_scroll[player],
                self.attack_scroll[player],
                player,
                frame,
            )
        });
        for player in 0..MAX_PLAYERS {
            let scroll = scrolls[player];
            let first_reverse = scroll.reverse_percent_for_column(0, COLS_PER_PLAYER);
            self.reverse[player] = first_reverse > 0.5;
            let start = player * COLS_PER_PLAYER;
            for local_col in 0..COLS_PER_PLAYER {
                let reverse = if local_col == 0 {
                    first_reverse
                } else {
                    scroll.reverse_percent_for_column(local_col, COLS_PER_PLAYER)
                };
                self.column_dirs[start + local_col] = 1.0 - 2.0 * reverse;
            }
        }
        for (player, scroll) in scrolls.into_iter().enumerate() {
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

    fn run_frame(&mut self, frame: usize, guard_idle: bool) -> GameplayFrameHotPathBenchOutput {
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

    fn run_frame(&mut self, frame: usize, share_cutoff: bool) -> GameplayFrameHotPathBenchOutput {
        let music_time_ns = 500_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        let player_refs = [&self.timing_players[0], &self.timing_players[1]];
        let rows =
            self.caches
                .missed_note_cutoff_rows(&self.profile, &player_refs, 1.0, music_time_ns, 2);
        let second_rows = if share_cutoff {
            rows
        } else {
            self.caches
                .missed_note_cutoff_rows(&self.profile, &player_refs, 1.0, music_time_ns, 2)
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

#[derive(Clone)]
pub struct ResolutionDistanceCacheBench {
    profile: TimingProfile,
    cached_distance_ns: SongTimeNs,
}

impl Default for ResolutionDistanceCacheBench {
    fn default() -> Self {
        let profile = TimingProfile::default_itg_with_fa_plus();
        let cached_distance_ns = max_step_distance_ns(&profile, 1.0);
        Self {
            profile,
            cached_distance_ns,
        }
    }
}

impl ResolutionDistanceCacheBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let music_time_ns = 500_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        let profile = std::hint::black_box(&self.profile);
        resolution_distance_output(
            music_time_ns.saturating_add(max_step_distance_ns(profile, 1.0)),
            music_time_ns.saturating_sub(max_step_distance_ns(profile, 1.0)),
        )
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let music_time_ns = 500_000_000_i64.saturating_add(frame as i64 * 8_333_333);
        resolution_distance_output(
            music_time_ns.saturating_add(self.cached_distance_ns),
            music_time_ns.saturating_sub(self.cached_distance_ns),
        )
    }
}

#[inline(always)]
fn resolution_distance_output(
    lookahead_time_ns: SongTimeNs,
    cutoff_time_ns: SongTimeNs,
) -> GameplayFrameHotPathBenchOutput {
    GameplayFrameHotPathBenchOutput {
        checksum: (lookahead_time_ns as u64).rotate_left(19) ^ cutoff_time_ns as u64,
        samples: 2,
    }
}

#[derive(Clone, Default)]
pub struct PressedLaneMaskBench {
    lane_counts: [u16; MAX_COLS],
    pressed_lane_mask: u8,
}

impl PressedLaneMaskBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        let mut inputs = [false; MAX_COLS];
        for (col, input) in inputs.iter_mut().enumerate() {
            *input = self.lane_counts[col] != 0;
        }
        pressed_lane_output(&inputs, None)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        let inputs = lane_inputs_from_mask(self.pressed_lane_mask, MAX_COLS);
        pressed_lane_output(&inputs, Some(self.pressed_lane_mask))
    }

    fn prepare_frame(&mut self, frame: usize) {
        self.lane_counts.fill(0);
        let col = frame / 120 % MAX_COLS;
        self.lane_counts[col] = 1;
        self.pressed_lane_mask = input_lane_bit(col);
    }
}

#[inline(always)]
fn pressed_lane_output(
    inputs: &[bool; MAX_COLS],
    pressed_lane_mask: Option<u8>,
) -> GameplayFrameHotPathBenchOutput {
    let mut output = GameplayFrameHotPathBenchOutput::default();
    if let Some(mut lanes) = pressed_lane_mask {
        while lanes != 0 {
            let col = lanes.trailing_zeros() as usize;
            lanes &= lanes - 1;
            record_pressed_input(&mut output, col, inputs[col]);
        }
    } else {
        for (col, &pressed) in inputs.iter().enumerate() {
            record_pressed_input(&mut output, col, pressed);
        }
    }
    output
}

#[derive(Clone)]
pub struct IdleReceptorGlowBench {
    noteskin_effects: GameplayNoteskinEffects,
    press_timers: [f32; MAX_COLS],
    lift_timers: [f32; MAX_COLS],
    lift_start_alpha: [f32; MAX_COLS],
    lift_start_zoom: [f32; MAX_COLS],
    lane_counts: [u16; MAX_COLS],
}

impl Default for IdleReceptorGlowBench {
    fn default() -> Self {
        Self {
            noteskin_effects: GameplayNoteskinEffects::default(),
            press_timers: [0.0; MAX_COLS],
            lift_timers: [0.0; MAX_COLS],
            lift_start_alpha: [0.0; MAX_COLS],
            lift_start_zoom: [1.0; MAX_COLS],
            lane_counts: [0; MAX_COLS],
        }
    }
}

impl IdleReceptorGlowBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        tick_receptor_glow_columns_legacy(
            &self.noteskin_effects,
            &self.lane_counts,
            &mut self.press_timers,
            &mut self.lift_timers,
            &mut self.lift_start_alpha,
            &mut self.lift_start_zoom,
        );
        receptor_glow_output(self)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        tick_receptor_glow_columns(
            &self.noteskin_effects,
            MAX_COLS,
            MAX_PLAYERS,
            MAX_COLS / MAX_PLAYERS,
            &self.lane_counts,
            &mut self.press_timers,
            &mut self.lift_timers,
            &mut self.lift_start_alpha,
            &mut self.lift_start_zoom,
            1.0 / 120.0,
        );
        receptor_glow_output(self)
    }

    fn prepare_frame(&mut self, frame: usize) {
        self.lane_counts.fill(0);
        if frame.is_multiple_of(257) {
            let col = frame / 257 % MAX_COLS;
            self.press_timers[col] = 0.12;
            self.lift_start_alpha[col] = 0.8;
            self.lift_start_zoom[col] = 1.15;
            self.lane_counts[col] = 1;
        }
    }
}

#[inline(never)]
fn tick_receptor_glow_columns_legacy(
    noteskin_effects: &GameplayNoteskinEffects,
    lane_counts: &[u16; MAX_COLS],
    press_timers: &mut [f32; MAX_COLS],
    lift_timers: &mut [f32; MAX_COLS],
    lift_start_alpha: &mut [f32; MAX_COLS],
    lift_start_zoom: &mut [f32; MAX_COLS],
) {
    for col in 0..MAX_COLS {
        let player = player_index_for_column(MAX_PLAYERS, MAX_COLS / MAX_PLAYERS, col);
        let timers = tick_receptor_glow_timers(
            noteskin_effects.receptor_glow_behavior_for_player(player),
            GameplayReceptorGlowTimers {
                press_timer: press_timers[col],
                lift_timer: lift_timers[col],
                lift_start_alpha: lift_start_alpha[col],
                lift_start_zoom: lift_start_zoom[col],
            },
            lane_counts[col] != 0,
            1.0 / 120.0,
        );
        press_timers[col] = timers.press_timer;
        lift_timers[col] = timers.lift_timer;
        lift_start_alpha[col] = timers.lift_start_alpha;
        lift_start_zoom[col] = timers.lift_start_zoom;
    }
}

fn receptor_glow_output(bench: &IdleReceptorGlowBench) -> GameplayFrameHotPathBenchOutput {
    let mut output = GameplayFrameHotPathBenchOutput::default();
    for values in [
        &bench.press_timers,
        &bench.lift_timers,
        &bench.lift_start_alpha,
        &bench.lift_start_zoom,
    ] {
        for &value in values {
            output.checksum = output.checksum.rotate_left(7) ^ u64::from(value.to_bits());
            output.samples += usize::from(value != 0.0);
        }
    }
    output
}

#[derive(Clone)]
pub struct JudgedRowCursorBench {
    row_entries: Vec<RowEntry>,
}

impl Default for JudgedRowCursorBench {
    fn default() -> Self {
        Self {
            row_entries: (0..8_192)
                .map(|index| RowEntry {
                    row_index: index * ROWS_PER_BEAT as usize,
                    time_ns: index as SongTimeNs * 500_000_000,
                    nonmine_note_indices: [usize::MAX; MAX_COLS],
                    nonmine_note_count: 0,
                    rescore_track_count: 0,
                    unresolved_count: 1,
                    unresolved_nonlift_count: 1,
                    had_provisional_early_hit: false,
                    final_outcome: None,
                })
                .collect(),
        }
    }
}

impl JudgedRowCursorBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, true)
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, false)
    }

    fn frame(&self, frame: usize, rescan_cursor: bool) -> GameplayFrameHotPathBenchOutput {
        let cursor = frame % (self.row_entries.len() - 1);
        let lookahead_time_ns = self.row_entries[cursor].time_ns;
        let mut events = [None; 8];
        let update = collect_ready_judged_row_events(
            &self.row_entries,
            (0, self.row_entries.len()),
            cursor,
            lookahead_time_ns,
            &mut events,
        );
        let next_cursor = if rescan_cursor {
            advance_judged_row_cursor_for_entries(
                &self.row_entries,
                (0, self.row_entries.len()),
                cursor,
                lookahead_time_ns,
            )
        } else {
            update.next_cursor
        };
        GameplayFrameHotPathBenchOutput {
            checksum: (next_cursor as u64).rotate_left(7)
                ^ (update.next_scan_start as u64).rotate_left(19)
                ^ (update.event_count as u64).rotate_left(31)
                ^ u64::from(update.stopped),
            samples: update.event_count + 1,
        }
    }
}

#[derive(Clone, Default)]
pub struct SparseFeedbackTickBench {
    visual: GameplayVisualFeedbackState,
    hold: GameplayHoldFeedbackState,
}

impl SparseFeedbackTickBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame, false);
        let now = frame as f32 / 120.0;
        for slot in &mut self.visual.tap_explosions {
            tick_tap_explosion_slot(slot, 1.0 / 120.0);
        }
        for slot in &mut self.visual.mine_explosions {
            tick_mine_explosion_slot(slot, 1.0 / 120.0);
        }
        for slot in &mut self.visual.column_flashes {
            if slot.is_some_and(|flash| column_flash_expired_at(flash, now)) {
                *slot = None;
            }
        }
        for slot in &mut self.hold.hold_judgments {
            if slot.is_some_and(|info| hold_judgment_expired_at(info, now)) {
                *slot = None;
            }
        }
        for slot in &mut self.hold.held_miss_judgments {
            if slot.is_some_and(|info| held_miss_judgment_expired_at(info, now)) {
                *slot = None;
            }
        }
        feedback_tick_output(self)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame, true);
        let now = frame as f32 / 120.0;
        self.visual.tick(1.0 / 120.0, now);
        self.hold.tick(now);
        feedback_tick_output(self)
    }

    fn prepare_frame(&mut self, frame: usize, sparse: bool) {
        let now = frame as f32 / 120.0;
        if frame.is_multiple_of(257) {
            let col = frame / 257 % MAX_COLS;
            let value = Some(ActiveTapExplosion {
                window: "W1",
                bright: frame.is_multiple_of(2),
                elapsed: 0.0,
                duration: 0.5,
                start_beat: now * 2.0,
            });
            if sparse {
                self.visual.set_tap_explosion(col, value);
            } else {
                self.visual.tap_explosions[col] = value;
            }
        }
        if frame.is_multiple_of(389) {
            let col = frame / 389 % MAX_COLS;
            let value = Some(ActiveColumnFlash {
                grade: JudgeGrade::Great,
                blue_fantastic: false,
                started_at_screen_s: now,
            });
            if sparse {
                self.visual.set_column_flash(col, value);
            } else {
                self.visual.column_flashes[col] = value;
            }
        }
        if frame.is_multiple_of(509) {
            let col = frame / 509 % MAX_COLS;
            let value = Some(ActiveMineExplosion {
                elapsed: 0.0,
                duration: MINE_EXPLOSION_DURATION,
                started_at_screen_s: now,
            });
            if sparse {
                self.visual.set_mine_explosion(col, value);
            } else {
                self.visual.mine_explosions[col] = value;
            }
        }
        if frame.is_multiple_of(601) {
            let col = frame / 601 % MAX_COLS;
            let value = Some(HoldJudgmentRenderInfo {
                result: HoldResult::Held,
                started_at_screen_s: now,
            });
            if sparse {
                self.hold.set_hold_judgment(col, value);
            } else {
                self.hold.hold_judgments[col] = value;
            }
        }
        if frame.is_multiple_of(733) {
            let col = frame / 733 % MAX_COLS;
            let value = Some(HeldMissRenderInfo {
                started_at_screen_s: now,
            });
            if sparse {
                self.hold.set_held_miss(col, value);
            } else {
                self.hold.held_miss_judgments[col] = value;
            }
        }
    }
}

fn feedback_tick_output(bench: &SparseFeedbackTickBench) -> GameplayFrameHotPathBenchOutput {
    let mut output = GameplayFrameHotPathBenchOutput::default();
    for col in 0..MAX_COLS {
        if let Some(active) = bench.visual.tap_explosions[col] {
            output.checksum = output.checksum.rotate_left(5)
                ^ u64::from(active.elapsed.to_bits())
                ^ u64::from(active.start_beat.to_bits()).rotate_left(11)
                ^ u64::from(active.bright);
            output.samples += 1;
        }
        if let Some(active) = &bench.visual.mine_explosions[col] {
            output.checksum = output.checksum.rotate_left(7)
                ^ u64::from(active.elapsed.to_bits())
                ^ u64::from(active.started_at_screen_s.to_bits()).rotate_left(13);
            output.samples += 1;
        }
        if let Some(active) = bench.visual.column_flashes[col] {
            output.checksum = output.checksum.rotate_left(9)
                ^ u64::from(active.started_at_screen_s.to_bits())
                ^ u64::from(active.blue_fantastic);
            output.samples += 1;
        }
        if let Some(active) = bench.hold.hold_judgments[col] {
            output.checksum = output.checksum.rotate_left(11)
                ^ u64::from(active.started_at_screen_s.to_bits())
                ^ match active.result {
                    HoldResult::Held => 1,
                    HoldResult::LetGo => 2,
                    HoldResult::Missed => 3,
                };
            output.samples += 1;
        }
        if let Some(active) = bench.hold.held_miss_judgments[col] {
            output.checksum = output.checksum.rotate_left(13)
                ^ u64::from(active.started_at_screen_s.to_bits());
            output.samples += 1;
        }
    }
    output
}

#[derive(Clone, Default)]
pub struct StableNotefieldRefreshBench {
    options: LiveNotefieldOptionsBench,
    cached_phase: Option<usize>,
}

impl StableNotefieldRefreshBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.options.new_frame(frame / 4_096)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let phase = frame / 4_096;
        if self.cached_phase != Some(phase) {
            self.cached_phase = Some(phase);
            self.options.new_frame(phase)
        } else {
            live_options_output(&self.options)
        }
    }
}

#[derive(Clone, Default)]
pub struct ActiveHoldMaskBench {
    active_holds: [Option<usize>; MAX_COLS],
    active_mask: u8,
}

impl ActiveHoldMaskBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        let active_holds = std::hint::black_box(&self.active_holds);
        if active_holds.iter().any(Option::is_some) {
            for (col, active) in active_holds.iter().enumerate() {
                if let Some(note_index) = active {
                    record_active_hold(&mut output, col, *note_index);
                }
            }
        }
        output
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.prepare_frame(frame);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        let mut active = std::hint::black_box(self.active_mask);
        while active != 0 {
            let col = active.trailing_zeros() as usize;
            active &= active - 1;
            if let Some(note_index) = self.active_holds[col] {
                record_active_hold(&mut output, col, note_index);
            }
        }
        output
    }

    fn prepare_frame(&mut self, frame: usize) {
        if frame.is_multiple_of(4_096) {
            let col = frame / 4_096 % MAX_COLS;
            if frame != 0 {
                let previous_col = (frame / 4_096 - 1) % MAX_COLS;
                self.active_holds[previous_col] = None;
            }
            self.active_holds[col] = Some(frame);
            self.active_mask = input_lane_bit(col);
        }
    }
}

#[inline(always)]
fn record_active_hold(
    output: &mut GameplayFrameHotPathBenchOutput,
    col: usize,
    note_index: usize,
) {
    output.checksum = output.checksum.rotate_left(7)
        ^ (col as u64).rotate_left(17)
        ^ note_index as u64;
    output.samples += 1;
}

#[derive(Clone)]
pub struct DisplayBpmCacheBench {
    timing: TimingData,
    cached_bpms: Vec<f32>,
}

impl Default for DisplayBpmCacheBench {
    fn default() -> Self {
        // The standalone runner measures the range after its warmup frames.
        const FRAMES: usize = 52_048;
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: (0..8_192)
                    .map(|index| (index as f32 * 4.0, 90.0 + (index % 181) as f32))
                    .collect(),
                ..TimingSegments::default()
            },
            &[],
        );
        let cached_bpms = (0..FRAMES)
            .map(|frame| timing.get_bpm_for_beat(display_bpm_bench_beat(frame)))
            .collect();
        Self {
            timing,
            cached_bpms,
        }
    }
}

impl DisplayBpmCacheBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let beat = display_bpm_bench_beat(frame);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        for pass in 0..3 {
            record_display_bpm(
                &mut output,
                std::hint::black_box(self.timing.get_bpm_for_beat(beat)),
                pass,
            );
        }
        output
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let bpm = self.cached_bpms[frame % self.cached_bpms.len()];
        let mut output = GameplayFrameHotPathBenchOutput::default();
        for pass in 0..3 {
            record_display_bpm(&mut output, bpm, pass);
        }
        output
    }
}

#[inline(always)]
fn display_bpm_bench_beat(frame: usize) -> f32 {
    (frame % 32_768) as f32 + 0.25
}

#[inline(always)]
fn record_display_bpm(
    output: &mut GameplayFrameHotPathBenchOutput,
    bpm: f32,
    pass: usize,
) {
    output.checksum = output.checksum.rotate_left(9)
        ^ u64::from(bpm.to_bits())
        ^ (pass as u64).rotate_left(23);
    output.samples += 1;
}

#[derive(Clone)]
pub struct SharedClockBeatBench {
    timing: TimingData,
    caches: GameplayTimeToBeatCaches,
}

impl Default for SharedClockBeatBench {
    fn default() -> Self {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: (0..8_192)
                    .map(|index| (index as f32 * 0.5, 90.0 + (index % 181) as f32))
                    .collect(),
                ..TimingSegments::default()
            },
            &[],
        );
        let timing_players = [&timing; MAX_PLAYERS];
        let caches = GameplayTimeToBeatCaches::new(&timing, &timing_players);
        Self { timing, caches }
    }
}

impl SharedClockBeatBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let time_ns = clock_bench_time_ns(frame);
        let song = self.caches.song_info(&self.timing, time_ns);
        let display = self.caches.display_info(&self.timing, time_ns);
        let search = self
            .caches
            .notefield_search_beat(0, &self.timing, time_ns);
        let visible = self.caches.visible_beat(0, &self.timing, time_ns);
        record_clock_beats(song, display, search, visible)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let time_ns = clock_bench_time_ns(frame);
        let song = self.caches.song_info(&self.timing, time_ns);
        record_clock_beats(song, song, song.beat, song.beat)
    }
}

#[inline(always)]
fn clock_bench_time_ns(frame: usize) -> SongTimeNs {
    (frame as SongTimeNs).saturating_mul(8_333_333)
}

#[inline(always)]
fn record_clock_beats(
    song: BeatInfo,
    display: BeatInfo,
    search: f32,
    visible: f32,
) -> GameplayFrameHotPathBenchOutput {
    let values = [song.beat, song.bpm, display.beat, display.bpm, search, visible];
    let mut output = GameplayFrameHotPathBenchOutput::default();
    for value in values {
        output.checksum = output.checksum.rotate_left(9) ^ u64::from(value.to_bits());
        output.samples += 1;
    }
    output.checksum ^= (song.is_in_freeze as u64) << 1
        | (song.is_in_delay as u64) << 2
        | (display.is_in_freeze as u64) << 3
        | (display.is_in_delay as u64) << 4;
    output
}

#[derive(Clone, Copy, Default)]
pub struct TwoPlayerColumnMapBench;

impl TwoPlayerColumnMapBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, false)
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, true)
    }

    fn frame(&self, frame: usize, optimized: bool) -> GameplayFrameHotPathBenchOutput {
        let cols_per_player = std::hint::black_box(MAX_COLS / MAX_PLAYERS);
        let num_players = std::hint::black_box(MAX_PLAYERS);
        let mut output = GameplayFrameHotPathBenchOutput::default();
        for sample in 0..96 {
            let column = std::hint::black_box((frame + sample * 5) % MAX_COLS);
            let player = if optimized {
                player_index_for_column(num_players, cols_per_player, column)
            } else {
                (column / cols_per_player).min(num_players - 1)
            };
            output.checksum = output.checksum.rotate_left(5) ^ player as u64;
            output.samples += 1;
        }
        output
    }
}

#[derive(Clone, Copy)]
pub struct MappedAudioClockBench {
    snapshot: GameplayAudioSnapshot,
}

impl Default for MappedAudioClockBench {
    fn default() -> Self {
        Self {
            snapshot: GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    music_seconds_per_second: 1.25,
                    has_music_mapping: true,
                    valid_at_host_nanos: 1,
                    ..GameplayStreamClockSnapshot::default()
                },
                ..GameplayAudioSnapshot::default()
            },
        }
    }
}

impl MappedAudioClockBench {
    pub fn old_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let audio_snapshot = mapped_audio_bench_snapshot(self.snapshot, frame);
        let music_rate = std::hint::black_box(1.25_f32);
        let stream_lead_in =
            std::hint::black_box(2.5_f32 / normalized_song_rate(music_rate));
        record_song_clock(current_song_clock_snapshot_legacy(
            audio_snapshot,
            music_rate,
            stream_lead_in,
            -0.008,
        ))
    }

    pub fn new_frame(&self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        let audio_snapshot = mapped_audio_bench_snapshot(self.snapshot, frame);
        let music_rate = std::hint::black_box(1.25_f32);
        let stream_lead_in = if audio_snapshot.stream_clock.has_music_mapping {
            0.0
        } else {
            2.5 / normalized_song_rate(music_rate)
        };
        record_song_clock(current_song_clock_snapshot(
            audio_snapshot,
            music_rate,
            stream_lead_in,
            -0.008,
        ))
    }
}

#[inline(always)]
fn mapped_audio_bench_snapshot(
    mut snapshot: GameplayAudioSnapshot,
    frame: usize,
) -> GameplayAudioSnapshot {
    snapshot.stream_clock.music_nanos = clock_bench_time_ns(frame);
    snapshot.stream_clock.valid_at_host_nanos = frame as u64 + 1;
    snapshot.timing_diag_callback_gap_ns = frame as u64 & 31;
    snapshot
}

#[inline(never)]
fn current_song_clock_snapshot_legacy(
    audio_snapshot: GameplayAudioSnapshot,
    music_rate: f32,
    audio_lead_in_seconds: f32,
    global_offset_seconds: f32,
) -> SongClockSnapshot {
    let stream_clock = audio_snapshot.stream_clock;
    let fallback_rate = normalized_song_rate(music_rate);
    if stream_clock.has_music_mapping {
        return SongClockSnapshot {
            song_time_ns: stream_clock.music_nanos,
            seconds_per_second: if stream_clock.music_seconds_per_second.is_finite()
                && stream_clock.music_seconds_per_second > 0.0
            {
                stream_clock.music_seconds_per_second
            } else {
                fallback_rate
            },
            mapped_audio: true,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
            timing_diag_enabled: audio_snapshot.timing_diag_enabled,
            timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
        };
    }
    let song_time = music_time_from_stream_position(
        stream_clock.stream_seconds,
        audio_lead_in_seconds,
        global_offset_seconds,
        fallback_rate,
    );
    SongClockSnapshot {
        song_time_ns: song_time_ns_from_seconds(song_time),
        seconds_per_second: fallback_rate,
        mapped_audio: false,
        valid_at: stream_clock.valid_at,
        valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        timing_diag_enabled: audio_snapshot.timing_diag_enabled,
        timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
    }
}

#[inline(always)]
fn record_song_clock(snapshot: SongClockSnapshot) -> GameplayFrameHotPathBenchOutput {
    GameplayFrameHotPathBenchOutput {
        checksum: snapshot.song_time_ns as u64
            ^ u64::from(snapshot.seconds_per_second.to_bits()).rotate_left(7)
            ^ snapshot.valid_at_host_nanos.rotate_left(19)
            ^ snapshot.timing_diag_callback_gap_ns.rotate_left(31)
            ^ (u64::from(snapshot.mapped_audio) << 61)
            ^ (u64::from(snapshot.timing_diag_enabled) << 62),
        samples: 1,
    }
}

#[derive(Clone)]
pub struct SparseTapMissDueBench {
    notes: Vec<Note>,
    note_times_ns: Vec<SongTimeNs>,
    held_windows: Vec<bool>,
    hold_decay_active: Vec<bool>,
    decaying_holds: Vec<usize>,
    cursors: [usize; MAX_PLAYERS],
    ranges: [(usize, usize); MAX_PLAYERS],
}

impl Default for SparseTapMissDueBench {
    fn default() -> Self {
        const NOTES_PER_PLAYER: usize = 4_096;
        let mut notes = Vec::with_capacity(NOTES_PER_PLAYER * MAX_PLAYERS);
        let mut note_times_ns = Vec::with_capacity(notes.capacity());
        for player in 0..MAX_PLAYERS {
            for index in 0..NOTES_PER_PLAYER {
                let row_index = index * 24;
                notes.push(Note {
                    beat: row_index as f32 / ROWS_PER_BEAT as f32,
                    quantization_idx: 1,
                    column: player * (MAX_COLS / MAX_PLAYERS),
                    note_type: NoteType::Tap,
                    row_index,
                    result: None,
                    early_result: None,
                    hold: None,
                    mine_result: None,
                    is_fake: false,
                    can_be_judged: true,
                });
                note_times_ns.push(index as SongTimeNs * 800_000_000);
            }
        }
        Self {
            held_windows: vec![false; notes.len()],
            hold_decay_active: vec![false; notes.len()],
            decaying_holds: Vec::with_capacity(8),
            cursors: [0, NOTES_PER_PLAYER],
            ranges: [
                (0, NOTES_PER_PLAYER),
                (NOTES_PER_PLAYER, NOTES_PER_PLAYER * MAX_PLAYERS),
            ],
            notes,
            note_times_ns,
        }
    }
}

impl SparseTapMissDueBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, false)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, true)
    }

    fn frame(&mut self, frame: usize, guarded: bool) -> GameplayFrameHotPathBenchOutput {
        let cutoff_rows = [frame / 4 + 1; MAX_PLAYERS];
        let ready = !guarded
            || time_based_tap_miss_work_ready_for_players(
                &self.notes,
                &self.note_times_ns,
                &self.cursors,
                &self.ranges,
                &cutoff_rows,
                MAX_PLAYERS,
            );
        if !ready {
            return GameplayFrameHotPathBenchOutput {
                checksum: (self.cursors[0] as u64).rotate_left(7)
                    ^ (self.cursors[1] as u64).rotate_left(19),
                samples: 0,
            };
        }
        let mut events = [None; 16];
        let event_count = collect_time_based_tap_misses_for_players(
            &mut self.notes,
            &self.note_times_ns,
            &self.held_windows,
            &mut self.hold_decay_active,
            &mut self.decaying_holds,
            &mut self.cursors,
            &self.ranges,
            &cutoff_rows,
            clock_bench_time_ns(frame),
            1.0,
            &[true; MAX_PLAYERS],
            MAX_PLAYERS,
            &mut events,
        )
        .event_count;
        let mut output = GameplayFrameHotPathBenchOutput {
            checksum: (self.cursors[0] as u64).rotate_left(7)
                ^ (self.cursors[1] as u64).rotate_left(19),
            samples: event_count,
        };
        for event in events.into_iter().take(event_count).flatten() {
            output.checksum = output.checksum.rotate_left(11)
                ^ event.player as u64
                ^ (event.event.note_index as u64).rotate_left(23)
                ^ u64::from(event.event.judgment.time_error_ms.to_bits());
        }
        output
    }
}

#[derive(Clone)]
pub struct SparseMineAvoidDueBench {
    notes: Vec<Note>,
    mine_note_ix: Vec<Vec<usize>>,
    cursors: [usize; MAX_PLAYERS],
    next_note_cursors: [usize; MAX_PLAYERS],
    ranges: [(usize, usize); MAX_PLAYERS],
}

impl Default for SparseMineAvoidDueBench {
    fn default() -> Self {
        const MINES_PER_PLAYER: usize = 4_096;
        let mut notes = Vec::with_capacity(MINES_PER_PLAYER * MAX_PLAYERS);
        let mut mine_note_ix = Vec::with_capacity(MAX_PLAYERS);
        for player in 0..MAX_PLAYERS {
            let start = notes.len();
            let mut player_mines = Vec::with_capacity(MINES_PER_PLAYER);
            for index in 0..MINES_PER_PLAYER {
                let note_index = notes.len();
                let row_index = index * 24;
                player_mines.push(note_index);
                notes.push(Note {
                    beat: row_index as f32 / ROWS_PER_BEAT as f32,
                    quantization_idx: 1,
                    column: player * (MAX_COLS / MAX_PLAYERS),
                    note_type: NoteType::Mine,
                    row_index,
                    result: None,
                    early_result: None,
                    hold: None,
                    mine_result: None,
                    is_fake: false,
                    can_be_judged: true,
                });
            }
            debug_assert_eq!(notes.len() - start, MINES_PER_PLAYER);
            mine_note_ix.push(player_mines);
        }
        Self {
            cursors: [0; MAX_PLAYERS],
            next_note_cursors: [0, MINES_PER_PLAYER],
            ranges: [
                (0, MINES_PER_PLAYER),
                (MINES_PER_PLAYER, MINES_PER_PLAYER * MAX_PLAYERS),
            ],
            notes,
            mine_note_ix,
        }
    }
}

impl SparseMineAvoidDueBench {
    pub fn old_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, false)
    }

    pub fn new_frame(&mut self, frame: usize) -> GameplayFrameHotPathBenchOutput {
        self.frame(frame, true)
    }

    fn frame(&mut self, frame: usize, guarded: bool) -> GameplayFrameHotPathBenchOutput {
        let cutoff_rows = [frame / 4 + 1; MAX_PLAYERS];
        let ready = !guarded
            || time_based_mine_avoidance_work_ready_for_players(
                &self.notes,
                &self.mine_note_ix,
                &self.cursors,
                &cutoff_rows,
                MAX_PLAYERS,
            );
        let mut output = GameplayFrameHotPathBenchOutput::default();
        if ready {
            let updates = apply_time_based_mine_avoidance_for_players(
                &mut self.notes,
                &self.mine_note_ix,
                &self.cursors,
                &cutoff_rows,
                &self.ranges,
                MAX_PLAYERS,
            );
            for player in 0..updates.players_scanned {
                let update = updates.updates[player];
                self.cursors[player] = update.mine_end;
                self.next_note_cursors[player] = update.next_mine_avoid_cursor;
                output.samples += update.avoided_count as usize;
                if let Some(event) = update.last_avoided {
                    output.checksum ^= (event.note_index as u64).rotate_left(41)
                        ^ (event.row_index as u64).rotate_left(53)
                        ^ event.column as u64;
                }
            }
        }
        output.checksum ^= (self.cursors[0] as u64).rotate_left(7)
            ^ (self.cursors[1] as u64).rotate_left(19)
            ^ (self.next_note_cursors[0] as u64).rotate_left(31)
            ^ (self.next_note_cursors[1] as u64).rotate_left(43);
        output
    }
}
