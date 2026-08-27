use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    ChartAttackWindow, apply_chart_attack_windows, apply_chart_attack_windows_order_reference,
};
use deadsync_rules::note::Note;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES: usize = 8_192;
const RUNTIME_WINDOWS: usize = 128;
const TURN_WINDOWS: usize = 16;
const OPERATIONS: usize = 8;
const SAMPLES: usize = 9;

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

// SAFETY: every allocation operation delegates unchanged to `System`;
// relaxed counters only observe successful calls during measurement.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied `layout` to the global allocator.
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
        // SAFETY: this pointer-layout pair came from the delegated allocator.
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

    const fn calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn bytes(self) -> u64 {
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

fn measure(
    source: &[Note],
    attacks: &[ChartAttackWindow],
    timing: &TimingData,
    mut operation: impl FnMut(&mut Vec<Note>, &[ChartAttackWindow], &TimingData),
) -> BenchResult {
    let mut result = BenchResult {
        elapsed: Duration::ZERO,
        worst_sample: Duration::ZERO,
        cycles: 0,
        allocated: AllocSnapshot::default(),
        checksum: 0,
    };
    for _ in 0..SAMPLES {
        let mut fixtures = (0..OPERATIONS).map(|_| source.to_vec()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for notes in &mut fixtures {
            operation(black_box(notes), black_box(attacks), black_box(timing));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        result.allocated.add(ALLOC.snapshot().delta(before));
        result.elapsed += elapsed;
        result.worst_sample = result.worst_sample.max(elapsed);
        result.cycles = result
            .cycles
            .wrapping_add(cycle_end.wrapping_sub(cycle_start));
        for notes in &fixtures {
            result.checksum = result.checksum.wrapping_add(note_checksum(notes));
        }
        black_box(fixtures);
    }
    result
}

fn note_fixture() -> Vec<Note> {
    (0..NOTES)
        .map(|index| {
            let row_index = (index / 4) * 12;
            Note {
                beat: row_index as f32 / ROWS_PER_BEAT as f32,
                quantization_idx: 0,
                column: index % 4,
                note_type: NoteType::Tap,
                row_index,
                result: None,
                early_result: None,
                hold: None,
                mine_result: None,
                is_fake: false,
                can_be_judged: true,
            }
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

fn run_reference(notes: &mut Vec<Note>, attacks: &[ChartAttackWindow], timing: &TimingData) {
    apply_chart_attack_windows_order_reference(notes, attacks, timing, 0, 4, 0, 29);
}

fn run_current(notes: &mut Vec<Note>, attacks: &[ChartAttackWindow], timing: &TimingData) {
    apply_chart_attack_windows(notes, attacks, timing, 0, 4, 0, 29);
}

fn print_pair(
    title: &str,
    unit: &str,
    items_per_operation: usize,
    old: &BenchResult,
    new: &BenchResult,
) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.allocated.calls() <= old.allocated.calls(),
        "{title} increased allocator calls"
    );
    assert!(
        new.allocated.bytes() <= old.allocated.bytes(),
        "{title} increased allocation churn"
    );
    let operations = (OPERATIONS * SAMPLES) as f64;
    let items = operations * items_per_operation as f64;
    println!("\n{title}");
    print_result(unit, "old", old, operations, items);
    print_result(unit, "new", new, operations, items);
    println!(
        "  change: {:+.1}% latency, {:+.1}% cycles, {:+.1}% throughput, {:+.1}% churn calls, {:+.1}% churn bytes",
        percent(old.elapsed.as_secs_f64(), new.elapsed.as_secs_f64()),
        percent(old.cycles as f64, new.cycles as f64),
        percent(
            items / old.elapsed.as_secs_f64(),
            items / new.elapsed.as_secs_f64(),
        ),
        percent(old.allocated.calls() as f64, new.allocated.calls() as f64),
        percent(old.allocated.bytes() as f64, new.allocated.bytes() as f64),
    );
}

fn print_result(unit: &str, label: &str, result: &BenchResult, operations: f64, items: f64) {
    println!(
        "  {label:<4} {:>9.2} ns/{unit}  {:>9.2} cycles/{unit}  {:>8.2} M{unit}/s  \
         {:>9.2} us worst/op  {:>5.1}/{:>4.1}/{:>5.1} a/r/f  {:>10.1} churn B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
        result.worst_sample.as_secs_f64() * 1.0e6 / OPERATIONS as f64,
        result.allocated.allocs as f64 / operations,
        result.allocated.reallocs as f64 / operations,
        result.allocated.frees as f64 / operations,
        result.allocated.bytes() as f64 / operations,
    );
}

fn percent(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return 0.0;
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let last_row = (NOTES / 4 + 4) * 12;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let segments = TimingSegments {
        bpms: vec![(0.0, 120.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &segments, &row_to_beat);
    let canonical = note_fixture();

    let runtime_attacks = (0..RUNTIME_WINDOWS)
        .map(|index| ChartAttackWindow {
            start_second: index as f32,
            len_seconds: 2.0,
            mods: "50% drunk,25% hidden,30% reverse".to_string(),
        })
        .collect::<Vec<_>>();
    let old_runtime = measure(&canonical, &runtime_attacks, &timing, run_reference);
    let new_runtime = measure(&canonical, &runtime_attacks, &timing, run_current);
    print_pair(
        "1. lazy owned runtime-only attacks",
        "note",
        NOTES,
        &old_runtime,
        &new_runtime,
    );

    let mut row_ordered = canonical.clone();
    for row in row_ordered.chunks_exact_mut(4) {
        row.reverse();
    }
    let one_turn = [ChartAttackWindow {
        start_second: 0.0,
        len_seconds: 512.0,
        mods: "mirror".to_string(),
    }];
    let old_canonicalize = measure(&row_ordered, &one_turn, &timing, run_reference);
    let new_canonicalize = measure(&row_ordered, &one_turn, &timing, run_current);
    print_pair(
        "2. row-local profile-turn canonicalization",
        "note",
        NOTES,
        &old_canonicalize,
        &new_canonicalize,
    );

    let repeated_turns = (0..TURN_WINDOWS)
        .map(|_| ChartAttackWindow {
            start_second: 0.0,
            len_seconds: 512.0,
            mods: "mirror".to_string(),
        })
        .collect::<Vec<_>>();
    let old_turns = measure(&canonical, &repeated_turns, &timing, run_reference);
    let new_turns = measure(&canonical, &repeated_turns, &timing, run_current);
    print_pair(
        "3. row-local turn restoration",
        "note-window",
        NOTES * TURN_WINDOWS,
        &old_turns,
        &new_turns,
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
