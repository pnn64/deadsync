// Frozen from f1cb2d207 (0.5.1150); only visibility and imports differ.
use crate::*;

pub(crate) fn pump_tap_rows(notes: &[Note], note_range: (usize, usize)) -> Vec<usize> {
    let end = note_range.1.min(notes.len());
    let start = note_range.0.min(end);
    let mut rows = Vec::with_capacity(end - start);
    let mut ordered = true;
    for note in &notes[start..end] {
        if !note.can_be_judged
            || note.is_fake
            || !matches!(
                note.note_type,
                NoteType::Tap | NoteType::Hold | NoteType::Roll
            )
        {
            continue;
        }
        let row = beat_to_note_row(note.beat).max(0) as usize;
        match rows.last().copied() {
            Some(last) if row == last => {}
            Some(last) => {
                ordered &= row > last;
                rows.push(row);
            }
            None => rows.push(row),
        }
    }
    if !ordered {
        rows.sort_unstable();
        rows.dedup();
    }
    rows
}

pub(crate) fn push_pump_checkpoints(
    events: &mut Vec<PumpHoldEvent>,
    notes: &[Note],
    tap_rows: &[usize],
    source: PumpHoldSource,
    timing: &TimingData,
    segments: &TimingSegments,
) {
    let note = &notes[source.note_index.get()];
    let mut time_cache = BeatTimeCache::new(timing);
    let cache_times = timing.supports_row_time_cache();
    for (first_row, last_row, rows_per_tick) in pump_checkpoint_ranges(note, segments) {
        let mut row = first_row;
        while row <= last_row {
            let beat = note_row_to_beat(row as i32);
            events.push(PumpHoldEvent {
                time_ns: if cache_times {
                    timing.get_time_for_beat_ns_cached(beat, &mut time_cache)
                } else {
                    timing.get_time_for_beat_ns(beat)
                },
                row_index: ChartRowIndex::from_validated(row),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Checkpoint,
                has_tap: tap_rows.binary_search(&row).is_ok(),
            });
            row = row.saturating_add(rows_per_tick);
        }
    }
}

/// Inclusive checkpoint ranges on ITG's fixed 48-row beat grid, shared by
/// allocation sizing and event emission. Segment order and boundaries stay
/// unchanged, including duplicate or reversed tickcount segments.
fn pump_checkpoint_ranges<'a>(
    note: &'a Note,
    segments: &'a TimingSegments,
) -> impl Iterator<Item = (usize, usize, usize)> + 'a {
    let note_row = beat_to_note_row(note.beat).max(0) as usize;
    let end_row = note
        .hold
        .as_ref()
        .map(|hold| beat_to_note_row(hold.end_beat).max(0) as usize);
    segments
        .tickcounts
        .iter()
        .enumerate()
        .filter_map(move |(index, segment)| {
            let end_row = end_row?;
            let ticks = usize::from(segment.ticks.min(48));
            if ticks == 0 {
                return None;
            }
            let segment_row = beat_to_note_row(segment.beat).max(0) as usize;
            let next_segment_row = segments
                .tickcounts
                .get(index + 1)
                .map_or(usize::MAX, |next| {
                    beat_to_note_row(next.beat).max(0) as usize
                });
            let first_body_row = note_row.saturating_add(1).max(segment_row);
            let last_row = end_row.min(next_segment_row.saturating_sub(1));
            let rows_per_tick = (ROWS_PER_BEAT as usize / ticks).max(1);
            let remainder = first_body_row % rows_per_tick;
            let first_row =
                first_body_row.saturating_add((rows_per_tick - remainder) % rows_per_tick);
            (first_row <= last_row).then_some((first_row, last_row, rows_per_tick))
        })
}

fn pump_event_capacity(
    notes: &[Note],
    note_ranges: &[(usize, usize); MAX_PLAYERS],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[SongTimeNs],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    num_players: usize,
) -> usize {
    let mut capacity = 0usize;
    for player in 0..num_players.min(MAX_PLAYERS) {
        let range = note_ranges[player];
        let end = range
            .1
            .min(notes.len())
            .min(note_time_cache_ns.len())
            .min(hold_end_time_cache_ns.len());
        for index in range.0.min(end)..end {
            let note = &notes[index];
            if note.can_be_judged
                && !note.is_fake
                && matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                && cached_hold_end_time_ns(hold_end_time_cache_ns[index]).is_some()
            {
                capacity = capacity.saturating_add(2);
                for (first, last, step) in
                    pump_checkpoint_ranges(note, &gameplay_charts[player].timing_segments)
                {
                    capacity = capacity.saturating_add((last - first) / step + 1);
                }
            }
        }
    }
    capacity
}

#[must_use]
pub fn build_pump_hold_events(
    notes: &[Note],
    note_ranges: &[(usize, usize); MAX_PLAYERS],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[SongTimeNs],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    num_players: usize,
) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
    build_pump_hold_events_core(
        notes,
        note_ranges,
        note_time_cache_ns,
        hold_end_time_cache_ns,
        timing_players,
        gameplay_charts,
        num_players,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_pump_hold_events_core(
    notes: &[Note],
    note_ranges: &[(usize, usize); MAX_PLAYERS],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[SongTimeNs],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    num_players: usize,
    reserve_events: bool,
) -> (Vec<PumpHoldEvent>, [u32; MAX_PLAYERS]) {
    let capacity = if reserve_events {
        pump_event_capacity(
            notes,
            note_ranges,
            note_time_cache_ns,
            hold_end_time_cache_ns,
            gameplay_charts,
            num_players,
        )
    } else {
        0
    };
    let mut events = Vec::with_capacity(capacity);
    for player in 0..num_players.min(MAX_PLAYERS) {
        let compact_player = u8::try_from(player).expect("gameplay player index must fit u8");
        let note_range = note_ranges[player];
        let tap_rows = pump_tap_rows(notes, note_range);
        let end = note_range
            .1
            .min(notes.len())
            .min(note_time_cache_ns.len())
            .min(hold_end_time_cache_ns.len());
        for note_index in note_range.0.min(end)..end {
            let note = &notes[note_index];
            if !note.can_be_judged
                || note.is_fake
                || !matches!(note.note_type, NoteType::Hold | NoteType::Roll)
            {
                continue;
            }
            let Some(end_time_ns) = cached_hold_end_time_ns(hold_end_time_cache_ns[note_index])
            else {
                continue;
            };
            let compact_note_index = ChartNoteIndex::try_from_usize(note_index)
                .expect("validated gameplay note index must fit u32");
            let compact_column =
                u8::try_from(note.column).expect("validated gameplay column must fit u8");
            let source = PumpHoldSource {
                note_index: compact_note_index,
                player: compact_player,
                column: compact_column,
            };
            events.push(PumpHoldEvent {
                time_ns: note_time_cache_ns[note_index],
                row_index: ChartRowIndex::from_validated(
                    beat_to_note_row(note.beat).max(0) as usize
                ),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Head,
                has_tap: true,
            });
            push_pump_checkpoints(
                &mut events,
                notes,
                &tap_rows,
                source,
                &timing_players[player],
                &gameplay_charts[player].timing_segments,
            );
            events.push(PumpHoldEvent {
                time_ns: end_time_ns,
                row_index: ChartRowIndex::from_validated(note.hold.as_ref().map_or_else(
                    || beat_to_note_row(note.beat).max(0) as usize,
                    |hold| beat_to_note_row(hold.end_beat).max(0) as usize,
                )),
                note_index: source.note_index,
                player: source.player,
                column: source.column,
                kind: PumpHoldEventKind::Tail,
                has_tap: false,
            });
        }
    }
    events.sort_unstable_by(|a, b| {
        a.time_ns
            .cmp(&b.time_ns)
            .then_with(|| a.row_index.cmp(&b.row_index))
            .then_with(|| a.player.cmp(&b.player))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.column.cmp(&b.column))
    });

    let mut score_rows = [0u32; MAX_PLAYERS];
    let mut previous = None;
    for event in &events {
        if event.kind != PumpHoldEventKind::Checkpoint || event.has_tap {
            continue;
        }
        let player = usize::from(event.player);
        let key = (event.player, event.row_index);
        if previous != Some(key) {
            score_rows[player] = score_rows[player].saturating_add(1);
            previous = Some(key);
        }
    }
    (events, score_rows)
}
