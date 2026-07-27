use deadlib_present::actors::TextContent;
use deadsync_theme_simply_love::screens::gameplay::{
    GameplayHudTextBenchmarkCache, GameplayHudTextBenchmarkSnapshot,
    benchmark_gameplay_hud_text_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 20_000;
const MEASURE_FRAMES: usize = 2_000_000;
const SMX_SENSOR_VALUES_PER_FRAME: usize = 8;

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

// SAFETY: all operations delegate to `System` with their original layouts;
// the atomics only observe successful allocation calls.
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

fn inputs(frame: usize) -> (f64, f32) {
    let bpm = if frame % 20_000 < 10_000 {
        150.0
    } else {
        175.25
    };
    let life = if frame % 120 < 60 { 87.3 } else { 85.2 };
    (bpm, life)
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: usize,
}

fn checksum(snapshot: &GameplayHudTextBenchmarkSnapshot) -> usize {
    snapshot
        .bpm
        .len()
        .wrapping_add(snapshot.life.len())
        .wrapping_add(snapshot.overlay.len())
        .wrapping_add(snapshot.overlay_line_count)
}

fn measure(mut frame: impl FnMut(usize) -> GameplayHudTextBenchmarkSnapshot) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        let snapshot = frame(index);
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for index in 0..MEASURE_FRAMES {
        output_checksum = output_checksum.wrapping_add(checksum(&black_box(frame(index))));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn smx_sensor_value(frame: usize, slot: usize) -> u16 {
    ((frame * 17 + slot * 61) % 501) as u16
}

fn text_checksum(text: &TextContent) -> usize {
    text.as_str().bytes().fold(0usize, |checksum, byte| {
        checksum.rotate_left(5) ^ byte as usize
    })
}

fn measure_smx_sensor_text(mut text: impl FnMut(u16) -> TextContent) -> BenchResult {
    for frame in 0..WARMUP_FRAMES {
        for slot in 0..SMX_SENSOR_VALUES_PER_FRAME {
            let value = smx_sensor_value(frame, slot);
            assert_eq!(text(value).as_str(), value.to_string());
        }
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        for slot in 0..SMX_SENSOR_VALUES_PER_FRAME {
            let value = black_box(smx_sensor_value(frame, slot));
            output_checksum =
                output_checksum.rotate_left(7) ^ text_checksum(&black_box(text(value)));
        }
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<13} {:>9.2} ns/frame  {:>8.0} cycles/frame  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    let legacy = measure(|frame| {
        let (bpm, life) = inputs(frame);
        benchmark_gameplay_hud_text_legacy(bpm, true, life, "AutoPlay")
    });
    let mut cache = GameplayHudTextBenchmarkCache::new("AutoPlay");
    let optimized = measure(|frame| {
        let (bpm, life) = inputs(frame);
        let snapshot = cache.snapshot(bpm, true, life);
        assert_eq!(
            snapshot.bpm.as_ref(),
            if bpm == 150.0 { "150" } else { "175.25" }
        );
        assert_eq!(
            snapshot.life.as_ref(),
            if life == 87.3 { "87.3%" } else { "85.2%" }
        );
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
        assert_eq!(snapshot.overlay_line_count, 1);
        snapshot
    });
    black_box((legacy.checksum, optimized.checksum));

    println!("gameplay HUD text benchmark");
    print_result("legacy frame", &legacy);
    print_result("cached frame", &optimized);

    let owned_sensor_text = measure_smx_sensor_text(|value| TextContent::from(value.to_string()));
    let inline_sensor_text = measure_smx_sensor_text(TextContent::inline_u16);
    assert_eq!(owned_sensor_text.checksum, inline_sensor_text.checksum);
    black_box((owned_sensor_text.checksum, inline_sensor_text.checksum));

    println!(
        "\nSMX gameplay sensor text benchmark \
         ({SMX_SENSOR_VALUES_PER_FRAME} live values/frame)"
    );
    print_result("owned values", &owned_sensor_text);
    print_result("inline values", &inline_sensor_text);
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
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
