use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    INITIAL_HOLD_LIFE, convert_taps_to_holds, convert_taps_to_holds_reference,
    hold_body_masks_bench, hold_body_masks_reference_bench, hold_row_local_bench,
    hold_row_local_reference_bench, hold_rows_bench, hold_rows_reference_bench,
};
use deadsync_rules::note::{HoldData, Note};
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES: usize = 2_048;
const CONVERT_NOTES: usize = 512;
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

fn empty_result() -> BenchResult {
    BenchResult {
        elapsed: Duration::ZERO,
        worst_sample: Duration::ZERO,
        cycles: 0,
        allocated: AllocSnapshot::default(),
        checksum: 0,
    }
}

fn measure_read(ops_per_sample: usize, mut operation: impl FnMut(usize) -> u64) -> BenchResult {
    let mut result = empty_result();
    for sample in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for operation_index in 0..ops_per_sample {
            result.checksum = result.checksum.wrapping_add(operation(black_box(
                sample * ops_per_sample + operation_index,
            )));
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
    mut operation: impl FnMut(&mut [Note]),
) -> BenchResult {
    let mut result = empty_result();
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

fn note(row: usize, column: usize, index: usize) -> Note {
    let selector = index * 5 + column;
    let note_type = if selector.is_multiple_of(43) {
        NoteType::Hold
    } else if selector.is_multiple_of(47) {
        NoteType::Roll
    } else if selector.is_multiple_of(29) {
        NoteType::Mine
    } else if selector.is_multiple_of(31) {
        NoteType::Lift
    } else {
        NoteType::Tap
    };
    let beat = row as f32 / ROWS_PER_BEAT as f32;
    let hold_end = row + 12 * (2 + selector % 5);
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
        is_fake: note_type == NoteType::Tap && selector.is_multiple_of(53),
        can_be_judged: true,
    }
}

fn notes_fixture(count: usize) -> Vec<Note> {
    (0..count)
        .map(|index| {
            let row_index = index / 2;
            let row = row_index * 12;
            let first_column = (row_index * 3) % 4;
            let column = (first_column + (index % 2) * 2) % 4;
            note(row, column, index)
        })
        .collect()
}

fn note_checksum(notes: &[Note]) -> u64 {
    notes.iter().fold(notes.len() as u64, |checksum, note| {
        checksum
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(note.row_index as u64)
            .wrapping_add((note.column as u64) << 32)
            .wrapping_add(note.note_type as u64)
            .wrapping_add(
                note.hold
                    .as_ref()
                    .map(|hold| hold.end_row_index as u64)
                    .unwrap_or(0),
            )
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
    let notes = notes_fixture(NOTES);

    const ROW_OPS: usize = 128;
    let old_rows = measure_read(ROW_OPS, |_| hold_rows_reference_bench(&notes, 0, 4));
    let new_rows = measure_read(ROW_OPS, |_| hold_rows_bench(&notes, 0, 4));
    print_pair(
        "1. allocation-free streamed hold rows",
        "note",
        NOTES,
        ROW_OPS,
        &old_rows,
        &new_rows,
    );
    assert_eq!(new_rows.allocated.churn_calls(), 0);

    const LOCAL_OPS: usize = 512;
    let row_count = NOTES / 2;
    let old_local = measure_read(LOCAL_OPS, |operation| {
        hold_row_local_reference_bench(&notes, (operation % row_count) * 12, 0, 4)
    });
    let new_local = measure_read(LOCAL_OPS, |operation| {
        hold_row_local_bench(&notes, (operation % row_count) * 12, 0, 4)
    });
    print_pair(
        "2. batched row head and cell lookup",
        "row",
        1,
        LOCAL_OPS,
        &old_local,
        &new_local,
    );
    assert_eq!(new_local.allocated.churn_calls(), 0);

    let body_rows = (0..row_count)
        .step_by(4)
        .map(|row| row * 12 + 6)
        .collect::<Vec<_>>();
    const BODY_OPS: usize = 16;
    let old_bodies = measure_read(BODY_OPS, |_| {
        hold_body_masks_reference_bench(&notes, &body_rows, 0, 4)
    });
    let new_bodies = measure_read(BODY_OPS, |_| {
        hold_body_masks_bench(&notes, &body_rows, 0, 4)
    });
    print_pair(
        "3. carried latest-note hold state",
        "row",
        body_rows.len(),
        BODY_OPS,
        &old_bodies,
        &new_bodies,
    );
    assert_eq!(new_bodies.allocated.churn_calls(), 0);

    let last_row = (CONVERT_NOTES / 2) * 12 + ROWS_PER_BEAT as usize * 2;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);
    const CONVERT_OPS: usize = 8;
    let old_convert = measure_mut(
        || notes_fixture(CONVERT_NOTES),
        CONVERT_OPS,
        |notes| convert_taps_to_holds_reference(notes, &timing, 0, 4, 3),
    );
    let new_convert = measure_mut(
        || notes_fixture(CONVERT_NOTES),
        CONVERT_OPS,
        |notes| convert_taps_to_holds(notes, &timing, 0, 4, 3),
    );
    print_pair(
        "end-to-end tap-to-hold conversion",
        "note",
        CONVERT_NOTES,
        CONVERT_OPS,
        &old_convert,
        &new_convert,
    );
    assert_eq!(new_convert.allocated.churn_calls(), 0);
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
