use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch};
use deadlib_present::font::FontMap;
use deadlib_present::space::Metrics;
use deadlib_render::{BlendMode, TexturedMeshVertex};
use glam::Mat4;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MESHES: usize = 256;
const GLOW_MESHES: usize = MESHES / 4;
const WARMUP_FRAMES: usize = 1_024;
const MEASURE_FRAMES: usize = 20_000;
const BENCH_RUNS: usize = 5;

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

// SAFETY: all operations delegate to `System`; the atomics only observe
// successful allocations and do not alter ownership or layout.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplied the original live allocation and layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
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
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let shared_vertices: Arc<[TexturedMeshVertex]> =
        Arc::from(vec![TexturedMeshVertex::default(); 6]);
    let reusable_vertices = Arc::new(vec![TexturedMeshVertex::default(); 6]);
    let actors = benchmark_actors(&shared_vertices, &reusable_vertices);
    let shared_owners = Arc::strong_count(&shared_vertices);
    let reusable_owners = Arc::strong_count(&reusable_vertices);
    let mut runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        runs.push(run_case(
            &actors,
            &shared_vertices,
            &reusable_vertices,
            shared_owners,
            reusable_owners,
        ));
    }
    let checksum = runs[0].checksum;
    assert!(runs.iter().all(|run| run.checksum == checksum));
    runs.sort_unstable_by_key(|run| run.elapsed);
    let median = &runs[BENCH_RUNS / 2];

    println!("textured-mesh actor composition benchmark");
    println!("{MESHES} actors/frame ({GLOW_MESHES} with glow), median of {BENCH_RUNS} runs");
    let frames = MEASURE_FRAMES as f64;
    let elapsed_ns = median.elapsed.as_secs_f64() * 1_000_000_000.0 / frames;
    let cycles = median
        .cycles
        .map(|cycles| format!("{:.0}", cycles as f64 / frames))
        .unwrap_or_else(|| String::from("n/a"));
    println!(
        "compose  {:>9.1} ns/frame  {:>7.2} ns/actor  {:>8} cycles/frame  \
         {:.2} allocs/frame  {:.1} bytes/frame  {:.2} reallocs/frame",
        elapsed_ns,
        elapsed_ns / MESHES as f64,
        cycles,
        median.alloc.allocs as f64 / frames,
        median.alloc.bytes as f64 / frames,
        median.alloc.reallocs as f64 / frames,
    );
    black_box(checksum);
}

fn run_case(
    actors: &[Actor],
    shared_vertices: &Arc<[TexturedMeshVertex]>,
    reusable_vertices: &Arc<Vec<TexturedMeshVertex>>,
    shared_owners: usize,
    reusable_owners: usize,
) -> BenchResult {
    let fonts = FontMap::default();
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };
    let mut cache = TextLayoutCache::new(1);
    let mut scratch = ComposeScratch::default();
    for frame in 0..WARMUP_FRAMES {
        black_box(compose_frame(
            frame,
            actors,
            &metrics,
            &fonts,
            &mut cache,
            &mut scratch,
        ));
    }
    assert_eq!(Arc::strong_count(shared_vertices), shared_owners);
    assert_eq!(Arc::strong_count(reusable_vertices), reusable_owners);

    let alloc_before = ALLOC.snapshot();
    let cycles_before = thread_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for frame in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(7)
            ^ black_box(compose_frame(
                frame,
                actors,
                &metrics,
                &fonts,
                &mut cache,
                &mut scratch,
            ));
    }
    let elapsed = started.elapsed();
    let cycles = cycles_before
        .zip(thread_cycles())
        .map(|(before, after)| after.saturating_sub(before));
    let alloc = ALLOC.snapshot().delta(alloc_before);
    assert_eq!(alloc.allocs, 0, "warmed mesh composition allocated");
    assert_eq!(alloc.reallocs, 0, "warmed mesh composition reallocated");
    assert_eq!(Arc::strong_count(shared_vertices), shared_owners);
    assert_eq!(Arc::strong_count(reusable_vertices), reusable_owners);
    BenchResult {
        elapsed,
        cycles,
        alloc,
        checksum,
    }
}

fn compose_frame(
    frame: usize,
    actors: &[Actor],
    metrics: &Metrics,
    fonts: &FontMap,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
) -> u64 {
    let mut render = build_screen_cached_with_scratch(
        black_box(actors),
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        fonts,
        black_box(frame as f32 / 120.0),
        cache,
        scratch,
    );
    assert_eq!(render.objects.len(), MESHES + GLOW_MESHES);
    let checksum = render.objects.len() as u64
        ^ u64::from(render.objects.first().unwrap().order)
        ^ (u64::from(render.objects.last().unwrap().order) << 32);
    black_box(&render);
    scratch.recycle_render_list(&mut render);
    checksum
}

fn benchmark_actors(
    shared_vertices: &Arc<[TexturedMeshVertex]>,
    reusable_vertices: &Arc<Vec<TexturedMeshVertex>>,
) -> Vec<Actor> {
    let texture = Arc::<str>::from("bench/textured_mesh");
    (0..MESHES)
        .map(|index| {
            let glow = if index % 4 == 0 {
                [0.5, 0.75, 1.0, 0.35]
            } else {
                [0.0; 4]
            };
            if index % 2 == 0 {
                Actor::TexturedMesh {
                    align: [0.0, 0.0],
                    offset: [(index % 32) as f32 * 12.0, (index / 32) as f32 * 14.0],
                    world_z: index as f32 * 0.001,
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    local_transform: Mat4::IDENTITY,
                    texture: Arc::clone(&texture),
                    tint: [0.8, 0.6, 0.4, 1.0],
                    glow,
                    vertices: Arc::clone(shared_vertices),
                    geom_cache_key: index as u64 + 1,
                    uv_scale: [1.0, 1.0],
                    uv_offset: [0.0, 0.0],
                    uv_tex_shift: [0.0, 0.0],
                    depth_test: index % 8 == 0,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 0,
                }
            } else {
                Actor::ReusableTexturedMesh {
                    align: [0.0, 0.0],
                    offset: [(index % 32) as f32 * 12.0, (index / 32) as f32 * 14.0],
                    world_z: index as f32 * 0.001,
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    local_transform: Mat4::IDENTITY,
                    texture: Arc::clone(&texture),
                    tint: [0.8, 0.6, 0.4, 1.0],
                    glow,
                    vertices: Arc::clone(reusable_vertices),
                    geom_cache_key: index as u64 + 1,
                    uv_scale: [1.0, 1.0],
                    uv_offset: [0.0, 0.0],
                    uv_tex_shift: [0.0, 0.0],
                    depth_test: index % 8 == 0,
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 0,
                }
            }
        })
        .collect()
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> isize;
    fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    let mut cycles = 0_u64;
    // SAFETY: `GetCurrentThread` returns a valid pseudo-handle and `cycles`
    // remains writable for the duration of the query.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}
