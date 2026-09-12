// Frozen from 0301eb86a; only visibility and parent imports adapted.
use super::*;

pub fn build_replay_input_edges(
    replay_edges: &[ReplayInputEdge],
    num_players: usize,
    cols_per_player: usize,
    num_cols: usize,
    recorded_beat0_time_ns: SongTimeNs,
    current_beat0_time_ns: [SongTimeNs; MAX_PLAYERS],
) -> Vec<RecordedLaneEdge> {
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let mut out_of_order = false;
    let mut prev_time_ns = None;

    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }

        let player = player_index_for_column(num_players, cols_per_player, lane);
        let player_beat0_time_ns = current_beat0_time_ns[player];
        let replay_beat0_shift_ns = if song_time_ns_invalid(recorded_beat0_time_ns)
            || song_time_ns_invalid(player_beat0_time_ns)
        {
            0
        } else {
            player_beat0_time_ns.saturating_sub(recorded_beat0_time_ns)
        };
        let event_music_time_ns = edge
            .event_music_time_ns
            .saturating_add(replay_beat0_shift_ns);

        if prev_time_ns.is_some_and(|prev| event_music_time_ns < prev) {
            out_of_order = true;
        }
        prev_time_ns = Some(event_music_time_ns);
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time_ns,
        });
    }

    if out_of_order {
        replay_input.sort_by_key(|edge| edge.event_music_time_ns);
    }
    replay_input
}

pub fn build_column_cues_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    note_time_cache_ns: &[SongTimeNs],
    col_start: usize,
    col_end: usize,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let (start, end) = note_range;
    if start >= end || col_start >= col_end {
        return Vec::new();
    }

    let mut cues = Vec::with_capacity((end - start).min(COLUMN_CUE_INITIAL_CAPACITY));
    let mut prev_time = 0.0_f32;
    let mut i = start;
    while i < end {
        let row = notes[i].row_index;
        let mut row_time = 0.0_f32;
        let mut has_row_time = false;
        let mut columns = ColumnCueColumns::default();
        while i < end && notes[i].row_index == row {
            let note = &notes[i];
            if note.column >= col_start
                && note.column < col_end
                && let Some(is_mine) = column_cue_is_mine(note)
            {
                if !has_row_time {
                    row_time = song_time_ns_to_seconds(note_time_cache_ns[i]);
                    has_row_time = true;
                }
                columns.insert(note.column, is_mine);
            }
            i += 1;
        }
        if has_row_time {
            let duration = row_time - prev_time;
            if duration >= COLUMN_CUE_MIN_SECONDS || prev_time == 0.0 {
                cues.push(ColumnCue {
                    start_time: prev_time,
                    duration,
                    columns,
                });
            }
            prev_time = row_time;
        }
    }

    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

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
