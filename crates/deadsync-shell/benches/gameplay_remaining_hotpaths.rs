use deadlib_assets::registry::{
    generated_texture_idle_poll_for_bench, generated_texture_idle_poll_legacy_for_bench,
    prepare_generated_texture_idle_poll_benchmark,
};
use deadlib_assets::texture_store::VideoTextureMetadataBenchmark;
use deadsync_notefield::{HoldPairAcquireBenchmark, ZmodMeasureCounterText};
use deadsync_shell::bench_support::GameplayBannerSyncBenchmark;
use deadsync_theme_simply_love::screens::components::gameplay::notefield::{
    benchmark_measure_counter_text, benchmark_measure_counter_text_legacy, benchmark_run_timer,
    benchmark_run_timer_legacy,
};
use deadsync_theme_simply_love::screens::gameplay::SongLuaActorBuildBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 2_000;
const PROXY_FRAMES: usize = 100_000;
const HOLD_FRAMES: usize = 500_000;
const VIDEO_FRAMES: usize = 200_000;
const HUD_FRAMES: usize = 500_000;
const BANNER_FRAMES: usize = 500_000;
const GENERATED_IDLE_FRAMES: usize = 1_000_000;
const TAIL_SAMPLES: usize = 20_000;
const TAIL_BATCH_FRAMES: usize = 512;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// SAFETY: every operation is forwarded unchanged to `System`; the atomics
// only observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller supplies the live allocation and original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer and its original layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
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
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed_ns: u128,
    cycles: u64,
    p999_ns: u128,
    worst_ns: u128,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let mut old_proxy = SongLuaActorBuildBenchmark::new(3);
    let mut new_proxy = SongLuaActorBuildBenchmark::new(3);
    assert_eq!(
        old_proxy.legacy_segmented_proxy_frame(),
        new_proxy.prewarmed_segmented_proxy_frame()
    );
    run_pair(
        "Song-Lua three-segment proxy",
        PROXY_FRAMES,
        || old_proxy.legacy_segmented_proxy_frame(),
        || new_proxy.prewarmed_segmented_proxy_frame(),
        true,
    );

    let old_holds = HoldPairAcquireBenchmark::new(16);
    let new_holds = HoldPairAcquireBenchmark::new(16);
    assert_eq!(old_holds.linear_frame(12), new_holds.bitmask_frame(12));
    run_pair(
        "12 hold-pair acquisitions",
        HOLD_FRAMES,
        || old_holds.linear_frame(12),
        || new_holds.bitmask_frame(12),
        false,
    );

    let old_video = VideoTextureMetadataBenchmark::new(1920, 1080);
    let new_video = VideoTextureMetadataBenchmark::new(1920, 1080);
    assert_eq!(old_video.global_frame(), new_video.local_frame());
    run_pair(
        "steady-size video metadata",
        VIDEO_FRAMES,
        || old_video.global_frame(),
        || new_video.local_frame(),
        false,
    );

    let mut old_hud_frame = 0usize;
    let mut new_hud_frame = 0usize;
    run_pair(
        "measure-counter and run-timer text",
        HUD_FRAMES,
        || {
            let checksum = hud_text_frame(
                old_hud_frame,
                benchmark_measure_counter_text_legacy,
                benchmark_run_timer_legacy,
            );
            old_hud_frame += 1;
            checksum
        },
        || {
            let checksum = hud_text_frame(
                new_hud_frame,
                benchmark_measure_counter_text,
                benchmark_run_timer,
            );
            new_hud_frame += 1;
            checksum
        },
        false,
    );

    let mut old_banners = GameplayBannerSyncBenchmark::default();
    let mut new_banners = GameplayBannerSyncBenchmark::default();
    run_pair(
        "settled gameplay banner request",
        BANNER_FRAMES,
        || old_banners.legacy_frame(),
        || new_banners.settled_frame(),
        false,
    );

    prepare_generated_texture_idle_poll_benchmark();
    run_pair(
        "idle generated-texture queue",
        GENERATED_IDLE_FRAMES,
        || generated_texture_idle_poll_legacy_for_bench() as u64,
        || generated_texture_idle_poll_for_bench() as u64,
        false,
    );
}

fn hud_text_frame(
    frame: usize,
    counter: fn(ZmodMeasureCounterText) -> deadlib_present::actors::TextContent,
    timer: fn(i32, i32, bool) -> deadlib_present::actors::TextContent,
) -> u64 {
    let current = (frame % 64 + 1) as i32;
    let seconds = (frame % 600) as i32;
    let values = [
        counter(ZmodMeasureCounterText::Ratio { current, total: 64 }),
        counter(ZmodMeasureCounterText::Break(16)),
        counter(ZmodMeasureCounterText::Total(64)),
        timer(seconds, 59, true),
    ];
    values.iter().fold(0_u64, |checksum, text| {
        text.as_str().bytes().fold(checksum, |checksum, byte| {
            checksum.rotate_left(5) ^ u64::from(byte)
        })
    })
}

fn run_pair(
    label: &str,
    frames: usize,
    mut old_frame: impl FnMut() -> u64,
    mut new_frame: impl FnMut() -> u64,
    expect_alloc_removed: bool,
) {
    let old = measure(frames, &mut old_frame);
    let new = measure(frames, &mut new_frame);
    assert_eq!(old.checksum, new.checksum, "{label} behavior changed");
    assert_eq!(new.alloc.allocs, 0, "{label} optimized path allocated");
    assert_eq!(new.alloc.reallocs, 0, "{label} optimized path reallocated");
    assert_eq!(new.alloc.bytes, 0, "{label} optimized path allocated bytes");
    if expect_alloc_removed {
        assert!(old.alloc.allocs > 0, "{label} old path did not allocate");
    }

    println!("{label} ({frames} frames)");
    print_result("old", frames, &old);
    print_result("new", frames, &new);
    println!(
        "  speedup {:.2}x | cycle reduction {:.1}% | p99.9 reduction {:.1}% | raw-max reduction {:.1}%\n",
        old.elapsed_ns as f64 / new.elapsed_ns as f64,
        reduction(old.cycles, new.cycles),
        reduction_u128(old.p999_ns, new.p999_ns),
        reduction_u128(old.worst_ns, new.worst_ns),
    );
}

fn measure(frames: usize, frame: &mut impl FnMut() -> u64) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }

    let cycle_start = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..frames {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed_ns = started.elapsed().as_nanos();
    let cycles = read_cycles().saturating_sub(cycle_start);

    let before = ALLOC.snapshot();
    ALLOC.set_enabled(true);
    let mut alloc_checksum = 0_u64;
    for _ in 0..frames {
        alloc_checksum = alloc_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.set_enabled(false);
    let alloc = ALLOC.snapshot().delta(before);
    black_box(alloc_checksum);

    let mut tail_samples = Vec::with_capacity(TAIL_SAMPLES.min(frames));
    let mut tail_checksum = 0_u64;
    for _ in 0..TAIL_SAMPLES.min(frames) {
        let started = Instant::now();
        for _ in 0..TAIL_BATCH_FRAMES {
            tail_checksum = tail_checksum.wrapping_add(black_box(frame()));
        }
        tail_samples.push(started.elapsed().as_nanos() / TAIL_BATCH_FRAMES as u128);
    }
    black_box(tail_checksum);
    tail_samples.sort_unstable();
    let p999_index = tail_samples.len().saturating_sub(1) * 999 / 1000;
    let p999_ns = tail_samples.get(p999_index).copied().unwrap_or_default();
    let worst_ns = tail_samples.last().copied().unwrap_or_default();

    BenchResult {
        elapsed_ns,
        cycles,
        p999_ns,
        worst_ns,
        alloc,
        checksum,
    }
}

fn print_result(label: &str, frames: usize, result: &BenchResult) {
    let frames = frames as f64;
    let seconds = result.elapsed_ns as f64 / 1.0e9;
    println!(
        "  {label:<3} {:>8.2} ns/frame {:>8.2} cycles/frame {:>8.2} Mframe/s p99.9/max batch avg={:>5}/{:>7} ns  alloc/realloc/free={:.2}/{:.2}/{:.2} bytes={:.1}/frame",
        result.elapsed_ns as f64 / frames,
        result.cycles as f64 / frames,
        frames / seconds / 1.0e6,
        result.p999_ns,
        result.worst_ns,
        result.alloc.allocs as f64 / frames,
        result.alloc.reallocs as f64 / frames,
        result.alloc.deallocs as f64 / frames,
        result.alloc.bytes as f64 / frames,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

fn reduction_u128(old: u128, new: u128) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and RDTSC only serialize and read the timestamp counter.
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
