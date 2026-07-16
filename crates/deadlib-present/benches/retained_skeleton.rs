use deadlib_present::actors::{
    Actor, RetainedActorFrame, SizeSpec, SpriteSource, TextAlign, TextContent,
};
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch};
use deadlib_present::font::{Font, Glyph};
use deadlib_present::space::Metrics;
use deadlib_render::BlendMode;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const STATIC_QUADS: usize = 12;
const STATIC_TEXTS: usize = 6;
const DYNAMIC_QUADS: usize = 96;
const WARMUP_FRAMES: usize = 128;
const MEASURE_FRAMES: usize = 20_000;
const BENCH_RUNS: usize = 5;

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
    let mut rebuilt_runs = Vec::with_capacity(BENCH_RUNS);
    let mut retained_runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        rebuilt_runs.push(run_rebuilt(&fonts));
        retained_runs.push(run_retained(&fonts));
    }
    let rebuilt = median_result(rebuilt_runs);
    let retained = median_result(retained_runs);
    assert_eq!(rebuilt.checksum, retained.checksum);

    println!("song-static presentation skeleton");
    println!(
        "{STATIC_QUADS} static quads + {STATIC_TEXTS} static texts + \
         {DYNAMIC_QUADS} dynamic quads/frame"
    );
    println!("median of {BENCH_RUNS} interleaved runs");
    print_result("rebuild + compose", &rebuilt);
    print_result("retained + compose", &retained);
    println!(
        "retained skeleton: {:.2}x throughput, {:.1}% fewer allocations",
        rebuilt.elapsed.as_secs_f64() / retained.elapsed.as_secs_f64(),
        percent_reduction(rebuilt.alloc.allocs, retained.alloc.allocs),
    );
}

fn median_result(mut results: Vec<BenchResult>) -> BenchResult {
    results.sort_unstable_by_key(|result| result.elapsed);
    results.swap_remove(results.len() / 2)
}

fn run_rebuilt(fonts: &HashMap<&'static str, Font>) -> BenchResult {
    let static_text: Arc<str> = Arc::from("STATIC HUD");
    let mut actors = Vec::with_capacity(STATIC_QUADS + STATIC_TEXTS + DYNAMIC_QUADS);
    let mut cache = TextLayoutCache::new(8);
    let mut scratch = ComposeScratch::default();
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };

    for frame in 0..WARMUP_FRAMES {
        black_box(rebuilt_frame(
            frame,
            &static_text,
            &mut actors,
            &mut cache,
            &mut scratch,
            &metrics,
            fonts,
        ));
    }
    cache.lock_growth();
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(rebuilt_frame(
            frame,
            &static_text,
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

fn run_retained(fonts: &HashMap<&'static str, Font>) -> BenchResult {
    let static_text: Arc<str> = Arc::from("STATIC HUD");
    let mut static_actors = Vec::with_capacity(STATIC_QUADS + STATIC_TEXTS);
    push_static_actors(&mut static_actors, &static_text);
    let frame = Arc::new(RetainedActorFrame::new(static_actors));
    let mut actors = Vec::with_capacity(1 + DYNAMIC_QUADS);
    let mut cache = TextLayoutCache::new(8);
    let mut scratch = ComposeScratch::default();
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };

    for frame_index in 0..WARMUP_FRAMES {
        black_box(retained_frame(
            frame_index,
            &frame,
            &mut actors,
            &mut cache,
            &mut scratch,
            &metrics,
            fonts,
        ));
    }
    cache.lock_growth();
    scratch.reset_retained_frame_stats();
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for frame_index in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(retained_frame(
            frame_index,
            &frame,
            &mut actors,
            &mut cache,
            &mut scratch,
            &metrics,
            fonts,
        )));
    }
    let stats = scratch.retained_frame_stats();
    assert_eq!(stats.hits as usize, MEASURE_FRAMES);
    assert_eq!(stats.misses, 0);
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn rebuilt_frame(
    frame: usize,
    static_text: &Arc<str>,
    actors: &mut Vec<Actor>,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
) -> usize {
    actors.clear();
    push_static_actors(actors, static_text);
    push_dynamic_actors(actors, frame);
    compose_frame(actors, cache, scratch, metrics, fonts)
}

fn retained_frame(
    frame_index: usize,
    frame: &Arc<RetainedActorFrame>,
    actors: &mut Vec<Actor>,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
) -> usize {
    actors.clear();
    actors.push(Actor::RetainedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        frame: Arc::clone(frame),
        z: 0,
        tint: [1.0; 4],
        blend: None,
        visible: true,
    });
    push_dynamic_actors(actors, frame_index);
    compose_frame(actors, cache, scratch, metrics, fonts)
}

fn push_static_actors(actors: &mut Vec<Actor>, text: &Arc<str>) {
    for index in 0..STATIC_QUADS {
        actors.push(quad_actor(
            [
                16.0 + (index % 16) as f32 * 39.0,
                16.0 + (index / 16) as f32 * 23.0,
            ],
            [0.12, 0.16, 0.2, 1.0],
            80 + (index % 8) as i16,
        ));
    }
    for index in 0..STATIC_TEXTS {
        actors.push(text_actor(
            Arc::clone(text),
            [
                20.0 + (index % 4) as f32 * 150.0,
                180.0 + (index / 4) as f32 * 28.0,
            ],
            90,
        ));
    }
}

fn push_dynamic_actors(actors: &mut Vec<Actor>, frame: usize) {
    for index in 0..DYNAMIC_QUADS {
        let phase = ((frame + index * 17) % 1_000) as f32 / 1_000.0;
        actors.push(quad_actor(
            [12.0 + index as f32 * 13.0, 320.0 + phase * 120.0],
            [phase, 0.4, 1.0 - phase, 1.0],
            100,
        ));
    }
}

fn compose_frame(
    actors: &[Actor],
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
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
    let checksum = render.objects.len().wrapping_add(
        render
            .sprite_instances
            .iter()
            .map(|instance| instance.center[0].to_bits() as usize)
            .fold(0usize, usize::wrapping_add),
    );
    scratch.recycle_render_list(&mut render);
    checksum
}

fn quad_actor(offset: [f32; 2], tint: [f32; 4], z: i16) -> Actor {
    Actor::Sprite {
        align: [0.5, 0.5],
        offset,
        world_z: 0.0,
        size: [SizeSpec::Px(34.0), SizeSpec::Px(18.0)],
        source: SpriteSource::TextureStatic("__white"),
        tint,
        glow: [0.0; 4],
        z,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0; 4],
        effect: Default::default(),
    }
}

fn text_actor(content: Arc<str>, offset: [f32; 2], z: i16) -> Actor {
    Actor::Text {
        align: [0.0, 0.0],
        offset,
        local_transform: Default::default(),
        color: [1.0; 4],
        stroke_color: None,
        glow: [0.0; 4],
        font: "bench",
        content: TextContent::Shared(content),
        attributes: Vec::new(),
        align_text: TextAlign::Left,
        z,
        scale: [0.5, 0.5],
        fit_width: None,
        fit_height: None,
        line_spacing: None,
        wrap_width_pixels: None,
        max_width: None,
        max_height: None,
        max_w_pre_zoom: false,
        max_h_pre_zoom: false,
        jitter: false,
        distortion: 0.0,
        clip: None,
        mask_dest: false,
        blend: BlendMode::Alpha,
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0; 4],
        effect: Default::default(),
    }
}

fn benchmark_fonts() -> HashMap<&'static str, Font> {
    let texture: Arc<str> = Arc::from("retained-skeleton-bench");
    let mut glyph_map = HashMap::new();
    for ch in "STATIC HUD".chars() {
        glyph_map.entry(ch).or_insert_with(|| Glyph {
            texture_key: Arc::clone(&texture),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 1.0, 1.0],
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            size: [24.0, 32.0],
            offset: [0.0, -28.0],
            advance: 24.0,
            advance_i32: 24,
        });
    }
    let ascii_glyphs = Box::new(std::array::from_fn(|index| {
        char::from_u32(index as u32).and_then(|ch| glyph_map.get(&ch).cloned())
    }));
    let font = Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: None,
        line_spacing: 32,
        height: 32,
        fallback_font_name: None,
        cache_tag: 1,
        chain_key: 1,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    };
    HashMap::from([("bench", font)])
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{name:<20} {:>9.2} us/frame  {:>7.1} allocs/frame  {:>9.1} KiB/frame  \
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
