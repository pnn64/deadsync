use deadsync_gameplay::{
    AccelOverrides, AppearanceOverrides, AttackMaskWindow, AttackWindowIndexBench,
    ChartAttackEffects, MiniAttackMode, PerspectiveOverrides, ReferenceAttackWindowIndexBench,
    ScrollOverrides, VisibilityOverrides, VisualOverrides,
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

struct FrameTimes {
    values: Vec<f32>,
    cursor: usize,
}

impl FrameTimes {
    fn new(song_seconds: f32, fps: usize) -> Self {
        let frames = (song_seconds * fps as f32).ceil() as usize;
        Self {
            values: (0..frames).map(|frame| frame as f32 / fps as f32).collect(),
            cursor: 0,
        }
    }

    #[inline(always)]
    fn next(&mut self) -> f32 {
        let now = self.values[self.cursor];
        self.cursor += 1;
        if self.cursor == self.values.len() {
            self.cursor = 0;
        }
        now
    }
}

fn window(start_second: f32, duration: f32, index: usize) -> AttackMaskWindow {
    AttackMaskWindow {
        start_second,
        end_second: start_second + duration,
        sustain_end_second: start_second + duration,
        persist_after_end: false,
        clear_all: false,
        chart: ChartAttackEffects::default(),
        accel: AccelOverrides::default(),
        visual: VisualOverrides {
            drunk: Some((index % 100) as f32 / 100.0),
            ..VisualOverrides::default()
        },
        visual_speed: VisualOverrides::default(),
        appearance: AppearanceOverrides::default(),
        appearance_speed: AppearanceOverrides::default(),
        visibility: VisibilityOverrides::default(),
        scroll: ScrollOverrides::default(),
        scroll_approach_speed: ScrollOverrides::default(),
        perspective: PerspectiveOverrides::default(),
        scroll_speed: None,
        mini_percent: None,
        mini_mode: MiniAttackMode::Absolute,
        mini_speed: None,
    }
}

fn spaced_windows(count: usize, spacing: f32, duration: f32) -> Vec<AttackMaskWindow> {
    (0..count)
        .map(|index| window(index as f32 * spacing, duration, index))
        .collect()
}

fn burst_windows(
    bursts: usize,
    per_burst: usize,
    spacing: f32,
    duration: f32,
) -> Vec<AttackMaskWindow> {
    (0..bursts * per_burst)
        .map(|index| window((index / per_burst) as f32 * spacing, duration, index))
        .collect()
}

fn active_checksum(active: &[usize]) -> u64 {
    active.len() as u64
        | (active.first().copied().unwrap_or_default() as u64) << 16
        | (active.last().copied().unwrap_or_default() as u64) << 40
}

struct BenchResult {
    ns_per_frame: f64,
    p95_ns: f64,
    cycles_per_frame: Option<f64>,
    frames_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(iterations / 20).max(2) {
        black_box(frame());
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
            checksum = checksum.wrapping_add(black_box(frame()));
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
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_frame: seconds * 1e9 / measured_iterations as f64,
        p95_ns: sample_ns[sample_ns.len() * 95 / 100],
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_iterations as f64),
        frames_per_second: measured_iterations as f64 / seconds,
        allocated,
        checksum,
    }
}

fn run(title: &str, iterations: usize, song_seconds: f32, windows: Vec<AttackMaskWindow>) {
    let mut old = ReferenceAttackWindowIndexBench::new(windows.clone());
    let mut old_times = FrameTimes::new(song_seconds, 120);
    let old_result = measure(iterations, || {
        old.update(old_times.next());
        active_checksum(old.active())
    });
    let mut new = AttackWindowIndexBench::new(windows);
    let mut new_times = FrameTimes::new(song_seconds, 120);
    let new_result = measure(iterations, || {
        new.update(new_times.next());
        active_checksum(new.active())
    });

    assert_eq!(
        old_result.checksum, new_result.checksum,
        "{title} behavior diverged"
    );
    assert_zero_alloc(&old_result);
    assert_zero_alloc(&new_result);

    println!("\n{title}");
    print_result("old", iterations, &old_result);
    print_result("new", iterations, &new_result);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old_result.ns_per_frame, new_result.ns_per_frame),
        percent_change(
            old_result.cycles_per_frame.unwrap_or(f64::NAN),
            new_result.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_change(old_result.frames_per_second, new_result.frames_per_second,),
        percent_change(old_result.p95_ns, new_result.p95_ns),
        percent_change(
            old_result.allocated.churn_bytes() as f64,
            new_result.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>10.2} p95 ns  \
         {:>8.2} Mframe/s  {:>5.2} alloc/frame  {:>5.2} realloc/frame  \
         {:>5.2} free/frame  {:>10.1} churn B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.p95_ns,
        result.frames_per_second / 1_000_000.0,
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
    let old_bytes = ReferenceAttackWindowIndexBench::index_storage_bytes();
    let new_bytes = AttackWindowIndexBench::index_storage_bytes();
    println!(
        "active-window index storage: old={old_bytes} B, new={new_bytes} B ({:+.2}%)",
        percent_change(old_bytes as f64, new_bytes as f64),
    );

    let sparse = spaced_windows(512, 1.0, 0.2);
    run("sparse short windows", 2_000_000, 513.0, sparse);

    let overlapping = spaced_windows(512, 0.25, 16.0);
    run(
        "sixty-four overlapping windows",
        1_000_000,
        145.0,
        overlapping,
    );

    let burst = burst_windows(64, 32, 1.0, 8.0);
    run("ordered thirty-two-window bursts", 1_000_000, 72.0, burst);
}
