use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    ChartAttackEffects, GameplayTurnOption, REMOVE_MASK_BIT_NO_MINES, apply_hyper_shuffle,
    apply_hyper_shuffle_reference, apply_turn_permutation, apply_turn_permutation_reference,
    apply_uncommon_chart_transforms, apply_uncommon_chart_transforms_reference,
};
use deadsync_rules::note::Note;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES: usize = 4_096;
const OPS_PER_SAMPLE: usize = 64;
const SAMPLES: usize = 7;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
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
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }

    fn add(&mut self, other: Self) {
        self.allocs += other.allocs;
        self.reallocs += other.reallocs;
        self.frees += other.frees;
        self.bytes += other.bytes;
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
    fixture: impl Fn() -> Vec<Note>,
    mut operation: impl FnMut(&mut Vec<Note>),
) -> BenchResult {
    let mut elapsed = Duration::ZERO;
    let mut worst_sample = Duration::ZERO;
    let mut cycles = 0u64;
    let mut allocated = AllocSnapshot::default();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let mut fixtures = (0..OPS_PER_SAMPLE).map(|_| fixture()).collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for notes in &mut fixtures {
            operation(black_box(notes));
        }
        black_box(&fixtures);
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

fn dense_notes() -> Vec<Note> {
    (0..NOTES)
        .map(|index| Note {
            beat: (index / 4) as f32 / 4.0,
            quantization_idx: 0,
            column: index % 4,
            note_type: if index % 17 == 0 {
                NoteType::Mine
            } else {
                NoteType::Tap
            },
            row_index: (index / 4) * 12,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        })
        .collect()
}

fn sparse_notes() -> Vec<Note> {
    (0..NOTES)
        .map(|index| Note {
            beat: index as f32 / 4.0,
            quantization_idx: 0,
            column: (index * 3 + index / 7) % 4,
            note_type: NoteType::Tap,
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

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(new.allocated.allocs, 0, "{title} still allocates");
    assert_eq!(new.allocated.reallocs, 0, "{title} still reallocates");
    assert_eq!(new.allocated.frees, 0, "{title} still frees");
    assert_eq!(new.allocated.bytes, 0, "{title} still allocates bytes");
    let operations = (OPS_PER_SAMPLE * SAMPLES) as f64;
    let items = operations * NOTES as f64;
    println!("\n{title}");
    print_result("old", old, operations, items);
    print_result("new", new, operations, items);
    println!(
        "  change: {:+.1}% latency, {:+.1}% cycles, {:+.1}% throughput, {:+.1}% bytes",
        percent_change(old.elapsed.as_secs_f64(), new.elapsed.as_secs_f64()),
        percent_change(old.cycles as f64, new.cycles as f64),
        percent_change(
            items / old.elapsed.as_secs_f64(),
            items / new.elapsed.as_secs_f64(),
        ),
        percent_change(old.allocated.bytes as f64, new.allocated.bytes as f64),
    );
}

fn print_result(label: &str, result: &BenchResult, operations: f64, items: f64) {
    let churn = result.allocated.allocs + result.allocated.reallocs + result.allocated.frees;
    println!(
        "  {label:<4} {:>8.2} ns/note  {:>8.2} cycles/note  {:>7.1} Mnote/s  \
         {:>8.2} us worst  {:>4.1} churn/op  {:>9.1} B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
        result.worst_sample.as_secs_f64() * 1.0e6 / OPS_PER_SAMPLE as f64,
        churn as f64 / operations,
        result.allocated.bytes as f64 / operations,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let old_rows = measure(sparse_notes, |notes| {
        let len = notes.len();
        apply_hyper_shuffle_reference(notes, (0, len), 0, 4, 0xA5A5_5A5A);
    });
    let new_rows = measure(sparse_notes, |notes| {
        let len = notes.len();
        apply_hyper_shuffle(notes, (0, len), 0, 4, 0xA5A5_5A5A);
    });
    print_pair("1. streamed random-turn rows", &old_rows, &new_rows);

    let old_turn = measure(dense_notes, |notes| {
        let len = notes.len();
        apply_turn_permutation_reference(notes, (0, len), 0, 4, GameplayTurnOption::Shuffle, 29);
    });
    let new_turn = measure(dense_notes, |notes| {
        let len = notes.len();
        apply_turn_permutation(notes, (0, len), 0, 4, GameplayTurnOption::Shuffle, 29);
    });
    print_pair("2. stack lane permutation", &old_turn, &new_turn);

    let last_row = (NOTES / 4) * 12;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / 48.0)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);
    let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
    let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
    effects[0].remove_mask = REMOVE_MASK_BIT_NO_MINES;
    let old_masks = measure(dense_notes, |notes| {
        let mut ranges = [(0, notes.len()), (0, 0)];
        apply_uncommon_chart_transforms_reference(notes, &mut ranges, 4, 1, &effects, &timing_refs);
        black_box(ranges);
    });
    let new_masks = measure(dense_notes, |notes| {
        let mut ranges = [(0, notes.len()), (0, 0)];
        apply_uncommon_chart_transforms(notes, &mut ranges, 4, 1, &effects, &timing_refs);
        black_box(ranges);
    });
    print_pair("3. in-place single-player masks", &old_masks, &new_masks);

    black_box(MAX_COLS);
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
