use deadlib_present::compose::ComposeScratch;
use deadlib_present::font::{Font, FontMap, Glyph, GlyphMap};
use deadsync_theme_simply_love::screens::components::gameplay::gameplay_stats::{
    GameplayStatsNumericHotBenchmark, benchmark_clock_text_setup, benchmark_count_text_setup,
};
use deadsync_theme_simply_love::screens::gameplay::{
    GameplayLifeTextHotBenchmark, benchmark_life_text_setup,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const WARMUPS: usize = 3;
const ITERATIONS: usize = 100;
const HOT_FRAMES: u32 = 1_000_000;
const COUNT_FONT: &str = "bench-count";

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every operation delegates unchanged to `System`; the relaxed atomics
// only observe successful operations while the single benchmark thread measures.
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
            self.frees.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller supplies the allocation's original pointer and layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
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
    allocated: AllocSnapshot,
    checksum: u64,
    retained_layouts: u64,
}

fn measure(mut setup: impl FnMut() -> (u64, u32)) -> BenchResult {
    for _ in 0..WARMUPS {
        black_box(setup());
    }
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut retained_layouts = 0u64;
    for iteration in 0..ITERATIONS {
        let (semantic, layouts) = black_box(setup());
        checksum ^= semantic.rotate_left(iteration as u32);
        retained_layouts += u64::from(layouts);
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
        retained_layouts,
    }
}

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");
    let old_ns = old.elapsed.as_secs_f64() * 1.0e9 / ITERATIONS as f64;
    let new_ns = new.elapsed.as_secs_f64() * 1.0e9 / ITERATIONS as f64;
    let old_cycles = old.cycles as f64 / ITERATIONS as f64;
    let new_cycles = new.cycles as f64 / ITERATIONS as f64;
    println!("{name}");
    print_result("old dense layouts", old);
    print_result("prepared slots", new);
    println!(
        "  improvement       {:>7.2}x throughput  {:>7.1}% cycles  {:>7.1}% bytes  {:>7.1}% layouts",
        old_ns / new_ns,
        100.0 * (1.0 - new_cycles / old_cycles),
        100.0 * (1.0 - new.allocated.bytes as f64 / old.allocated.bytes as f64),
        100.0 * (1.0 - new.retained_layouts as f64 / old.retained_layouts as f64),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let iterations = ITERATIONS as f64;
    let ns = result.elapsed.as_secs_f64() * 1.0e9 / iterations;
    let churn = result.allocated.allocs + result.allocated.reallocs + result.allocated.frees;
    println!(
        "  {label:<17} {ns:>11.1} ns  {:>11.1} cycles  {:>8.2} Ksetup/s  \
         {:>8.1} churn  {:>10.1} B  {:>7.1} layouts",
        result.cycles as f64 / iterations,
        1.0e6 / ns,
        churn as f64 / iterations,
        result.allocated.bytes as f64 / iterations,
        result.retained_layouts as f64 / iterations,
    );
}

fn benchmark_fonts() -> FontMap {
    let texture_key = Arc::<str>::from("numeric-benchmark-font");
    let mut glyph_map = GlyphMap::default();
    let mut ascii = std::array::from_fn(|_| None);
    for byte in b".%:0123456789" {
        let glyph = Glyph {
            texture_key: Arc::clone(&texture_key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 12.0],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            size: [8.0, 12.0],
            offset: [0.0, 0.0],
            advance: 8.0,
            advance_i32: 8,
        };
        glyph_map.insert(char::from(*byte), glyph.clone());
        ascii[*byte as usize] = Some(glyph);
    }
    let font = Font {
        glyph_map,
        ascii_glyphs: Box::new(ascii),
        default_glyph: None,
        line_spacing: 12,
        height: 12,
        fallback_font_name: None,
        cache_tag: 1,
        chain_key: 1,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    };
    let mut fonts = FontMap::default();
    fonts.insert("miso", font.clone());
    fonts.insert(COUNT_FONT, font);
    fonts
}

fn measure_life(fonts: &FontMap, optimized: bool) -> BenchResult {
    let mut scratch = ComposeScratch::default();
    measure(|| benchmark_life_text_setup(fonts, &mut scratch, optimized))
}

fn measure_counts(fonts: &FontMap, optimized: bool) -> BenchResult {
    let mut scratch = ComposeScratch::default();
    measure(|| benchmark_count_text_setup(fonts, &mut scratch, COUNT_FONT, optimized))
}

fn measure_clocks(fonts: &FontMap, optimized: bool) -> BenchResult {
    let mut scratch = ComposeScratch::default();
    measure(|| benchmark_clock_text_setup(fonts, &mut scratch, optimized))
}

fn verify_hot_frames() {
    let life = GameplayLifeTextHotBenchmark::new();
    let stats = GameplayStatsNumericHotBenchmark::new();
    for frame in 0..10_000 {
        black_box(life.frame(frame * 2) ^ life.frame(frame * 2 + 1) ^ stats.frame(frame));
    }

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for frame in 0..HOT_FRAMES {
        checksum = checksum.wrapping_add(black_box(
            life.frame(frame * 2) ^ life.frame(frame * 2 + 1) ^ stats.frame(frame),
        ));
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    assert_eq!(allocated.allocs, 0, "hot numeric HUD frames allocated");
    assert_eq!(allocated.reallocs, 0, "hot numeric HUD frames reallocated");
    assert_eq!(allocated.frees, 0, "hot numeric HUD frames freed");
    assert_eq!(allocated.bytes, 0, "hot numeric HUD frames allocated bytes");
    let ns = elapsed.as_secs_f64() * 1.0e9 / f64::from(HOT_FRAMES);
    println!("prewarmed live numeric HUD (2 life, 4 clocks, 14 counts)");
    println!(
        "  prepared slots    {ns:>11.1} ns  {:>11.1} cycles  {:>8.2} Mframe/s  \
         0 alloc/realloc/free  0 B  {checksum:016x}",
        cycles as f64 / f64::from(HOT_FRAMES),
        1_000.0 / ns,
    );
}

fn main() {
    let fonts = benchmark_fonts();
    println!("gameplay numeric text transition setup ({ITERATIONS} iterations)");
    print_pair(
        "life percent 0.0..100.0",
        &measure_life(&fonts, false),
        &measure_life(&fonts, true),
    );
    print_pair(
        "judgment counts 0..2048",
        &measure_counts(&fonts, false),
        &measure_counts(&fonts, true),
    );
    print_pair(
        "game clock 0..600s",
        &measure_clocks(&fonts, false),
        &measure_clocks(&fonts, true),
    );
    verify_hot_frames();
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
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
