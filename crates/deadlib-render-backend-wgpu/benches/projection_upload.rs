use glam::{Mat4 as Matrix4, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CAMERAS: usize = 4;
const STRIDE: usize = 256;
const WARMUP_FRAMES: usize = 64;
const MEASURE_FRAMES: usize = 500_000;
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

// SAFETY: all allocation operations delegate to `System` with their original
// pointer and layout; the counters only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees this is a live allocation from `System`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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
    uploads: usize,
}

fn main() {
    let cameras = cameras();
    let fallback = Matrix4::from_translation(Vec3::new(427.0, 240.0, 0.0));
    run_workload("stable", &cameras, fallback, false);
    run_workload("changing", &cameras, fallback, true);
}

fn run_workload(label: &str, cameras: &[Matrix4], fallback: Matrix4, changing: bool) {
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_current(cameras, fallback, changing);
            let old = measure_legacy(cameras, fallback, changing);
            (old, new)
        } else {
            let old = measure_legacy(cameras, fallback, changing);
            let new = measure_current(cameras, fallback, changing);
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
        "{label} WGPU projection staging ({CAMERAS} cameras + fallback, median of {BENCH_RUNS})"
    );
    print_result("pack every frame", &legacy);
    print_result("bit-key cache", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | queue writes {} -> {}",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.uploads,
        current.uploads,
    );
}

fn measure_legacy(cameras: &[Matrix4], fallback: Matrix4, changing: bool) -> BenchResult {
    let mut cameras = cameras.to_vec();
    let mut upload = Vec::new();
    for frame in 0..WARMUP_FRAMES {
        update_camera(&mut cameras, frame, changing);
        deadlib_render_backend_wgpu::__benchmark_stage_projection_upload_legacy(
            &mut upload,
            &cameras,
            fallback,
            STRIDE,
        );
        black_box(&upload);
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for frame in 0..MEASURE_FRAMES {
        update_camera(&mut cameras, frame, changing);
        deadlib_render_backend_wgpu::__benchmark_stage_projection_upload_legacy(
            &mut upload,
            black_box(&cameras),
            fallback,
            STRIDE,
        );
        checksum ^= upload_checksum(&upload).rotate_left(frame as u32);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        uploads: MEASURE_FRAMES,
    }
}

fn measure_current(cameras: &[Matrix4], fallback: Matrix4, changing: bool) -> BenchResult {
    let mut cameras = cameras.to_vec();
    let mut upload = Vec::new();
    let mut keys = Vec::new();
    for frame in 0..WARMUP_FRAMES {
        update_camera(&mut cameras, frame, changing);
        deadlib_render_backend_wgpu::__benchmark_stage_projection_upload(
            &mut upload,
            &mut keys,
            &cameras,
            fallback,
            STRIDE,
        );
        black_box((&upload, &keys));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut uploads = 0;
    for frame in 0..MEASURE_FRAMES {
        update_camera(&mut cameras, frame, changing);
        uploads += usize::from(
            deadlib_render_backend_wgpu::__benchmark_stage_projection_upload(
                &mut upload,
                &mut keys,
                black_box(&cameras),
                fallback,
                STRIDE,
            ),
        );
        checksum ^= upload_checksum(&upload).rotate_left(frame as u32);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        uploads,
    }
}

fn update_camera(cameras: &mut [Matrix4], frame: usize, changing: bool) {
    if changing {
        cameras[0].w_axis.x = (frame & 1) as f32;
    }
}

fn cameras() -> Vec<Matrix4> {
    (0..CAMERAS)
        .map(|index| {
            Matrix4::from_scale_rotation_translation(
                Vec3::splat(1.0 + index as f32 * 0.125),
                glam::Quat::from_rotation_z(index as f32 * 0.07),
                Vec3::new(index as f32 * 16.0, index as f32 * -9.0, 0.0),
            )
        })
        .collect()
}

fn upload_checksum(upload: &[u8]) -> u64 {
    let mut checksum = upload.len() as u64;
    for chunk in upload.chunks_exact(64) {
        checksum = checksum.rotate_left(7) ^ u64::from(chunk[0]);
        checksum = checksum.rotate_left(11) ^ u64::from(chunk[31]);
        checksum = checksum.rotate_left(13) ^ u64::from(chunk[63]);
    }
    black_box(checksum)
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<16} {:>7.2} ns/frame  {:>7.2} cycles/frame  \
         {:>7.1} Mframes/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64() / 1_000_000.0,
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
