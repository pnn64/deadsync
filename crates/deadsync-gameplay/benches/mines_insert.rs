use deadsync_core::{note::NoteType, timing::ROWS_PER_BEAT};
use deadsync_gameplay::{apply_mines_insert, apply_mines_insert_legacy_for_bench};
use deadsync_rules::note::{HoldData, Note};
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 128;
const NOTE_COUNT: usize = 2_048;
const COLS: usize = 4;
type Transform = fn(&mut Vec<Note>, &[Note], &TimingData, usize, usize, usize, usize);

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation operations are forwarded unchanged to `System`; the
// independent atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let (notes, context, timing, end_row) = fixture();
    let mut legacy = notes.clone();
    let mut current = notes.clone();
    apply(
        &mut legacy,
        &context,
        &timing,
        end_row,
        apply_mines_insert_legacy_for_bench,
    );
    apply(&mut current, &context, &timing, end_row, apply_mines_insert);
    assert_eq!(notes_checksum(&legacy), notes_checksum(&current));

    let old = measure(
        fixture_batches(&notes),
        &context,
        &timing,
        end_row,
        apply_mines_insert_legacy_for_bench,
    );
    let new = measure(
        fixture_batches(&notes),
        &context,
        &timing,
        end_row,
        apply_mines_insert,
    );
    assert_eq!(old.checksum, new.checksum);

    println!("Mines chart transform ({NOTE_COUNT} notes x {RUNS} charts)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture() -> (Vec<Note>, Vec<Note>, TimingData, usize) {
    let mut notes = Vec::with_capacity(NOTE_COUNT + NOTE_COUNT / 16);
    for index in 0..NOTE_COUNT {
        let row = index * 12;
        let mut note = note(row, index % COLS, NoteType::Tap);
        if index % 20 == 0 {
            note.note_type = NoteType::Hold;
            note.hold = Some(HoldData {
                end_row_index: row + ROWS_PER_BEAT as usize,
                end_beat: (row + ROWS_PER_BEAT as usize) as f32 / ROWS_PER_BEAT as f32,
                result: None,
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 1.0,
                last_held_row_index: row,
                last_held_beat: row as f32 / ROWS_PER_BEAT as f32,
            });
        }
        notes.push(note);
    }
    let context = (0..128)
        .map(|index| note(index * 192 + 72, index % COLS, NoteType::Tap))
        .collect::<Vec<_>>();
    let end_row = NOTE_COUNT * 12 + ROWS_PER_BEAT as usize * 2;
    let row_to_beat = (0..=end_row)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let timing = TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat);
    (notes, context, timing, end_row)
}

fn note(row: usize, column: usize, note_type: NoteType) -> Note {
    Note {
        beat: row as f32 / ROWS_PER_BEAT as f32,
        quantization_idx: 0,
        column,
        note_type,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn fixture_batches(notes: &[Note]) -> Vec<Vec<Note>> {
    (0..RUNS)
        .map(|_| {
            let mut fixture = Vec::with_capacity(notes.len() + notes.len() / 16);
            fixture.extend_from_slice(notes);
            fixture
        })
        .collect()
}

fn apply(
    notes: &mut Vec<Note>,
    context: &[Note],
    timing: &TimingData,
    end_row: usize,
    transform: Transform,
) {
    transform(notes, context, timing, 0, COLS, 0, end_row);
}

fn measure(
    mut batches: Vec<Vec<Note>>,
    context: &[Note],
    timing: &TimingData,
    end_row: usize,
    transform: Transform,
) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for notes in &mut batches {
        apply(
            black_box(notes),
            black_box(context),
            black_box(timing),
            end_row,
            transform,
        );
        checksum = checksum.rotate_left(7) ^ black_box(notes_checksum(notes));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn notes_checksum(notes: &[Note]) -> u64 {
    notes.iter().fold(notes.len() as u64, |checksum, note| {
        let kind = match note.note_type {
            NoteType::Mine => 1,
            NoteType::Hold => 2,
            NoteType::Roll => 3,
            _ => 0,
        };
        checksum.rotate_left(3) ^ note.row_index as u64 ^ ((note.column as u64) << 32) ^ kind
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = RUNS as f64;
    println!(
        "  {label:<4} {:>8.2} us/chart {:>10.0} cycles/chart {:>7.1} charts/s",
        result.elapsed.as_secs_f64() * 1.0e6 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64(),
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per chart, {:.1} KiB/chart",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations / 1024.0,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
