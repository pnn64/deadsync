use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    apply_insert_intelligent_taps, apply_insert_intelligent_taps_reference,
    intelligent_candidate_count_bench, intelligent_candidate_count_reference_bench,
    intelligent_endpoint_checksum_bench, intelligent_endpoint_checksum_reference_bench,
    intelligent_window_checksum_bench, intelligent_window_checksum_reference_bench,
    sort_player_notes,
};
use deadsync_rules::note::{HoldData, Note};
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES: usize = 2_048;
const INSERT_NOTES: usize = 512;
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

    fn add(&mut self, other: Self) {
        self.allocs += other.allocs;
        self.reallocs += other.reallocs;
        self.frees += other.frees;
        self.alloc_bytes += other.alloc_bytes;
        self.realloc_bytes += other.realloc_bytes;
        self.free_bytes += other.free_bytes;
    }

    fn churn_calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    fn churn_bytes(self) -> u64 {
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

fn measure_read(ops_per_sample: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    let mut result = empty_result();
    for _ in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..ops_per_sample {
            result.checksum = result.checksum.wrapping_add(black_box(operation()));
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

fn component_fixture() -> Vec<Note> {
    (0..NOTES)
        .map(|index| {
            let row = index * ROWS_PER_BEAT as usize;
            let note_type = if index.is_multiple_of(43) {
                NoteType::Hold
            } else if index.is_multiple_of(29) {
                NoteType::Mine
            } else if index.is_multiple_of(31) {
                NoteType::Lift
            } else {
                NoteType::Tap
            };
            let end_row = row + ROWS_PER_BEAT as usize * 2;
            Note {
                beat: index as f32,
                quantization_idx: 0,
                column: (index * 3) % 4,
                note_type,
                row_index: row,
                result: None,
                early_result: None,
                hold: (note_type == NoteType::Hold).then_some(HoldData {
                    end_row_index: end_row,
                    end_beat: end_row as f32 / ROWS_PER_BEAT as f32,
                    result: None,
                    life: 1.0,
                    let_go_started_at: None,
                    let_go_starting_life: 1.0,
                    last_held_row_index: row,
                    last_held_beat: index as f32,
                }),
                mine_result: None,
                is_fake: index.is_multiple_of(47),
                can_be_judged: true,
            }
        })
        .collect()
}

fn insert_fixture() -> Vec<Note> {
    (0..INSERT_NOTES)
        .map(|index| Note {
            beat: index as f32,
            quantization_idx: 0,
            column: (index * 3) % 4,
            note_type: NoteType::Tap,
            row_index: index * ROWS_PER_BEAT as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
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
            items / new.elapsed.as_secs_f64(),
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
    let notes = component_fixture();

    const CANDIDATE_OPS: usize = 128;
    let old_candidates = measure_read(CANDIDATE_OPS, || {
        intelligent_candidate_count_reference_bench(&notes, 0, 4, ROWS_PER_BEAT as usize) as u64
    });
    let new_candidates = measure_read(CANDIDATE_OPS, || {
        intelligent_candidate_count_bench(&notes, 0, 4, ROWS_PER_BEAT as usize) as u64
    });
    print_pair(
        "1. streamed intelligent candidates",
        "note",
        NOTES,
        CANDIDATE_OPS,
        &old_candidates,
        &new_candidates,
    );
    assert_eq!(new_candidates.allocated.churn_calls(), 0);

    let rows = (0..NOTES - 1)
        .step_by(4)
        .map(|index| index * ROWS_PER_BEAT as usize)
        .collect::<Vec<_>>();
    const SUMMARY_OPS: usize = 8;
    let old_summaries = measure_read(SUMMARY_OPS, || {
        intelligent_endpoint_checksum_reference_bench(&notes, &rows, ROWS_PER_BEAT as usize, 0, 4)
    });
    let new_summaries = measure_read(SUMMARY_OPS, || {
        intelligent_endpoint_checksum_bench(&notes, &rows, ROWS_PER_BEAT as usize, 0, 4)
    });
    print_pair(
        "2. bounded endpoint summaries",
        "row",
        rows.len(),
        SUMMARY_OPS,
        &old_summaries,
        &new_summaries,
    );
    assert_eq!(new_summaries.allocated.churn_calls(), 0);

    const WINDOW_OPS: usize = 8;
    let old_windows = measure_read(WINDOW_OPS, || {
        intelligent_window_checksum_reference_bench(&notes, &rows, ROWS_PER_BEAT as usize, 0, 4)
    });
    let new_windows = measure_read(WINDOW_OPS, || {
        intelligent_window_checksum_bench(&notes, &rows, ROWS_PER_BEAT as usize, 0, 4)
    });
    print_pair(
        "3. carried hold and bounded windows",
        "row",
        rows.len(),
        WINDOW_OPS,
        &old_windows,
        &new_windows,
    );
    assert_eq!(new_windows.allocated.churn_calls(), 0);

    let last_row = INSERT_NOTES * ROWS_PER_BEAT as usize + ROWS_PER_BEAT as usize * 2;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);
    const INSERT_OPS: usize = 4;
    let old_insert = measure_mut(insert_fixture, INSERT_OPS, |notes| {
        apply_insert_intelligent_taps_reference(
            notes,
            &timing,
            0,
            4,
            ROWS_PER_BEAT as usize,
            ROWS_PER_BEAT as usize / 2,
            ROWS_PER_BEAT as usize,
            false,
        );
        sort_player_notes(notes);
    });
    let new_insert = measure_mut(insert_fixture, INSERT_OPS, |notes| {
        apply_insert_intelligent_taps(
            notes,
            &timing,
            0,
            4,
            ROWS_PER_BEAT as usize,
            ROWS_PER_BEAT as usize / 2,
            ROWS_PER_BEAT as usize,
            false,
        );
    });
    print_pair(
        "end-to-end intelligent insertion",
        "note",
        INSERT_NOTES,
        INSERT_OPS,
        &old_insert,
        &new_insert,
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
