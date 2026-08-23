use deadsync_core::input::{InputSource, LaneMask, MAX_COLS};
use deadsync_gameplay::{GameplayInputState, LaneInputUpdate, ReferenceGameplayInputState};
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

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct Edge {
    lane: usize,
    source: InputSource,
    slot: u32,
    pressed: bool,
}

struct ToggleWorkload {
    bindings: usize,
    cursor: usize,
    press_next: bool,
}

impl ToggleWorkload {
    const fn new(bindings: usize) -> Self {
        Self {
            bindings,
            cursor: 0,
            press_next: false,
        }
    }

    fn next(&mut self) -> Edge {
        let index = self.cursor;
        let edge = binding_edge(index, self.press_next);
        if self.press_next {
            self.cursor = (self.cursor + 1) % self.bindings;
        }
        self.press_next = !self.press_next;
        edge
    }
}

const fn binding_edge(index: usize, pressed: bool) -> Edge {
    Edge {
        lane: index % MAX_COLS,
        source: if index.is_multiple_of(2) {
            InputSource::Keyboard
        } else {
            InputSource::Gamepad
        },
        slot: (index / 2) as u32,
        pressed,
    }
}

struct ReferenceState {
    state: ReferenceGameplayInputState,
}

impl ReferenceState {
    fn new(bindings: usize) -> Self {
        let mut state = Self {
            state: ReferenceGameplayInputState::default(),
        };
        for index in 0..bindings {
            state.update(binding_edge(index, true));
        }
        state
    }

    fn update(&mut self, edge: Edge) -> u64 {
        let (slot_was_down, update) =
            self.state
                .benchmark_input_edge(edge.lane, edge.source, edge.slot, edge.pressed);
        update_checksum(
            slot_was_down,
            update,
            self.state.lane_counts()[edge.lane],
            self.state.pressed_lane_mask(),
        )
    }
}

struct PackedState {
    state: GameplayInputState,
}

impl PackedState {
    fn new(bindings: usize) -> Self {
        let mut state = GameplayInputState::default();
        for index in 0..bindings {
            let edge = binding_edge(index, true);
            state.benchmark_input_edge(edge.lane, edge.source, edge.slot, edge.pressed);
        }
        Self { state }
    }

    fn update(&mut self, edge: Edge) -> u64 {
        let (slot_was_down, update) =
            self.state
                .benchmark_input_edge(edge.lane, edge.source, edge.slot, edge.pressed);
        update_checksum(
            slot_was_down,
            update,
            self.state.lane_counts()[edge.lane],
            self.state.pressed_lane_mask(),
        )
    }
}

const fn update_checksum(
    slot_was_down: bool,
    update: LaneInputUpdate,
    lane_count: u16,
    pressed_lane_mask: LaneMask,
) -> u64 {
    slot_was_down as u64
        | (update.was_down as u64) << 1
        | (update.is_down as u64) << 2
        | (update.slot_was_down as u64) << 3
        | (update.slot_table_full as u64) << 4
        | (lane_count as u64) << 8
        | (pressed_lane_mask as u64) << 24
}

struct BenchResult {
    ns_per_edge: f64,
    p95_ns: f64,
    cycles_per_edge: Option<f64>,
    edges_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut edge: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(2) {
        black_box(edge());
    }

    let batch = (iterations / SAMPLES).max(2);
    let batches = iterations.div_ceil(batch);
    let measured_iterations = batches * batch;
    let mut sample_ns = Vec::with_capacity(batches);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..batches {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(edge()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0_u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(edge()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_edge: seconds * 1e9 / measured_iterations as f64,
        p95_ns: sample_ns[sample_ns.len() * 95 / 100],
        cycles_per_edge: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_iterations as f64),
        edges_per_second: measured_iterations as f64 / seconds,
        allocated,
        checksum,
    }
}

fn run(title: &str, iterations: usize, bindings: usize) {
    let mut old_state = ReferenceState::new(bindings);
    let mut old_workload = ToggleWorkload::new(bindings);
    let mut new_state = PackedState::new(bindings);
    let mut new_workload = ToggleWorkload::new(bindings);
    let old = measure(iterations, || old_state.update(old_workload.next()));
    let new = measure(iterations, || new_state.update(new_workload.next()));

    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\n{title}");
    print_result("edge", "old", iterations, &old);
    print_result("edge", "new", iterations, &new);
    print_change(&old, &new);
}

fn run_reset(iterations: usize) {
    let mut old_state = ReferenceGameplayInputState::default();
    let mut new_state = GameplayInputState::default();
    for index in 0..MAX_COLS {
        let edge = binding_edge(index, true);
        old_state.benchmark_input_edge(edge.lane, edge.source, edge.slot, edge.pressed);
        new_state.benchmark_input_edge(edge.lane, edge.source, edge.slot, edge.pressed);
    }
    let old = measure(iterations, || {
        old_state.benchmark_reset_live_state();
        black_box(&old_state);
        old_state.pressed_lane_mask() as u64
    });
    let new = measure(iterations, || {
        new_state.benchmark_reset_live_state();
        black_box(&new_state);
        new_state.pressed_lane_mask() as u64
    });
    assert_eq!(old.checksum, new.checksum, "live-state reset diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nlive input state reset");
    print_result("reset", "old", iterations, &old);
    print_result("reset", "new", iterations, &new);
    print_change(&old, &new);
}

fn print_change(old: &BenchResult, new: &BenchResult) {
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_edge, new.ns_per_edge),
        percent_change(
            old.cycles_per_edge.unwrap_or(f64::NAN),
            new.cycles_per_edge.unwrap_or(f64::NAN),
        ),
        percent_change(old.edges_per_second, new.edges_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(unit: &str, label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/{unit}  {:>10.2} cycles/{unit}  {:>10.2} p95 ns  \
         {:>8.2} M{unit}/s  {:>5.2} alloc/{unit}  {:>5.2} realloc/{unit}  \
         {:>5.2} free/{unit}  {:>10.1} churn B/{unit}",
        result.ns_per_edge,
        result.cycles_per_edge.unwrap_or(f64::NAN),
        result.p95_ns,
        result.edges_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
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
    let old_bytes = ReferenceGameplayInputState::active_slot_storage_bytes();
    let new_bytes = GameplayInputState::active_slot_storage_bytes();
    println!(
        "active-slot storage: old={old_bytes} B, new={new_bytes} B ({:+.2}%)",
        percent_change(old_bytes as f64, new_bytes as f64),
    );

    run("one active binding", 20_000_000, 1);
    run("four active bindings", 20_000_000, 4);
    run("ten active bindings", 10_000_000, 10);
    run("sixty-four active bindings", 4_000_000, 64);
    run_reset(5_000_000);
}
