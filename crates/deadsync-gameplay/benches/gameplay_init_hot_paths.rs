use deadsync_chart::GameplayChartData;
use deadsync_core::{input::MAX_COLS, note::NoteType};
use deadsync_gameplay::{
    CrossoverRow, PumpHoldEventKind, SongLuaEaseMaskWindow, SongLuaNoteHideWindows,
    append_song_lua_ease_targets, append_song_lua_ease_targets_reference,
    build_assist_clap_rows_preallocated_for_bench, build_assist_clap_rows_reference_for_bench,
    build_column_cues_for_player, build_column_cues_for_player_reference,
    build_crossover_cues_for_bench, build_crossover_cues_reference_for_bench, build_crossover_rows,
    build_crossover_rows_reference, build_note_count_stats,
    build_note_count_stats_for_players_for_bench, build_note_count_stats_reference,
    build_pump_hold_events, build_pump_hold_events_reference,
    build_song_lua_message_command_indices, build_song_lua_message_command_indices_reference,
    build_song_lua_note_hide_windows_for_players,
    build_song_lua_note_hide_windows_for_players_reference, parse_attack_mods,
    parse_attack_mods_reference, parse_chart_attack_windows, parse_chart_attack_windows_reference,
    parse_song_lua_runtime_mods, parse_song_lua_runtime_mods_reference, pump_tap_rows_for_bench,
    pump_tap_rows_reference_for_bench, quantization_index_from_beat,
    quantization_index_from_beat_reference, song_lua_message_command_index,
    song_lua_message_command_index_reference, song_lua_note_hidden, turn_option_bits,
};
use deadsync_rules::note::{HoldData, Note};
use deadsync_rules::timing::{TickcountSegment, TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 4_096;
const LANES: usize = 4;
const SAMPLES: usize = 7;
const COLUMN_ITERS: usize = 128;
const PUMP_ITERS: usize = 256;
const CROSSOVER_ITERS: usize = 64;
const METADATA_ITERS: usize = 128;
const PUMP_EVENT_ITERS: usize = 32;
const ATTACK_PARSE_ITERS: usize = 64;
const SONG_LUA_HIDE_ITERS: usize = 32;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the original allocation
// arguments. Relaxed counters are diagnostic only and do not affect ownership.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: `ptr` and `layout` came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct TimeSample {
    ns_per_note: f64,
    cycles_per_note: f64,
    notes_per_second: f64,
    elapsed_us: f64,
    checksum: u64,
}

fn measure_time(
    notes_per_op: usize,
    iterations: usize,
    mut operation: impl FnMut() -> u64,
) -> TimeSample {
    for _ in 0..(iterations / 8).max(1) {
        black_box(operation());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed();
    let cycles = cycle_start
        .zip(cycle_counter())
        .map_or(f64::NAN, |(start, end)| end.wrapping_sub(start) as f64);
    let notes = (notes_per_op * iterations) as f64;
    TimeSample {
        ns_per_note: elapsed.as_secs_f64() * 1.0e9 / notes,
        cycles_per_note: cycles / notes,
        notes_per_second: notes / elapsed.as_secs_f64(),
        elapsed_us: elapsed.as_secs_f64() * 1.0e6 / iterations as f64,
        checksum,
    }
}

fn measure_alloc(mut operation: impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn measure_pair(
    notes_per_op: usize,
    iterations: usize,
    mut old: impl FnMut() -> u64,
    mut new: impl FnMut() -> u64,
) -> (TimeSample, TimeSample, AllocSnapshot, AllocSnapshot) {
    let mut old_samples = Vec::with_capacity(SAMPLES);
    let mut new_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample % 2 == 0 {
            (
                measure_time(notes_per_op, iterations, &mut old),
                measure_time(notes_per_op, iterations, &mut new),
            )
        } else {
            let new_sample = measure_time(notes_per_op, iterations, &mut new);
            let old_sample = measure_time(notes_per_op, iterations, &mut old);
            (old_sample, new_sample)
        };
        assert_eq!(old_sample.checksum, new_sample.checksum);
        old_samples.push(old_sample);
        new_samples.push(new_sample);
    }
    old_samples.sort_by(|left, right| left.ns_per_note.total_cmp(&right.ns_per_note));
    new_samples.sort_by(|left, right| left.ns_per_note.total_cmp(&right.ns_per_note));
    let old_time = old_samples[SAMPLES / 2];
    let new_time = new_samples[SAMPLES / 2];
    let (old_alloc, old_checksum) = measure_alloc(&mut old);
    let (new_alloc, new_checksum) = measure_alloc(&mut new);
    assert_eq!(old_checksum, new_checksum);
    (old_time, new_time, old_alloc, new_alloc)
}

fn print_pair(
    name: &str,
    old: TimeSample,
    new: TimeSample,
    old_alloc: AllocSnapshot,
    new_alloc: AllocSnapshot,
) {
    println!("\n{name}");
    println!(
        "  old {:>8.2} ns/note {:>8.2} cycles/note {:>8.2} Mnote/s {:>9.2} us/op {:>4} calls {:>10} churn B/op",
        old.ns_per_note,
        old.cycles_per_note,
        old.notes_per_second / 1.0e6,
        old.elapsed_us,
        old_alloc.calls(),
        old_alloc.churn_bytes(),
    );
    println!(
        "  new {:>8.2} ns/note {:>8.2} cycles/note {:>8.2} Mnote/s {:>9.2} us/op {:>4} calls {:>10} churn B/op",
        new.ns_per_note,
        new.cycles_per_note,
        new.notes_per_second / 1.0e6,
        new.elapsed_us,
        new_alloc.calls(),
        new_alloc.churn_bytes(),
    );
    println!(
        "  change {:+.2}% latency/cycles {:+.2}% throughput {:+.2}% churn bytes",
        percent(new.ns_per_note, old.ns_per_note),
        percent(new.notes_per_second, old.notes_per_second),
        percent(
            new_alloc.churn_bytes() as f64,
            old_alloc.churn_bytes() as f64,
        ),
    );
}

fn percent(new: f64, old: f64) -> f64 {
    if old == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

fn note(row: usize, lane: usize) -> Note {
    let note_type = match (row + lane) % 9 {
        0 => NoteType::Mine,
        1 => NoteType::Lift,
        2 | 3 => NoteType::Hold,
        4 => NoteType::Roll,
        5 => NoteType::Fake,
        _ => NoteType::Tap,
    };
    let beat = row as f32 * 0.25;
    let hold = matches!(note_type, NoteType::Hold | NoteType::Roll).then(|| HoldData {
        end_row_index: row * 12 + 24 + lane,
        end_beat: beat + 0.5 + lane as f32 / 48.0,
        result: None,
        life: 1.0,
        let_go_started_at: None,
        let_go_starting_life: 1.0,
        last_held_row_index: 0,
        last_held_beat: 0.0,
    });
    Note {
        beat,
        quantization_idx: 0,
        column: lane,
        note_type,
        row_index: row * 12,
        result: None,
        early_result: None,
        hold,
        mine_result: None,
        is_fake: (row + lane) % 31 == 0,
        can_be_judged: (row + lane) % 37 != 0,
    }
}

fn fixture() -> (Vec<Note>, Vec<i64>) {
    let mut notes = Vec::with_capacity(ROWS * LANES);
    let mut times = Vec::with_capacity(ROWS * LANES);
    for row in 0..ROWS {
        for lane in 0..LANES {
            notes.push(note(row, lane));
            times.push(row as i64 * 125_000_000);
        }
    }
    (notes, times)
}

fn cue_checksum(cues: Vec<deadsync_gameplay::ColumnCue>) -> u64 {
    cues.into_iter().fold(0u64, |sum, cue| {
        sum.wrapping_add(cue.start_time.to_bits() as u64)
            .rotate_left(5)
            .wrapping_add(cue.duration.to_bits() as u64)
            .wrapping_add(cue.columns.len() as u64)
    })
}

fn row_checksum(rows: (Vec<[u8; LANES]>, Vec<f32>, Vec<usize>)) -> u64 {
    let (arrays, beats, indices) = rows;
    arrays
        .into_iter()
        .zip(beats)
        .zip(indices)
        .fold(0u64, |sum, ((row, beat), index)| {
            row.into_iter()
                .fold(sum.wrapping_add(index as u64), |sum, value| {
                    sum.rotate_left(3).wrapping_add(value as u64)
                })
                .wrapping_add(beat.to_bits() as u64)
        })
}

fn note_stat_checksum(stats: Vec<deadsync_gameplay::NoteCountStat>) -> u64 {
    stats.into_iter().fold(0u64, |sum, stat| {
        sum.rotate_left(7)
            .wrapping_add(stat.beat.to_bits() as u64)
            .wrapping_add(stat.notes_lower as u64)
            .wrapping_add(stat.notes_upper as u64)
    })
}

fn player_note_stat_checksum(stats: deadsync_gameplay::GameplayNoteCountStatsState) -> u64 {
    (0..2).fold(0u64, |sum, player| {
        stats
            .player_stats(player)
            .iter()
            .fold(sum.rotate_left(3), |sum, stat| {
                sum.rotate_left(7)
                    .wrapping_add(stat.beat.to_bits() as u64)
                    .wrapping_add(stat.notes_lower as u64)
                    .wrapping_add(stat.notes_upper as u64)
            })
    })
}

fn player_note_stat_vec_checksum(stats: [Vec<deadsync_gameplay::NoteCountStat>; 2]) -> u64 {
    stats.into_iter().fold(0u64, |sum, player| {
        player.into_iter().fold(sum.rotate_left(3), |sum, stat| {
            sum.rotate_left(7)
                .wrapping_add(stat.beat.to_bits() as u64)
                .wrapping_add(stat.notes_lower as u64)
                .wrapping_add(stat.notes_upper as u64)
        })
    })
}

fn crossover_cue_fixture() -> Vec<CrossoverRow> {
    let mut rows = Vec::with_capacity(96);
    for pair in 0..48 {
        let beat = pair as f32 * 2.0;
        rows.push(CrossoverRow {
            beat,
            column_mask: 0b0010,
            crossover: false,
            bracket: false,
        });
        rows.push(CrossoverRow {
            beat: beat + 0.25,
            column_mask: if pair % 2 == 0 { 0b0001 } else { 0b1000 },
            crossover: true,
            bracket: false,
        });
    }
    rows
}

fn quantization_checksum(beats: &[f32], classify: impl Fn(f32) -> u8) -> u64 {
    beats.iter().fold(0u64, |sum, &beat| {
        sum.rotate_left(3).wrapping_add(u64::from(classify(beat)))
    })
}

fn attack_window_checksum(windows: Vec<deadsync_gameplay::ChartAttackWindow>) -> u64 {
    windows.into_iter().fold(0u64, |sum, window| {
        window.mods.bytes().fold(
            sum.rotate_left(5)
                .wrapping_add(u64::from(window.start_second.to_bits()))
                .wrapping_add(u64::from(window.len_seconds.to_bits())),
            |sum, byte| sum.rotate_left(3).wrapping_add(u64::from(byte)),
        )
    })
}

fn parsed_mod_checksum(mods: deadsync_gameplay::ParsedAttackMods) -> u64 {
    let option_bits = |value: Option<f32>| value.map_or(0, |value| u64::from(value.to_bits()));
    u64::from(mods.insert_mask)
        | (u64::from(mods.remove_mask) << 8)
        | (u64::from(mods.holds_mask) << 16)
        | (u64::from(turn_option_bits(mods.turn_option)) << 24)
            ^ option_bits(mods.visual.drunk).rotate_left(7)
            ^ option_bits(mods.visual.bumpy_cols[2]).rotate_left(13)
            ^ option_bits(mods.appearance.hidden_offset).rotate_left(19)
            ^ option_bits(mods.scroll.reverse).rotate_left(29)
            ^ option_bits(mods.mini_percent).rotate_left(37)
            ^ u64::from(mods.clear_all)
}

fn command_index_checksum(commands: &[String], queries: &[String], reference: bool) -> u64 {
    if reference {
        let indices = build_song_lua_message_command_indices_reference(
            commands
                .iter()
                .enumerate()
                .map(|(index, command)| (index, command.as_str())),
        );
        return queries.iter().fold(0u64, |sum, query| {
            sum.rotate_left(5).wrapping_add(
                song_lua_message_command_index_reference(&indices, query).unwrap_or(usize::MAX)
                    as u64,
            )
        });
    }
    let indices = build_song_lua_message_command_indices(
        commands
            .iter()
            .enumerate()
            .map(|(index, command)| (index, command.as_str())),
    );
    queries.iter().fold(0u64, |sum, query| {
        sum.rotate_left(5).wrapping_add(
            song_lua_message_command_index(&indices, query).unwrap_or(usize::MAX) as u64,
        )
    })
}

fn ease_window_checksum(windows: Vec<SongLuaEaseMaskWindow>, supported: usize) -> u64 {
    windows.into_iter().fold(supported as u64, |sum, window| {
        sum.rotate_left(5)
            ^ u64::from(window.start_second.to_bits())
            ^ u64::from(window.end_second.to_bits()).rotate_left(7)
            ^ u64::from(window.sustain_end_second.to_bits()).rotate_left(13)
            ^ u64::from(window.from.to_bits()).rotate_left(19)
            ^ u64::from(window.to.to_bits()).rotate_left(29)
    })
}

fn ease_target_checksum(targets: &[&str], reference: bool) -> u64 {
    let mut windows = Vec::with_capacity(targets.len().saturating_mul(2));
    let mut supported = 0usize;
    for (index, target) in targets.iter().copied().enumerate() {
        let appended = if reference {
            append_song_lua_ease_targets_reference(
                &mut windows,
                index as f32,
                index as f32 + 1.0,
                index as f32 + 2.0,
                target,
                -25.0,
                75.0,
                Some("outQuad"),
                Some(0.5),
                Some(1.5),
            )
        } else {
            append_song_lua_ease_targets(
                &mut windows,
                index as f32,
                index as f32 + 1.0,
                index as f32 + 2.0,
                target,
                -25.0,
                75.0,
                Some("outQuad"),
                Some(0.5),
                Some(1.5),
            )
        };
        supported += usize::from(appended);
    }
    ease_window_checksum(windows, supported)
}

fn note_hide_checksum(players: [SongLuaNoteHideWindows; 2]) -> u64 {
    players
        .iter()
        .enumerate()
        .fold(0u64, |sum, (player, windows)| {
            let sum = windows.iter().fold(
                sum.rotate_left(3)
                    .wrapping_add(windows.storage_bytes() as u64),
                |sum, window| {
                    sum.rotate_left(5)
                        ^ window.column as u64
                        ^ u64::from(window.start_beat.to_bits()).rotate_left(11)
                        ^ u64::from(window.end_beat.to_bits()).rotate_left(23)
                },
            );
            (0..MAX_COLS).fold(sum, |sum, column| {
                let beat = ((column * 37 + player * 11) % 512) as f32 * 0.25;
                sum.rotate_left(1) ^ u64::from(song_lua_note_hidden(windows, column, beat))
            })
        })
}

fn usize_checksum(values: Vec<usize>) -> u64 {
    values
        .into_iter()
        .fold(0u64, |sum, value| sum.rotate_left(5) ^ value as u64)
}

struct PumpFixture {
    notes: Vec<Note>,
    note_ranges: [(usize, usize); 2],
    note_times: Vec<i64>,
    hold_end_times: Vec<Option<i64>>,
    timing_players: [Arc<TimingData>; 2],
    gameplay_charts: [Arc<GameplayChartData>; 2],
}

fn pump_fixture(notes: &[Note]) -> PumpFixture {
    let mut segments = TimingSegments::default();
    segments.bpms = vec![(0.0, 120.0)];
    segments.tickcounts = vec![TickcountSegment {
        beat: 0.0,
        ticks: 4,
    }];
    let row_to_beat = (0..=ROWS + 8)
        .map(|row| row as f32 * 0.25)
        .collect::<Vec<_>>();
    let timing = Arc::new(TimingData::from_segments(0.0, 0.0, &segments, &row_to_beat));
    let note_times = notes
        .iter()
        .map(|note| timing.get_time_for_beat_ns(note.beat))
        .collect::<Vec<_>>();
    let hold_end_times = notes
        .iter()
        .map(|note| {
            note.hold
                .as_ref()
                .map(|hold| timing.get_time_for_beat_ns(hold.end_beat))
        })
        .collect::<Vec<_>>();
    let chart = Arc::new(GameplayChartData {
        notes: Vec::new(),
        parsed_notes: Vec::new(),
        row_to_beat,
        timing_segments: segments,
        timing: timing.as_ref().clone(),
        chart_attacks: None,
    });
    PumpFixture {
        notes: notes.to_vec(),
        note_ranges: [(0, notes.len()), (notes.len(), notes.len())],
        note_times,
        hold_end_times,
        timing_players: std::array::from_fn(|_| Arc::clone(&timing)),
        gameplay_charts: std::array::from_fn(|_| Arc::clone(&chart)),
    }
}

fn pump_event_checksum(value: (Vec<deadsync_gameplay::PumpHoldEvent>, [u32; 2])) -> u64 {
    let (events, score_rows) = value;
    events.into_iter().fold(
        u64::from(score_rows[0]) | (u64::from(score_rows[1]) << 32),
        |sum, event| {
            let kind = match event.kind {
                PumpHoldEventKind::Head => 1,
                PumpHoldEventKind::Checkpoint => 2,
                PumpHoldEventKind::Tail => 3,
            };
            sum.rotate_left(5)
                ^ event.time_ns as u64
                ^ (event.row_index as u64).rotate_left(11)
                ^ (event.note_index as u64).rotate_left(19)
                ^ (event.column as u64).rotate_left(27)
                ^ kind
                ^ u64::from(event.has_tap)
        },
    )
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: `_mm_lfence` and `_rdtsc` require only the guaranteed x86-64
    // instruction set and read timing state without dereferencing memory.
    Some(unsafe {
        core::arch::x86_64::_mm_lfence();
        let value = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        value
    })
}

#[cfg(not(target_arch = "x86_64"))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    let (notes, times) = fixture();
    let sparse_times = notes
        .iter()
        .map(|note| note.row_index as i64 / 12 * 2_000_000_000)
        .collect::<Vec<_>>();
    println!(
        "fixture: {} notes, {} rows, {} samples; setup excluded",
        notes.len(),
        ROWS,
        SAMPLES,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        COLUMN_ITERS,
        || {
            cue_checksum(build_column_cues_for_player_reference(
                black_box(&notes),
                (0, notes.len()),
                black_box(&times),
                0,
                LANES,
                -0.5,
            ))
        },
        || {
            cue_checksum(build_column_cues_for_player(
                black_box(&notes),
                (0, notes.len()),
                black_box(&times),
                0,
                LANES,
                -0.5,
            ))
        },
    );
    print_pair("1. streamed column cues", old, new, old_alloc, new_alloc);

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        COLUMN_ITERS,
        || {
            cue_checksum(build_column_cues_for_player_reference(
                black_box(&notes),
                (0, notes.len()),
                black_box(&sparse_times),
                0,
                LANES,
                -0.5,
            ))
        },
        || {
            cue_checksum(build_column_cues_for_player(
                black_box(&notes),
                (0, notes.len()),
                black_box(&sparse_times),
                0,
                LANES,
                -0.5,
            ))
        },
    );
    print_pair("1b. column cues every row", old, new, old_alloc, new_alloc);

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        PUMP_ITERS,
        || {
            pump_tap_rows_reference_for_bench(black_box(&notes), (0, notes.len()))
                .into_iter()
                .fold(0u64, |sum, row| sum.wrapping_add(row as u64))
        },
        || {
            pump_tap_rows_for_bench(black_box(&notes), (0, notes.len()))
                .into_iter()
                .fold(0u64, |sum, row| sum.wrapping_add(row as u64))
        },
    );
    print_pair("2. ordered Pump tap rows", old, new, old_alloc, new_alloc);

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        CROSSOVER_ITERS,
        || {
            row_checksum(build_crossover_rows_reference::<LANES>(
                black_box(&notes),
                (0, notes.len()),
                0,
            ))
        },
        || {
            row_checksum(build_crossover_rows::<LANES>(
                black_box(&notes),
                (0, notes.len()),
                0,
            ))
        },
    );
    print_pair("3. indexed crossover rows", old, new, old_alloc, new_alloc);

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        METADATA_ITERS,
        || {
            note_stat_checksum(build_note_count_stats_reference(
                black_box(&notes),
                (0, notes.len()),
            ))
        },
        || note_stat_checksum(build_note_count_stats(black_box(&notes), (0, notes.len()))),
    );
    print_pair(
        "4. density-sized note stats",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len(),
        METADATA_ITERS,
        || {
            usize_checksum(build_assist_clap_rows_reference_for_bench(
                black_box(&notes),
                (0, notes.len()),
            ))
        },
        || {
            usize_checksum(build_assist_clap_rows_preallocated_for_bench(
                black_box(&notes),
                (0, notes.len()),
                ROWS,
            ))
        },
    );
    print_pair(
        "5. row-sized assist clap storage",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let pump = pump_fixture(&notes);
    let (old, new, old_alloc, new_alloc) = measure_pair(
        pump.notes.len(),
        PUMP_EVENT_ITERS,
        || {
            pump_event_checksum(build_pump_hold_events_reference(
                black_box(&pump.notes),
                black_box(&pump.note_ranges),
                black_box(&pump.note_times),
                black_box(&pump.hold_end_times),
                black_box(&pump.timing_players),
                black_box(&pump.gameplay_charts),
                1,
            ))
        },
        || {
            pump_event_checksum(build_pump_hold_events(
                black_box(&pump.notes),
                black_box(&pump.note_ranges),
                black_box(&pump.note_times),
                black_box(&pump.hold_end_times),
                black_box(&pump.timing_players),
                black_box(&pump.gameplay_charts),
                1,
            ))
        },
    );
    print_pair("6. pre-sized Pump events", old, new, old_alloc, new_alloc);

    let single_ranges = [(0, notes.len()), (0, notes.len())];
    let (old, new, old_alloc, new_alloc) = measure_pair(
        notes.len() * 2,
        METADATA_ITERS,
        || {
            player_note_stat_vec_checksum([
                build_note_count_stats(black_box(&notes), black_box(single_ranges[0])),
                build_note_count_stats(black_box(&notes), black_box(single_ranges[1])),
            ])
        },
        || {
            player_note_stat_checksum(build_note_count_stats_for_players_for_bench(
                black_box(&notes),
                black_box(&single_ranges),
                1,
            ))
        },
    );
    print_pair(
        "7. shared single-player note stats",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let quantization_beats = (-32_768..32_768)
        .map(|row| row as f32 / 48.0)
        .collect::<Vec<_>>();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        quantization_beats.len(),
        METADATA_ITERS,
        || {
            quantization_checksum(black_box(&quantization_beats), |beat| {
                quantization_index_from_beat_reference(black_box(beat))
            })
        },
        || {
            quantization_checksum(black_box(&quantization_beats), |beat| {
                quantization_index_from_beat(black_box(beat))
            })
        },
    );
    print_pair(
        "8. precomputed quantization lookup",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let crossover_cues = crossover_cue_fixture();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        crossover_cues.len(),
        CROSSOVER_ITERS,
        || {
            cue_checksum(build_crossover_cues_reference_for_bench(black_box(
                &crossover_cues,
            )))
        },
        || cue_checksum(build_crossover_cues_for_bench(black_box(&crossover_cues))),
    );
    print_pair(
        "9. pre-sized crossover cues",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let attack_chunks = (0..128)
        .map(|index| {
            format!(
                "TiMe={}:LeN=2.5:MoDs=*2 50% drunk,25% hidden",
                index as f32 * 3.0
            )
        })
        .collect::<Vec<_>>()
        .join(":");
    let (old, new, old_alloc, new_alloc) = measure_pair(
        128,
        ATTACK_PARSE_ITERS,
        || {
            attack_window_checksum(parse_chart_attack_windows_reference(black_box(
                &attack_chunks,
            )))
        },
        || attack_window_checksum(parse_chart_attack_windows(black_box(&attack_chunks))),
    );
    print_pair(
        "10. borrowed chart-attack case folding",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let attack_mods = (0..16)
        .map(|_| {
            "50% Dr_Un-K,*2.4 75% HiddenOffset,30% reverse,bumpy3,\
             confusion-offset4,00125% mini,wide,noholds"
        })
        .collect::<Vec<_>>()
        .join(",");
    let attack_mod_count = attack_mods.split(',').count();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        attack_mod_count,
        ATTACK_PARSE_ITERS,
        || parsed_mod_checksum(parse_attack_mods_reference(black_box(&attack_mods))),
        || parsed_mod_checksum(parse_attack_mods(black_box(&attack_mods))),
    );
    print_pair(
        "11. stack-normalized attack keys",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let commands = (0..128)
        .map(|index| {
            if index % 16 == 0 {
                format!("{}Command{index}", "A".repeat(144))
            } else {
                format!("Command_{index}")
            }
        })
        .collect::<Vec<_>>();
    let queries = commands
        .iter()
        .map(|command| command.to_ascii_uppercase())
        .collect::<Vec<_>>();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        commands.len() + queries.len(),
        ATTACK_PARSE_ITERS,
        || command_index_checksum(black_box(&commands), black_box(&queries), true),
        || command_index_checksum(black_box(&commands), black_box(&queries), false),
    );
    print_pair(
        "12. contiguous Song-Lua command index",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let song_lua_mods = (0..16)
        .map(|_| {
            "*9999 25 invert,*9999 no hidden,*9999 3x,*9999 -25 tiny,\
             *9999 25 mini,*9999 50 incoming,*9999 15 bumpy3,\
             *9999 250 tiny2,*9999 -125 bumpyperiod,*9999 100 pulseouter"
        })
        .collect::<Vec<_>>()
        .join(",");
    let song_lua_mod_count = song_lua_mods.split(',').count();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        song_lua_mod_count,
        ATTACK_PARSE_ITERS,
        || {
            parsed_mod_checksum(parse_song_lua_runtime_mods_reference(black_box(
                &song_lua_mods,
            )))
        },
        || parsed_mod_checksum(parse_song_lua_runtime_mods(black_box(&song_lua_mods))),
    );
    print_pair(
        "13. stack-normalized Song-Lua runtime mods",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    const EASE_TARGETS: [&str; 10] = [
        "Dr_Un-K",
        "Bumpy4",
        "reverse vanish",
        "incoming",
        "confusion-offset3",
        "mini",
        "cmod",
        "pulseouter",
        "tiny2",
        "unsupported",
    ];
    let ease_targets = (0..128)
        .map(|index| EASE_TARGETS[index % EASE_TARGETS.len()])
        .collect::<Vec<_>>();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        ease_targets.len(),
        ATTACK_PARSE_ITERS,
        || ease_target_checksum(black_box(&ease_targets), true),
        || ease_target_checksum(black_box(&ease_targets), false),
    );
    print_pair(
        "14. stack-normalized Song-Lua ease targets",
        old,
        new,
        old_alloc,
        new_alloc,
    );

    let note_hides = (0..4_096)
        .map(|index| {
            let start = ((index * 37) % 4_096) as f32 * 0.25;
            (index % 2, (index * 13) % MAX_COLS, start, start + 2.0)
        })
        .collect::<Vec<_>>();
    let (old, new, old_alloc, new_alloc) = measure_pair(
        note_hides.len(),
        SONG_LUA_HIDE_ITERS,
        || {
            note_hide_checksum(build_song_lua_note_hide_windows_for_players_reference(
                black_box(note_hides.iter().copied()),
            ))
        },
        || {
            note_hide_checksum(build_song_lua_note_hide_windows_for_players(black_box(
                note_hides.iter().copied(),
            )))
        },
    );
    print_pair(
        "15. single-pass Song-Lua note-hide index",
        old,
        new,
        old_alloc,
        new_alloc,
    );
}
