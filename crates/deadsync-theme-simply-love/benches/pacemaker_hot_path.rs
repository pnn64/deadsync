use deadlib_present::actors::{Actor, InlineText, TextAlign, TextContent};
use deadlib_present::compose::{
    ComposeScratch, TextLayoutCache, build_screen_cached_with_scratch_and_texture_context,
    prewarm_frame_inline_text_slot, prewarm_prepared_inline_text_slot,
};
use deadlib_present::dsl::TextBuilder;
use deadlib_present::font::{Font, FontMap, Glyph};
use deadlib_present::space::Metrics;
use deadlib_present::texture::{TextureContext, TextureMeta};
use deadlib_render::{DrawOp, TexturedMeshUploads, TexturedMeshVertices, resolve_textured_meshes};
use deadsync_theme_simply_love::screens::components::gameplay::notefield::{
    PacemakerFrameBench, benchmark_pacemaker_text, benchmark_pacemaker_text_legacy,
    reset_mini_text_benchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_OPS: usize = 20_000;
const FRAME_OPS: usize = 500_000;
const SONG_TEXT_OPS: usize = 24_000;
const DYNAMIC_LAYOUT_FRAMES: usize = 24_000;
const SAMPLE_BATCH: usize = 256;

#[derive(Clone, Copy)]
enum DynamicTextWarmup {
    FrameSlots,
    U16Domain(u16),
}

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
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

// SAFETY: every operation delegates to `System` with the original allocator
// arguments; relaxed atomics only observe allocation churn.
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
        self.frees.fetch_add(1, Ordering::Relaxed);
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
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
    max_batch: Duration,
    allocated: AllocSnapshot,
    checksum: usize,
}

fn measure(ops: usize, warmup: bool, mut operation: impl FnMut(usize) -> usize) -> BenchResult {
    if warmup {
        for frame in 0..WARMUP_OPS {
            black_box(operation(frame));
        }
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut max_batch = Duration::ZERO;
    let mut checksum = 0usize;
    for batch_start in (0..ops).step_by(SAMPLE_BATCH) {
        let batch_started = Instant::now();
        for frame in batch_start..(batch_start + SAMPLE_BATCH).min(ops) {
            checksum = checksum.rotate_left(7) ^ black_box(operation(frame));
        }
        max_batch = max_batch.max(batch_started.elapsed());
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        max_batch,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn text_checksum(text: TextContent) -> usize {
    text.as_str().bytes().fold(0usize, |checksum, byte| {
        checksum.rotate_left(5) ^ byte as usize
    })
}

fn pacemaker_text_input(frame: usize) -> (f64, bool) {
    (((frame * 37) % 10_001) as f64 * 0.01, frame & 1 == 0)
}

fn smx_sensor_value(frame: usize, actor: usize) -> u16 {
    ((frame / 2 * 17 + actor * 61) % 501) as u16
}

fn measure_counter_value(frame: usize, actor: usize) -> InlineText {
    let update = frame / 30;
    let mut text = InlineText::new();
    if actor == 5 {
        assert!(text.push_ascii(b'('));
        assert!(text.push_i32((update % 64 + 1) as i32));
        assert!(text.push_ascii(b')'));
    } else {
        let total = (update % 64 + 1) as i32;
        let current = ((update + actor) % total as usize + 1) as i32;
        assert!(text.push_i32(current));
        assert!(text.push_ascii(b'/'));
        assert!(text.push_i32(total));
    }
    text
}

fn run_timer_value(frame: usize, _: usize) -> InlineText {
    let seconds = (frame / 60) as u32;
    let mut text = InlineText::new();
    if seconds < 10 {
        assert!(text.push_ascii(b'0'));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_ascii(b'0'));
        assert!(text.push_u32(seconds));
    } else if seconds > 60 {
        assert!(text.push_u32(seconds / 60));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_ascii(b'0' + ((seconds % 60) / 10) as u8));
        assert!(text.push_ascii(b'0' + (seconds % 10) as u8));
    } else {
        assert!(text.push_ascii(b'0'));
        assert!(text.push_ascii(b'.'));
        assert!(text.push_u32(seconds));
    }
    if frame / 600 % 2 == 1 {
        assert!(text.push_ascii(b' '));
    }
    text
}

fn print_result(label: &str, result: &BenchResult, ops: usize) {
    let ns_per_op = result.elapsed.as_secs_f64() * 1e9 / ops as f64;
    let cycles_per_op = result.cycles as f64 / ops as f64;
    let throughput = ops as f64 / result.elapsed.as_secs_f64();
    println!(
        "{label:22} {:9.2} ns/op  {:9.2} cycles/op  {:10.0} ops/s  \
         max {max_us:8.2} us/{SAMPLE_BATCH}  alloc {allocs}  realloc {reallocs}  \
         free {frees}  bytes {bytes}",
        ns_per_op,
        cycles_per_op,
        throughput,
        max_us = result.max_batch.as_secs_f64() * 1e6,
        allocs = result.allocated.allocs,
        reallocs = result.allocated.reallocs,
        frees = result.allocated.frees,
        bytes = result.allocated.bytes,
    );
}

fn main() {
    reset_mini_text_benchmark();
    let legacy_text = measure(SONG_TEXT_OPS, false, |frame| {
        let (value, negative) = pacemaker_text_input(frame);
        text_checksum(benchmark_pacemaker_text_legacy(value, negative))
    });
    let inline_text = measure(SONG_TEXT_OPS, false, |frame| {
        let (value, negative) = pacemaker_text_input(frame);
        text_checksum(benchmark_pacemaker_text(value, negative))
    });
    assert_eq!(legacy_text.checksum, inline_text.checksum);

    println!("pacemaker text song workload ({SONG_TEXT_OPS} changing frames)");
    print_result("bounded hash cache", &legacy_text, SONG_TEXT_OPS);
    print_result("inline fixed buffer", &inline_text, SONG_TEXT_OPS);

    let fonts = numeric_font();
    let metrics = Metrics {
        left: 0.0,
        right: 640.0,
        top: 480.0,
        bottom: 0.0,
    };
    let texture_ctx = BenchTextureContext;
    let mut legacy_cache = TextLayoutCache::new(4_097);
    let mut legacy_scratch = ComposeScratch::default();
    let mut legacy_uploads = TexturedMeshUploads::default();
    let mut legacy_actors = vec![text_actor(TextContent::Static("+0.00%"))];
    warm_compose(
        &mut legacy_actors,
        &metrics,
        &fonts,
        &mut legacy_cache,
        &mut legacy_scratch,
        &mut legacy_uploads,
        &texture_ctx,
    );
    legacy_cache.lock_growth_with_reserve(4_096);

    let mut frame_cache = TextLayoutCache::new(1);
    let mut frame_scratch = ComposeScratch::default();
    let mut frame_uploads = TexturedMeshUploads::default();
    let mut frame_actors = vec![text_actor(benchmark_pacemaker_text(100.0, false))];
    let glyph_domain =
        InlineText::copy_from("+-.%0123456789").expect("benchmark glyph domain fits inline");

    let mut transient_cache = TextLayoutCache::new(1);
    let mut transient_scratch = ComposeScratch::default();
    let mut transient_uploads = TexturedMeshUploads::default();
    let mut transient_actors = vec![text_actor(TextContent::frame_inline(glyph_domain))];
    prewarm_frame_inline_text_slot(
        &mut transient_cache,
        &mut transient_scratch,
        &fonts,
        "test",
        glyph_domain,
        0,
        4,
    );
    warm_compose(
        &mut transient_actors,
        &metrics,
        &fonts,
        &mut transient_cache,
        &mut transient_scratch,
        &mut transient_uploads,
        &texture_ctx,
    );
    transient_cache.lock_growth();

    prewarm_prepared_inline_text_slot(
        &mut frame_cache,
        &mut frame_scratch,
        &fonts,
        "test",
        glyph_domain,
        0,
        TextAlign::Center,
        4,
    );
    warm_compose(
        &mut frame_actors,
        &metrics,
        &fonts,
        &mut frame_cache,
        &mut frame_scratch,
        &mut frame_uploads,
        &texture_ctx,
    );

    let legacy_compose = measure(SONG_TEXT_OPS, false, |frame| {
        let (value, negative) = pacemaker_text_input(frame);
        let current = benchmark_pacemaker_text(value, negative);
        let inline = InlineText::copy_from(current.as_str()).expect("pacemaker text fits inline");
        compose_text(
            &mut legacy_actors,
            TextContent::Inline(inline),
            &metrics,
            &fonts,
            &mut legacy_cache,
            &mut legacy_scratch,
            &mut legacy_uploads,
            &texture_ctx,
        )
    });
    let transient_compose = measure(SONG_TEXT_OPS, false, |frame| {
        let (value, negative) = pacemaker_text_input(frame);
        compose_text(
            &mut transient_actors,
            benchmark_pacemaker_text(value, negative),
            &metrics,
            &fonts,
            &mut transient_cache,
            &mut transient_scratch,
            &mut transient_uploads,
            &texture_ctx,
        )
    });
    let frame_compose = measure(SONG_TEXT_OPS, false, |frame| {
        let (value, negative) = pacemaker_text_input(frame);
        compose_text(
            &mut frame_actors,
            benchmark_pacemaker_text(value, negative),
            &metrics,
            &fonts,
            &mut frame_cache,
            &mut frame_scratch,
            &mut frame_uploads,
            &texture_ctx,
        )
    });
    assert_eq!(legacy_compose.checksum, frame_compose.checksum);
    assert_eq!(transient_compose.checksum, frame_compose.checksum);
    assert_eq!(transient_compose.allocated.allocs, 0);
    assert_eq!(transient_compose.allocated.reallocs, 0);
    assert_eq!(transient_compose.allocated.frees, 0);

    println!("\npacemaker actor-to-upload ({SONG_TEXT_OPS} changing frames)");
    print_result("whole-string cache", &legacy_compose, SONG_TEXT_OPS);
    print_result("transient slots", &transient_compose, SONG_TEXT_OPS);
    print_result("direct frame slots", &frame_compose, SONG_TEXT_OPS);

    run_dynamic_layout_workload(
        "offset-ms actor-to-upload",
        1,
        |frame, _| {
            let centi = (frame / 6 % 36_001) as i32 - 18_000;
            InlineText::format(format_args!("{:.2}ms", centi as f64 * 0.01))
                .expect("offset benchmark text fits inline")
        },
        InlineText::copy_from("-.ms0123456789").expect("offset glyph domain fits inline"),
        DynamicTextWarmup::FrameSlots,
        false,
        None,
        &metrics,
        &fonts,
        &texture_ctx,
    );
    run_dynamic_layout_workload(
        "six live-timing pairs actor-to-upload",
        6,
        |frame, actor| {
            let update = frame / 6;
            let recent = (update.wrapping_mul(17 + actor) % 4_001) as f32 * 0.1 - 200.0;
            let all = (update.wrapping_mul(29 + actor) % 4_001) as f32 * 0.1 - 200.0;
            InlineText::format(format_args!("{recent:.1}/{all:.1}"))
                .expect("timing benchmark text fits inline")
        },
        InlineText::copy_from("-./0123456789").expect("timing glyph domain fits inline"),
        DynamicTextWarmup::FrameSlots,
        false,
        None,
        &metrics,
        &fonts,
        &texture_ctx,
    );
    run_dynamic_layout_workload(
        "six measure counters actor-to-upload",
        6,
        measure_counter_value,
        InlineText::copy_from("-/()0123456789").expect("counter glyph domain fits inline"),
        DynamicTextWarmup::FrameSlots,
        true,
        None,
        &metrics,
        &fonts,
        &texture_ctx,
    );
    run_dynamic_layout_workload(
        "run timer actor-to-upload",
        1,
        run_timer_value,
        InlineText::copy_from(" .0123456789").expect("timer glyph domain fits inline"),
        DynamicTextWarmup::FrameSlots,
        true,
        None,
        &metrics,
        &fonts,
        &texture_ctx,
    );
    run_dynamic_layout_workload(
        "eight SMX sensor values",
        8,
        |frame, actor| {
            let mut text = InlineText::new();
            assert!(text.push_u32(u32::from(smx_sensor_value(frame, actor))));
            text
        },
        InlineText::copy_from("500").expect("sensor text fits inline"),
        DynamicTextWarmup::U16Domain(500),
        false,
        Some(smx_sensor_value),
        &metrics,
        &fonts,
        &texture_ctx,
    );

    let bench = PacemakerFrameBench::default();
    for frame in [0, 1, 255, 8_192, 16_000] {
        assert_eq!(bench.legacy_frame(frame), bench.optimized_frame(frame));
    }
    reset_mini_text_benchmark();
    let legacy_frame = measure(FRAME_OPS, true, |frame| bench.legacy_frame(frame));
    let optimized_frame = measure(FRAME_OPS, true, |frame| bench.optimized_frame(frame));
    assert_eq!(legacy_frame.checksum, optimized_frame.checksum);

    println!("\npacemaker live-frame preparation ({FRAME_OPS} frames)");
    print_result("generic indicator prep", &legacy_frame, FRAME_OPS);
    print_result("mode-specific prep", &optimized_frame, FRAME_OPS);
    black_box((legacy_text.checksum, optimized_frame.checksum));
}

fn text_actor(content: TextContent) -> Actor {
    let mut text = TextBuilder::new();
    text.font("test");
    text.settext(content);
    text.shadowlength(1.0);
    text.build(0)
}

fn compose_text(
    actors: &mut [Actor],
    content: TextContent,
    metrics: &Metrics,
    fonts: &FontMap,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    uploads: &mut TexturedMeshUploads,
    texture_ctx: &BenchTextureContext,
) -> usize {
    actors[0] = text_actor(content);
    compose_text_group(actors, metrics, fonts, cache, scratch, uploads, texture_ctx)
}

fn compose_text_group(
    actors: &[Actor],
    metrics: &Metrics,
    fonts: &FontMap,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    uploads: &mut TexturedMeshUploads,
    texture_ctx: &BenchTextureContext,
) -> usize {
    let mut render = build_screen_cached_with_scratch_and_texture_context(
        actors,
        [0.0; 4],
        metrics,
        fonts,
        0.0,
        cache,
        scratch,
        texture_ctx,
    );
    resolve_textured_meshes(&render, uploads, |_, _| true);
    black_box((uploads.vertices.len(), uploads.sources.len()));
    let checksum = render.ops.iter().fold(0usize, |mut checksum, op| {
        let DrawOp::TexturedMesh(run) = *op else {
            unreachable!("pacemaker text benchmark only emits textured meshes")
        };
        let geometry = &render.tmesh_geometries[run.geometry as usize];
        let vertices = match &geometry.vertices {
            TexturedMeshVertices::Shared(vertices) => vertices.as_ref(),
            TexturedMeshVertices::Reusable(vertices) => vertices.as_slice(),
            TexturedMeshVertices::Transient(vertices) => vertices.as_slice(),
        };
        for instance in &render.tmesh_instances
            [run.instance_start as usize..(run.instance_start + run.instance_count) as usize]
        {
            // Normalize shared batching versus transient one-actor runs: visual
            // behavior is the ordered instance plus its resolved geometry, not
            // how many draw runs happen to reference that geometry.
            checksum = checksum.rotate_left(3)
                ^ run.texture_handle as usize
                ^ (run.camera as usize) << 8
                ^ (run.depth_test as usize) << 16
                ^ (run.blend as usize) << 24
                ^ vertices.len();
            for value in instance
                .model_col0
                .iter()
                .chain(&instance.model_col1)
                .chain(&instance.model_col2)
                .chain(&instance.model_col3)
                .chain(&instance.tint)
                .chain(&instance.uv_scale)
                .chain(&instance.uv_offset)
                .chain(&instance.uv_tex_shift)
                .chain(std::slice::from_ref(&instance.texture_mask))
            {
                checksum = checksum.rotate_left(3) ^ value.to_bits() as usize;
            }
            for vertex in vertices {
                for value in vertex
                    .pos
                    .iter()
                    .chain(&vertex.uv)
                    .chain(&vertex.color)
                    .chain(&vertex.tex_matrix_scale)
                {
                    checksum = checksum.rotate_left(3) ^ value.to_bits() as usize;
                }
            }
        }
        checksum
    });
    scratch.recycle_frame(&mut render);
    checksum
}

fn run_dynamic_layout_workload(
    title: &str,
    actor_count: usize,
    value: impl Fn(usize, usize) -> InlineText + Copy,
    glyph_domain: InlineText,
    warmup: DynamicTextWarmup,
    prewarm_whole_values: bool,
    dense_value: Option<fn(usize, usize) -> u16>,
    metrics: &Metrics,
    fonts: &FontMap,
    texture_ctx: &BenchTextureContext,
) {
    let mut legacy_cache = TextLayoutCache::new(4_097);
    let mut legacy_scratch = ComposeScratch::default();
    let mut legacy_uploads = TexturedMeshUploads::default();
    let mut legacy_actors = (0..actor_count)
        .map(|_| text_actor(TextContent::Inline(glyph_domain)))
        .collect::<Vec<_>>();
    black_box(compose_text_group(
        &legacy_actors,
        metrics,
        fonts,
        &mut legacy_cache,
        &mut legacy_scratch,
        &mut legacy_uploads,
        texture_ctx,
    ));
    if prewarm_whole_values {
        for frame in 0..DYNAMIC_LAYOUT_FRAMES {
            for actor in 0..actor_count {
                let text = value(frame, actor);
                legacy_cache.prewarm_text(fonts, "test", text.as_str(), None);
            }
        }
        legacy_cache.lock_growth();
    } else {
        legacy_cache.lock_growth_with_reserve(4_096);
    }

    let mut transient = matches!(warmup, DynamicTextWarmup::FrameSlots).then(|| {
        let mut cache = TextLayoutCache::new(1);
        let mut scratch = ComposeScratch::default();
        let mut uploads = TexturedMeshUploads::default();
        for slot in 0..actor_count {
            prewarm_frame_inline_text_slot(
                &mut cache,
                &mut scratch,
                fonts,
                "test",
                glyph_domain,
                slot as u8,
                actor_count * 4,
            );
        }
        let actors = (0..actor_count)
            .map(|actor| text_actor(TextContent::frame_inline_slot(glyph_domain, actor as u8)))
            .collect::<Vec<_>>();
        black_box(compose_text_group(
            &actors,
            metrics,
            fonts,
            &mut cache,
            &mut scratch,
            &mut uploads,
            texture_ctx,
        ));
        cache.lock_growth();
        (cache, scratch, uploads, actors)
    });

    let optimized_capacity = match warmup {
        DynamicTextWarmup::FrameSlots => 1,
        DynamicTextWarmup::U16Domain(_) => 1,
    };
    let mut optimized_cache = TextLayoutCache::new(optimized_capacity);
    let mut optimized_scratch = ComposeScratch::default();
    let mut optimized_uploads = TexturedMeshUploads::default();
    match warmup {
        DynamicTextWarmup::FrameSlots => {
            for slot in 0..actor_count {
                prewarm_prepared_inline_text_slot(
                    &mut optimized_cache,
                    &mut optimized_scratch,
                    fonts,
                    "test",
                    glyph_domain,
                    slot as u8,
                    TextAlign::Center,
                    actor_count * 4,
                );
            }
        }
        DynamicTextWarmup::U16Domain(max) => {
            optimized_cache.prewarm_u16_domain(fonts, "test", 0, max, None, TextAlign::Left);
        }
    }
    let optimized_content = |frame, actor| match warmup {
        DynamicTextWarmup::FrameSlots => {
            TextContent::frame_inline_slot(value(frame, actor), actor as u8)
        }
        DynamicTextWarmup::U16Domain(_) => {
            let value =
                dense_value.expect("dense workload supplies its numeric value")(frame, actor);
            TextContent::prewarmed_u16(value, 0)
        }
    };
    let mut optimized_actors = (0..actor_count)
        .map(|actor| {
            let content = match warmup {
                DynamicTextWarmup::FrameSlots => {
                    TextContent::frame_inline_slot(glyph_domain, actor as u8)
                }
                DynamicTextWarmup::U16Domain(_) => optimized_content(0, actor),
            };
            text_actor(content)
        })
        .collect::<Vec<_>>();
    black_box(compose_text_group(
        &optimized_actors,
        metrics,
        fonts,
        &mut optimized_cache,
        &mut optimized_scratch,
        &mut optimized_uploads,
        texture_ctx,
    ));
    if matches!(warmup, DynamicTextWarmup::U16Domain(_)) {
        optimized_scratch.prewarm_draw_sort(actor_count * 2);
    }
    optimized_cache.lock_growth();

    let legacy = measure(DYNAMIC_LAYOUT_FRAMES, false, |frame| {
        for (actor, output) in legacy_actors.iter_mut().enumerate() {
            *output = text_actor(TextContent::Inline(value(frame, actor)));
        }
        compose_text_group(
            &legacy_actors,
            metrics,
            fonts,
            &mut legacy_cache,
            &mut legacy_scratch,
            &mut legacy_uploads,
            texture_ctx,
        )
    });
    let transient_result = transient.as_mut().map(|(cache, scratch, uploads, actors)| {
        measure(DYNAMIC_LAYOUT_FRAMES, false, |frame| {
            for (actor, output) in actors.iter_mut().enumerate() {
                *output = text_actor(TextContent::frame_inline_slot(
                    value(frame, actor),
                    actor as u8,
                ));
            }
            compose_text_group(actors, metrics, fonts, cache, scratch, uploads, texture_ctx)
        })
    });
    let optimized = measure(DYNAMIC_LAYOUT_FRAMES, false, |frame| {
        for (actor, output) in optimized_actors.iter_mut().enumerate() {
            *output = text_actor(optimized_content(frame, actor));
        }
        compose_text_group(
            &optimized_actors,
            metrics,
            fonts,
            &mut optimized_cache,
            &mut optimized_scratch,
            &mut optimized_uploads,
            texture_ctx,
        )
    });

    println!("\n{title} ({DYNAMIC_LAYOUT_FRAMES} song frames)");
    print_result(
        if prewarm_whole_values {
            "prewarmed layouts"
        } else {
            "whole-string cache"
        },
        &legacy,
        DYNAMIC_LAYOUT_FRAMES,
    );
    if let Some(transient) = transient_result.as_ref() {
        print_result("transient slots", transient, DYNAMIC_LAYOUT_FRAMES);
    }
    let optimized_label = match warmup {
        DynamicTextWarmup::FrameSlots => "direct frame slots",
        DynamicTextWarmup::U16Domain(_) => "prewarmed domain",
    };
    print_result(optimized_label, &optimized, DYNAMIC_LAYOUT_FRAMES);
    assert_eq!(
        legacy.checksum, optimized.checksum,
        "{title} behavior changed"
    );
    if let Some(transient) = transient_result.as_ref() {
        assert_eq!(
            transient.checksum, optimized.checksum,
            "{title} transient-slot behavior changed"
        );
        assert_eq!(transient.allocated.allocs, 0, "{title} baseline allocated");
        assert_eq!(
            transient.allocated.reallocs, 0,
            "{title} baseline reallocated"
        );
        assert_eq!(transient.allocated.frees, 0, "{title} baseline freed");
    }
    assert_eq!(optimized.allocated.allocs, 0, "{title} allocated");
    assert_eq!(optimized.allocated.reallocs, 0, "{title} reallocated");
    assert_eq!(optimized.allocated.frees, 0, "{title} freed");
}

fn warm_compose(
    actors: &mut [Actor],
    metrics: &Metrics,
    fonts: &FontMap,
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    uploads: &mut TexturedMeshUploads,
    texture_ctx: &BenchTextureContext,
) {
    let content = match &actors[0] {
        Actor::Text { content, .. } => content.clone(),
        _ => unreachable!("benchmark actor is text"),
    };
    black_box(compose_text(
        actors,
        content,
        metrics,
        fonts,
        cache,
        scratch,
        uploads,
        texture_ctx,
    ));
}

struct BenchTextureContext;

impl TextureContext for BenchTextureContext {
    fn texture_registry_generation(&self) -> u64 {
        1
    }

    fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
        None
    }

    fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
        (1, 1)
    }

    fn texture_handle(&self, _key: &str) -> deadlib_render::TextureHandle {
        1
    }
}

fn numeric_font() -> FontMap {
    let glyph = Glyph {
        texture_key: std::sync::Arc::from("pacemaker-bench-font"),
        stroke_texture_key: None,
        tex_rect: [0.0, 0.0, 8.0, 16.0],
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        size: [8.0, 16.0],
        offset: [0.0, -16.0],
        advance: 8.0,
        advance_i32: 8,
    };
    let mut glyph_map = HashMap::new();
    let mut ascii_glyphs = Box::new(std::array::from_fn(|_| None));
    for ch in "+-0123456789.%/ms() ".chars() {
        glyph_map.insert(ch, glyph.clone());
        ascii_glyphs[ch as usize] = Some(glyph.clone());
    }
    FontMap::from_iter([(
        "test",
        Font {
            glyph_map,
            ascii_glyphs,
            default_glyph: None,
            line_spacing: 16,
            height: 16,
            fallback_font_name: None,
            cache_tag: 1,
            chain_key: 1,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: HashMap::new(),
            texture_hints_map: HashMap::new(),
        },
    )])
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
