use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    ParsedAttackMods, apply_chart_attack_window, apply_chart_attack_window_reference,
    chart_attack_note_range_bench, chart_attack_note_range_reference_bench, parse_attack_mods,
};
use deadsync_rules::note::Note;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CHART_NOTES: usize = 8_192;
const WINDOW_NOTES: usize = 128;
const RANGE_QUERIES: usize = 200_000;
const TRANSFORMS_PER_SAMPLE: usize = 32;
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

// SAFETY: all operations delegate unchanged to `System`; relaxed counters only
// observe successful allocation activity while measurement is enabled.
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

type AttackTransform =
    fn(&mut Vec<Note>, &TimingData, usize, usize, usize, (usize, usize), ParsedAttackMods, u64);

fn chart_notes() -> Vec<Note> {
    (0..CHART_NOTES)
        .map(|index| Note {
            beat: index as f32 / 4.0,
            quantization_idx: 0,
            column: index % 4,
            note_type: if index.is_multiple_of(17) {
                NoteType::Mine
            } else if index.is_multiple_of(31) {
                NoteType::Lift
            } else {
                NoteType::Tap
            },
            row_index: index * 12,
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
        let kind = match note.note_type {
            NoteType::Tap => 1,
            NoteType::Hold => 2,
            NoteType::Roll => 3,
            NoteType::Mine => 4,
            NoteType::Lift => 5,
            NoteType::Fake => 6,
        };
        checksum
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(note.row_index as u64)
            .wrapping_add((note.column as u64) << 32)
            .wrapping_add(kind)
    })
}

fn measure_ranges(range: fn(&[Note], usize, usize) -> (usize, usize)) -> BenchResult {
    let notes = chart_notes();
    let mut elapsed = Duration::ZERO;
    let mut worst_sample = Duration::ZERO;
    let mut cycles = 0u64;
    let mut allocated = AllocSnapshot::default();
    let mut checksum = 0u64;
    for sample in 0..SAMPLES {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for query in 0..RANGE_QUERIES {
            let start_index =
                (query.wrapping_mul(97) + sample * 131) % (CHART_NOTES - WINDOW_NOTES);
            let start_row = start_index * 12;
            let end_row = (start_index + WINDOW_NOTES - 1) * 12;
            let (start, end) = black_box(range(black_box(&notes), start_row, end_row));
            checksum = checksum
                .rotate_left(7)
                .wrapping_add(start as u64)
                .wrapping_add((end as u64) << 32);
        }
        let sample_elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        allocated.add(ALLOC.snapshot().delta(before));
        elapsed += sample_elapsed;
        worst_sample = worst_sample.max(sample_elapsed);
        cycles = cycles.wrapping_add(cycle_end.wrapping_sub(cycle_start));
    }
    BenchResult {
        elapsed,
        worst_sample,
        cycles,
        allocated,
        checksum,
    }
}

fn measure_transform(
    timing: &TimingData,
    mods: ParsedAttackMods,
    transform: AttackTransform,
) -> BenchResult {
    let start_index = CHART_NOTES / 2;
    let row_bounds = (start_index * 12, (start_index + WINDOW_NOTES - 1) * 12);
    let mut elapsed = Duration::ZERO;
    let mut worst_sample = Duration::ZERO;
    let mut cycles = 0u64;
    let mut allocated = AllocSnapshot::default();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let mut fixtures = (0..TRANSFORMS_PER_SAMPLE)
            .map(|_| chart_notes())
            .collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for notes in &mut fixtures {
            transform(
                black_box(notes),
                timing,
                0,
                4,
                0,
                row_bounds,
                mods,
                0xA5A5_5A5A,
            );
        }
        let sample_elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        allocated.add(ALLOC.snapshot().delta(before));
        for notes in &fixtures {
            checksum = checksum.wrapping_add(note_checksum(notes));
        }
        elapsed += sample_elapsed;
        worst_sample = worst_sample.max(sample_elapsed);
        cycles = cycles.wrapping_add(cycle_end.wrapping_sub(cycle_start));
        black_box(fixtures);
    }
    BenchResult {
        elapsed,
        worst_sample,
        cycles,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult, operations: usize) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    let operations = operations as f64;
    println!("\n{title}");
    print_result("old", old, operations);
    print_result("new", new, operations);
    println!(
        "  change: {:+.1}% latency, {:+.1}% cycles, {:+.1}% throughput, {:+.1}% churn bytes",
        percent_change(old.elapsed.as_secs_f64(), new.elapsed.as_secs_f64()),
        percent_change(old.cycles as f64, new.cycles as f64),
        percent_change(
            operations / old.elapsed.as_secs_f64(),
            operations / new.elapsed.as_secs_f64(),
        ),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult, operations: f64) {
    println!(
        "  {label:<4} {:>9.2} ns/op  {:>9.2} cycles/op  {:>8.2} Kop/s  \
         {:>9.2} us worst  {:>4.1}/{:>3.1}/{:>3.1} A/R/F  {:>10.1} churn B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e3,
        result.worst_sample.as_secs_f64() * 1.0e6 / (operations / SAMPLES as f64),
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
    let old_range = measure_ranges(chart_attack_note_range_reference_bench);
    let new_range = measure_ranges(chart_attack_note_range_bench);
    print_pair(
        "1. binary affected-row lookup",
        &old_range,
        &new_range,
        RANGE_QUERIES * SAMPLES,
    );

    let last_row = CHART_NOTES * 12 + 48;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / 48.0)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);

    let remove = parse_attack_mods("nomines,nolifts");
    let old_remove = measure_transform(&timing, remove, apply_chart_attack_window_reference);
    let new_remove = measure_transform(&timing, remove, apply_chart_attack_window);
    print_pair(
        "2. retained-buffer narrow remove",
        &old_remove,
        &new_remove,
        TRANSFORMS_PER_SAMPLE * SAMPLES,
    );

    let turn = parse_attack_mods("mirror");
    let old_turn = measure_transform(&timing, turn, apply_chart_attack_window_reference);
    let new_turn = measure_transform(&timing, turn, apply_chart_attack_window);
    print_pair(
        "3. bounded narrow-window sort",
        &old_turn,
        &new_turn,
        TRANSFORMS_PER_SAMPLE * SAMPLES,
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
