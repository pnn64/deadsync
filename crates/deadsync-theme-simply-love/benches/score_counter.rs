use deadlib_present::actors::{Actor, TextAlign, TextContent};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch};
use deadlib_present::dsl::TextBuilder;
use deadlib_present::font::{Font, FontMap, Glyph};
use deadlib_present::space::Metrics;
use deadsync_theme_simply_love::screens::components::gameplay::score_counter::{
    ScoreCounterParams, prewarm_score_counter_layout, push_score_counter,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const COUNTERS: usize = 4;
const WARMUP_FRAMES: usize = 64;
const MEASURE_FRAMES: usize = 20_000;

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

// SAFETY: every operation delegates to `System` with the original allocation
// layout and only observes successful calls through independent atomics.
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
        // SAFETY: the caller guarantees `ptr` and `layout` identify a live
        // allocation made by this allocator, which delegates to `System`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live
        // allocation; all allocation operations delegate to `System`.
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

struct BenchResult {
    elapsed: Duration,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn main() {
    let fonts = benchmark_fonts();
    let full = run_full(&fonts);
    let glyphs = run_glyphs(&fonts);
    assert_eq!(full.checksum, glyphs.checksum);

    println!("gameplay score text composition");
    println!("{COUNTERS} counters above the prior 8,192-entry source-cache range");
    print_result("full score strings", &full);
    print_result("cached glyph actors", &glyphs);
    println!(
        "cached glyphs: {:.2}x throughput, {:.1}% fewer allocations",
        full.elapsed.as_secs_f64() / glyphs.elapsed.as_secs_f64(),
        percent_reduction(full.alloc.allocs, glyphs.alloc.allocs),
    );
}

fn run_full(fonts: &FontMap) -> BenchResult {
    let mut cache = TextLayoutCache::new(1);
    cache.prewarm_text(fonts, "numbers", "0.00", None);
    cache.lock_growth();
    run(fonts, cache, full_string_frame)
}

fn run_glyphs(fonts: &FontMap) -> BenchResult {
    let mut cache = TextLayoutCache::new(11);
    prewarm_score_counter_layout(&mut cache, fonts, "numbers");
    cache.lock_growth();
    run(fonts, cache, glyph_frame)
}

type FrameFn = fn(
    usize,
    &mut Vec<Actor>,
    &mut TextLayoutCache,
    &mut ComposeScratch,
    &Metrics,
    &FontMap,
) -> usize;

fn run(fonts: &FontMap, mut cache: TextLayoutCache, frame_fn: FrameFn) -> BenchResult {
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };
    let mut actors = Vec::with_capacity(COUNTERS * 6);
    let mut scratch = ComposeScratch::default();

    for frame in 0..WARMUP_FRAMES {
        black_box(frame_fn(
            frame,
            &mut actors,
            &mut cache,
            &mut scratch,
            &metrics,
            fonts,
        ));
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame_fn(
            frame,
            &mut actors,
            &mut cache,
            &mut scratch,
            &metrics,
            fonts,
        )));
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn full_string_frame(
    frame: usize,
    actors: &mut Vec<Actor>,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &FontMap,
) -> usize {
    actors.clear();
    for counter in 0..COUNTERS {
        let centi = 8_200 + ((frame * COUNTERS + counter) % 1_801) as u32;
        let text: Arc<str> = Arc::from(format!("{:.2}", centi as f64 / 100.0));
        let mut actor = TextBuilder::new();
        actor.font("numbers");
        actor.settext(TextContent::Shared(text));
        actor.align(1.0, 1.0);
        actor.horizalign(TextAlign::Right);
        actor.xy(320.0, 240.0);
        actor.zoom(0.25);
        actors.push(actor.build(0));
    }
    compose_frame(actors, cache, scratch, metrics, fonts)
}

fn glyph_frame(
    frame: usize,
    actors: &mut Vec<Actor>,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &FontMap,
) -> usize {
    actors.clear();
    for counter in 0..COUNTERS {
        let centi = 8_200 + ((frame * COUNTERS + counter) % 1_801) as u32;
        push_score_counter(
            actors,
            fonts,
            ScoreCounterParams {
                value: centi as f64 / 100.0,
                font: "numbers",
                position: [320.0, 240.0],
                align: [1.0, 1.0],
                text_align: TextAlign::Right,
                zoom: 0.25,
                color: [1.0; 4],
                z: 0,
            },
        );
    }
    compose_frame(actors, cache, scratch, metrics, fonts)
}

fn compose_frame(
    actors: &[Actor],
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &FontMap,
) -> usize {
    let mut render = build_screen_cached_with_scratch(
        actors,
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        fonts,
        0.0,
        cache,
        scratch,
    );
    let checksum = render
        .objects
        .iter()
        .map(|object| match &object.object_type {
            deadlib_render::ObjectType::TexturedMesh { vertices, .. } => vertices.len(),
            _ => 0,
        })
        .sum();
    scratch.recycle_render_list(&mut render);
    checksum
}

fn benchmark_fonts() -> FontMap {
    let texture: Arc<str> = Arc::from("score-counter-bench");
    let mut glyph_map = HashMap::new();
    for ch in "0123456789.".chars() {
        let advance = if matches!(ch, '1' | '.') { 16 } else { 38 };
        glyph_map.insert(
            ch,
            Glyph {
                texture_key: Arc::clone(&texture),
                stroke_texture_key: None,
                tex_rect: [0.0, 0.0, 1.0, 1.0],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                size: [advance as f32, 48.0],
                offset: [0.0, -42.0],
                advance: advance as f32,
                advance_i32: advance,
            },
        );
    }
    let ascii_glyphs = Box::new(std::array::from_fn(|index| {
        char::from_u32(index as u32).and_then(|ch| glyph_map.get(&ch).cloned())
    }));
    let font = Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: None,
        line_spacing: 48,
        height: 48,
        fallback_font_name: None,
        cache_tag: 1,
        chain_key: 1,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    };
    FontMap::from_iter([("numbers", font)])
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{name:<20} {:>9.2} us/frame  {:>8.1} allocs/frame  {:>9.1} KiB/frame  \
         {:>5.1} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.alloc.allocs as f64 / frames,
        result.alloc.bytes as f64 / frames / 1024.0,
        result.alloc.reallocs as f64 / frames,
    );
}

fn percent_reduction(before: u64, after: u64) -> f64 {
    if before == 0 {
        return 0.0;
    }
    (1.0 - after as f64 / before as f64) * 100.0
}
