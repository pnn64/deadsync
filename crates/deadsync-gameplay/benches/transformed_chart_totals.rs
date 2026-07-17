use deadsync_core::note::NoteType;
use deadsync_rules::judgment::max_grade_points;
use deadsync_rules::note::{HoldData, Note, recompute_player_totals};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 32_768;
const RUNS: usize = 64;
const ROW_SPACING: usize = 48;

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

// SAFETY: allocation requests are forwarded unchanged to `System`; the
// independent atomics only observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees that this is the live allocation and
        // layout originally returned through this allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer and its original layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
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
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let notes = transformed_chart();
    let expected = evaluate(&notes);
    let result = measure(&notes);
    assert_eq!(result.checksum, expected.wrapping_mul(RUNS as u64));

    println!("transformed chart totals microbenchmark");
    println!(
        "{} notes across {ROWS} rows, {RUNS} initializations",
        notes.len()
    );
    println!(
        "{:>10.1} us/init  {:>10.0} init/s  alloc/realloc={}/{} bytes={} checksum={}",
        result.elapsed.as_secs_f64() * 1.0e6 / RUNS as f64,
        RUNS as f64 / result.elapsed.as_secs_f64(),
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.bytes,
        result.checksum,
    );
}

fn measure(notes: &[Note]) -> BenchResult {
    for _ in 0..4 {
        black_box(evaluate(black_box(notes)));
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..RUNS {
        checksum = checksum.wrapping_add(black_box(evaluate(black_box(notes))));
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

#[inline(never)]
fn evaluate(notes: &[Note]) -> u64 {
    let totals = recompute_player_totals(notes, (0, notes.len()));
    let grade_points = max_grade_points(totals.steps, totals.holds, totals.rolls, 1_000);
    u64::from(totals.steps)
        ^ (u64::from(totals.holds) << 12)
        ^ (u64::from(totals.rolls) << 24)
        ^ (u64::from(totals.mines) << 36)
        ^ (u64::from(totals.hands) << 44)
        ^ (grade_points as u64).rotate_left(17)
}

fn transformed_chart() -> Vec<Note> {
    let mut notes = Vec::with_capacity(ROWS * 3);
    for row in 0..ROWS {
        let row_index = row * ROW_SPACING;
        if row % 16 == 0 {
            notes.push(hold_note(
                0,
                row_index,
                row_index + 12 * ROW_SPACING,
                NoteType::Hold,
            ));
        }
        notes.push(note(1, row_index, NoteType::Tap));
        if row % 2 == 0 {
            notes.push(note(2, row_index, NoteType::Tap));
        }
        if row % 8 == 4 {
            notes.push(hold_note(
                3,
                row_index,
                row_index + 3 * ROW_SPACING,
                NoteType::Roll,
            ));
        } else if row % 7 == 0 {
            notes.push(note(3, row_index, NoteType::Mine));
        }
    }
    notes
}

fn note(column: usize, row_index: usize, note_type: NoteType) -> Note {
    Note {
        beat: row_index as f32 / ROW_SPACING as f32,
        quantization_idx: 0,
        column,
        note_type,
        row_index,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn hold_note(column: usize, row_index: usize, end_row_index: usize, note_type: NoteType) -> Note {
    Note {
        hold: Some(HoldData {
            end_row_index,
            end_beat: end_row_index as f32 / ROW_SPACING as f32,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row_index,
            last_held_beat: row_index as f32 / ROW_SPACING as f32,
        }),
        ..note(column, row_index, note_type)
    }
}
