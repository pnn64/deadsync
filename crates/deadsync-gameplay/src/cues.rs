#[derive(Clone, Debug, PartialEq)]
pub struct GameplayBoundaryRuntimeState {
    pub commands: GameplayCommandQueue,
    pub total_elapsed_in_screen: f32,
}

impl GameplayBoundaryRuntimeState {
    #[inline(always)]
    pub fn new(audio_command_capacity: usize, session_command_capacity: usize) -> Self {
        Self {
            commands: GameplayCommandQueue::with_capacity(
                audio_command_capacity,
                session_command_capacity,
            ),
            total_elapsed_in_screen: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameplayPendingInputState<T> {
    pub edges: VecDeque<T>,
}

impl<T> GameplayPendingInputState<T> {
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            edges: VecDeque::with_capacity(capacity),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnCueColumn {
    pub column: usize,
    pub is_mine: bool,
}

#[derive(Clone, Debug)]
pub struct ColumnCue {
    pub start_time: f32,
    pub duration: f32,
    pub columns: Vec<ColumnCueColumn>,
}

#[inline(always)]
pub fn active_column_cue(cues: &[ColumnCue], current_time: f32) -> Option<&ColumnCue> {
    if cues.is_empty() {
        return None;
    }
    let idx = cues.partition_point(|cue| cue.start_time <= current_time);
    idx.checked_sub(1).and_then(|i| cues.get(i))
}

// Lead-in/out fade applied to every crossover cue.
pub const CROSSOVER_CUE_FADE_SECONDS: f32 = 0.075;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CrossoverRow {
    pub beat: f32,
    // Occupancy bitmask of the foot-bearing columns for this row.
    pub column_mask: u8,
    // Whether the parity solver flagged this row as a crossover.
    pub crossover: bool,
    // Kept raw so the cue builder can honor the per-player bracket toggle.
    pub bracket: bool,
}

impl CrossoverRow {
    // A bracket crossover only counts when the player opts brackets in.
    #[inline]
    pub const fn is_active_crossover(&self, include_brackets: bool) -> bool {
        self.crossover && (include_brackets || !self.bracket)
    }
}

pub fn build_crossover_rows<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
) -> (Vec<[u8; LANES]>, Vec<f32>) {
    use std::collections::BTreeMap;

    let (start, end) = note_range;
    let mut rows: BTreeMap<usize, ([u8; LANES], f32)> = BTreeMap::new();
    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        let lane = note.column - col_start;
        let ch = if note.is_fake {
            match note.note_type {
                NoteType::Mine => b'M',
                _ => continue,
            }
        } else {
            match note.note_type {
                NoteType::Tap => b'1',
                NoteType::Lift => b'L',
                NoteType::Hold => b'2',
                NoteType::Roll => b'4',
                NoteType::Mine => b'M',
                NoteType::Fake => continue,
            }
        };
        let entry = rows
            .entry(note.row_index)
            .or_insert(([b'0'; LANES], note.beat));
        if ch == b'M' {
            if entry.0[lane] == b'0' {
                entry.0[lane] = b'M';
            }
        } else {
            entry.0[lane] = ch;
        }
        if let Some(hold) = note.hold.as_ref() {
            let tail = rows
                .entry(hold.end_row_index)
                .or_insert(([b'0'; LANES], hold.end_beat));
            if tail.0[lane] == b'0' {
                tail.0[lane] = b'3';
            }
        }
    }
    let mut row_arrays = Vec::with_capacity(rows.len());
    let mut row_to_beat = Vec::with_capacity(rows.len());
    for (_row_index, (arr, beat)) in rows {
        row_arrays.push(arr);
        row_to_beat.push(beat);
    }
    (row_arrays, row_to_beat)
}

pub type CrossoverAnnotationBuilder =
    fn(&[Note], (usize, usize), &TimingSegments, usize, usize) -> Vec<CrossoverRow>;

#[inline(always)]
pub fn empty_crossover_annotations(
    _notes: &[Note],
    _note_range: (usize, usize),
    _timing_segments: &TimingSegments,
    _cols_per_player: usize,
    _col_start: usize,
) -> Vec<CrossoverRow> {
    Vec::new()
}

#[inline(always)]
pub fn build_crossover_cues_for_player_annotations(
    build_annotations: CrossoverAnnotationBuilder,
    notes: &[Note],
    note_range: (usize, usize),
    timing_segments: &TimingSegments,
    timing_player: &TimingData,
    cols_per_player: usize,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }
    let annos = build_annotations(
        notes,
        note_range,
        timing_segments,
        cols_per_player,
        col_start,
    );

    build_crossover_cues_from_annotations(
        &annos,
        timing_player,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
    )
}

// Lowest matching lane wins so results are deterministic. `pos % 4` keeps this
// working for the second pad of doubles, not just the left pad.
pub fn crossover_arrow_col(column_mask: u8, want_outer: bool) -> Option<usize> {
    let mut m = column_mask;
    while m != 0 {
        let c = m.trailing_zeros() as usize;
        m &= m - 1;
        let pos = c % 4;
        let is_outer = pos == 0 || pos == 3;
        if is_outer == want_outer {
            return Some(c);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn build_crossover_cues_from_annotations(
    annos: &[CrossoverRow],
    timing_player: &TimingData,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let arrow_time =
        |beat: f32| -> f32 { song_time_ns_to_seconds(timing_player.get_time_for_beat_ns(beat)) };
    build_crossover_cues_core(
        annos,
        arrow_time,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
    )
}

// Split from the TimingData entry so tests can use a compact beat-to-seconds
// mapping without constructing full timing data.
#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_core(
    annos: &[CrossoverRow],
    arrow_time: impl Fn(f32) -> f32,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    if annos.len() < 2 {
        return Vec::new();
    }
    let duration = f32::from(duration_ms) / 1000.0;
    let fade = CROSSOVER_CUE_FADE_SECONDS;
    let quant = if quantization == 0 {
        1.0
    } else {
        f32::from(quantization)
    };
    let spacing_threshold = 4.0 / quant + 0.001;

    let mut cues: Vec<ColumnCue> = Vec::new();
    for i in 1..annos.len() {
        let current = &annos[i];
        let prev = &annos[i - 1];
        if !current.is_active_crossover(include_brackets)
            || prev.is_active_crossover(include_brackets)
        {
            continue;
        }
        let next = annos.get(i + 1);
        let next_next = annos.get(i + 2);
        let is_scooby = next.is_some_and(|a| a.is_active_crossover(include_brackets));
        let first_condition = current.beat - prev.beat <= spacing_threshold;
        let second_condition = next.is_some_and(|n| n.beat - current.beat <= spacing_threshold);
        let third_condition = is_scooby
            && match (next, next_next) {
                (Some(n), Some(nn)) => nn.beat - n.beat <= spacing_threshold,
                _ => false,
            };
        if !(first_condition || second_condition || third_condition) {
            continue;
        }
        let (Some(prev_col), Some(curr_col)) = (
            crossover_arrow_col(prev.column_mask, false),
            crossover_arrow_col(current.column_mask, true),
        ) else {
            continue;
        };
        let prev_arrow_time = arrow_time(prev.beat);
        let cur_arrow_time = arrow_time(current.beat);
        let mut columns = vec![
            ColumnCueColumn {
                column: col_start + curr_col,
                is_mine: false,
            },
            ColumnCueColumn {
                column: col_start + prev_col,
                is_mine: false,
            },
        ];
        let mut start_time = prev_arrow_time - duration;
        let mut cue_duration = duration + fade;
        if !first_condition {
            cue_duration += cur_arrow_time - prev_arrow_time;
        }
        if is_scooby
            && let Some(next_anno) = next
            && let Some(next_col) = crossover_arrow_col(next_anno.column_mask, true)
        {
            columns.push(ColumnCueColumn {
                column: col_start + next_col,
                is_mine: true,
            });
        }
        let overlap = cues.last().map(|last| {
            let prev_end = last.start_time + last.duration;
            // Only one cue is active at a time and each cue drives all of its
            // columns with a single fade envelope, so a column shared by two
            // overlapping cues would fade out and back in (a visible reflash).
            let shares_column = last
                .columns
                .iter()
                .any(|prev_col| columns.iter().any(|c| c.column == prev_col.column));
            (prev_end, shares_column)
        });
        if let Some((prev_end, shares_column)) = overlap
            && start_time < prev_end
        {
            if shares_column {
                // Merge into the previous cue so the shared column stays lit
                // continuously across the overlap instead of reflashing.
                let merged_end = (start_time + cue_duration).max(prev_end);
                let last = cues
                    .last_mut()
                    .expect("cues is non-empty when overlap is Some");
                last.duration = merged_end - last.start_time;
                for col in columns {
                    if !last.columns.iter().any(|c| c.column == col.column) {
                        last.columns.push(col);
                    }
                }
                continue;
            }
            let duration_difference = prev_end - start_time;
            start_time = prev_end - fade;
            cue_duration = cue_duration - duration_difference + fade;
        }
        cues.push(ColumnCue {
            start_time,
            duration: cue_duration,
            columns,
        });
    }

    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
        && first.start_time <= 0.0
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

