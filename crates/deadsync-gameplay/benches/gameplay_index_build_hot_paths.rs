use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::timing::beat_to_note_row;
use deadsync_gameplay::{
    build_gameplay_lane_mine_indices_for_bench,
    build_gameplay_lane_mine_indices_reference_for_bench,
    build_lane_note_row_indices_reference_for_bench, count_gameplay_setup_notes_for_bench,
    count_gameplay_setup_notes_reference_for_bench, reuse_lane_note_indices_for_bench,
};
use deadsync_rules::note::{HoldData, Note};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const NOTES_PER_PLAYER: usize = 8_192;
const NOTES: usize = NOTES_PER_PLAYER * MAX_PLAYERS;
const COLS: usize = 8;
const SAMPLES: usize = 7;
const COUNT_ITERS: usize = 128;
const FILL_ITERS: usize = 128;
const ROW_ITERS: usize = 256;

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

// SAFETY: every allocation operation delegates to `System` with the original
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
struct BenchResult {
    elapsed: Duration,
    worst: Duration,
    cycles: u64,
    checksum: u64,
    allocated: AllocSnapshot,
}

fn measure(iterations: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..8 {
        black_box(operation());
    }
    let mut elapsed = Duration::ZERO;
    let mut worst = Duration::ZERO;
    let mut cycles = 0u64;
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..iterations {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        let sample_elapsed = started.elapsed();
        elapsed += sample_elapsed;
        worst = worst.max(sample_elapsed);
        cycles = cycles.wrapping_add(cycle_counter().wrapping_sub(cycle_start));
    }

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    BenchResult {
        elapsed,
        worst,
        cycles,
        checksum: checksum.wrapping_add(allocation_checksum),
        allocated,
    }
}

fn print_pair(
    title: &str,
    items_per_operation: usize,
    iterations: usize,
    old: BenchResult,
    new: BenchResult,
) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.allocated.calls() <= old.allocated.calls(),
        "{title} increased allocator calls"
    );
    assert!(
        new.allocated.churn_bytes() <= old.allocated.churn_bytes(),
        "{title} increased allocation churn"
    );
    let operations = (iterations * SAMPLES) as f64;
    let items = operations * items_per_operation as f64;
    println!("\n{title}");
    print_result("old", old, items, iterations);
    print_result("new", new, items, iterations);
    println!(
        "  change: {:+.1}% latency, {:+.1}% cycles, {:+.1}% throughput, {:+.1}% churn calls, {:+.1}% churn bytes",
        percent(old.elapsed.as_secs_f64(), new.elapsed.as_secs_f64()),
        percent(old.cycles as f64, new.cycles as f64),
        percent(
            items / old.elapsed.as_secs_f64(),
            items / new.elapsed.as_secs_f64()
        ),
        percent(old.allocated.calls() as f64, new.allocated.calls() as f64),
        percent(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64
        ),
    );
}

fn print_result(label: &str, result: BenchResult, items: f64, iterations: usize) {
    println!(
        "  {label:<4} {:>8.2} ns/note  {:>8.2} cycles/note  {:>7.2} Mnote/s  \
         {:>8.2} us worst/op  {:>2}/{:>2}/{:>2} a/r/f  {:>9} churn B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
        result.worst.as_secs_f64() * 1.0e6 / iterations as f64,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.churn_bytes(),
    );
}

fn percent(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

fn note_fixture() -> Vec<Note> {
    let mut notes = Vec::with_capacity(NOTES);
    for player in 0..MAX_PLAYERS {
        for local_index in 0..NOTES_PER_PLAYER {
            let note_type = match local_index % 32 {
                3 => NoteType::Mine,
                11 => NoteType::Hold,
                19 => NoteType::Roll,
                _ => NoteType::Tap,
            };
            let beat = local_index as f32 * 0.125;
            notes.push(Note {
                beat,
                quantization_idx: 0,
                column: player * 4 + local_index % 4,
                note_type,
                row_index: local_index * 6,
                result: None,
                early_result: None,
                hold: matches!(note_type, NoteType::Hold | NoteType::Roll).then(|| HoldData {
                    end_row_index: local_index * 6 + 3,
                    end_beat: beat + 0.0625,
                    result: None,
                    life: 1.0,
                    let_go_started_at: None,
                    let_go_starting_life: 1.0,
                    last_held_row_index: local_index * 6,
                    last_held_beat: beat,
                }),
                mine_result: None,
                is_fake: false,
                can_be_judged: true,
            });
        }
    }
    notes
}

fn main() {
    let notes = note_fixture();
    let note_ranges = [
        (0, NOTES_PER_PLAYER),
        (NOTES_PER_PLAYER, NOTES_PER_PLAYER * 2),
    ];
    let note_time_cache_ns = (0..NOTES)
        .map(|index| (index % NOTES_PER_PLAYER) as i64 * 62_500_000)
        .collect::<Vec<_>>();
    let mines_total = [(NOTES_PER_PLAYER / 32) as u32; MAX_PLAYERS];
    let mut lane_note_counts = [0usize; MAX_COLS];
    let mut lane_hold_counts = [0usize; MAX_COLS];
    let mut lane_note_indices: [Vec<usize>; MAX_COLS] = std::array::from_fn(|_| Vec::new());
    for (note_index, note) in notes.iter().enumerate() {
        lane_note_counts[note.column] += 1;
        lane_note_indices[note.column].push(note_index);
        if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
            lane_hold_counts[note.column] += 1;
        }
    }
    let note_itg_rows = notes
        .iter()
        .map(|note| beat_to_note_row(note.beat))
        .collect::<Vec<_>>();

    let old = measure(COUNT_ITERS, || {
        count_gameplay_setup_notes_reference_for_bench(black_box(&notes), COLS)
    });
    let new = measure(COUNT_ITERS, || {
        count_gameplay_setup_notes_for_bench(black_box(&notes), COLS)
    });
    print_pair(
        "1. fused lane counts and ITG-row cache",
        NOTES,
        COUNT_ITERS,
        old,
        new,
    );

    let old = measure(FILL_ITERS, || {
        build_gameplay_lane_mine_indices_reference_for_bench(
            black_box(&notes),
            black_box(&note_ranges),
            black_box(&note_time_cache_ns),
            black_box(&mines_total),
            black_box(&lane_note_counts),
            black_box(&lane_hold_counts),
            MAX_PLAYERS,
            COLS,
        )
    });
    let new = measure(FILL_ITERS, || {
        build_gameplay_lane_mine_indices_for_bench(
            black_box(&notes),
            black_box(&note_ranges),
            black_box(&note_time_cache_ns),
            black_box(&mines_total),
            black_box(&lane_note_counts),
            black_box(&lane_hold_counts),
            MAX_PLAYERS,
            COLS,
        )
    });
    print_pair(
        "2. fused lane and mine index fill",
        NOTES,
        FILL_ITERS,
        old,
        new,
    );

    let old = measure(ROW_ITERS, || {
        build_lane_note_row_indices_reference_for_bench(
            black_box(&lane_note_indices),
            black_box(&note_itg_rows),
            COLS,
        )
    });
    let new = measure(ROW_ITERS, || {
        reuse_lane_note_indices_for_bench(
            black_box(&lane_note_indices),
            black_box(&note_itg_rows),
            COLS,
        )
    });
    print_pair(
        "3. shared canonical lane-row indices",
        NOTES,
        ROW_ITERS,
        old,
        new,
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
