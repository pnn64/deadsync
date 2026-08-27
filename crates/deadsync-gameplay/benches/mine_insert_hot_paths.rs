use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    INITIAL_HOLD_LIFE, apply_mines_insert, apply_mines_insert_reference, player_rows,
    player_rows_reference, sorted_track_range_has_any_note_bench, track_range_has_any_note,
};
use deadsync_rules::note::{HoldData, Note};
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES: usize = 4_096;
const SAMPLES: usize = 7;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful allocation activity while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn add(&mut self, other: Self) {
        self.allocs += other.allocs;
        self.reallocs += other.reallocs;
        self.frees += other.frees;
        self.alloc_bytes += other.alloc_bytes;
        self.realloc_bytes += other.realloc_bytes;
        self.free_bytes += other.free_bytes;
    }

    const fn churn_calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    elapsed: Duration,
    worst_sample: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_read(
    notes: &[Note],
    ops_per_sample: usize,
    mut operation: impl FnMut(&[Note], usize) -> u64,
) -> BenchResult {
    let mut result = BenchResult {
        elapsed: Duration::ZERO,
        worst_sample: Duration::ZERO,
        cycles: 0,
        allocated: AllocSnapshot::default(),
        checksum: 0,
    };
    for sample in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for operation_index in 0..ops_per_sample {
            result.checksum = result.checksum.wrapping_add(operation(
                black_box(notes),
                black_box(sample * ops_per_sample + operation_index),
            ));
        }
        let sample_elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        result.allocated.add(ALLOC.snapshot().delta(before));
        result.elapsed += sample_elapsed;
        result.worst_sample = result.worst_sample.max(sample_elapsed);
        result.cycles = result
            .cycles
            .wrapping_add(cycle_end.wrapping_sub(cycle_start));
    }
    result
}

fn measure_mut(
    fixture: impl Fn() -> Vec<Note>,
    ops_per_sample: usize,
    mut operation: impl FnMut(&mut Vec<Note>),
) -> BenchResult {
    let mut result = BenchResult {
        elapsed: Duration::ZERO,
        worst_sample: Duration::ZERO,
        cycles: 0,
        allocated: AllocSnapshot::default(),
        checksum: 0,
    };
    for _ in 0..SAMPLES {
        let mut fixtures = (0..ops_per_sample).map(|_| fixture()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for notes in &mut fixtures {
            operation(black_box(notes));
        }
        let sample_elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        result.allocated.add(ALLOC.snapshot().delta(before));
        for notes in &fixtures {
            result.checksum = result.checksum.wrapping_add(note_checksum(notes));
        }
        result.elapsed += sample_elapsed;
        result.worst_sample = result.worst_sample.max(sample_elapsed);
        result.cycles = result
            .cycles
            .wrapping_add(cycle_end.wrapping_sub(cycle_start));
        black_box(fixtures);
    }
    result
}

fn note(row: usize, column: usize, note_type: NoteType, hold_end: usize) -> Note {
    let beat = row as f32 / ROWS_PER_BEAT as f32;
    Note {
        beat,
        quantization_idx: 0,
        column,
        note_type,
        row_index: row,
        result: None,
        early_result: None,
        hold: matches!(note_type, NoteType::Hold | NoteType::Roll).then_some(HoldData {
            end_row_index: hold_end,
            end_beat: hold_end as f32 / ROWS_PER_BEAT as f32,
            result: None,
            life: INITIAL_HOLD_LIFE,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row,
            last_held_beat: beat,
        }),
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn dense_rows() -> Vec<Note> {
    (0..NOTES)
        .map(|index| note((index / 4) * 12, index % 4, NoteType::Tap, 0))
        .collect()
}

fn sparse_rows() -> Vec<Note> {
    (0..NOTES)
        .map(|index| note(index * 48, (index * 3 + index / 7) % 4, NoteType::Tap, 0))
        .collect()
}

fn mine_fixture() -> Vec<Note> {
    let hold_count = NOTES.div_ceil(13);
    let mut notes = Vec::with_capacity(NOTES + hold_count);
    notes.extend((0..NOTES).map(|index| {
        let row = index * 48;
        let is_hold = index % 13 == 0;
        note(
            row,
            (index * 3 + index / 11) % 4,
            if is_hold {
                NoteType::Hold
            } else {
                NoteType::Tap
            },
            row + 24 + (index % 5) * 12,
        )
    }));
    notes
}

fn row_checksum(rows: &[usize]) -> u64 {
    rows.iter().fold(rows.len() as u64, |checksum, &row| {
        checksum.wrapping_mul(0x9E37_79B1).wrapping_add(row as u64)
    })
}

fn note_checksum(notes: &[Note]) -> u64 {
    notes.iter().fold(notes.len() as u64, |checksum, note| {
        checksum
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(note.row_index as u64)
            .wrapping_add((note.column as u64) << 32)
            .wrapping_add(note.note_type as u64)
    })
}

fn range_checksum(
    notes: &[Note],
    operation_index: usize,
    query: impl Fn(&[Note], usize, usize, usize) -> bool,
) -> u64 {
    let max_row = (NOTES - 1) * 48;
    (0..128).fold(0u64, |checksum, query_index| {
        let center = (operation_index * 977 + query_index * 1_541) % max_row;
        let start = center.saturating_sub(23);
        let end = center + 23;
        let column = (operation_index + query_index * 3) % 4;
        checksum.rotate_left(1) ^ u64::from(query(notes, column, start, end))
    })
}

fn print_pair(
    title: &str,
    unit: &str,
    items_per_operation: usize,
    operations_per_sample: usize,
    old: &BenchResult,
    new: &BenchResult,
) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    let operations = (operations_per_sample * SAMPLES) as f64;
    let items = operations * items_per_operation as f64;
    println!("\n{title}");
    print_result("old", unit, old, operations, items);
    print_result("new", unit, new, operations, items);
    println!(
        "  change: {:+.1}% latency, {:+.1}% cycles, {:+.1}% throughput, {:+.1}% churn calls, {:+.1}% churn bytes",
        percent_change(old.elapsed.as_secs_f64(), new.elapsed.as_secs_f64()),
        percent_change(old.cycles as f64, new.cycles as f64),
        percent_change(
            items / old.elapsed.as_secs_f64(),
            items / new.elapsed.as_secs_f64()
        ),
        percent_change(
            old.allocated.churn_calls() as f64,
            new.allocated.churn_calls() as f64,
        ),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, unit: &str, result: &BenchResult, operations: f64, items: f64) {
    println!(
        "  {label:<4} {:>9.2} ns/{unit}  {:>9.2} cycles/{unit}  {:>8.2} M{unit}/s  \
         {:>9.2} us worst/op  {:>4.1}/{:>4.1}/{:>4.1} a/r/f  {:>10.1} churn B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
        result.worst_sample.as_secs_f64() * 1.0e6 / operations * SAMPLES as f64,
        result.allocated.allocs as f64 / operations,
        result.allocated.reallocs as f64 / operations,
        result.allocated.frees as f64 / operations,
        result.allocated.churn_bytes() as f64 / operations,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    const ROW_OPS: usize = 128;
    let dense = dense_rows();
    let old_rows = measure_read(&dense, ROW_OPS, |notes, _| {
        row_checksum(&player_rows_reference(notes, 0, 4))
    });
    let new_rows = measure_read(&dense, ROW_OPS, |notes, _| {
        row_checksum(&player_rows(notes, 0, 4))
    });
    print_pair(
        "1. streamed unique player rows",
        "note",
        NOTES,
        ROW_OPS,
        &old_rows,
        &new_rows,
    );

    const RANGE_OPS: usize = 32;
    let sparse = sparse_rows();
    let old_ranges = measure_read(&sparse, RANGE_OPS, |notes, operation_index| {
        range_checksum(notes, operation_index, track_range_has_any_note)
    });
    let new_ranges = measure_read(&sparse, RANGE_OPS, |notes, operation_index| {
        range_checksum(
            notes,
            operation_index,
            sorted_track_range_has_any_note_bench,
        )
    });
    print_pair(
        "2. bounded hold-tail range queries",
        "query",
        128,
        RANGE_OPS,
        &old_ranges,
        &new_ranges,
    );
    assert_eq!(new_ranges.allocated.churn_calls(), 0);

    let max_row = NOTES * 48 + ROWS_PER_BEAT as usize * 2;
    let row_to_beat = (0..=max_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);
    const MINE_OPS: usize = 8;
    let old_mines = measure_mut(mine_fixture, MINE_OPS, |notes| {
        apply_mines_insert_reference(notes, &[], &timing, 0, 4, 0, max_row);
    });
    let new_mines = measure_mut(mine_fixture, MINE_OPS, |notes| {
        apply_mines_insert(notes, &[], &timing, 0, 4, 0, max_row);
    });
    print_pair(
        "3. fused mine insertion pass",
        "note",
        NOTES,
        MINE_OPS,
        &old_mines,
        &new_mines,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> u64 {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        std::arch::x86::_rdtsc()
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> u64 {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        std::arch::x86_64::_rdtsc()
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> u64 {
    0
}
