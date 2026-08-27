use deadsync_theme_simply_love::screens::select_music::{
    LobbyReconcileBench, benchmark_lobby_reconcile_fixture,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 10_000;
const TIMED_FRAMES: usize = 1_000_000;
const ALLOC_FRAMES: usize = 100_000;
const SAMPLE_COUNT: usize = 2_000;
const FRAMES_PER_SAMPLE: usize = 100;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: requests are delegated unchanged to `System`; relaxed counters only
// observe this single-threaded benchmark while its measurement gate is active.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
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
            self.freed_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }
}

struct BenchResult {
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    p50_ns: f64,
    p95_ns: f64,
    p99_ns: f64,
    worst_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

type FrameFn = fn(&mut LobbyReconcileBench) -> u64;

fn legacy_frame(state: &mut LobbyReconcileBench) -> u64 {
    state.legacy_frame()
}

fn retained_frame(state: &mut LobbyReconcileBench) -> u64 {
    state.retained_frame()
}

fn measure(frame: FrameFn) -> BenchResult {
    let mut state = benchmark_lobby_reconcile_fixture();
    for _ in 0..WARMUP_FRAMES {
        black_box(frame(&mut state));
    }

    let cycle_start = thread_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..TIMED_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame(&mut state)));
    }
    let elapsed = started.elapsed();
    let cycle_end = thread_cycles();

    let mut samples = Vec::with_capacity(SAMPLE_COUNT);
    for _ in 0..SAMPLE_COUNT {
        let started = Instant::now();
        for _ in 0..FRAMES_PER_SAMPLE {
            checksum = checksum.wrapping_add(black_box(frame(&mut state)));
        }
        samples.push(started.elapsed().as_secs_f64() * 1e9 / FRAMES_PER_SAMPLE as f64);
    }
    samples.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..ALLOC_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame(&mut state)));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1e9 / TIMED_FRAMES as f64,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / TIMED_FRAMES as f64),
        p50_ns: percentile(&samples, 50),
        p95_ns: percentile(&samples, 95),
        p99_ns: percentile(&samples, 99),
        worst_ns: samples.last().copied().unwrap_or_default(),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn percentile(samples: &[f64], percentile: usize) -> f64 {
    let index = (samples.len().saturating_sub(1) * percentile) / 100;
    samples.get(index).copied().unwrap_or_default()
}

fn main() {
    deadsync_theme_simply_love::i18n::init_for_tests();
    let legacy = measure(legacy_frame);
    let retained = measure(retained_frame);
    assert_eq!(
        legacy.checksum, retained.checksum,
        "reconciliation diverged"
    );
    assert_eq!(
        retained.allocated.allocs, 0,
        "stable retained path allocated"
    );
    assert_eq!(
        retained.allocated.reallocs, 0,
        "stable retained path reallocated"
    );
    assert_eq!(retained.allocated.frees, 0, "stable retained path freed");
    assert_eq!(retained.allocated.allocated_bytes, 0);
    assert_eq!(retained.allocated.freed_bytes, 0);

    println!("stable joined-lobby Select Music reconciliation");
    print_result("legacy DTO/signatures", &legacy);
    print_result("retained identity", &retained);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% p99  {:+.2}% bytes",
        change(legacy.ns_per_frame, retained.ns_per_frame),
        change(
            legacy.cycles_per_frame.unwrap_or(f64::NAN),
            retained.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        change(legacy.p99_ns, retained.p99_ns),
        change(
            legacy.allocated.allocated_bytes as f64,
            retained.allocated.allocated_bytes as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = ALLOC_FRAMES as f64;
    println!(
        "  {label:<22} {:>9.2} ns  {:>9.2} cyc  p50 {:>8.2}  p95 {:>8.2}  \
         p99 {:>8.2}  max {:>8.2}  {:>7.3} Mframe/s  {:>5.2} alloc  \
         {:>7.1} B alloc  {:>7.1} B free/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.p50_ns,
        result.p95_ns,
        result.p99_ns,
        result.worst_ns,
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs as f64 / frames,
        result.allocated.allocated_bytes as f64 / frames,
        result.allocated.freed_bytes as f64 / frames,
    );
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        0.0
    } else {
        (new / old - 1.0) * 100.0
    }
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0u64;
    // SAFETY: the pseudo-handle is valid for the calling thread and `cycles`
    // points to writable storage for the duration of this call.
    let succeeded = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (succeeded != 0).then_some(cycles)
}

#[cfg(all(not(windows), target_arch = "x86_64"))]
fn thread_cycles() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        Some(core::arch::x86_64::_rdtsc())
    }
}

#[cfg(all(not(windows), target_arch = "x86"))]
fn thread_cycles() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        core::arch::x86::_mm_lfence();
        Some(core::arch::x86::_rdtsc())
    }
}

#[cfg(not(any(windows, target_arch = "x86", target_arch = "x86_64")))]
fn thread_cycles() -> Option<u64> {
    None
}
