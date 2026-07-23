use deadlib_present::actors::Actor;
use deadsync_theme_simply_love::screens::components::shared::visual_style_bg::{
    TechniqueBackgroundBench, technique_layer_checksum_for_bench,
    technique_layer_legacy_checksum_for_bench, technique_layout_checksum_for_bench,
    technique_layout_legacy_checksum_for_bench, technique_projection_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LAYOUT_RUNS: usize = 200_000;
const LAYER_RUNS: usize = 100_000;
const PROJECTION_RUNS: usize = 500_000;
const BUILD_RUNS: usize = 10_000;

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

// SAFETY: every operation is forwarded unchanged to `System`; the atomics only
// observe successful allocations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
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
    let state = TechniqueBackgroundBench::new();

    assert_eq!(
        technique_layout_legacy_checksum_for_bench(12.375),
        technique_layout_checksum_for_bench(12.375)
    );
    assert_eq!(
        technique_layer_legacy_checksum_for_bench(12.375),
        technique_layer_checksum_for_bench(12.375)
    );
    assert_eq!(
        technique_projection_legacy_for_bench(854.0, 480.0),
        state.projection(854.0, 480.0)
    );
    assert_eq!(
        actor_checksum(&state.build_legacy(12.375)),
        actor_checksum(&state.build(12.375))
    );

    run_pair(
        "deterministic circle layout",
        LAYOUT_RUNS,
        18.0,
        |run| technique_layout_legacy_checksum_for_bench(elapsed_for(run)),
        |run| technique_layout_checksum_for_bench(elapsed_for(run)),
    );
    run_pair(
        "model layer evaluation",
        LAYER_RUNS,
        21.0,
        |run| technique_layer_legacy_checksum_for_bench(elapsed_for(run)),
        |run| technique_layer_checksum_for_bench(elapsed_for(run)),
    );
    run_pair(
        "camera projection",
        PROJECTION_RUNS,
        1.0,
        |run| {
            let width = if run & 0x3fff == 0 { 640.0 } else { 854.0 };
            matrix_checksum(technique_projection_legacy_for_bench(width, 480.0))
        },
        |run| {
            let width = if run & 0x3fff == 0 { 640.0 } else { 854.0 };
            matrix_checksum(state.projection(width, 480.0))
        },
    );
    run_pair(
        "full Technique background build",
        BUILD_RUNS,
        21.0,
        |run| actor_checksum(&state.build_legacy(elapsed_for(run))),
        |run| actor_checksum(&state.build(elapsed_for(run))),
    );
}

fn run_pair(
    name: &str,
    runs: usize,
    units_per_run: f64,
    mut legacy: impl FnMut(usize) -> u64,
    mut optimized: impl FnMut(usize) -> u64,
) {
    let old = measure(runs, &mut legacy);
    let new = measure(runs, &mut optimized);
    assert_eq!(old.checksum, new.checksum);

    println!("{name} ({runs} runs)");
    print_result("old", runs, units_per_run, &old);
    print_result("new", runs, units_per_run, &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn measure(runs: usize, operation: &mut impl FnMut(usize) -> u64) -> BenchResult {
    for run in 0..8 {
        black_box(operation(black_box(run)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..runs {
        checksum = checksum.rotate_left(7) ^ black_box(operation(black_box(run))) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, runs: usize, units_per_run: f64, result: &BenchResult) {
    let runs = runs as f64;
    println!(
        "  {label:<4} {:>9.3} us/run {:>10.0} cycles/run {:>8.2} Munits/s",
        result.elapsed.as_secs_f64() * 1.0e6 / runs,
        result.cycles as f64 / runs,
        runs * units_per_run / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per run, {:.2} KiB/run",
        result.alloc.allocs as f64 / runs,
        result.alloc.reallocs as f64 / runs,
        result.alloc.bytes as f64 / runs / 1024.0,
    );
}

fn elapsed_for(run: usize) -> f64 {
    1_000.0 + run as f64 / 240.0
}

fn matrix_checksum(matrix: [f32; 16]) -> u64 {
    matrix.into_iter().fold(0_u64, |sum, value| {
        sum.rotate_left(5) ^ u64::from(value.to_bits())
    })
}

fn actor_checksum(actors: &[Actor]) -> u64 {
    fn visit(actor: &Actor, checksum: &mut u64) {
        match actor {
            Actor::Camera {
                view_proj,
                children,
            } => {
                *checksum ^= matrix_checksum(view_proj.to_cols_array());
                for child in children {
                    visit(child, checksum);
                }
            }
            Actor::CameraPush { view_proj } => {
                *checksum ^= matrix_checksum(view_proj.to_cols_array());
            }
            Actor::CameraPop => {}
            Actor::TexturedMesh {
                local_transform,
                texture,
                tint,
                vertices,
                geom_cache_key,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                ..
            } => {
                *checksum = checksum.rotate_left(3) ^ *geom_cache_key ^ vertices.len() as u64;
                for byte in texture.bytes() {
                    *checksum = checksum.rotate_left(5) ^ u64::from(byte);
                }
                for value in local_transform
                    .to_cols_array()
                    .into_iter()
                    .chain(*tint)
                    .chain(*uv_scale)
                    .chain(*uv_offset)
                    .chain(*uv_tex_shift)
                {
                    *checksum = checksum.rotate_left(7) ^ u64::from(value.to_bits());
                }
            }
            Actor::Sprite {
                offset,
                tint,
                uv_rect,
                ..
            } => {
                *checksum = checksum.rotate_left(11) ^ 1;
                for value in offset.iter().chain(tint).chain(uv_rect.iter().flatten()) {
                    *checksum = checksum.rotate_left(5) ^ u64::from(value.to_bits());
                }
            }
            _ => {
                *checksum = checksum.rotate_left(11) ^ 2;
            }
        }
    }

    let mut checksum = 0_u64;
    for actor in actors {
        visit(actor, &mut checksum);
    }
    checksum
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: timestamp reads and fences do not access memory.
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
