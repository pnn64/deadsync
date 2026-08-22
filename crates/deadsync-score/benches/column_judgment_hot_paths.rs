use deadsync_core::note::NoteType;
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::Note;
use deadsync_score::{
    ColumnJudgments, compute_column_judgments, compute_column_judgments_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 8_192;
const ITERATIONS: usize = 1_000;
const SAMPLES: usize = 100;

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
// only observe successful calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    notes_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(
    iterations: usize,
    notes_per_op: usize,
    mut operation: impl FnMut() -> u64,
) -> BenchResult {
    for _ in 0..(iterations / 20).max(1) {
        black_box(operation());
    }

    let batch = (iterations / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(iterations.div_ceil(batch));
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations.div_ceil(batch) {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_iterations = iterations.div_ceil(batch) * batch;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_iterations as f64,
        p95_ns: sample_ns[sample_ns.len() * 95 / 100],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_iterations as f64),
        notes_per_second: measured_iterations as f64 * notes_per_op as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.notes_per_second, new.notes_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} p95 ns  \
         {:>8.2} Mnote/s  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>10.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.notes_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

struct Fixture {
    notes: Vec<Note>,
    eligible: Vec<bool>,
    cols: usize,
}

fn fixture(rows: usize, cols: usize) -> Fixture {
    let mut notes = Vec::with_capacity(rows * cols);
    let mut eligible = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        let width = 1 + row % cols;
        for lane in 0..width {
            let seed = row.wrapping_mul(17).wrapping_add(lane * 23);
            let (grade, window) = grade(seed);
            let time_error_ms = (seed % 281) as f32 - 140.0;
            let mut result = Judgment {
                time_error_ms,
                time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                    time_error_ms,
                    1.0,
                ),
                grade,
                window,
                miss_because_held: grade == JudgeGrade::Miss && lane.is_multiple_of(3),
            };
            if grade == JudgeGrade::Miss {
                result.time_error_music_ns = 0;
            }
            let early_result = (row + lane).is_multiple_of(3).then(|| Judgment {
                time_error_ms: -96.0,
                time_error_music_ns: -96_000_000,
                grade: JudgeGrade::WayOff,
                window: Some(TimingWindow::W5),
                miss_because_held: false,
            });
            notes.push(Note {
                beat: row as f32 / 12.0,
                quantization_idx: (row % 8) as u8,
                column: lane,
                note_type: if (row + lane).is_multiple_of(47) {
                    NoteType::Lift
                } else {
                    NoteType::Tap
                },
                row_index: row * 48,
                result: (!(row.is_multiple_of(997) && lane == 0)).then_some(result),
                early_result,
                hold: None,
                mine_result: None,
                is_fake: (row + lane).is_multiple_of(541),
                can_be_judged: !(row + lane).is_multiple_of(607),
            });
            eligible.push(!(row + lane).is_multiple_of(389));
        }
    }
    Fixture {
        notes,
        eligible,
        cols,
    }
}

fn grade(seed: usize) -> (JudgeGrade, Option<TimingWindow>) {
    match seed % 7 {
        0 => (JudgeGrade::Fantastic, Some(TimingWindow::W0)),
        1 => (JudgeGrade::Fantastic, Some(TimingWindow::W1)),
        2 => (JudgeGrade::Excellent, Some(TimingWindow::W2)),
        3 => (JudgeGrade::Great, Some(TimingWindow::W3)),
        4 => (JudgeGrade::Decent, Some(TimingWindow::W4)),
        5 => (JudgeGrade::WayOff, Some(TimingWindow::W5)),
        _ => (JudgeGrade::Miss, None),
    }
}

fn checksum(columns: &[ColumnJudgments]) -> u64 {
    let mut sum = columns.len() as u64;
    for value in columns {
        for count in [
            value.w0,
            value.w1,
            value.w2,
            value.w3,
            value.w4,
            value.w5,
            value.miss,
            value.early_w1,
            value.early_w2,
            value.early_w3,
            value.early_w4,
            value.early_w5,
            value.early_total_w0,
            value.early_total_w1,
            value.early_total_w2,
            value.early_total_w3,
            value.early_total_w4,
            value.early_total_w5,
            value.held_miss,
        ] {
            sum = sum.rotate_left(5) ^ u64::from(count);
        }
    }
    sum
}

fn run(title: &str, fixture: &Fixture) {
    let old_once = compute_column_judgments_reference(
        &fixture.notes,
        &fixture.eligible,
        fixture.cols,
        0,
        true,
    );
    let new_once =
        compute_column_judgments(&fixture.notes, &fixture.eligible, fixture.cols, 0, true);
    assert_eq!(old_once.as_slice(), new_once.as_slice());

    let old = measure(ITERATIONS, fixture.notes.len(), || {
        checksum(&compute_column_judgments_reference(
            black_box(&fixture.notes),
            black_box(&fixture.eligible),
            fixture.cols,
            0,
            true,
        ))
    });
    let new = measure(ITERATIONS, fixture.notes.len(), || {
        checksum(&compute_column_judgments(
            black_box(&fixture.notes),
            black_box(&fixture.eligible),
            fixture.cols,
            0,
            true,
        ))
    });
    assert_eq!(old.allocated.allocs, ITERATIONS as u64);
    assert_eq!(old.allocated.frees, ITERATIONS as u64);
    if fixture.cols <= deadsync_core::input::MAX_COLS {
        assert_eq!(new.allocated.allocs, 0);
        assert_eq!(new.allocated.reallocs, 0);
        assert_eq!(new.allocated.frees, 0);
    } else {
        assert_eq!(new.allocated.allocs, ITERATIONS as u64);
        assert_eq!(new.allocated.frees, ITERATIONS as u64);
    }
    print_pair(title, ITERATIONS, &old, &new);
}

fn main() {
    run("mixed one-to-four lane rows", &fixture(ROWS, 4));
    run("mixed one-to-ten lane rows", &fixture(ROWS, 10));
    run("wide fallback rows", &fixture(ROWS / 2, 16));
}
