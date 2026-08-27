use deadsync_core::note::NoteType;
use deadsync_rules::{
    judgment::{
        ExScoreTotals, JudgeGrade, Judgment, bench_support, calculate_ex_score_from_notes,
        calculate_ex_score_percents_from_notes, judgment_time_error_music_ns_from_ms,
    },
    note::{HoldData, HoldResult, MineResult, Note},
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 8_192;
const ITERATIONS: usize = 1_000;
const SAMPLE_BATCHES: usize = 50;

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

// SAFETY: allocator operations delegate unchanged to `System`; relaxed
// counters only observe successful calls while this single-threaded bench runs.
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

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    p95_sample_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(items_per_op: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(ITERATIONS / 20) {
        black_box(operation());
    }
    let batch = (ITERATIONS / SAMPLE_BATCHES).max(1);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut sample_ns = [0.0f64; SAMPLE_BATCHES];
    for sample in &mut sample_ns {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        *sample = sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / batch as f64;
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..ITERATIONS {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    sample_ns.sort_unstable_by(f64::total_cmp);
    BenchResult {
        ns_per_op: seconds * 1_000_000_000.0 / ITERATIONS as f64,
        p95_sample_ns: sample_ns[SAMPLE_BATCHES * 95 / 100],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ITERATIONS as f64),
        items_per_second: ITERATIONS as f64 * items_per_op as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        change(old.ns_per_op, new.ns_per_op),
        change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        change(old.items_per_second, new.items_per_second),
        change(old.p95_sample_ns, new.p95_sample_ns),
        change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let count = ITERATIONS as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>12.2} cycles/op  {:>11.2} p95 ns  \
         {:>8.2} Mnote/s  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_sample_ns,
        result.items_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
    );
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

struct Fixture {
    notes: Vec<Note>,
    note_times_ns: Vec<i64>,
    hold_end_times_ns: Vec<i64>,
    totals: ExScoreTotals,
    fail_time_ns: i64,
}

fn fixture() -> Fixture {
    let mut notes = Vec::with_capacity(ROWS * 4);
    let mut note_times_ns = Vec::with_capacity(ROWS * 4);
    let mut hold_end_times_ns = Vec::with_capacity(ROWS * 4);
    let mut totals = ExScoreTotals {
        total_steps: ROWS as u32,
        ..ExScoreTotals::default()
    };
    for row in 0..ROWS {
        let row_time_ns = row as i64 * 125_000_000;
        push_note(
            &mut notes,
            &mut note_times_ns,
            &mut hold_end_times_ns,
            scored_note(row, 0, grade(row), error_ms(row)),
            row_time_ns,
            i64::MIN,
        );
        push_note(
            &mut notes,
            &mut note_times_ns,
            &mut hold_end_times_ns,
            scored_note(
                row,
                1,
                if row % 37 == 0 {
                    JudgeGrade::Miss
                } else {
                    JudgeGrade::Fantastic
                },
                11.0 + (row % 9) as f32,
            ),
            row_time_ns,
            i64::MIN,
        );

        let mut third = scored_note(row, 2, JudgeGrade::Excellent, -18.0);
        let third_end_ns = match row % 16 {
            0 => {
                third.note_type = NoteType::Hold;
                third.hold = Some(hold(row, row % 48 != 0));
                totals.holds_total += 1;
                row_time_ns + 750_000_000
            }
            8 => {
                third.note_type = NoteType::Roll;
                third.hold = Some(hold(row, row % 40 != 8));
                totals.rolls_total += 1;
                row_time_ns + 1_000_000_000
            }
            _ => i64::MIN,
        };
        push_note(
            &mut notes,
            &mut note_times_ns,
            &mut hold_end_times_ns,
            third,
            row_time_ns,
            third_end_ns,
        );

        let mut fourth = scored_note(row, 3, JudgeGrade::Great, 28.0);
        match row % 8 {
            0 => {
                fourth.note_type = NoteType::Mine;
                fourth.result = None;
                fourth.mine_result = Some(if row % 24 == 0 {
                    MineResult::Hit
                } else {
                    MineResult::Avoided
                });
                totals.mines_total += 1;
            }
            1 => fourth.is_fake = true,
            2 => fourth.can_be_judged = false,
            _ => {}
        }
        push_note(
            &mut notes,
            &mut note_times_ns,
            &mut hold_end_times_ns,
            fourth,
            row_time_ns,
            i64::MIN,
        );
    }
    Fixture {
        notes,
        note_times_ns,
        hold_end_times_ns,
        totals,
        fail_time_ns: ROWS as i64 / 2 * 125_000_000 + 50_000_000,
    }
}

fn scored_note(row: usize, column: usize, grade: JudgeGrade, error_ms: f32) -> Note {
    Note {
        beat: row as f32 / 4.0,
        quantization_idx: 0,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: Some(Judgment {
            time_error_ms: error_ms,
            time_error_music_ns: judgment_time_error_music_ns_from_ms(error_ms, 1.0),
            grade,
            window: None,
            miss_because_held: false,
        }),
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn hold(row: usize, held: bool) -> HoldData {
    HoldData {
        end_row_index: row + 24,
        end_beat: (row + 24) as f32 / 4.0,
        result: Some(if held {
            HoldResult::Held
        } else {
            HoldResult::LetGo
        }),
        life: if held { 1.0 } else { 0.0 },
        let_go_started_at: None,
        let_go_starting_life: 1.0,
        last_held_row_index: row,
        last_held_beat: row as f32 / 4.0,
    }
}

const fn grade(row: usize) -> JudgeGrade {
    [
        JudgeGrade::Fantastic,
        JudgeGrade::Excellent,
        JudgeGrade::Great,
        JudgeGrade::Decent,
        JudgeGrade::WayOff,
        JudgeGrade::Miss,
    ][row % 6]
}

fn error_ms(row: usize) -> f32 {
    (row % 101) as f32 - 50.0
}

fn push_note(
    notes: &mut Vec<Note>,
    note_times_ns: &mut Vec<i64>,
    hold_end_times_ns: &mut Vec<i64>,
    note: Note,
    time_ns: i64,
    hold_end_ns: i64,
) {
    notes.push(note);
    note_times_ns.push(time_ns);
    hold_end_times_ns.push(hold_end_ns);
}

#[inline(never)]
fn old_ex(fixture: &Fixture, fail_time_ns: Option<i64>) -> u64 {
    bench_support::ex_score_percent_from_notes(
        &fixture.notes,
        &fixture.note_times_ns,
        &fixture.hold_end_times_ns,
        fixture.totals,
        fail_time_ns,
    )
    .to_bits()
}

#[inline(never)]
fn new_ex(fixture: &Fixture, fail_time_ns: Option<i64>) -> u64 {
    calculate_ex_score_from_notes(
        &fixture.notes,
        &fixture.note_times_ns,
        &fixture.hold_end_times_ns,
        fixture.totals.total_steps,
        fixture.totals.holds_total,
        fixture.totals.rolls_total,
        fixture.totals.mines_total,
        fail_time_ns,
        false,
    )
    .to_bits()
}

#[inline(never)]
fn old_paired(fixture: &Fixture) -> u64 {
    let (ex, hard_ex) = bench_support::ex_score_percents_from_notes(
        &fixture.notes,
        &fixture.note_times_ns,
        &fixture.hold_end_times_ns,
        fixture.totals,
        None,
    );
    ex.to_bits().rotate_left(17) ^ hard_ex.to_bits()
}

#[inline(never)]
fn new_paired(fixture: &Fixture) -> u64 {
    let (ex, hard_ex) = calculate_ex_score_percents_from_notes(
        &fixture.notes,
        &fixture.note_times_ns,
        &fixture.hold_end_times_ns,
        fixture.totals,
        None,
    );
    ex.to_bits().rotate_left(17) ^ hard_ex.to_bits()
}

fn main() {
    let fixture = fixture();
    assert_eq!(old_ex(&fixture, None), new_ex(&fixture, None));
    assert_eq!(
        old_ex(&fixture, Some(fixture.fail_time_ns)),
        new_ex(&fixture, Some(fixture.fail_time_ns)),
    );
    assert_eq!(old_paired(&fixture), new_paired(&fixture));

    let old = measure(fixture.notes.len(), || old_ex(black_box(&fixture), None));
    let new = measure(fixture.notes.len(), || new_ex(black_box(&fixture), None));
    print_pair("full-song EX reconstruction", &old, &new);

    let old = measure(fixture.notes.len(), || {
        old_ex(black_box(&fixture), Some(fixture.fail_time_ns))
    });
    let new = measure(fixture.notes.len(), || {
        new_ex(black_box(&fixture), Some(fixture.fail_time_ns))
    });
    print_pair("failed-song EX reconstruction", &old, &new);

    let old = measure(fixture.notes.len(), || old_paired(black_box(&fixture)));
    let new = measure(fixture.notes.len(), || new_paired(black_box(&fixture)));
    print_pair("paired EX and Hard EX reconstruction", &old, &new);
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC read the x86 timestamp counter without memory access.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC read the x86-64 timestamp counter without memory access.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
