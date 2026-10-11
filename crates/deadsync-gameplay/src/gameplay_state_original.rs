// Frozen starting implementations from 5df9ca170. Test-only.
use super::*;

pub(super) fn build_pump_hold_events_core(
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
        // Timing is immutable during emission; validate its cache once per player.
        let mut cache_times = None;
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
            push_pump_checkpoints_cached(
                &mut events,
                notes,
                &tap_rows,
                source,
                &timing_players[player],
                &gameplay_charts[player].timing_segments,
                *cache_times
                    .get_or_insert_with(|| timing_players[player].supports_row_time_cache()),
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

pub(super) fn build_crossover_cues_core(
    annos: &[CrossoverRow],
    arrow_time: impl FnMut(f32) -> f32,
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

pub(super) fn build_crossover_cues_core_with_capacity(
    annos: &[CrossoverRow],
    mut arrow_time: impl FnMut(f32) -> f32,
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

pub(super) const fn crossover_arrow_col(column_mask: u8, want_outer: bool) -> Option<usize> {
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
