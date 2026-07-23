use deadlib_present::actors::Actor;
use deadsync_theme_simply_love::screens::components::shared::visual_style_bg::OtherVisualBackgroundBench;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const BUILD_RUNS: usize = 1_000_000;
const SAMPLE_PAIRS: usize = 7;

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
    pin_benchmark_thread();
    let state = OtherVisualBackgroundBench::new();

    state.set_srpg_key(None);
    assert_eq!(
        actor_checksum(&state.build_srpg_legacy()),
        actor_checksum(&state.build_srpg())
    );
    run_pair(
        "SRPG static fallback build",
        BUILD_RUNS,
        1.0,
        |_| actor_checksum(&state.build_srpg_legacy()),
        |_| actor_checksum(&state.build_srpg()),
    );

    state.set_srpg_key(Some("dynamic/srpg-video"));
    assert_eq!(
        actor_checksum(&state.build_srpg_legacy()),
        actor_checksum(&state.build_srpg())
    );
    run_pair(
        "repeated SRPG video-key publication",
        BUILD_RUNS,
        1.0,
        |_| {
            state.set_srpg_key_legacy(Some("dynamic/srpg-video"));
            1
        },
        |_| {
            state.set_srpg_key(Some("dynamic/srpg-video"));
            1
        },
    );
    state.set_srpg_key(None);
}

#[cfg(windows)]
fn pin_benchmark_thread() {
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut core::ffi::c_void, affinity_mask: usize) -> usize;
    }

    // SAFETY: the pseudo-handle returned by `GetCurrentThread` is valid for the
    // calling thread, and bit zero selects the first logical processor.
    let previous_mask = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) };
    assert_ne!(
        previous_mask, 0,
        "failed to pin the benchmark thread to one logical processor"
    );
}

#[cfg(not(windows))]
fn pin_benchmark_thread() {}

fn run_pair(
    name: &str,
    runs: usize,
    units_per_run: f64,
    mut legacy: impl FnMut(usize) -> u64,
    mut optimized: impl FnMut(usize) -> u64,
) {
    let mut pairs = Vec::with_capacity(SAMPLE_PAIRS);
    for sample in 0..SAMPLE_PAIRS {
        if sample % 2 == 0 {
            pairs.push((measure(runs, &mut legacy), measure(runs, &mut optimized)));
        } else {
            let new = measure(runs, &mut optimized);
            let old = measure(runs, &mut legacy);
            pairs.push((old, new));
        }
    }
    pairs.sort_unstable_by(|(old_a, new_a), (old_b, new_b)| {
        (u128::from(old_a.cycles) * u128::from(new_b.cycles))
            .cmp(&(u128::from(old_b.cycles) * u128::from(new_a.cycles)))
    });
    let (old, new) = &pairs[SAMPLE_PAIRS / 2];
    assert_eq!(old.checksum, new.checksum);

    println!("{name} ({runs} runs, median of {SAMPLE_PAIRS} alternating-order sample pairs)");
    print_result("old", runs, units_per_run, old);
    print_result("new", runs, units_per_run, new);
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

fn actor_checksum(actors: &[Actor]) -> u64 {
    actors.iter().fold(0_u64, |mut checksum, actor| {
        let Actor::Sprite {
            offset,
            source,
            tint,
            uv_rect,
            z,
            ..
        } = actor
        else {
            return checksum.rotate_left(11) ^ 1;
        };
        checksum = checksum.rotate_left(7) ^ (*z as u16 as u64);
        if let Some(texture) = source.texture_key() {
            for byte in texture.bytes() {
                checksum = checksum.rotate_left(5) ^ u64::from(byte);
            }
        }
        for value in offset.iter().chain(tint).chain(uv_rect.iter().flatten()) {
            checksum = checksum.rotate_left(5) ^ u64::from(value.to_bits());
        }
        checksum
    })
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
