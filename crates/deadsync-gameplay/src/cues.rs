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
    pub edges: Vec<T>,
}

impl<T> GameplayPendingInputState<T> {
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            edges: Vec::with_capacity(capacity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnCueColumn {
    pub column: usize,
    pub is_mine: bool,
}

/// Compact set of the lanes highlighted by one gameplay cue.
///
/// Cue columns are bounded by [`MAX_COLS`], so two masks retain the complete
/// column/mine state inline. This removes one heap allocation and one pointer
/// chase per cue while preserving ascending-column iteration order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnCueColumns {
    lanes: LaneMask,
    mines: LaneMask,
}

const _: () = {
    assert!(MAX_COLS <= LaneMask::BITS as usize);
    assert!(
        std::mem::size_of::<ColumnCueColumns>() == std::mem::size_of::<LaneMask>() * 2
    );
};

impl ColumnCueColumns {
    #[inline(always)]
    pub fn insert(&mut self, column: usize, is_mine: bool) -> bool {
        if column >= MAX_COLS {
            return false;
        }
        let Some(bit) = (1 as LaneMask).checked_shl(column as u32) else {
            return false;
        };
        let inserted = self.lanes & bit == 0;
        self.lanes |= bit;
        if is_mine {
            self.mines |= bit;
        }
        inserted
    }

    #[inline(always)]
    pub const fn contains(&self, column: usize) -> bool {
        column < MAX_COLS && self.lanes & ((1 as LaneMask) << column) != 0
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.lanes.count_ones() as usize
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.lanes == 0
    }

    #[inline(always)]
    pub const fn shares_lane(self, other: Self) -> bool {
        self.lanes & other.lanes != 0
    }

    #[inline(always)]
    pub fn extend_missing(&mut self, other: Self) {
        let added = other.lanes & !self.lanes;
        self.lanes |= added;
        self.mines |= other.mines & added;
    }

    #[inline(always)]
    pub const fn iter(self) -> ColumnCueColumnIter {
        ColumnCueColumnIter {
            remaining: self.lanes,
            mines: self.mines,
        }
    }

    #[inline(always)]
    pub fn get(self, index: usize) -> Option<ColumnCueColumn> {
        self.iter().nth(index)
    }

    #[inline(always)]
    pub fn last(self) -> Option<ColumnCueColumn> {
        if self.is_empty() {
            return None;
        }
        let column = (LaneMask::BITS - 1 - self.lanes.leading_zeros()) as usize;
        Some(ColumnCueColumn {
            column,
            is_mine: self.mines & ((1 as LaneMask) << column) != 0,
        })
    }
}

impl FromIterator<ColumnCueColumn> for ColumnCueColumns {
    fn from_iter<T: IntoIterator<Item = ColumnCueColumn>>(iter: T) -> Self {
        let mut columns = Self::default();
        for column in iter {
            columns.insert(column.column, column.is_mine);
        }
        columns
    }
}

impl<const N: usize> From<[ColumnCueColumn; N]> for ColumnCueColumns {
    fn from(columns: [ColumnCueColumn; N]) -> Self {
        columns.into_iter().collect()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnCueColumnIter {
    remaining: LaneMask,
    mines: LaneMask,
}

impl Iterator for ColumnCueColumnIter {
    type Item = ColumnCueColumn;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let column = self.remaining.trailing_zeros() as usize;
        let bit = (1 as LaneMask) << column;
        self.remaining &= self.remaining - 1;
        Some(ColumnCueColumn {
            column,
            is_mine: self.mines & bit != 0,
        })
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining.count_ones() as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for ColumnCueColumnIter {}

impl IntoIterator for &ColumnCueColumns {
    type Item = ColumnCueColumn;
    type IntoIter = ColumnCueColumnIter;

    #[inline(always)]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnCue {
    pub start_time: f32,
    pub duration: f32,
    pub columns: ColumnCueColumns,
}

#[inline(always)]
pub fn active_column_cue(cues: &[ColumnCue], current_time: f32) -> Option<&ColumnCue> {
    if cues.is_empty() {
        return None;
    }
    let idx = cues.partition_point(|cue| cue.start_time <= current_time);
    idx.checked_sub(1).and_then(|i| cues.get(i))
}

// Returns the half-open index range of cues whose `[start_time, start_time +
// duration]` window contains `current_time`, in chronological order.
// Consecutive crossover cues are built to overlap by the fade time, so up to
// two cues can be active at once; rendering both lets the outgoing cue's
// fade-out crossfade with the incoming cue's fade-in.
#[inline]
pub fn active_column_cue_range(cues: &[ColumnCue], current_time: f32) -> core::ops::Range<usize> {
    let end = cues.partition_point(|cue| cue.start_time <= current_time);
    active_column_cue_range_from_cursor(cues, current_time, end)
}

#[inline]
pub fn active_column_cue_range_from_cursor(
    cues: &[ColumnCue],
    current_time: f32,
    cursor: usize,
) -> core::ops::Range<usize> {
    let end = cursor.min(cues.len());
    let mut begin = end;
    while begin > 0 {
        let cue = &cues[begin - 1];
        if current_time < cue.start_time + cue.duration {
            begin -= 1;
        } else {
            break;
        }
    }
    begin..end
}

#[inline(always)]
pub fn column_cue_cursor_from_hint(cues: &[ColumnCue], current_time: f32, cursor: usize) -> usize {
    if cursor > cues.len() {
        return cues.partition_point(|cue| cue.start_time <= current_time);
    }
    if cursor < cues.len() && cues[cursor].start_time <= current_time {
        let next = cursor + 1;
        if next == cues.len() || cues[next].start_time > current_time {
            return next;
        }
        return cues.partition_point(|cue| cue.start_time <= current_time);
    }
    if cursor > 0 && cues[cursor - 1].start_time > current_time {
        let previous = cursor - 1;
        if previous == 0 || cues[previous - 1].start_time <= current_time {
            return previous;
        }
        return cues.partition_point(|cue| cue.start_time <= current_time);
    }
    if !current_time.is_finite() {
        return cues.partition_point(|cue| cue.start_time <= current_time);
    }
    cursor
}

// Returns every cue whose `[start_time, start_time + duration]` window contains
// `current_time`, as a contiguous slice in chronological order. See
// `active_column_cue_range`.
#[inline]
pub fn active_column_cues(cues: &[ColumnCue], current_time: f32) -> &[ColumnCue] {
    &cues[active_column_cue_range(cues, current_time)]
}

// Lead-in/out fade applied to every crossover cue.
pub const CROSSOVER_CUE_FADE_SECONDS: f32 = 0.075;

// When the playhead first crosses a cue's start, the cue's fade-in anchors to
// its own start only if it was reached within this window; otherwise (a
// practice-mode seek that lands past it) the cue fades in from the landing
// point instead of popping in at partial/full alpha. Using the fade time
// guarantees a cue caught during normal play is still inside its fade-in.
pub const CROSSOVER_CUE_SEEK_GUARD_SECONDS: f32 = CROSSOVER_CUE_FADE_SECONDS;

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
) -> (Vec<[u8; LANES]>, Vec<f32>, Vec<usize>) {
    let (start, end) = note_range;
    let mut row_indices = Vec::with_capacity((end - start).saturating_mul(2));
    for note in &notes[start..end] {
        if note.column < col_start
            || note.column - col_start >= LANES
            || crossover_note_char(note).is_none()
        {
            continue;
        }
        row_indices.push(note.row_index);
        if let Some(hold) = note.hold.as_ref() {
            row_indices.push(hold.end_row_index);
        }
    }
    row_indices.sort_unstable();
    row_indices.dedup();

    let mut row_arrays = vec![[b'0'; LANES]; row_indices.len()];
    let mut row_to_beat = vec![0.0_f32; row_indices.len()];
    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        let Some(ch) = crossover_note_char(note) else {
            continue;
        };
        let lane = note.column - col_start;
        let head = row_indices
            .binary_search(&note.row_index)
            .expect("eligible crossover head row must be indexed");
        if row_arrays[head] == [b'0'; LANES] {
            row_to_beat[head] = note.beat;
        }
        apply_crossover_cell(&mut row_arrays[head], lane, ch);
        if let Some(hold) = note.hold.as_ref() {
            let tail = row_indices
                .binary_search(&hold.end_row_index)
                .expect("eligible crossover tail row must be indexed");
            if row_arrays[tail] == [b'0'; LANES] {
                row_to_beat[tail] = hold.end_beat;
            }
            apply_crossover_cell(&mut row_arrays[tail], lane, b'3');
        }
    }
    (row_arrays, row_to_beat, row_indices)
}

#[inline(always)]
fn crossover_note_char(note: &Note) -> Option<u8> {
    if note.is_fake {
        return (note.note_type == NoteType::Mine).then_some(b'M');
    }
    match note.note_type {
        NoteType::Tap => Some(b'1'),
        NoteType::Lift => Some(b'L'),
        NoteType::Hold => Some(b'2'),
        NoteType::Roll => Some(b'4'),
        NoteType::Mine => Some(b'M'),
        NoteType::Fake => None,
    }
}

#[inline(always)]
fn apply_crossover_cell<const LANES: usize>(row: &mut [u8; LANES], lane: usize, ch: u8) {
    if matches!(ch, b'M' | b'3') {
        if row[lane] == b'0' {
            row[lane] = ch;
        }
    } else {
        row[lane] = ch;
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn build_crossover_rows_reference<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
) -> (Vec<[u8; LANES]>, Vec<f32>, Vec<usize>) {
    use std::collections::BTreeMap;

    let (start, end) = note_range;
    let mut rows: BTreeMap<usize, ([u8; LANES], f32)> = BTreeMap::new();
    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        let Some(ch) = crossover_note_char(note) else {
            continue;
        };
        let lane = note.column - col_start;
        let entry = rows
            .entry(note.row_index)
            .or_insert(([b'0'; LANES], note.beat));
        apply_crossover_cell(&mut entry.0, lane, ch);
        if let Some(hold) = note.hold.as_ref() {
            let tail = rows
                .entry(hold.end_row_index)
                .or_insert(([b'0'; LANES], hold.end_beat));
            apply_crossover_cell(&mut tail.0, lane, b'3');
        }
    }
    let mut row_arrays = Vec::with_capacity(rows.len());
    let mut row_to_beat = Vec::with_capacity(rows.len());
    let mut row_indices = Vec::with_capacity(rows.len());
    for (row_index, (arr, beat)) in rows {
        row_arrays.push(arr);
        row_to_beat.push(beat);
        row_indices.push(row_index);
    }
    (row_arrays, row_to_beat, row_indices)
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
    let cue_capacity = annos
        .windows(2)
        .filter(|pair| {
            pair[1].is_active_crossover(include_brackets)
                && !pair[0].is_active_crossover(include_brackets)
        })
        .count();
    build_crossover_cues_core_with_capacity(
        annos,
        arrow_time,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
        cue_capacity,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_core_with_capacity(
    annos: &[CrossoverRow],
    arrow_time: impl Fn(f32) -> f32,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
    initial_capacity: usize,
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
        let mut columns = [
            ColumnCueColumn {
                column: col_start + curr_col,
                is_mine: false,
            },
            ColumnCueColumn {
                column: col_start + prev_col,
                is_mine: false,
            },
        ]
        .into_iter()
        .collect::<ColumnCueColumns>();
        let mut start_time = prev_arrow_time - duration;
        let mut cue_duration = duration + fade;
        if !first_condition {
            cue_duration += cur_arrow_time - prev_arrow_time;
        }
        if is_scooby
            && let Some(next_anno) = next
            && let Some(next_col) = crossover_arrow_col(next_anno.column_mask, true)
        {
            columns.insert(col_start + next_col, true);
        }
        let overlap = cues.last().map(|last| {
            let prev_end = last.start_time + last.duration;
            // Only one cue is active at a time and each cue drives all of its
            // columns with a single fade envelope, so a column shared by two
            // overlapping cues would fade out and back in (a visible reflash).
            let shares_column = last.columns.shares_lane(columns);
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
                last.columns.extend_missing(columns);
                continue;
            }
            let duration_difference = prev_end - start_time;
            start_time = prev_end - fade;
            cue_duration = cue_duration - duration_difference + fade;
        }
        if cues.is_empty() {
            cues.reserve(initial_capacity);
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn build_crossover_cues_for_bench(annos: &[CrossoverRow]) -> Vec<ColumnCue> {
    build_crossover_cues_core(annos, |beat| beat * 0.5, 0, 500, 8, false, 0.0)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn build_crossover_cues_reference_for_bench(annos: &[CrossoverRow]) -> Vec<ColumnCue> {
    build_crossover_cues_core_with_capacity(
        annos,
        |beat| beat * 0.5,
        0,
        500,
        8,
        false,
        0.0,
        0,
    )
}
