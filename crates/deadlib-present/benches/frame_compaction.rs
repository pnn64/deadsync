use deadlib_present::compose::{RetainedAppendBenchmark, SpriteGatherBenchmark};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RETAINED_DRAWS: usize = 256;
const SPRITES: usize = 1_024;
const SPRITE_LAYERS: usize = 8;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 50_000;
const BENCH_RUNS: usize = 7;

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

// SAFETY: every operation delegates to `System` with the caller's original
// pointer and layout; atomics only observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the live pointer and its original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
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
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
}

fn main() {
    benchmark_retained_append();
    benchmark_sprite_analysis();
    benchmark_sprite_gather();
}

fn benchmark_retained_append() {
    let legacy = median_retained(RetainedAppendBenchmark::legacy_frame);
    let bulk = median_retained(RetainedAppendBenchmark::bulk_frame);
    assert_eq!(legacy.checksum, bulk.checksum);
    assert_zero_alloc(&legacy);
    assert_zero_alloc(&bulk);

    println!("retained typed-array append ({RETAINED_DRAWS} mixed draws)");
    print_result("per-object", &legacy, RETAINED_DRAWS);
    print_result("typed splice", &bulk, RETAINED_DRAWS);
    print_ratio(&legacy, &bulk);
}

fn median_retained(plan: fn(&mut RetainedAppendBenchmark) -> u64) -> BenchResult {
    median((0..BENCH_RUNS).map(|_| {
        let mut bench = RetainedAppendBenchmark::new(RETAINED_DRAWS);
        measure(|| plan(&mut bench), 0)
    }))
}

fn benchmark_sprite_analysis() {
    let scanned = median_gather(SpriteGatherBenchmark::scanned_analysis_frame);
    let inline = median_gather(SpriteGatherBenchmark::inline_analysis_frame);
    assert_eq!(scanned.checksum, inline.checksum);
    assert_eq!(scanned.ops, inline.ops);
    assert_zero_alloc(&scanned);
    assert_zero_alloc(&inline);

    println!(
        "\nfallback sprite analysis ({SPRITES} sprites across {SPRITE_LAYERS} interleaved layers)"
    );
    print_result("second pass", &scanned, SPRITES);
    print_result("inline", &inline, SPRITES);
    print_ratio(&scanned, &inline);
}

fn benchmark_sprite_gather() {
    let legacy = median_gather(SpriteGatherBenchmark::legacy_frame);
    let gathered = median_gather(SpriteGatherBenchmark::gathered_frame);
    assert_eq!(legacy.checksum, gathered.checksum);
    assert_zero_alloc(&legacy);
    assert_zero_alloc(&gathered);

    println!(
        "\nfallback sprite gather ({SPRITES} sprites across {SPRITE_LAYERS} interleaved layers)"
    );
    print_result("fragmented", &legacy, SPRITES);
    print_result("gathered", &gathered, SPRITES);
    println!("  draw operations/frame {} -> {}", legacy.ops, gathered.ops);
    print_ratio(&legacy, &gathered);
}

fn median_gather(plan: fn(&mut SpriteGatherBenchmark) -> u64) -> BenchResult {
    median((0..BENCH_RUNS).map(|_| {
        let mut bench = SpriteGatherBenchmark::new(SPRITES, SPRITE_LAYERS);
        for _ in 0..WARMUP_FRAMES {
            black_box(plan(&mut bench));
        }
        let before = ALLOC.snapshot();
        let cycles_before = read_cycles();
        let started = Instant::now();
        let mut checksum = 0u64;
        for _ in 0..MEASURE_FRAMES {
            checksum = checksum.rotate_left(7) ^ black_box(plan(&mut bench));
        }
        BenchResult {
            elapsed: started.elapsed(),
            cycles: read_cycles().saturating_sub(cycles_before),
            alloc: ALLOC.snapshot().delta(before),
            checksum,
            ops: bench.op_count(),
        }
    }))
}

fn measure(mut frame: impl FnMut() -> u64, ops: usize) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(7) ^ black_box(frame());
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        ops,
    }
}

fn median(results: impl IntoIterator<Item = BenchResult>) -> BenchResult {
    let mut results = results.into_iter().collect::<Vec<_>>();
    results.sort_unstable_by_key(|result| result.elapsed);
    results.swap_remove(results.len() / 2)
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.alloc.allocs, 0);
    assert_eq!(result.alloc.reallocs, 0);
    assert_eq!(result.alloc.bytes, 0);
}

fn print_result(label: &str, result: &BenchResult, items: usize) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<12} {:>9.2} us/frame  {:>9.0} cycles/frame  {:>7.1} M items/s  \
         {:.2} allocs/frame  {:.2} reallocs/frame  {:.1} bytes/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * items as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.alloc.allocs as f64 / frames,
        result.alloc.reallocs as f64 / frames,
        result.alloc.bytes as f64 / frames,
    );
}

fn print_ratio(legacy: &BenchResult, current: &BenchResult) {
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter without
    // dereferencing memory.
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
