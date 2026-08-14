use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::note::NoteType;
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    ChartAttackEffects, ChartAttackTransformPlayer, GameplayAttackMode, REMOVE_MASK_BIT_NO_MINES,
    apply_chart_attack_transforms, apply_chart_attack_transforms_reference,
    apply_uncommon_chart_transforms, apply_uncommon_chart_transforms_reference,
};
use deadsync_rules::note::Note;
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NOTES_PER_PLAYER: usize = 4_096;
const OPS_PER_SAMPLE: usize = 8;
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

struct TransformFixture {
    notes: Vec<Note>,
    ranges: [(usize, usize); MAX_PLAYERS],
}

struct BenchResult {
    elapsed: Duration,
    worst_sample: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(
    players: usize,
    mut operation: impl FnMut(&mut Vec<Note>, &mut [(usize, usize); MAX_PLAYERS]),
) -> BenchResult {
    let mut result = BenchResult {
        elapsed: Duration::ZERO,
        worst_sample: Duration::ZERO,
        cycles: 0,
        allocated: AllocSnapshot::default(),
        checksum: 0,
    };
    for _ in 0..SAMPLES {
        let mut fixtures = (0..OPS_PER_SAMPLE)
            .map(|_| transform_fixture(players))
            .collect::<Vec<_>>();
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for fixture in &mut fixtures {
            operation(
                black_box(&mut fixture.notes),
                black_box(&mut fixture.ranges),
            );
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
        for fixture in &fixtures {
            result.checksum = result
                .checksum
                .wrapping_add(note_checksum(&fixture.notes))
                .wrapping_add(range_checksum(fixture.ranges));
        }
        black_box(fixtures);
    }
    result
}

fn transform_fixture(players: usize) -> TransformFixture {
    let mut notes = Vec::with_capacity(players * NOTES_PER_PLAYER);
    for player in 0..players {
        notes.extend((0..NOTES_PER_PLAYER).map(|index| {
            let row_index = (index / 4) * 12;
            Note {
                beat: row_index as f32 / ROWS_PER_BEAT as f32,
                quantization_idx: 0,
                column: player * 4 + index % 4,
                note_type: if index % 19 == 0 {
                    NoteType::Mine
                } else {
                    NoteType::Tap
                },
                row_index,
                result: None,
                early_result: None,
                hold: None,
                mine_result: None,
                is_fake: false,
                can_be_judged: true,
            }
        }));
    }
    let ranges = if players == 1 {
        [(0, notes.len()), (0, 0)]
    } else {
        [(0, NOTES_PER_PLAYER), (NOTES_PER_PLAYER, notes.len())]
    };
    TransformFixture { notes, ranges }
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

fn range_checksum(ranges: [(usize, usize); MAX_PLAYERS]) -> u64 {
    ranges.into_iter().fold(0, |checksum, (start, end)| {
        checksum.rotate_left(11) ^ start as u64 ^ (end as u64).rotate_left(29)
    })
}

fn print_pair(title: &str, players: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.allocated.churn_calls() < old.allocated.churn_calls(),
        "{title} did not reduce allocation calls"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title} did not reduce allocation churn"
    );
    let operations = (OPS_PER_SAMPLE * SAMPLES) as f64;
    let items = operations * (players * NOTES_PER_PLAYER) as f64;
    println!("\n{title}");
    print_result("old", old, operations, items);
    print_result("new", new, operations, items);
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

fn print_result(label: &str, result: &BenchResult, operations: f64, items: f64) {
    println!(
        "  {label:<4} {:>9.2} ns/note  {:>9.2} cycles/note  {:>8.2} Mnote/s  \
         {:>9.2} us worst/op  {:>4.1}/{:>4.1}/{:>4.1} a/r/f  {:>10.1} churn B/op",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
        result.worst_sample.as_secs_f64() * 1.0e6 / OPS_PER_SAMPLE as f64,
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
    let last_row = (NOTES_PER_PLAYER / 4 + 4) * 12;
    let row_to_beat = (0..=last_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let segments = TimingSegments {
        bpms: vec![(0.0, 120.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &segments, &row_to_beat);
    let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
    let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
    effects[0].remove_mask = REMOVE_MASK_BIT_NO_MINES;
    effects[1].remove_mask = REMOVE_MASK_BIT_NO_MINES;

    let attack_players = [
        ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0:LEN=512:MODS=nomines"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        },
        ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0:LEN=512:MODS=nomines"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        },
    ];

    let old_single_attack = measure(1, |notes, ranges| {
        apply_chart_attack_transforms_reference(
            notes,
            ranges,
            4,
            1,
            &attack_players,
            0xA5A5_5A5A,
            512.0,
        );
    });
    let new_single_attack = measure(1, |notes, ranges| {
        apply_chart_attack_transforms(notes, ranges, 4, 1, &attack_players, 0xA5A5_5A5A, 512.0);
    });
    print_pair(
        "1. in-place single-player chart attacks",
        1,
        &old_single_attack,
        &new_single_attack,
    );

    let old_two_masks = measure(2, |notes, ranges| {
        apply_uncommon_chart_transforms_reference(notes, ranges, 4, 2, &effects, &timing_refs);
    });
    let new_two_masks = measure(2, |notes, ranges| {
        apply_uncommon_chart_transforms(notes, ranges, 4, 2, &effects, &timing_refs);
    });
    print_pair(
        "2. split-buffer two-player uncommon masks",
        2,
        &old_two_masks,
        &new_two_masks,
    );

    let old_two_attacks = measure(2, |notes, ranges| {
        apply_chart_attack_transforms_reference(
            notes,
            ranges,
            4,
            2,
            &attack_players,
            0x1234_5678,
            512.0,
        );
    });
    let new_two_attacks = measure(2, |notes, ranges| {
        apply_chart_attack_transforms(notes, ranges, 4, 2, &attack_players, 0x1234_5678, 512.0);
    });
    print_pair(
        "3. split-buffer two-player chart attacks",
        2,
        &old_two_attacks,
        &new_two_attacks,
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
