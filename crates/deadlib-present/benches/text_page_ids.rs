use deadlib_present::actors::{Actor, TextAlign, TextAttribute, TextContent};
use deadlib_present::compose::{
    ComposeScratch, TextLayoutCache, benchmark_text_layout_type_sizes,
    build_screen_cached_with_scratch_and_texture_context,
};
use deadlib_present::font::{Font, Glyph};
use deadlib_present::space::Metrics;
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::{BlendMode, ObjectType, TexturedMeshVertices};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PAGE_COUNT: usize = 4;
const GLYPH_COUNT: usize = 256;
const WARMUP_FRAMES: usize = 2_048;
const CACHED_FRAMES: usize = 262_144;
const CACHED_BLOCK: usize = 1_024;
const TRANSIENT_FRAMES: usize = 32_768;
const TRANSIENT_BLOCK: usize = 128;
const BENCH_RUNS: usize = 7;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn add_live(&self, bytes: u64) {
        let live = self.live_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        self.peak_live_bytes.fetch_max(live, Ordering::Relaxed);
    }

    fn remove_live(&self, bytes: u64) {
        self.live_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn reset_peak(&self) {
        let live = self.live_bytes.load(Ordering::Relaxed);
        self.peak_live_bytes.store(live, Ordering::Relaxed);
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
            peak_live_bytes: self.peak_live_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller's original
// pointer and layout. Independent atomics only observe allocation churn.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            let bytes = layout.size() as u64;
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes.fetch_add(bytes, Ordering::Relaxed);
            self.add_live(bytes);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let bytes = layout.size() as u64;
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        self.freed_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.remove_live(bytes);
        // SAFETY: the caller guarantees this is the live allocation's layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size >= old.size() {
                let growth = (new_size - old.size()) as u64;
                self.allocated_bytes.fetch_add(growth, Ordering::Relaxed);
                self.add_live(growth);
            } else {
                let shrink = (old.size() - new_size) as u64;
                self.freed_bytes.fetch_add(shrink, Ordering::Relaxed);
                self.remove_live(shrink);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> AllocDelta {
        AllocDelta {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
            live_bytes: self.live_bytes as i64 - before.live_bytes as i64,
            peak_growth: self.peak_live_bytes.saturating_sub(before.live_bytes),
        }
    }
}

#[derive(Clone, Copy)]
struct AllocDelta {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
    live_bytes: i64,
    peak_growth: u64,
}

#[derive(Clone, Copy)]
enum TextMode {
    Cached,
    Transient,
}

struct BenchResult {
    elapsed: Duration,
    cycles: Option<u64>,
    alloc: AllocDelta,
    block_ns: Vec<u64>,
    checksum: u64,
    measured_handle_calls: u64,
    refresh_handle_calls: u64,
    frames: usize,
}

struct BenchTextureContext {
    generation: AtomicU64,
    handle_calls: AtomicU64,
}

impl BenchTextureContext {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(1),
            handle_calls: AtomicU64::new(0),
        }
    }

    fn set_generation(&self, generation: u64) {
        self.generation.store(generation, Ordering::Relaxed);
    }

    fn handle_calls(&self) -> u64 {
        self.handle_calls.load(Ordering::Relaxed)
    }
}

impl TextureContext for BenchTextureContext {
    fn texture_registry_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
        None
    }

    fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
        (1, 1)
    }

    fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
        self.handle_calls.fetch_add(1, Ordering::Relaxed);
        page_handle(self.generation.load(Ordering::Relaxed), page_index(key))
    }
}

fn main() {
    let (glyph_size, batch_size, layout_size) = benchmark_text_layout_type_sizes();
    let _ = cold_layout_memory();
    let cold = cold_layout_memory();
    let mut cached_runs = Vec::with_capacity(BENCH_RUNS);
    let mut transient_runs = Vec::with_capacity(BENCH_RUNS);

    for _ in 0..BENCH_RUNS {
        cached_runs.push(run_case(TextMode::Cached, CACHED_FRAMES, CACHED_BLOCK));
        transient_runs.push(run_case(
            TextMode::Transient,
            TRANSIENT_FRAMES,
            TRANSIENT_BLOCK,
        ));
    }
    assert_same_checksums(&cached_runs);
    assert_same_checksums(&transient_runs);
    let cached = median_result(cached_runs);
    let transient = median_result(transient_runs);

    println!("cached/transient multi-page text benchmark");
    println!(
        "{PAGE_COUNT} pages, {GLYPH_COUNT} glyphs/frame, median of {BENCH_RUNS} interleaved runs"
    );
    println!(
        "record sizes: CachedGlyph={glyph_size} B, CachedTextMeshBatch={batch_size} B, \
         CachedTextLayout={layout_size} B"
    );
    println!(
        "cold layout: alloc/realloc={} / {}, requested={} B, retained={} B, peak growth={} B",
        cold.allocs, cold.reallocs, cold.allocated_bytes, cold.live_bytes, cold.peak_growth,
    );
    print_result("cached mesh", &cached);
    print_result("attributed transient", &transient);
}

fn run_case(mode: TextMode, frames: usize, block_frames: usize) -> BenchResult {
    assert_eq!(frames % block_frames, 0);
    let fonts = benchmark_fonts();
    let text: Arc<str> = Arc::from(benchmark_text());
    let actors = [text_actor(Arc::clone(&text), mode)];
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };
    let texture_ctx = BenchTextureContext::new();
    let mut cache = TextLayoutCache::new(4);
    let mut scratch = ComposeScratch::default();

    for frame in 0..WARMUP_FRAMES {
        black_box(compose_frame(
            frame,
            &actors,
            &metrics,
            &fonts,
            &texture_ctx,
            &mut cache,
            &mut scratch,
        ));
    }
    cache.lock_growth();
    assert_eq!(texture_ctx.handle_calls(), PAGE_COUNT as u64);

    let mut block_ns = Vec::with_capacity(frames / block_frames);
    ALLOC.reset_peak();
    let alloc_before = ALLOC.snapshot();
    let calls_before = texture_ctx.handle_calls();
    let cycles_before = thread_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for block_start in (0..frames).step_by(block_frames) {
        let block_started = Instant::now();
        for frame in block_start..block_start + block_frames {
            checksum = checksum.rotate_left(7)
                ^ black_box(compose_frame(
                    frame,
                    &actors,
                    &metrics,
                    &fonts,
                    &texture_ctx,
                    &mut cache,
                    &mut scratch,
                ))
                ^ frame as u64;
        }
        block_ns.push((block_started.elapsed().as_nanos() as u64) / block_frames as u64);
    }
    let elapsed = started.elapsed();
    let cycles = cycles_before
        .zip(thread_cycles())
        .map(|(before, after)| after.saturating_sub(before));
    let alloc = ALLOC.snapshot().delta(alloc_before);
    let measured_handle_calls = texture_ctx.handle_calls() - calls_before;

    assert_eq!(alloc.allocs, 0, "warmed text composition allocated");
    assert_eq!(alloc.reallocs, 0, "warmed text composition reallocated");
    assert_eq!(alloc.deallocs, 0, "warmed text composition freed storage");
    assert_eq!(
        measured_handle_calls, 0,
        "stable generation reloaded handles"
    );

    texture_ctx.set_generation(2);
    let refresh_before = texture_ctx.handle_calls();
    let _ = compose_frame(
        frames,
        &actors,
        &metrics,
        &fonts,
        &texture_ctx,
        &mut cache,
        &mut scratch,
    );
    let refresh_handle_calls = texture_ctx.handle_calls() - refresh_before;
    assert_eq!(refresh_handle_calls, PAGE_COUNT as u64);
    let stable_before = texture_ctx.handle_calls();
    let _ = compose_frame(
        frames + 1,
        &actors,
        &metrics,
        &fonts,
        &texture_ctx,
        &mut cache,
        &mut scratch,
    );
    assert_eq!(texture_ctx.handle_calls(), stable_before);

    BenchResult {
        elapsed,
        cycles,
        alloc,
        block_ns,
        checksum,
        measured_handle_calls,
        refresh_handle_calls,
        frames,
    }
}

fn compose_frame(
    frame: usize,
    actors: &[Actor],
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
    texture_ctx: &BenchTextureContext,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
) -> u64 {
    let mut render = build_screen_cached_with_scratch_and_texture_context(
        black_box(actors),
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        fonts,
        black_box(frame as f32 / 120.0),
        cache,
        scratch,
        texture_ctx,
    );
    assert_handles(&render, texture_ctx.texture_registry_generation());
    let checksum = render_checksum(&render);
    scratch.recycle_render_list(&mut render);
    checksum
}

fn render_checksum(render: &deadlib_render::RenderList) -> u64 {
    let mut checksum = render.objects.len() as u64;
    for object in &render.objects {
        checksum = checksum.rotate_left(5) ^ object.texture_handle;
        checksum ^= (object.z as u16 as u64) << 32;
        let ObjectType::TexturedMesh {
            vertices,
            geom_cache_key,
            ..
        } = &object.object_type
        else {
            continue;
        };
        checksum = checksum.rotate_left(7) ^ vertices.len() as u64;
        checksum ^= u64::from(*geom_cache_key != deadlib_render::INVALID_TMESH_CACHE_KEY) << 63;
        let vertices = vertices.as_ref();
        for vertex in vertices.first().into_iter().chain(vertices.last()) {
            checksum = checksum.rotate_left(11) ^ vertex.pos[0].to_bits() as u64;
            checksum = checksum.rotate_left(11) ^ vertex.pos[1].to_bits() as u64;
            checksum = checksum.rotate_left(11) ^ vertex.uv[0].to_bits() as u64;
            checksum = checksum.rotate_left(11) ^ vertex.color[0].to_bits() as u64;
        }
        checksum ^= match vertices {
            _ if matches!(
                &object.object_type,
                ObjectType::TexturedMesh {
                    vertices: TexturedMeshVertices::Shared(_),
                    ..
                }
            ) =>
            {
                1
            }
            _ => 2,
        };
    }
    checksum
}

fn assert_handles(render: &deadlib_render::RenderList, generation: u64) {
    assert_eq!(render.objects.len(), PAGE_COUNT);
    for (page, object) in render.objects.iter().enumerate() {
        assert_eq!(object.texture_handle, page_handle(generation, page));
    }
}

fn page_handle(generation: u64, page: usize) -> u64 {
    generation * 16 + page as u64 + 1
}

fn page_index(key: &str) -> usize {
    match key.as_bytes().last().copied() {
        Some(b'0'..=b'3') => (key.as_bytes()[key.len() - 1] - b'0') as usize,
        _ => panic!("benchmark texture key must end in a page index: {key}"),
    }
}

fn cold_layout_memory() -> AllocDelta {
    let fonts = benchmark_fonts();
    let text = benchmark_text();
    let mut cache = TextLayoutCache::new(1);
    cache.begin_frame_stats(true);
    ALLOC.reset_peak();
    let before = ALLOC.snapshot();
    cache.prewarm_text(&fonts, "bench", &text, None);
    let after = ALLOC.snapshot();
    let stats = cache.frame_stats();
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.built_glyphs as usize, GLYPH_COUNT);
    after.delta(before)
}

fn benchmark_text() -> String {
    (0..GLYPH_COUNT)
        .map(|index| (b'A' + (index % PAGE_COUNT) as u8) as char)
        .collect()
}

fn benchmark_fonts() -> HashMap<&'static str, Font> {
    let pages: [Arc<str>; PAGE_COUNT] = std::array::from_fn(|page| {
        Arc::from(format!(
            "bench/fonts/multi_page/realistic_texture_page_{page}"
        ))
    });
    let mut glyph_map = HashMap::new();
    let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
    for (page, texture_key) in pages.iter().enumerate() {
        let ch = (b'A' + page as u8) as char;
        let glyph = Glyph {
            texture_key: Arc::clone(texture_key),
            stroke_texture_key: None,
            tex_rect: [0.0, 0.0, 8.0, 10.0],
            uv_scale: [0.125, 0.125],
            uv_offset: [page as f32 * 0.125, 0.0],
            size: [8.0, 10.0],
            offset: [0.0, -10.0],
            advance: 8.0,
            advance_i32: 8,
        };
        glyph_map.insert(ch, glyph.clone());
        ascii_glyphs[ch as usize] = Some(glyph);
    }
    let font = Font {
        glyph_map,
        ascii_glyphs,
        default_glyph: None,
        line_spacing: 10,
        height: 10,
        fallback_font_name: None,
        cache_tag: 0x7465_7874,
        chain_key: 0x7061_6765,
        default_stroke_color: [0.0; 4],
        stroke_texture_map: HashMap::new(),
        texture_hints_map: HashMap::new(),
    };
    HashMap::from([("bench", font)])
}

fn text_actor(content: Arc<str>, mode: TextMode) -> Actor {
    let attributes = match mode {
        TextMode::Cached => Vec::new(),
        TextMode::Transient => vec![TextAttribute {
            start: 0,
            length: GLYPH_COUNT,
            color: [0.25, 0.5, 0.75, 1.0],
            vertex_colors: None,
            glow: None,
        }],
    };
    Actor::Text {
        align: [0.0, 0.0],
        offset: [32.0, 48.0],
        local_transform: Default::default(),
        color: [0.8, 0.9, 1.0, 1.0],
        stroke_color: None,
        glow: [0.0; 4],
        font: "bench",
        content: TextContent::Shared(content),
        attributes,
        align_text: TextAlign::Left,
        z: 3,
        scale: [1.0, 1.0],
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

fn median_result(mut results: Vec<BenchResult>) -> BenchResult {
    results.sort_unstable_by_key(|result| result.elapsed);
    results.swap_remove(results.len() / 2)
}

fn assert_same_checksums(results: &[BenchResult]) {
    let expected = results.first().expect("benchmark run").checksum;
    assert!(
        results.iter().all(|result| result.checksum == expected),
        "benchmark output changed between runs"
    );
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = result.frames as f64;
    let mut samples = result.block_ns.clone();
    samples.sort_unstable();
    let cycles = result
        .cycles
        .map(|cycles| format!("{:>9.0} thread cycles/frame", cycles as f64 / frames))
        .unwrap_or_else(|| "thread cycles unavailable".to_owned());
    println!(
        "{name:<20} {:>9.1} ns/frame  {:>10.0} frames/s  {:>12.0} glyphs/s  {cycles}",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        frames / result.elapsed.as_secs_f64(),
        frames * GLYPH_COUNT as f64 / result.elapsed.as_secs_f64(),
    );
    println!(
        "{:<20} block p50 {:>7} ns  p95 {:>7} ns  p99 {:>7} ns  worst {:>7} ns/frame",
        "latency",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "{:<20} alloc/realloc/free={}/{}/{}  +{} B -{} B  live={:+} B  peak=+{} B  \
         handle calls stable/refresh={}/{}",
        "memory + handles",
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.deallocs,
        result.alloc.allocated_bytes,
        result.alloc.freed_bytes,
        result.alloc.live_bytes,
        result.alloc.peak_growth,
        result.measured_handle_calls,
        result.refresh_handle_calls,
    );
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> isize;
    fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    let mut cycles = 0_u64;
    // SAFETY: `GetCurrentThread` returns a valid pseudo-handle for this thread,
    // and `cycles` remains a valid writable `u64` for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}
