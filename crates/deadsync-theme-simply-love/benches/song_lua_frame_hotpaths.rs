use deadsync_theme_simply_love::screens::gameplay::{
    SongLuaMessageStateBenchmark, SongLuaOrderBenchmark, benchmark_projected_mesh_scratch,
    benchmark_projected_mesh_scratch_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MESSAGE_EVENTS: usize = 2_048;
const ORDER_ACTORS: usize = 256;
const WARMUP_FRAMES: usize = 1_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
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
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn measure(iterations: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.set_enabled(true);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.set_enabled(false);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1_000_000_000.0 / iterations as f64,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let frames = iterations as f64;
    println!(
        "{label:<20} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>5.2} allocs/frame  {:>8.1} bytes/frame  {:>5.2} reallocs/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    const MESSAGE_FRAMES: usize = 20_000;
    const ORDER_FRAMES: usize = 100_000;
    const MESH_FRAMES: usize = 50_000;

    let now = MESSAGE_EVENTS as f32 * 0.125 + 1.0;
    let mut cached_messages = SongLuaMessageStateBenchmark::new(MESSAGE_EVENTS);
    let legacy_messages = SongLuaMessageStateBenchmark::new(MESSAGE_EVENTS);
    assert_eq!(
        legacy_messages.legacy_frame(now),
        cached_messages.cached_frame(now)
    );
    let legacy_message_result = measure(MESSAGE_FRAMES, || {
        legacy_messages.legacy_frame(black_box(now)).to_bits() as u64
    });
    let cached_message_result = measure(MESSAGE_FRAMES, || {
        cached_messages.cached_frame(black_box(now)).to_bits() as u64
    });

    let mut legacy_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let mut cached_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    assert_eq!(legacy_order.legacy_frame(), cached_order.cached_frame());
    let legacy_order_result = measure(ORDER_FRAMES, || {
        legacy_order.legacy_frame().min(u64::MAX as usize) as u64
    });
    let cached_order_result = measure(ORDER_FRAMES, || {
        cached_order.cached_frame().min(u64::MAX as usize) as u64
    });

    let mut changing_tick = 0usize;
    let mut legacy_changing_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let legacy_changing_order_result = measure(ORDER_FRAMES, || {
        let checksum = legacy_changing_order.legacy_changing_frame(changing_tick);
        changing_tick = changing_tick.wrapping_add(1);
        checksum.min(u64::MAX as usize) as u64
    });
    let mut changing_tick = 0usize;
    let mut cached_changing_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let cached_changing_order_result = measure(ORDER_FRAMES, || {
        let checksum = cached_changing_order.cached_changing_frame(changing_tick);
        changing_tick = changing_tick.wrapping_add(1);
        checksum.min(u64::MAX as usize) as u64
    });

    let legacy_vertices = benchmark_projected_mesh_scratch_legacy(0.25, 0.25);
    let inline_vertices = benchmark_projected_mesh_scratch(0.25, 0.25);
    assert_eq!(legacy_vertices.as_ref(), inline_vertices.as_ref());
    let legacy_mesh_result = measure(MESH_FRAMES, || {
        let vertices = benchmark_projected_mesh_scratch_legacy(black_box(0.25), black_box(0.25));
        vertices.len() as u64
    });
    let inline_mesh_result = measure(MESH_FRAMES, || {
        let vertices = benchmark_projected_mesh_scratch(black_box(0.25), black_box(0.25));
        vertices.len() as u64
    });

    assert_eq!(
        legacy_message_result.checksum,
        cached_message_result.checksum
    );
    assert_eq!(legacy_order_result.checksum, cached_order_result.checksum);
    assert_eq!(
        legacy_changing_order_result.checksum,
        cached_changing_order_result.checksum
    );
    assert_eq!(legacy_mesh_result.checksum, inline_mesh_result.checksum);

    println!("Song Lua gameplay frame hot paths");
    println!("message state ({MESSAGE_EVENTS} prior events)");
    print_result("replay history", MESSAGE_FRAMES, &legacy_message_result);
    print_result("incremental cache", MESSAGE_FRAMES, &cached_message_result);
    println!("dynamic order ({ORDER_ACTORS} actors, unchanged keys)");
    print_result("sort every frame", ORDER_FRAMES, &legacy_order_result);
    print_result("key cache", ORDER_FRAMES, &cached_order_result);
    println!("dynamic order ({ORDER_ACTORS} actors, one changed key/frame)");
    print_result(
        "sort changed frame",
        ORDER_FRAMES,
        &legacy_changing_order_result,
    );
    print_result(
        "cache changed frame",
        ORDER_FRAMES,
        &cached_changing_order_result,
    );
    println!("projected mesh (4x4 scratch grid)");
    print_result("heap scratch", MESH_FRAMES, &legacy_mesh_result);
    print_result("inline scratch", MESH_FRAMES, &inline_mesh_result);
}
