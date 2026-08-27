use deadlib_present::compose::ComposeScratch;
use deadlib_present::font::{Font, FontMap, Glyph, GlyphMap};
use deadsync_theme_simply_love::screens::components::gameplay::gameplay_stats::GameplayHudMemoBenchmark;
use deadsync_theme_simply_love::screens::gameplay::{
    GameplayBpmTextBenchmark, benchmark_bpm_text_setup,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const SETUP_WARMUPS: usize = 3;
const SETUP_ITERATIONS: usize = 50;
const LIVE_WARMUP_FRAMES: u32 = 10_000;
const LIVE_FRAMES: u32 = 1_000_000;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed atomics
// only observe successful operations on the single benchmark thread.
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
        // SAFETY: the caller supplies the original allocation pair.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
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

    const fn churn(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
    items: u64,
    retained_layouts: u64,
}

fn measure_setup(mut setup: impl FnMut() -> (u64, u32)) -> BenchResult {
    for _ in 0..SETUP_WARMUPS {
        black_box(setup());
    }
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut retained_layouts = 0u64;
    for iteration in 0..SETUP_ITERATIONS {
        let (semantic, layouts) = black_box(setup());
        checksum ^= semantic.rotate_left(iteration as u32);
        retained_layouts += u64::from(layouts);
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    ALLOC.enabled.store(false, Ordering::Relaxed);
    BenchResult {
        elapsed,
        cycles,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
        items: SETUP_ITERATIONS as u64,
        retained_layouts,
    }
}

fn measure_live(mut frame: impl FnMut(u32) -> u64) -> BenchResult {
    for index in 0..LIVE_WARMUP_FRAMES {
        black_box(frame(index));
    }
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for index in LIVE_WARMUP_FRAMES..LIVE_WARMUP_FRAMES + LIVE_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame(index)));
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    ALLOC.enabled.store(false, Ordering::Relaxed);
    BenchResult {
        elapsed,
        cycles,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
        items: u64::from(LIVE_FRAMES),
        retained_layouts: 0,
    }
}

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");
    let old_ns = old.elapsed.as_secs_f64() * 1.0e9 / old.items as f64;
    let new_ns = new.elapsed.as_secs_f64() * 1.0e9 / new.items as f64;
    let old_cycles = old.cycles as f64 / old.items as f64;
    let new_cycles = new.cycles as f64 / new.items as f64;
    println!("{name}");
    print_result("old shared memo", old);
    print_result("direct actor slot", new);
    println!(
        "  improvement       {:>7.2}x throughput  {:>7.1}% cycles  {:>7.1}% churn  {:>7.1}% bytes",
        old_ns / new_ns,
        100.0 * (1.0 - new_cycles / old_cycles),
        percent_reduction(old.allocated.churn(), new.allocated.churn()),
        percent_reduction(old.allocated.bytes, new.allocated.bytes),
    );
}

fn print_live_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(new.allocated.allocs, 0, "{name} optimized path allocated");
    assert_eq!(
        new.allocated.reallocs, 0,
        "{name} optimized path reallocated"
    );
    assert_eq!(new.allocated.frees, 0, "{name} optimized path freed");
    assert_eq!(
        new.allocated.bytes, 0,
        "{name} optimized path allocated bytes"
    );
    print_pair(name, old, new);
}

fn print_result(label: &str, result: &BenchResult) {
    let items = result.items as f64;
    let ns = result.elapsed.as_secs_f64() * 1.0e9 / items;
    println!(
        "  {label:<17} {ns:>10.2} ns  {:>10.2} cycles  {:>8.2} Mitem/s  \
         {:>8.2} churn  {:>9.1} B  {:>7.1} layouts",
        result.cycles as f64 / items,
        1_000.0 / ns,
        result.allocated.churn() as f64 / items,
        result.allocated.bytes as f64 / items,
        result.retained_layouts as f64 / items,
    );
}

fn percent_reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

fn benchmark_fonts() -> FontMap {
    let texture_key = Arc::<str>::from("numeric-memo-benchmark-font");
    let mut glyph_map = GlyphMap::default();
    let mut ascii = std::array::from_fn(|_| None);
    for byte in b".0123456789" {
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
    fonts.insert("miso", font);
    fonts
}

fn main() {
    let fonts = benchmark_fonts();
    let mut old_scratch = ComposeScratch::default();
    let mut new_scratch = ComposeScratch::default();
    println!("second-pass gameplay numeric text benchmark");
    print_pair(
        "BPM transition (2048 values)",
        &measure_setup(|| benchmark_bpm_text_setup(&fonts, &mut old_scratch, false)),
        &measure_setup(|| benchmark_bpm_text_setup(&fonts, &mut new_scratch, true)),
    );

    let mut old_bpm = GameplayBpmTextBenchmark::new();
    let mut new_bpm = GameplayBpmTextBenchmark::new();
    print_live_pair(
        "BPM live actor",
        &measure_live(|frame| old_bpm.legacy_frame(frame)),
        &measure_live(|frame| new_bpm.optimized_frame(frame)),
    );

    let mut old_counts = GameplayHudMemoBenchmark::new();
    let mut new_counts = GameplayHudMemoBenchmark::new();
    old_counts.warm(0);
    new_counts.warm(0);
    print_live_pair(
        "disabled counts (7 rows)",
        &measure_live(|frame| old_counts.legacy_count_frame(frame)),
        &measure_live(|frame| new_counts.optimized_count_frame(frame)),
    );

    let mut old_widths = GameplayHudMemoBenchmark::new();
    let mut new_widths = GameplayHudMemoBenchmark::new();
    old_widths.warm(0);
    new_widths.warm(0);
    print_live_pair(
        "clock widths (3 reads)",
        &measure_live(|frame| old_widths.legacy_width_frame(frame)),
        &measure_live(|frame| new_widths.optimized_width_frame(frame)),
    );
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
