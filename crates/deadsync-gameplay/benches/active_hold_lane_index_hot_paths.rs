use deadsync_core::input::{LaneMask, MAX_COLS};
use deadsync_core::note::NoteType;
use deadsync_gameplay::{
    ActiveHold, collect_active_autoplay_roll_columns_indexed,
    collect_active_autoplay_roll_columns_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const SAMPLES: usize = 100;

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
    ns_per_frame: f64,
    p95_ns: f64,
    cycles_per_frame: Option<f64>,
    frames_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_sample(batch: usize, frame: &mut impl FnMut() -> u64) -> (f64, Option<u64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..batch {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed_ns = started.elapsed().as_secs_f64() * 1e9;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start));
    (elapsed_ns, cycles, checksum)
}

fn measure_pair(
    iterations: usize,
    mut old_frame: impl FnMut() -> u64,
    mut new_frame: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..(iterations / 20).max(2) {
        black_box(old_frame());
        black_box(new_frame());
    }
    let batch = (iterations / SAMPLES).max(2);
    let batches = iterations.div_ceil(batch);
    let measured_iterations = batches * batch;
    let mut old_samples = Vec::with_capacity(batches);
    let mut new_samples = Vec::with_capacity(batches);
    let mut old_elapsed_ns = 0.0;
    let mut new_elapsed_ns = 0.0;
    let mut old_cycles = Some(0_u64);
    let mut new_cycles = Some(0_u64);
    let mut old_checksum = 0_u64;
    let mut new_checksum = 0_u64;
    for sample_index in 0..batches {
        let (old_sample, new_sample) = if sample_index.is_multiple_of(2) {
            (
                measure_sample(batch, &mut old_frame),
                measure_sample(batch, &mut new_frame),
            )
        } else {
            let new_sample = measure_sample(batch, &mut new_frame);
            let old_sample = measure_sample(batch, &mut old_frame);
            (old_sample, new_sample)
        };
        old_elapsed_ns += old_sample.0;
        new_elapsed_ns += new_sample.0;
        old_cycles = old_cycles
            .zip(old_sample.1)
            .map(|(total, sample)| total.wrapping_add(sample));
        new_cycles = new_cycles
            .zip(new_sample.1)
            .map(|(total, sample)| total.wrapping_add(sample));
        old_checksum = old_checksum.wrapping_add(old_sample.2);
        new_checksum = new_checksum.wrapping_add(new_sample.2);
        old_samples.push(old_sample.0 / batch as f64);
        new_samples.push(new_sample.0 / batch as f64);
    }
    old_samples.sort_unstable_by(f64::total_cmp);
    new_samples.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut old_allocation_checksum = 0_u64;
    for _ in 0..iterations {
        old_allocation_checksum = old_allocation_checksum.wrapping_add(black_box(old_frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let old_allocated = ALLOC.snapshot().delta(before);
    black_box(old_allocation_checksum);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut new_allocation_checksum = 0_u64;
    for _ in 0..iterations {
        new_allocation_checksum = new_allocation_checksum.wrapping_add(black_box(new_frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let new_allocated = ALLOC.snapshot().delta(before);
    black_box(new_allocation_checksum);

    let result =
        |elapsed_ns: f64, samples: &[f64], cycles: Option<u64>, allocated, checksum| BenchResult {
            ns_per_frame: elapsed_ns / measured_iterations as f64,
            p95_ns: samples[samples.len() * 95 / 100],
            cycles_per_frame: cycles.map(|cycles| cycles as f64 / measured_iterations as f64),
            frames_per_second: measured_iterations as f64 / (elapsed_ns / 1e9),
            allocated,
            checksum,
        };
    (
        result(
            old_elapsed_ns,
            &old_samples,
            old_cycles,
            old_allocated,
            old_checksum,
        ),
        result(
            new_elapsed_ns,
            &new_samples,
            new_cycles,
            new_allocated,
            new_checksum,
        ),
    )
}

struct RollState {
    active_holds: [Option<ActiveHold>; MAX_COLS],
    active_mask: LaneMask,
    columns: [usize; MAX_COLS],
}

impl RollState {
    fn new(active_mask: LaneMask) -> Self {
        Self {
            active_holds: std::array::from_fn(|column| {
                (active_mask & (1 << column) != 0).then_some(ActiveHold {
                    note_index: column,
                    start_time_ns: 0,
                    end_time_ns: i64::MAX / 4,
                    note_type: NoteType::Roll,
                    let_go: false,
                    is_pressed: false,
                    life: 1.0,
                    last_update_time_ns: 0,
                })
            }),
            active_mask,
            columns: [usize::MAX; MAX_COLS],
        }
    }

    fn probe(&self, count: usize) -> u64 {
        let first = self.columns.first().copied().unwrap_or(usize::MAX) as u64;
        let last = count
            .checked_sub(1)
            .map_or(usize::MAX, |index| self.columns[index]) as u64;
        count as u64 ^ first.rotate_left(21) ^ last.rotate_left(43)
    }
}

#[inline(never)]
fn collect_rolls_reference(state: &mut RollState) -> u64 {
    let count = collect_active_autoplay_roll_columns_reference(
        &state.active_holds,
        MAX_COLS,
        &mut state.columns,
    );
    state.probe(count)
}

#[inline(never)]
fn collect_rolls_masked(state: &mut RollState) -> u64 {
    let count = collect_active_autoplay_roll_columns_indexed(
        state.active_mask,
        MAX_COLS,
        &mut state.columns,
    );
    state.probe(count)
}

fn run_rolls(title: &str, iterations: usize, active_mask: LaneMask) {
    let mut old_state = RollState::new(active_mask);
    let mut new_state = RollState::new(active_mask);
    let (old, new) = measure_pair(
        iterations,
        || collect_rolls_reference(&mut old_state),
        || collect_rolls_masked(&mut new_state),
    );

    assert_eq!(old.checksum, new.checksum, "{title} diverged");
    assert_eq!(
        old_state.columns, new_state.columns,
        "{title} state diverged"
    );
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);
    print_comparison(title, &old, &new);
}

fn print_comparison(title: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{title}");
    println!("  roll-index storage: old=0 B, new=2 B inline; no heap storage");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_frame, new.ns_per_frame),
        percent_change(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_change(old.frames_per_second, new.frames_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>10.2} p95 ns  \
         {:>8.2} Mframe/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>8} churn B",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.p95_ns,
        result.frames_per_second / 1_000_000.0,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.churn_bytes(),
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.frees, 0);
    assert_eq!(result.allocated.churn_bytes(), 0);
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

fn main() {
    run_rolls("autoplay without active holds", 20_000_000, 0);
    run_rolls("autoplay with one active roll", 15_000_000, 1 << 7);
    run_rolls(
        "autoplay with four active rolls",
        15_000_000,
        (1 << 0) | (1 << 3) | (1 << 6) | (1 << 9),
    );
    run_rolls(
        "autoplay with ten active rolls",
        10_000_000,
        (1 << MAX_COLS) - 1,
    );
}
