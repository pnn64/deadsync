use deadlib_render::SpriteInstanceRaw;
use deadlib_render_backend_software::SpriteProjectionBenchScratch;
use glam::Mat4 as Matrix4;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WIDTH: usize = 1_920;
const HEIGHT: usize = 1_080;
const STRIPES: usize = HEIGHT.div_ceil(32);
const SPRITES: usize = 512;
const WARMUP_FRAMES: usize = 16;
const MEASURE_FRAMES: usize = 250;
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

// SAFETY: every allocation operation delegates to `System` with the original
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
}

fn main() {
    let sprites = gameplay_sprites();
    let projection =
        glam::camera::rh::proj::opengl::orthographic(-427.0, 427.0, -240.0, 240.0, -1.0, 1.0);
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);

    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_current(&sprites, projection);
            let old = measure_legacy(&sprites, projection);
            (old, new)
        } else {
            let old = measure_legacy(&sprites, projection);
            let new = measure_current(&sprites, projection);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
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
        "software sprite projection ({SPRITES} gameplay sprites, {STRIPES} x 32-row stripes, \
         median of {BENCH_RUNS})"
    );
    print_result("project/stripe", &legacy);
    print_result("prepare/frame", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | projection evaluations/frame {} -> {}",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        SPRITES * STRIPES,
        SPRITES,
    );
}

fn measure_legacy(sprites: &[SpriteInstanceRaw], projection: Matrix4) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(
            deadlib_render_backend_software::__benchmark_project_sprites_per_stripe(
                black_box(sprites),
                projection,
                WIDTH,
                HEIGHT,
                STRIPES,
            ),
        );
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(9)
            ^ deadlib_render_backend_software::__benchmark_project_sprites_per_stripe(
                black_box(sprites),
                projection,
                WIDTH,
                HEIGHT,
                STRIPES,
            );
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
    }
}

fn measure_current(sprites: &[SpriteInstanceRaw], projection: Matrix4) -> BenchResult {
    let mut scratch = SpriteProjectionBenchScratch::default();
    for _ in 0..WARMUP_FRAMES {
        black_box(
            deadlib_render_backend_software::__benchmark_prepare_sprite_projections(
                &mut scratch,
                black_box(sprites),
                projection,
                WIDTH,
                HEIGHT,
                STRIPES,
            ),
        );
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(9)
            ^ deadlib_render_backend_software::__benchmark_prepare_sprite_projections(
                &mut scratch,
                black_box(sprites),
                projection,
                WIDTH,
                HEIGHT,
                STRIPES,
            );
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
    }
}

fn gameplay_sprites() -> Vec<SpriteInstanceRaw> {
    (0..SPRITES)
        .map(|index| {
            let angle = index as f32 * 0.017;
            let offset_angle = index as f32 * -0.011;
            SpriteInstanceRaw {
                center: [
                    (index % 32) as f32 * 25.0 - 387.5,
                    (index / 32) as f32 * 28.0 - 210.0,
                    (index % 11) as f32 * 0.01,
                    1.0,
                ],
                size: [20.0 + (index % 7) as f32, 42.0 + (index % 13) as f32],
                rot_sin_cos: [angle.sin(), angle.cos()],
                tint: [0.8, 0.9, 1.0, 1.0],
                uv_scale: [0.5 + (index % 3) as f32 * 0.25, 1.0],
                uv_offset: [(index % 4) as f32 * 0.125, (index % 5) as f32 * 0.1],
                local_offset: [(index % 7) as f32 - 3.0, (index % 9) as f32 * 0.5 - 2.0],
                local_offset_rot_sin_cos: [offset_angle.sin(), offset_angle.cos()],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            }
        })
        .collect()
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let visits = (SPRITES * STRIPES * MEASURE_FRAMES) as f64;
    println!(
        "  {label:<14} {:>8.2} us/frame  {:>10.0} cycles/frame  \
         {:>7.1} M sprite-stripes/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        visits / result.elapsed.as_secs_f64() / 1_000_000.0,
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
