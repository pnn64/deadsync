use deadsync_chart::GameplayChartData;
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    PumpHoldEventKind, build_assist_clap_rows_preallocated_for_bench,
    build_assist_clap_rows_reference_for_bench, build_column_cues_for_player,
    build_column_cues_for_player_reference, build_crossover_rows, build_crossover_rows_reference,
    build_note_count_stats, build_note_count_stats_reference, build_pump_hold_events,
    build_pump_hold_events_reference, pump_tap_rows_for_bench, pump_tap_rows_reference_for_bench,
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
}
