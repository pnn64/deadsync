use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    AttackMaskWindow, GameplayAttackMode, apply_chart_attacks_for_mode,
    apply_chart_attacks_for_mode_reference, build_attack_mask_windows_for_mode,
    build_attack_mask_windows_for_mode_reference,
};
use deadsync_rules::note::Note;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const WINDOWS: usize = 128;
const NOTES: usize = 8_192;
const MASK_OPS: usize = 64;
const APPLY_OPS: usize = 8;
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

// SAFETY: allocation operations delegate unchanged to `System`; relaxed
// counters only observe successful calls while benchmark measurement is on.
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

fn measure_outputs<T>(
    operations: usize,
    mut operation: impl FnMut() -> Vec<T>,
    checksum: impl Fn(&[T]) -> u64,
) -> BenchResult {
    let mut result = empty_result();
    for _ in 0..SAMPLES {
        let mut outputs = (0..operations).map(|_| Vec::new()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for output in &mut outputs {
            *output = black_box(operation());
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        record_sample(&mut result, before, cycle_start, cycle_end, elapsed);
        for output in &outputs {
            result.checksum = result.checksum.wrapping_add(checksum(output));
        }
        black_box(outputs);
    }
    result
}

fn measure_apply(
    timing: &TimingData,
    raw: &str,
    mut operation: impl FnMut(&mut Vec<Note>, &str, &TimingData),
) -> BenchResult {
    let mut result = empty_result();
    for _ in 0..SAMPLES {
        let mut fixtures = (0..APPLY_OPS).map(|_| note_fixture()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for notes in &mut fixtures {
            operation(black_box(notes), black_box(raw), black_box(timing));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ALLOC.enabled.store(false, Ordering::Relaxed);
        record_sample(&mut result, before, cycle_start, cycle_end, elapsed);
        for notes in &fixtures {
            result.checksum = result.checksum.wrapping_add(note_checksum(notes));
        }
        black_box(fixtures);
    }
    result
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

fn record_sample(
    result: &mut BenchResult,
    before: AllocSnapshot,
    cycle_start: u64,
    cycle_end: u64,
    elapsed: Duration,
) {
    result.allocated.add(ALLOC.snapshot().delta(before));
    result.elapsed += elapsed;
    result.worst_sample = result.worst_sample.max(elapsed);
    result.cycles = result
        .cycles
        .wrapping_add(cycle_end.wrapping_sub(cycle_start));
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

fn mask_checksum(windows: &[AttackMaskWindow]) -> u64 {
    debug_checksum(windows)
}

fn debug_checksum<T: std::fmt::Debug + ?Sized>(value: &T) -> u64 {
    format!("{value:?}").bytes().fold(0, |checksum, byte| {
        checksum
            .wrapping_mul(0x100_0000_01B3)
            .wrapping_add(byte as u64)
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

fn print_pair(
    title: &str,
    unit: &str,
    operations: usize,
    items_per_operation: usize,
    old: &BenchResult,
    new: &BenchResult,
) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.allocated.churn_calls() < old.allocated.churn_calls(),
        "{title} did not reduce allocation calls"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title} did not reduce allocation churn"
    );
    let total_operations = (operations * SAMPLES) as f64;
    let items = total_operations * items_per_operation as f64;
    println!("\n{title}");
    print_result(unit, "old", old, total_operations, items);
    print_result(unit, "new", new, total_operations, items);
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

fn print_result(unit: &str, label: &str, result: &BenchResult, operations: f64, items: f64) {
    println!(
        "  {label:<4} {:>9.2} ns/{unit}  {:>9.2} cycles/{unit}  {:>8.2} M{unit}/s  \
         {:>9.2} us worst/op  {:>5.1}/{:>4.1}/{:>5.1} a/r/f  {:>10.1} churn B/op",
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
    let mask_raw = (0..WINDOWS)
        .map(|index| {
            format!(
                "TIME={}:LEN=2.5:MODS=50% drunk,25% hidden,30% reverse,wide,nomines",
                index as f32 * 3.0
            )
        })
        .collect::<Vec<_>>()
        .join(":");
    let old_masks = measure_outputs(
        MASK_OPS,
        || {
            build_attack_mask_windows_for_mode_reference(
                Some(black_box(&mask_raw)),
                GameplayAttackMode::On,
                0,
                29,
                512.0,
            )
        },
        mask_checksum,
    );
    let new_masks = measure_outputs(
        MASK_OPS,
        || {
            build_attack_mask_windows_for_mode(
                Some(black_box(&mask_raw)),
                GameplayAttackMode::On,
                0,
                29,
                512.0,
            )
        },
        mask_checksum,
    );
    print_pair(
        "1. borrowed runtime-mask construction",
        "window",
        MASK_OPS,
        WINDOWS,
        &old_masks,
        &new_masks,
    );

    let runtime_raw = (0..WINDOWS)
        .map(|index| {
            format!(
                "TIME={}:LEN=2.5:MODS=50% drunk,25% hidden,30% reverse",
                index as f32 * 3.0
            )
        })
        .collect::<Vec<_>>()
        .join(":");
    let last_row = (NOTES / 4 + 4) * 12;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let segments = TimingSegments {
        bpms: vec![(0.0, 120.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &segments, &row_to_beat);
    let old_apply = measure_apply(&timing, &runtime_raw, |notes, raw, timing| {
        apply_chart_attacks_for_mode_reference(
            notes,
            Some(raw),
            GameplayAttackMode::On,
            timing,
            0,
            4,
            0,
            29,
            512.0,
        );
    });
    let new_apply = measure_apply(&timing, &runtime_raw, |notes, raw, timing| {
        apply_chart_attacks_for_mode(
            notes,
            Some(raw),
            GameplayAttackMode::On,
            timing,
            0,
            4,
            0,
            29,
            512.0,
        );
    });
    print_pair(
        "2. lazy runtime-only chart application",
        "note",
        APPLY_OPS,
        NOTES,
        &old_apply,
        &new_apply,
    );

    let chart_raw = (0..WINDOWS)
        .map(|index| format!("TIME={}:LEN=0.5:MODS=mirror", index as f32 * 2.0))
        .collect::<Vec<_>>()
        .join(":");
    let old_chart_apply = measure_apply(&timing, &chart_raw, |notes, raw, timing| {
        apply_chart_attacks_for_mode_reference(
            notes,
            Some(raw),
            GameplayAttackMode::On,
            timing,
            0,
            4,
            0,
            29,
            512.0,
        );
    });
    let new_chart_apply = measure_apply(&timing, &chart_raw, |notes, raw, timing| {
        apply_chart_attacks_for_mode(
            notes,
            Some(raw),
            GameplayAttackMode::On,
            timing,
            0,
            4,
            0,
            29,
            512.0,
        );
    });
    print_pair(
        "3. in-slice chart turn application",
        "window",
        APPLY_OPS,
        WINDOWS,
        &old_chart_apply,
        &new_chart_apply,
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
