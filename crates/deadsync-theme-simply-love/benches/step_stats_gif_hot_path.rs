use deadlib_present::actors::Actor;
use deadsync_profile::{PlayerSide, StepStatsExtra};
use deadsync_theme_simply_love::screens::components::gameplay::step_stats_gifs::{
    benchmark_gif_actor, benchmark_gif_actor_legacy,
};
use deadsync_theme_simply_love::step_stats_gifs::{
    GifRenderParams, ResolvedStepStatsExtra, benchmark_gif_frame_legacy,
    benchmark_gif_render_layout_legacy, catalog, gif_render_layout, resolve_extra,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 20_000;
const MEASURE_FRAMES: usize = 1_000_000;
const SAMPLE_BATCH: usize = 256;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the allocator arguments
// unchanged; relaxed atomics only observe allocation churn.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.frees.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the pointer/layout pair came from the delegated allocator.
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    max_batch: Duration,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[derive(Clone, Copy)]
struct Scenario {
    index: usize,
    resolved: ResolvedStepStatsExtra,
    params: GifRenderParams,
}

fn scenario(name: &str) -> Scenario {
    let index = catalog()
        .iter()
        .position(|gif| gif.name() == name)
        .unwrap_or_else(|| panic!("missing Step Stats GIF benchmark fixture '{name}'"));
    Scenario {
        index,
        resolved: resolve_extra(&StepStatsExtra::gif(name)),
        params: GifRenderParams {
            player_side: PlayerSide::P1,
            wide: true,
            aspect_ratio: 16.0 / 9.0,
            pane_x: 147.0,
            pane_y: 232.0,
            banner_data_zoom: 0.8,
            note_field_is_centered: false,
        },
    }
}

fn clocks(frame: usize) -> (f32, f32) {
    let seconds = frame as f32 * (1.0 / 60.0);
    (seconds * 2.75, seconds)
}

fn legacy_frame(frame: usize, scenario: Scenario) -> u64 {
    let (beat, seconds) = clocks(frame);
    let layout = benchmark_gif_render_layout_legacy(scenario.index, scenario.params)
        .expect("legacy fixture should resolve");
    let cell = benchmark_gif_frame_legacy(layout, beat, seconds);
    actor_checksum(&benchmark_gif_actor_legacy(layout, cell))
}

fn optimized_frame(frame: usize, scenario: Scenario) -> u64 {
    let (beat, seconds) = clocks(frame);
    let layout = gif_render_layout(scenario.resolved, scenario.params)
        .expect("optimized fixture should resolve");
    let cell = layout.frame_at(beat, seconds);
    actor_checksum(&benchmark_gif_actor(layout, cell))
}

fn actor_checksum(actor: &Actor) -> u64 {
    let Actor::Sprite {
        align,
        offset,
        source,
        z,
        cell,
        cropleft,
        cropright,
        croptop,
        cropbottom,
        scale,
        ..
    } = actor
    else {
        return 0;
    };
    let mut checksum = source
        .texture_key()
        .unwrap_or_default()
        .bytes()
        .fold(0u64, |value, byte| value.rotate_left(5) ^ u64::from(byte));
    for value in [
        align[0],
        align[1],
        offset[0],
        offset[1],
        *cropleft,
        *cropright,
        *croptop,
        *cropbottom,
        scale[0],
        scale[1],
    ] {
        checksum = checksum.rotate_left(7) ^ u64::from(value.to_bits());
    }
    checksum ^= (*z as u16 as u64) << 32;
    if let Some((frame, row)) = cell {
        checksum ^= u64::from(*frame) | (u64::from(*row) << 32);
    }
    checksum
}

fn measure(mut operation: impl FnMut(usize) -> u64) -> BenchResult {
    for frame in 0..WARMUP_FRAMES {
        black_box(operation(frame));
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut max_batch = Duration::ZERO;
    let mut checksum = 0u64;
    for batch_start in (0..MEASURE_FRAMES).step_by(SAMPLE_BATCH) {
        let batch_started = Instant::now();
        for frame in batch_start..(batch_start + SAMPLE_BATCH).min(MEASURE_FRAMES) {
            checksum = checksum.rotate_left(7) ^ black_box(operation(frame));
        }
        max_batch = max_batch.max(batch_started.elapsed());
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        max_batch,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let ns_per_frame = result.elapsed.as_secs_f64() * 1e9 / MEASURE_FRAMES as f64;
    let cycles_per_frame = result.cycles as f64 / MEASURE_FRAMES as f64;
    let throughput = MEASURE_FRAMES as f64 / result.elapsed.as_secs_f64();
    println!(
        "{label:22} {ns_per_frame:9.2} ns/frame  {cycles_per_frame:9.2} cycles/frame  \
         {throughput:10.0} frames/s  max {max_us:8.2} us/{SAMPLE_BATCH}  \
         alloc {allocs}  realloc {reallocs}  free {frees}  bytes {bytes}",
        max_us = result.max_batch.as_secs_f64() * 1e6,
        allocs = result.allocated.allocs,
        reallocs = result.allocated.reallocs,
        frees = result.allocated.frees,
        bytes = result.allocated.bytes,
    );
}

fn run(name: &str) {
    let scenario = scenario(name);
    let legacy = measure(|frame| legacy_frame(frame, scenario));
    let optimized = measure(|frame| optimized_frame(frame, scenario));
    assert_eq!(
        legacy.checksum, optimized.checksum,
        "{name} behavior changed"
    );
    assert_eq!(optimized.allocated.allocs, 0, "{name} still allocates");
    assert_eq!(optimized.allocated.reallocs, 0, "{name} still reallocates");
    assert_eq!(optimized.allocated.frees, 0, "{name} still frees");
    assert_eq!(optimized.allocated.bytes, 0, "{name} still allocates bytes");
    assert!(
        legacy.allocated.allocs >= MEASURE_FRAMES as u64,
        "legacy {name} should expose the per-frame texture-key allocation"
    );

    println!("Step Stats GIF '{name}' ({MEASURE_FRAMES} gameplay frames)");
    print_result("legacy indexed/dynamic", &legacy);
    print_result("precompiled/static", &optimized);
    println!();
}

fn main() {
    // AmongUs exercises mixed delays; CatJAM exercises the longest bundled
    // uniform-delay schedule and its compiled direct-index path.
    run("AmongUs");
    run("CatJAM");
}

#[inline(always)]
fn read_cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: `_rdtsc` reads the processor timestamp counter and has no
        // memory-safety preconditions.
        unsafe { std::arch::x86_64::_rdtsc() }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}
