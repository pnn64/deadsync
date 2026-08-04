use deadlib_render::CameraUploadCache;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CAMERA_SPANS: usize = 128;
const PROGRAM_KINDS: usize = 3;
const WARMUP_FRAMES: usize = 1_000;
const MEASURE_FRAMES: usize = 50_000;
const BENCH_RUNS: usize = 7;

#[derive(Clone, Copy)]
struct DrawRun {
    kind: u8,
    camera: u8,
}

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
    uploads: usize,
}

type Planner = fn(&[DrawRun]) -> (usize, u64);

fn main() {
    let runs = gameplay_runs();
    compare(
        "OpenGL per-program projection uploads",
        &runs,
        plan_gl_legacy,
        plan_gl_current,
        48,
    );
    compare(
        "Vulkan compatible push-constant uploads",
        &runs,
        plan_vulkan_legacy,
        plan_vulkan_current,
        16,
    );
}

fn compare(
    label: &str,
    runs: &[DrawRun],
    legacy_plan: Planner,
    current_plan: Planner,
    expected_current_uploads: usize,
) {
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);

    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure(runs, current_plan);
            let old = measure(runs, legacy_plan);
            (old, new)
        } else {
            let old = measure(runs, legacy_plan);
            let new = measure(runs, current_plan);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.uploads, runs.len());
        assert_eq!(new.uploads, expected_current_uploads);
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

    println!("{label} ({} draw runs, median of {BENCH_RUNS})", runs.len());
    print_result("draw-run keyed", &legacy, runs.len());
    print_result("camera keyed", &current, runs.len());
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | projection updates/frame {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.uploads,
        current.uploads,
        100.0 * (1.0 - current.uploads as f64 / legacy.uploads as f64),
    );
}

fn measure(runs: &[DrawRun], plan: Planner) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(plan(black_box(runs)));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut uploads = 0;
    for _ in 0..MEASURE_FRAMES {
        let (frame_uploads, frame_checksum) = plan(black_box(runs));
        uploads = frame_uploads;
        checksum = checksum.rotate_left(9) ^ frame_checksum;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        uploads,
    }
}

fn plan_gl_legacy(runs: &[DrawRun]) -> (usize, u64) {
    let mut uploads = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        observe_upload(run, &mut uploads);
        checksum = observe_draw(checksum, run);
    }
    (uploads, black_box(checksum))
}

fn plan_gl_current(runs: &[DrawRun]) -> (usize, u64) {
    let mut cameras = [CameraUploadCache::default(); PROGRAM_KINDS];
    let mut uploads = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        if cameras[run.kind as usize].update_required(run.camera) {
            observe_upload(run, &mut uploads);
        }
        checksum = observe_draw(checksum, run);
    }
    (uploads, black_box(checksum))
}

fn plan_vulkan_legacy(runs: &[DrawRun]) -> (usize, u64) {
    let mut last_kind = None;
    let mut camera = CameraUploadCache::default();
    let mut uploads = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        if last_kind != Some(run.kind) {
            last_kind = Some(run.kind);
            camera = CameraUploadCache::default();
        }
        if camera.update_required(run.camera) {
            observe_upload(run, &mut uploads);
        }
        checksum = observe_draw(checksum, run);
    }
    (uploads, black_box(checksum))
}

fn plan_vulkan_current(runs: &[DrawRun]) -> (usize, u64) {
    let mut camera = CameraUploadCache::default();
    let mut uploads = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        if camera.update_required(run.camera) {
            observe_upload(run, &mut uploads);
        }
        checksum = observe_draw(checksum, run);
    }
    (uploads, black_box(checksum))
}

#[inline(never)]
fn observe_upload(run: DrawRun, uploads: &mut usize) {
    *uploads += 1;
    black_box((run.kind, run.camera));
}

#[inline(always)]
fn observe_draw(checksum: u64, run: DrawRun) -> u64 {
    checksum.rotate_left(7) ^ u64::from(run.kind) ^ (u64::from(run.camera) << 32)
}

fn gameplay_runs() -> Vec<DrawRun> {
    let mut runs = Vec::with_capacity(CAMERA_SPANS * PROGRAM_KINDS);
    for span in 0..CAMERA_SPANS {
        let camera = u8::from(span % 16 == 15);
        for kind in 0..PROGRAM_KINDS {
            runs.push(DrawRun {
                kind: kind as u8,
                camera,
            });
        }
    }
    runs
}

fn print_result(label: &str, result: &BenchResult, draw_runs: usize) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<14} {:>7.2} ns/frame  {:>8.1} cycles/frame  \
         {:>7.1} M runs/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * draw_runs as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
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
