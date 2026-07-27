use deadlib_render::draw_prep::TexturedMeshSource;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const TRANSIENT_GEOMETRIES: usize = 96;
const TRANSIENT_PASSES: usize = 2;
const CACHED_GEOMETRIES: usize = 48;
const WARMUP_FRAMES: usize = 1_000;
const MEASURE_FRAMES: usize = 100_000;
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

// SAFETY: all allocation operations delegate to `System` with the original
// pointer and layout; the counters only observe successful operations.
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
        // SAFETY: the caller guarantees this is the layout used to allocate `ptr`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `ptr` and `old` identify a live allocation from `System`.
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
    binds: usize,
}

fn main() {
    let sources = gameplay_sources();
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);

    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure(&sources, plan_current);
            let old = measure(&sources, plan_legacy);
            (old, new)
        } else {
            let old = measure(&sources, plan_legacy);
            let new = measure(&sources, plan_current);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.binds, 144);
        assert_eq!(new.binds, 49);
        for result in [&old, &new] {
            assert_eq!(result.alloc.allocs, 0);
            assert_eq!(result.alloc.reallocs, 0);
            assert_eq!(result.alloc.bytes, 0);
        }
        legacy.push(old);
        current.push(new);
    }

    legacy.sort_unstable_by_key(|result| result.elapsed);
    current.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let current = current.swap_remove(BENCH_RUNS / 2);

    println!(
        "OpenGL/Vulkan/WGPU textured-mesh binding plan ({} runs, median of {BENCH_RUNS})",
        sources.len()
    );
    print_result("draw-range key", &legacy);
    print_result("buffer key", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | GPU buffer binds/frame {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.binds,
        current.binds,
        100.0 * (1.0 - current.binds as f64 / legacy.binds as f64),
    );
}

fn measure(
    sources: &[TexturedMeshSource],
    plan: fn(&[TexturedMeshSource]) -> (usize, u64),
) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(plan(black_box(sources)));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut binds = 0;
    for _ in 0..MEASURE_FRAMES {
        let (frame_binds, frame_checksum) = plan(black_box(sources));
        binds = frame_binds;
        checksum = checksum.rotate_left(9) ^ frame_checksum;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        binds,
    }
}

#[inline(never)]
fn observe_bind(source: TexturedMeshSource, binds: &mut usize) {
    *binds += 1;
    black_box(source);
}

fn plan_legacy(sources: &[TexturedMeshSource]) -> (usize, u64) {
    let mut last = None;
    let mut binds = 0;
    let mut checksum = 0_u64;
    for &source in sources {
        if last != Some(source) {
            observe_bind(source, &mut binds);
            last = Some(source);
        }
        checksum = observe_draw(checksum, source);
    }
    (binds, black_box(checksum))
}

fn plan_current(sources: &[TexturedMeshSource]) -> (usize, u64) {
    let mut last: Option<TexturedMeshSource> = None;
    let mut binds = 0;
    let mut checksum = 0_u64;
    for &source in sources {
        if last.is_none_or(|bound| !bound.shares_vertex_buffer(source)) {
            observe_bind(source, &mut binds);
            last = Some(source);
        }
        checksum = observe_draw(checksum, source);
    }
    (binds, black_box(checksum))
}

#[inline(always)]
fn observe_draw(checksum: u64, source: TexturedMeshSource) -> u64 {
    checksum.rotate_left(7)
        ^ u64::from(source.vertex_start())
        ^ (u64::from(source.vertex_count()) << 32)
}

fn gameplay_sources() -> Vec<TexturedMeshSource> {
    let mut sources =
        Vec::with_capacity(TRANSIENT_GEOMETRIES * TRANSIENT_PASSES + CACHED_GEOMETRIES);
    for geometry in 0..TRANSIENT_GEOMETRIES {
        let vertex_start = geometry as u32 * 48;
        let source = TexturedMeshSource::Transient {
            vertex_start,
            vertex_count: 48,
            geom_key: (u64::from(vertex_start) << 32) | 48,
        };
        sources.extend(std::iter::repeat_n(source, TRANSIENT_PASSES));
    }
    for geometry in 0..CACHED_GEOMETRIES {
        sources.push(TexturedMeshSource::Cached {
            cache_key: 1_000 + geometry as u64,
            vertex_count: 72,
        });
    }
    sources
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<14} {:>7.2} ns/frame  {:>8.1} cycles/frame  \
         {:>7.1} M runs/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * (TRANSIENT_GEOMETRIES * TRANSIENT_PASSES + CACHED_GEOMETRIES) as f64
            / result.elapsed.as_secs_f64()
            / 1_000_000.0,
        result.alloc.allocs as f64,
        result.alloc.reallocs as f64,
        result.alloc.bytes as f64,
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
