use deadlib_present::compose::{TextLayoutCache, TextLayoutFrameStats};
use deadlib_present::font::{Font, Glyph};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LIVE_VALUES: usize = 256;
const MEASURE_LOOKUPS: usize = 200_000;

struct CountingAlloc {
    allocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> (u64, u64) {
        (
            self.allocs.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        )
    }
}

// SAFETY: every operation delegates to `System` with the original allocation
// layout and observes successful allocations through independent atomics.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees this is a live `System` allocation.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the original live allocation and layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

fn main() {
    let fonts = HashMap::from([("test", numeric_font())]);
    let values = (1..=LIVE_VALUES)
        .map(|value| format!("{}.{:01}", value / 10, value % 10))
        .collect::<Vec<_>>();
    let saturated = run_case(&fonts, &values, false);
    let retained = run_case(&fonts, &values, true);

    assert_eq!(saturated.stats.misses as usize, MEASURE_LOOKUPS);
    assert_eq!(saturated.stats.owned_hits, 0);
    assert_eq!(retained.stats.owned_hits as usize, MEASURE_LOOKUPS);
    assert_eq!(retained.stats.misses, 0);

    println!("saturated text-layout repeat microbenchmark");
    println!("{LIVE_VALUES} live numeric values, {MEASURE_LOOKUPS} repeated lookups");
    print_result("exact freeze", saturated);
    print_result("bounded retain", retained);
    println!(
        "bounded retention: {:.2}x throughput, {:.1} -> {:.1} B/lookup",
        saturated.elapsed_ns / retained.elapsed_ns,
        saturated.bytes_per_lookup,
        retained.bytes_per_lookup,
    );
}

#[derive(Clone, Copy)]
struct BenchResult {
    elapsed_ns: f64,
    allocs_per_lookup: f64,
    bytes_per_lookup: f64,
    stats: TextLayoutFrameStats,
}

fn run_case(
    fonts: &HashMap<&'static str, Font>,
    values: &[String],
    retain_late: bool,
) -> BenchResult {
    let mut cache = TextLayoutCache::new(LIVE_VALUES + 1);
    cache.prewarm_text(fonts, "test", "0.0", None);
    if retain_late {
        cache.lock_growth_with_reserve(LIVE_VALUES);
    } else {
        cache.lock_growth();
    }

    for value in values {
        cache.prewarm_text(fonts, "test", value, None);
    }
    cache.begin_frame_stats(true);

    let before = ALLOC.snapshot();
    let started = Instant::now();
    for lookup in 0..MEASURE_LOOKUPS {
        cache.prewarm_text(
            fonts,
            "test",
            black_box(&values[lookup % LIVE_VALUES]),
            None,
        );
    }
    let elapsed = started.elapsed();
    let after = ALLOC.snapshot();
    let stats = cache.frame_stats();
    BenchResult {
        elapsed_ns: elapsed.as_secs_f64() * 1_000_000_000.0 / MEASURE_LOOKUPS as f64,
        allocs_per_lookup: (after.0 - before.0) as f64 / MEASURE_LOOKUPS as f64,
        bytes_per_lookup: (after.1 - before.1) as f64 / MEASURE_LOOKUPS as f64,
        stats,
    }
}

fn print_result(name: &str, result: BenchResult) {
    println!(
        "{name:<14} {:>7.2} ns/lookup, {:.3} allocs/lookup, {:>5.1} B/lookup, hits={}, misses={}",
        result.elapsed_ns,
        result.allocs_per_lookup,
        result.bytes_per_lookup,
        result.stats.owned_hits,
        result.stats.misses,
    );
}

fn numeric_font() -> Font {
    let texture_key = Arc::<str>::from("bench_numeric_font");
    let glyph = Glyph {
        texture_key,
        stroke_texture_key: None,
        tex_rect: [0.0, 0.0, 8.0, 10.0],
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        size: [8.0, 10.0],
        offset: [0.0, -10.0],
        advance: 8.0,
        advance_i32: 8,
    };
    let mut glyph_map = HashMap::new();
    let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
    for ch in "-0123456789.".chars() {
        glyph_map.insert(ch, glyph.clone());
        ascii_glyphs[ch as usize] = Some(glyph.clone());
    }
    Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: None,
        line_spacing: 10,
        height: 10,
        fallback_font_name: None,
        cache_tag: 1,
        chain_key: 1,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    }
}
