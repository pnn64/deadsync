use deadlib_present::actors::{
    Actor, ActorResourceArena, FlatDraw, FlatMeshVertices, FlatPreparedInline, FlatPreparedU32,
    FlatSprite, FlatTexturedMesh, InlineText, InlineU32Text, SizeSpec, SpriteSource, TextAlign,
    TextContent,
};
use deadlib_present::compose::{
    ActorSegment, ComposeScratch, NullTextureContext, TextLayoutCache,
    build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources,
    prewarm_cached_prepared_inline_text_slot, prewarm_prepared_inline_text_slot,
    prewarm_u32_text_slot,
};
use deadlib_present::dsl::TextBuilder;
use deadlib_present::font::{self, Font, Glyph, GlyphMap};
use deadlib_render_core::{BlendMode, MeshVertex, TexturedMeshVertex};
use deadsync_theme_simply_love::screens::gameplay::{
    BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY, BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    benchmark_present_identity_notefield, benchmark_present_transformed_notefield,
};
use glam::{Mat4, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FIELD_ACTORS: usize = 224;
const HUD_ACTORS: usize = 32;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 50_000;
const PEAK_FIELD_ACTORS: usize = BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY;
const PEAK_HUD_ACTORS: usize = BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY;
const PEAK_PLAYER_ACTORS: usize = PEAK_FIELD_ACTORS + PEAK_HUD_ACTORS;
const WARMUP_PEAK_FRAMES: usize = 512;
const MEASURE_PEAK_FRAMES: usize = 10_000;
const TRANSFORM_BATCH_FRAMES: usize = 128;
const TRANSFORM_WARMUP_BATCHES: usize = 8;
const TRANSFORM_MEASURE_BATCHES: usize = 400;
const BOUNDARY_PLAYERS: usize = 2;
const BOUNDARY_FIELD_DRAWS: usize = PEAK_FIELD_ACTORS;
const BOUNDARY_HUD_ACTORS: usize = PEAK_HUD_ACTORS;
const BOUNDARY_BATCH_FRAMES: usize = 32;
const BOUNDARY_WARMUP_BATCHES: usize = 32;
const BOUNDARY_MEASURE_BATCHES: usize = 400;
const ERROR_BAR_MEASURE_BATCHES: usize = 400;
const CAMERA_HANDOFF_BATCH_FRAMES: usize = 4_096;
const CAMERA_HANDOFF_WARMUP_BATCHES: usize = 16;
const CAMERA_HANDOFF_MEASURE_BATCHES: usize = 400;
const HUD_TEXT_RUNS: usize = 8;
const ERROR_BAR_TEXT_RUNS: usize = 4;
const CUE_COUNTDOWN_RUNS: usize = 3;

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

// SAFETY: calls delegate unchanged to `System`; atomics only observe
// successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
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

    fn add(&mut self, other: Self) {
        self.allocs += other.allocs;
        self.reallocs += other.reallocs;
        self.bytes += other.bytes;
    }
}

fn fill(actors: &mut Vec<Actor>, count: usize, base: f32) {
    actors.clear();
    actors.extend((0..count).map(|index| Actor::CameraPush {
        view_proj: Mat4::from_translation(Vec3::new(base + index as f32, 0.0, 0.0)),
    }));
}

fn fill_incrementally(actors: &mut Vec<Actor>, count: usize, base: f32) {
    actors.clear();
    for index in 0..count {
        actors.push(Actor::CameraPush {
            view_proj: Mat4::from_translation(Vec3::new(base + index as f32, 0.0, 0.0)),
        });
    }
}

fn frame(
    field: &mut Vec<Actor>,
    hud: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
    present: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>),
) -> f32 {
    fill(field, FIELD_ACTORS, 0.0);
    fill(hud, HUD_ACTORS, 10_000.0);
    present(field, hud, out);
    let first = match out.first() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    let last = match out.last() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    first + last + out.len() as f32
}

struct BenchResult {
    elapsed: std::time::Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: f32,
}

fn measure(present: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>)) -> BenchResult {
    let mut field = Vec::with_capacity(FIELD_ACTORS);
    let mut hud = Vec::with_capacity(HUD_ACTORS);
    let mut out = Vec::with_capacity(FIELD_ACTORS + HUD_ACTORS);
    for _ in 0..WARMUP_FRAMES {
        black_box(frame(&mut field, &mut hud, &mut out, present));
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0.0f32;
    for _ in 0..MEASURE_FRAMES {
        checksum += black_box(frame(&mut field, &mut hud, &mut out, present));
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(before_cycles);
    let allocated = ALLOC.snapshot().delta(before);
    assert!(field.is_empty());
    assert!(hud.is_empty());
    assert_eq!(out.len(), FIELD_ACTORS + HUD_ACTORS);

    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<17} {:>9.2} ns/frame  {:>8.0} cycles/frame  {:>6.1} M actors/s  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * (FIELD_ACTORS + HUD_ACTORS) as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.bytes, 0);
}

struct PeakScratch {
    field: [Vec<Actor>; 2],
    hud: [Vec<Actor>; 2],
    assembled: [Vec<Actor>; 2],
}

impl PeakScratch {
    fn new() -> Self {
        Self {
            field: std::array::from_fn(|_| Vec::with_capacity(PEAK_FIELD_ACTORS)),
            hud: std::array::from_fn(|_| Vec::with_capacity(PEAK_HUD_ACTORS)),
            assembled: std::array::from_fn(|_| Vec::with_capacity(PEAK_PLAYER_ACTORS)),
        }
    }

    fn frame(&mut self) -> f32 {
        let mut checksum = 0.0;
        for player in 0..2 {
            let field = &mut self.field[player];
            let hud = &mut self.hud[player];
            let assembled = &mut self.assembled[player];
            fill_incrementally(field, PEAK_FIELD_ACTORS, player as f32 * 20_000.0);
            fill_incrementally(hud, PEAK_HUD_ACTORS, player as f32 * 20_000.0 + 10_000.0);
            benchmark_present_identity_notefield(field, hud, assembled);
            let first = match assembled.first() {
                Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
                _ => -1.0,
            };
            let last = match assembled.last() {
                Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
                _ => -1.0,
            };
            checksum += first + last + assembled.len() as f32;
        }
        checksum
    }
}

fn measure_peak_scratch() -> BenchResult {
    let mut scratch = PeakScratch::new();
    for _ in 0..WARMUP_PEAK_FRAMES {
        black_box(scratch.frame());
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0.0;
    for _ in 0..MEASURE_PEAK_FRAMES {
        checksum += black_box(scratch.frame());
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_peak_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_PEAK_FRAMES as f64;
    println!(
        "{label:<17} {:>9.2} us/frame  {:>8.0} cycles/frame  \
         {:>5.2} allocs/frame  {:>8.1} KiB/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames / 1024.0,
        result.allocated.reallocs as f64 / frames,
    );
}

struct TransformActors {
    field: Vec<Actor>,
    hud: Vec<Actor>,
}

fn transform_actor(vertices: &Arc<[MeshVertex]>, index: usize) -> Actor {
    Actor::Mesh {
        align: [0.0, 0.0],
        offset: [index as f32, (index % 17) as f32],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        tint: [0.5, 0.25, 0.75, 0.8],
        vertices: Arc::clone(vertices),
        visible: true,
        blend: BlendMode::Alpha,
        z: (index % 64) as i16,
    }
}

fn transform_batch(players: usize, vertices: &Arc<[MeshVertex]>) -> Vec<TransformActors> {
    (0..TRANSFORM_BATCH_FRAMES * players)
        .map(|frame_player| {
            let base = frame_player * (FIELD_ACTORS + HUD_ACTORS);
            let mut field = Vec::with_capacity(FIELD_ACTORS);
            field.extend((0..FIELD_ACTORS).map(|index| transform_actor(vertices, base + index)));
            let mut hud = Vec::with_capacity(HUD_ACTORS);
            hud.extend(
                (0..HUD_ACTORS).map(|index| transform_actor(vertices, base + FIELD_ACTORS + index)),
            );
            TransformActors { field, hud }
        })
        .collect()
}

fn measure_transform(players: usize) -> BenchResult {
    let vertices: Arc<[MeshVertex]> = Arc::from([
        MeshVertex {
            pos: [0.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [12.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [0.0, 9.0],
            color: [1.0; 4],
        },
    ]);
    let mut elapsed = Duration::ZERO;
    let mut cycles = 0u64;
    let mut allocated = AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    };
    let mut checksum = 0.0f32;
    for batch in 0..TRANSFORM_WARMUP_BATCHES + TRANSFORM_MEASURE_BATCHES {
        let mut actors = transform_batch(players, &vertices);
        let before = ALLOC.snapshot();
        let before_cycles = read_cycles();
        let started = Instant::now();
        let mut batch_checksum = 0.0f32;
        for actors in &mut actors {
            let segments = benchmark_present_transformed_notefield(&actors.field, &actors.hud);
            black_box(segments);
            batch_checksum += (actors.field.len() + actors.hud.len()) as f32;
        }
        let batch_elapsed = started.elapsed();
        let batch_cycles = read_cycles().saturating_sub(before_cycles);
        let batch_allocated = ALLOC.snapshot().delta(before);
        black_box(batch_checksum);
        if batch >= TRANSFORM_WARMUP_BATCHES {
            elapsed += batch_elapsed;
            cycles += batch_cycles;
            allocated.add(batch_allocated);
            checksum += batch_checksum;
        }
    }
    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
    }
}

fn measure_transform_compose(players: usize) -> BenchResult {
    let vertices: Arc<[MeshVertex]> = Arc::from([
        MeshVertex {
            pos: [0.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [12.0, 0.0],
            color: [1.0; 4],
        },
        MeshVertex {
            pos: [0.0, 9.0],
            color: [1.0; 4],
        },
    ]);
    let metrics = deadlib_present::space::metrics_for_window(854, 480);
    let fonts = font::FontMap::default();
    let resources = ActorResourceArena::new(0);
    let mut text = TextLayoutCache::default();
    let mut scratch = ComposeScratch::default();
    let mut elapsed = Duration::ZERO;
    let mut cycles = 0u64;
    let mut allocated = AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    };
    let mut checksum = 0.0f32;
    for batch in 0..TRANSFORM_WARMUP_BATCHES + TRANSFORM_MEASURE_BATCHES {
        let mut actors = transform_batch(players, &vertices);
        let before = ALLOC.snapshot();
        let before_cycles = read_cycles();
        let started = Instant::now();
        let mut batch_checksum = 0.0f32;
        for frame_actors in actors.chunks_exact_mut(players) {
            let mut segments = [ActorSegment::new(&[]); 4];
            let mut segment_count = 0usize;
            for actors in frame_actors.iter() {
                for segment in benchmark_present_transformed_notefield(&actors.field, &actors.hud) {
                    segments[segment_count] = segment;
                    segment_count += 1;
                }
            }
            let mut frame =
                build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
                    &segments[..segment_count],
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    &fonts,
                    0.0,
                    &mut text,
                    &mut scratch,
                    &NullTextureContext,
                    &resources,
                );
            batch_checksum += (frame.ops.len() + frame.mesh_vertices.len()) as f32;
            black_box(&frame);
            scratch.recycle_frame(&mut frame);
        }
        let batch_elapsed = started.elapsed();
        let batch_cycles = read_cycles().saturating_sub(before_cycles);
        let batch_allocated = ALLOC.snapshot().delta(before);
        black_box(batch_checksum);
        if batch >= TRANSFORM_WARMUP_BATCHES {
            elapsed += batch_elapsed;
            cycles += batch_cycles;
            allocated.add(batch_allocated);
            checksum += batch_checksum;
        }
    }
    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
    }
}

fn print_transform_result(label: &str, result: &BenchResult, players: usize) {
    let frames = (TRANSFORM_BATCH_FRAMES * TRANSFORM_MEASURE_BATCHES) as f64;
    println!(
        "{label:<17} {:>9.2} us/frame  {:>8.0} cycles/frame  {:>6.1} M actors/s  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * players as f64 * (FIELD_ACTORS + HUD_ACTORS) as f64
            / result.elapsed.as_secs_f64()
            / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
    );
}

#[derive(Clone, Copy)]
enum BoundaryKind {
    WideActors,
    FlatDraws,
}

struct BoundaryScratch {
    actor_fields: [Vec<Actor>; BOUNDARY_PLAYERS],
    flat_fields: [Vec<FlatDraw>; BOUNDARY_PLAYERS],
    huds: [Vec<Actor>; BOUNDARY_PLAYERS],
    hud_vertices: Arc<[MeshVertex]>,
    hold_vertices: Arc<Vec<TexturedMeshVertex>>,
    hold_texture: Arc<str>,
}

impl BoundaryScratch {
    fn new() -> Self {
        Self {
            actor_fields: std::array::from_fn(|_| Vec::with_capacity(BOUNDARY_FIELD_DRAWS)),
            flat_fields: std::array::from_fn(|_| Vec::with_capacity(BOUNDARY_FIELD_DRAWS)),
            huds: std::array::from_fn(|_| Vec::with_capacity(BOUNDARY_HUD_ACTORS)),
            hud_vertices: Arc::from([
                MeshVertex {
                    pos: [0.0, 0.0],
                    color: [1.0; 4],
                },
                MeshVertex {
                    pos: [12.0, 0.0],
                    color: [1.0; 4],
                },
                MeshVertex {
                    pos: [0.0, 9.0],
                    color: [1.0; 4],
                },
            ]),
            hold_vertices: Arc::new(vec![TexturedMeshVertex::default(); 6]),
            hold_texture: Arc::from("bench-hold"),
        }
    }

    fn prepare(&mut self, kind: BoundaryKind, frame: usize, field_draws: usize, hold_mix: bool) {
        assert!(field_draws <= BOUNDARY_FIELD_DRAWS);
        for player in 0..BOUNDARY_PLAYERS {
            let base = player * 20_000 + frame;
            let hud = &mut self.huds[player];
            hud.clear();
            for index in 0..BOUNDARY_HUD_ACTORS {
                hud.push(transform_actor(&self.hud_vertices, base + index));
            }
            match kind {
                BoundaryKind::WideActors => {
                    let field = &mut self.actor_fields[player];
                    field.clear();
                    for index in 0..field_draws {
                        field.push(if hold_mix && index % 2 == 0 {
                            boundary_hold_actor(
                                base + index,
                                &self.hold_vertices,
                                &self.hold_texture,
                            )
                        } else {
                            boundary_actor(base + index)
                        });
                    }
                }
                BoundaryKind::FlatDraws => {
                    let field = &mut self.flat_fields[player];
                    field.clear();
                    for index in 0..field_draws {
                        field.push(if hold_mix && index % 2 == 0 {
                            boundary_hold_flat_draw(
                                base + index,
                                &self.hold_vertices,
                                &self.hold_texture,
                            )
                        } else {
                            boundary_flat_draw(base + index)
                        });
                    }
                }
            }
        }
    }
}

fn boundary_fields(index: usize) -> ([f32; 2], f32, [f32; 4], [f32; 4], i16) {
    let lane = (index % 8) as f32;
    let row = (index % 96) as f32;
    let glow = if index % 7 == 0 {
        [1.0, 1.0, 1.0, 0.35]
    } else {
        [1.0, 1.0, 1.0, 0.0]
    };
    (
        [96.0 + lane * 32.0, -120.0 + row * 7.0],
        (index % 9) as f32 * 0.125,
        [0.8, 0.9, 1.0, 0.75],
        glow,
        (index % 4) as i16 + 140,
    )
}

fn boundary_actor(index: usize) -> Actor {
    let (center, world_z, tint, glow, z) = boundary_fields(index);
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: center,
        world_z,
        size: [SizeSpec::Px(64.0), SizeSpec::Px(64.0)],
        source: SpriteSource::Solid,
        tint,
        glow,
        z,
        cell: None,
        grid: None,
        uv_rect: Some([0.0, 0.0, 1.0, 1.0]),
        visible: true,
        flip_x: index % 2 == 0,
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
        rot_z_deg: (index % 360) as f32,
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

fn boundary_flat_draw(index: usize) -> FlatDraw {
    let (center, world_z, tint, glow, z) = boundary_fields(index);
    FlatDraw::Sprite(FlatSprite {
        center,
        world_z,
        size: [64.0, 64.0],
        source: SpriteSource::Solid,
        tint,
        glow,
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: index % 2 == 0,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: (index % 360) as f32,
        z,
    })
}

fn boundary_hold_actor(
    index: usize,
    vertices: &Arc<Vec<TexturedMeshVertex>>,
    texture: &Arc<str>,
) -> Actor {
    let (offset, world_z, tint, glow, z) = boundary_fields(index);
    Actor::ReusableTexturedMesh {
        align: [0.0, 0.0],
        offset,
        world_z,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Mat4::IDENTITY,
        texture: Arc::clone(texture),
        tint,
        glow,
        vertices: Arc::clone(vertices),
        geom_cache_key: deadlib_render_core::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        uv_tex_shift: [0.0; 2],
        depth_test: true,
        visible: true,
        blend: BlendMode::Alpha,
        z,
    }
}

fn boundary_hold_flat_draw(
    index: usize,
    vertices: &Arc<Vec<TexturedMeshVertex>>,
    texture: &Arc<str>,
) -> FlatDraw {
    let (offset, world_z, tint, glow, z) = boundary_fields(index);
    FlatDraw::TexturedMesh(FlatTexturedMesh {
        offset,
        world_z,
        local_transform: Mat4::IDENTITY,
        texture: Arc::clone(texture),
        tint,
        glow,
        vertices: FlatMeshVertices::Reusable(Arc::clone(vertices)),
        geom_cache_key: deadlib_render_core::INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        uv_tex_shift: [0.0; 2],
        depth_test: true,
        blend: BlendMode::Alpha,
        z,
    })
}

struct BoundaryResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    samples_ns: Vec<u64>,
    checksum: f32,
}

#[allow(clippy::too_many_arguments)]
fn boundary_frame(
    kind: BoundaryKind,
    frame_index: usize,
    field_draws: usize,
    hold_mix: bool,
    source: &mut BoundaryScratch,
    metrics: &deadlib_present::space::Metrics,
    fonts: &font::FontMap,
    resources: &ActorResourceArena,
    text: &mut TextLayoutCache,
    compose: &mut ComposeScratch,
) -> f32 {
    source.prepare(kind, frame_index, field_draws, hold_mix);
    let root_camera = Mat4::from_rotation_x(0.07) * Mat4::from_rotation_z(0.11);
    let tint = [0.8, 0.7, 0.6, 0.5];
    let mut segments = [ActorSegment::new(&[]); BOUNDARY_PLAYERS * 2];
    for player in 0..BOUNDARY_PLAYERS {
        segments[player * 2] = ActorSegment::transformed(
            &source.huds[player],
            900,
            tint,
            Some(BlendMode::Add),
            root_camera,
            Mat4::IDENTITY,
            None,
        );
        segments[player * 2 + 1] = match kind {
            BoundaryKind::WideActors => ActorSegment::transformed(
                &source.actor_fields[player],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            ),
            BoundaryKind::FlatDraws => ActorSegment::transformed(
                &[],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            )
            .with_flat_draws(&source.flat_fields[player], Some(root_camera)),
        };
    }
    let mut output =
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &segments,
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            fonts,
            0.0,
            text,
            compose,
            &NullTextureContext,
            resources,
        );
    let checksum =
        (output.ops.len() + output.sprite_instances.len() + output.mesh_vertices.len()) as f32;
    black_box(&output);
    compose.recycle_frame(&mut output);
    checksum
}

fn measure_boundary(kind: BoundaryKind, field_draws: usize, hold_mix: bool) -> BoundaryResult {
    let metrics = deadlib_present::space::metrics_for_window(854, 480);
    let fonts = font::FontMap::default();
    let resources = ActorResourceArena::new(0);
    let mut text = TextLayoutCache::default();
    let mut compose = ComposeScratch::default();
    let mut source = BoundaryScratch::new();
    let mut frame_index = 0usize;
    for _ in 0..BOUNDARY_WARMUP_BATCHES * BOUNDARY_BATCH_FRAMES {
        black_box(boundary_frame(
            kind,
            frame_index,
            field_draws,
            hold_mix,
            &mut source,
            &metrics,
            &fonts,
            &resources,
            &mut text,
            &mut compose,
        ));
        frame_index += 1;
    }

    let mut samples_ns = Vec::with_capacity(BOUNDARY_MEASURE_BATCHES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let mut elapsed = Duration::ZERO;
    let mut checksum = 0.0;
    for _ in 0..BOUNDARY_MEASURE_BATCHES {
        let started = Instant::now();
        for _ in 0..BOUNDARY_BATCH_FRAMES {
            checksum += black_box(boundary_frame(
                kind,
                frame_index,
                field_draws,
                hold_mix,
                &mut source,
                &metrics,
                &fonts,
                &resources,
                &mut text,
                &mut compose,
            ));
            frame_index += 1;
        }
        let sample = started.elapsed();
        elapsed += sample;
        samples_ns.push((sample.as_nanos() / BOUNDARY_BATCH_FRAMES as u128) as u64);
    }
    let cycles = read_cycles().saturating_sub(before_cycles);
    let allocated = ALLOC.snapshot().delta(before_alloc);
    samples_ns.sort_unstable();
    BoundaryResult {
        elapsed,
        cycles,
        allocated,
        samples_ns,
        checksum,
    }
}

fn measure_boundary_pair(field_draws: usize, hold_mix: bool) -> [BoundaryResult; 2] {
    let metrics = deadlib_present::space::metrics_for_window(854, 480);
    let fonts = font::FontMap::default();
    let resources = ActorResourceArena::new(0);
    let mut text = TextLayoutCache::default();
    let mut compose = ComposeScratch::default();
    let mut source = BoundaryScratch::new();
    let kinds = [BoundaryKind::WideActors, BoundaryKind::FlatDraws];
    let mut frame_index = 0usize;

    for batch in 0..BOUNDARY_WARMUP_BATCHES {
        for offset in 0..2 {
            let kind = kinds[(batch + offset) % 2];
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                black_box(boundary_frame(
                    kind,
                    frame_index + frame_offset,
                    field_draws,
                    hold_mix,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text,
                    &mut compose,
                ));
            }
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [0_u64; 2];
    let mut allocated = [AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    }; 2];
    let mut samples_ns: [Vec<u64>; 2] =
        std::array::from_fn(|_| Vec::with_capacity(BOUNDARY_MEASURE_BATCHES));
    let mut checksum = [0.0_f32; 2];

    for batch in 0..BOUNDARY_MEASURE_BATCHES {
        for offset in 0..2 {
            let kind_index = (batch + offset) % 2;
            let before_alloc = ALLOC.snapshot();
            let before_cycles = read_cycles();
            let started = Instant::now();
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                checksum[kind_index] += black_box(boundary_frame(
                    kinds[kind_index],
                    frame_index + frame_offset,
                    field_draws,
                    hold_mix,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text,
                    &mut compose,
                ));
            }
            let sample = started.elapsed();
            elapsed[kind_index] += sample;
            cycles[kind_index] += read_cycles().saturating_sub(before_cycles);
            allocated[kind_index].add(ALLOC.snapshot().delta(before_alloc));
            samples_ns[kind_index].push((sample.as_nanos() / BOUNDARY_BATCH_FRAMES as u128) as u64);
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    for samples in &mut samples_ns {
        samples.sort_unstable();
    }
    std::array::from_fn(|index| BoundaryResult {
        elapsed: elapsed[index],
        cycles: cycles[index],
        allocated: allocated[index],
        samples_ns: std::mem::take(&mut samples_ns[index]),
        checksum: checksum[index],
    })
}

fn print_boundary_result(label: &str, result: &BoundaryResult) {
    print_sampled_result(label, result, BOUNDARY_BATCH_FRAMES);
}

fn print_sampled_result(label: &str, result: &BoundaryResult, batch_frames: usize) {
    let frames = (result.samples_ns.len() * batch_frames) as f64;
    let p95_index = (result.samples_ns.len() * 95)
        .div_ceil(100)
        .saturating_sub(1);
    let p99_index = (result.samples_ns.len() * 99)
        .div_ceil(100)
        .saturating_sub(1);
    let p95_ns = result.samples_ns[p95_index];
    let p99_ns = result.samples_ns[p99_index];
    let worst_ns = result.samples_ns.last().copied().unwrap_or_default();
    println!(
        "{label:<17} mean {:>8.2} us  p95 {:>8.2} us  p99 {:>8.2} us  worst {:>8.2} us  \
         {:>8.0} cycles/frame  {} alloc  {} realloc  {} bytes",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        p95_ns as f64 / 1_000.0,
        p99_ns as f64 / 1_000.0,
        worst_ns as f64 / 1_000.0,
        result.cycles as f64 / frames,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.bytes,
    );
}

#[derive(Clone, Copy)]
enum CameraHandoffKind {
    WideActorScope,
    DirectMatrix,
}

fn camera_handoff_frame(kind: CameraHandoffKind, actors: &mut Vec<Actor>, view_proj: Mat4) -> f32 {
    actors.clear();
    match kind {
        CameraHandoffKind::WideActorScope => {
            actors.push(Actor::CameraPush { view_proj });
            black_box(actors.last());
            actors.truncate(0);
        }
        CameraHandoffKind::DirectMatrix => {
            black_box(&view_proj);
        }
    }
    view_proj.x_axis.x
}

fn measure_camera_handoff_pair() -> [BoundaryResult; 2] {
    let cameras = [
        Mat4::from_rotation_x(0.07) * Mat4::from_rotation_z(0.11),
        Mat4::from_rotation_x(-0.05) * Mat4::from_rotation_z(-0.09),
    ];
    let mut actor_scratch: [Vec<Actor>; 2] = std::array::from_fn(|_| Vec::with_capacity(1));
    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [0u64; 2];
    let mut allocated = [AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    }; 2];
    let mut samples_ns: [Vec<u64>; 2] =
        std::array::from_fn(|_| Vec::with_capacity(CAMERA_HANDOFF_MEASURE_BATCHES));
    let mut checksum = [0.0f32; 2];
    for batch in 0..CAMERA_HANDOFF_WARMUP_BATCHES + CAMERA_HANDOFF_MEASURE_BATCHES {
        let order = if batch % 2 == 0 { [0, 1] } else { [1, 0] };
        for kind_index in order {
            let kind = [
                CameraHandoffKind::WideActorScope,
                CameraHandoffKind::DirectMatrix,
            ][kind_index];
            let before_alloc = ALLOC.snapshot();
            let before_cycles = read_cycles();
            let started = Instant::now();
            let mut batch_checksum = 0.0;
            for _ in 0..CAMERA_HANDOFF_BATCH_FRAMES {
                for player in 0..2 {
                    batch_checksum +=
                        camera_handoff_frame(kind, &mut actor_scratch[player], cameras[player]);
                }
            }
            let sample = started.elapsed();
            let sample_cycles = read_cycles().saturating_sub(before_cycles);
            let sample_allocated = ALLOC.snapshot().delta(before_alloc);
            black_box(batch_checksum);
            if batch >= CAMERA_HANDOFF_WARMUP_BATCHES {
                elapsed[kind_index] += sample;
                cycles[kind_index] += sample_cycles;
                allocated[kind_index].add(sample_allocated);
                samples_ns[kind_index]
                    .push((sample.as_nanos() / CAMERA_HANDOFF_BATCH_FRAMES as u128) as u64);
                checksum[kind_index] += batch_checksum;
            }
        }
    }
    for samples in &mut samples_ns {
        samples.sort_unstable();
    }
    std::array::from_fn(|index| BoundaryResult {
        elapsed: elapsed[index],
        cycles: cycles[index],
        allocated: allocated[index],
        samples_ns: std::mem::take(&mut samples_ns[index]),
        checksum: checksum[index],
    })
}

fn print_camera_handoff_benchmark() {
    println!("\ndirect field-camera handoff benchmark (2 players)");
    let [wide, direct] = measure_camera_handoff_pair();
    assert_eq!(wide.checksum, direct.checksum);
    print_sampled_result("wide actor scope", &wide, CAMERA_HANDOFF_BATCH_FRAMES);
    print_sampled_result("direct matrix", &direct, CAMERA_HANDOFF_BATCH_FRAMES);
    for result in [&wide, &direct] {
        assert_zero_alloc(&BenchResult {
            elapsed: result.elapsed,
            cycles: result.cycles,
            allocated: result.allocated,
            checksum: result.checksum,
        });
    }
}

fn print_boundary_sweep(label: &str, hold_mix: bool, draw_counts: &[usize]) {
    println!("\n{label} (draws/player)");
    for &field_draws in draw_counts {
        println!("{field_draws} draws/player");
        let [wide, flat] = measure_boundary_pair(field_draws, hold_mix);
        assert_eq!(wide.checksum, flat.checksum);
        for result in [&wide, &flat] {
            assert_zero_alloc(&BenchResult {
                elapsed: result.elapsed,
                cycles: result.cycles,
                allocated: result.allocated,
                checksum: result.checksum,
            });
        }
        print_boundary_result("wide actor control", &wide);
        print_boundary_result("flat draw path", &flat);
    }
}

fn numeric_font() -> Font {
    let fill = Arc::<str>::from("bench_numeric_fill");
    let stroke = Arc::<str>::from("bench_numeric_stroke");
    let mut glyph_map = GlyphMap::default();
    let mut ascii = std::array::from_fn(|_| None);
    for byte in b"0123456789-./%() msEarlyLateFastSlow" {
        let ch = char::from(*byte);
        let glyph = Glyph {
            texture_key: Arc::clone(&fill),
            stroke_texture_key: Some(Arc::clone(&stroke)),
            tex_rect: [0.0, 0.0, 8.0, 8.0],
            uv_scale: [0.5, 0.5],
            uv_offset: [0.0, 0.0],
            size: [8.0, 10.0],
            offset: [0.0, -10.0],
            advance: 8.0,
            advance_i32: 8,
        };
        glyph_map.insert(ch, glyph.clone());
        ascii[ch as usize] = Some(glyph);
    }
    Font {
        glyph_map,
        ascii_glyphs: Box::new(ascii),
        default_glyph: None,
        line_spacing: 10,
        height: 10,
        fallback_font_name: None,
        cache_tag: 1,
        chain_key: 1,
        default_stroke_color: [1.0; 4],
        stroke_texture_map: HashMap::from([(fill.to_string(), stroke.to_string())]),
        texture_hints_map: HashMap::new(),
    }
}

#[derive(Clone, Copy)]
enum NumericCase {
    Combo,
    CueCountdown,
}

const fn numeric_run_count(case: NumericCase) -> usize {
    match case {
        NumericCase::Combo => 1,
        NumericCase::CueCountdown => CUE_COUNTDOWN_RUNS,
    }
}

fn numeric_value(case: NumericCase, frame: usize, player: usize, run: usize) -> u32 {
    match case {
        NumericCase::Combo => ((frame + 1) * (player + 3)) as u32,
        NumericCase::CueCountdown => ((frame / 60 + player * 11 + run * 7) % 60 + 1) as u32,
    }
}

fn numeric_position(case: NumericCase, player: usize, run: usize) -> [f32; 2] {
    match case {
        NumericCase::Combo => [240.0 + player as f32 * 160.0, 265.0],
        NumericCase::CueCountdown => [
            220.0 + player as f32 * 240.0 + run as f32 * 24.0,
            if run % 2 == 0 { 160.0 } else { 340.0 },
        ],
    }
}

fn numeric_actor(
    case: NumericCase,
    value: u32,
    player: usize,
    run: usize,
    cached: Arc<str>,
) -> Actor {
    let mut text = TextBuilder::new();
    text.font("bench-numeric");
    text.settext(match case {
        NumericCase::Combo => TextContent::prepared_u32(value, player as u8),
        NumericCase::CueCountdown => TextContent::Shared(cached),
    });
    text.align(0.5, 0.5);
    let [x, y] = numeric_position(case, player, run);
    text.xy(x, y);
    text.zoom(match case {
        NumericCase::Combo => 0.75,
        NumericCase::CueCountdown => 0.5,
    });
    text.horizalign(TextAlign::Center);
    if matches!(case, NumericCase::Combo) {
        text.shadowlength(1.0);
    }
    text.diffuse([0.2, 0.8, 0.4, 0.9]);
    text.z(match case {
        NumericCase::Combo => 90,
        NumericCase::CueCountdown => 200,
    });
    text.build(0)
}

fn numeric_draw(case: NumericCase, value: u32, player: usize, run: usize) -> FlatDraw {
    let combo = matches!(case, NumericCase::Combo);
    FlatDraw::PreparedU32(FlatPreparedU32 {
        align: [0.5, 0.5],
        offset: numeric_position(case, player, run),
        color: [0.2, 0.8, 0.4, 0.9],
        font: "bench-numeric",
        text: InlineU32Text::new(value),
        slot: (player * numeric_run_count(case) + run) as u8,
        align_text: TextAlign::Center,
        z: if combo { 90 } else { 200 },
        scale: [if combo { 0.75 } else { 0.5 }; 2],
        blend: BlendMode::Alpha,
        shadow_len: if combo { [1.0, -1.0] } else { [0.0; 2] },
        shadow_color: [0.0, 0.0, 0.0, 0.5],
    })
}

struct NumericScratch {
    actors: [Vec<Actor>; BOUNDARY_PLAYERS],
    draws: [Vec<FlatDraw>; BOUNDARY_PLAYERS],
    cached_values: [Arc<str>; 65],
}

impl NumericScratch {
    fn new(case: NumericCase) -> Self {
        let runs = numeric_run_count(case);
        Self {
            actors: std::array::from_fn(|_| Vec::with_capacity(runs)),
            draws: std::array::from_fn(|_| Vec::with_capacity(runs)),
            cached_values: std::array::from_fn(|value| Arc::from(value.to_string())),
        }
    }

    fn prepare(&mut self, case: NumericCase, kind: BoundaryKind, frame: usize) {
        for player in 0..BOUNDARY_PLAYERS {
            match kind {
                BoundaryKind::WideActors => {
                    self.actors[player].clear();
                    for run in 0..numeric_run_count(case) {
                        let value = numeric_value(case, frame, player, run);
                        let cached = match case {
                            NumericCase::Combo => Arc::clone(&self.cached_values[0]),
                            NumericCase::CueCountdown => {
                                Arc::clone(&self.cached_values[value as usize])
                            }
                        };
                        self.actors[player].push(numeric_actor(case, value, player, run, cached));
                    }
                }
                BoundaryKind::FlatDraws => {
                    self.draws[player].clear();
                    for run in 0..numeric_run_count(case) {
                        let value = numeric_value(case, frame, player, run);
                        self.draws[player].push(numeric_draw(case, value, player, run));
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn numeric_frame(
    case: NumericCase,
    kind: BoundaryKind,
    frame_index: usize,
    source: &mut NumericScratch,
    metrics: &deadlib_present::space::Metrics,
    fonts: &font::FontMap,
    resources: &ActorResourceArena,
    text: &mut TextLayoutCache,
    compose: &mut ComposeScratch,
) -> f32 {
    source.prepare(case, kind, frame_index);
    let root_camera = Mat4::from_rotation_z(0.11);
    let tint = [0.8, 0.7, 0.6, 0.5];
    let mut segments = [ActorSegment::new(&[]); BOUNDARY_PLAYERS];
    for (player, segment) in segments.iter_mut().enumerate() {
        *segment = match kind {
            BoundaryKind::WideActors => ActorSegment::transformed(
                &source.actors[player],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            ),
            BoundaryKind::FlatDraws => ActorSegment::transformed(
                &[],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            )
            .with_flat_draws(&source.draws[player], Some(root_camera)),
        };
    }
    let mut output =
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &segments,
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            fonts,
            0.0,
            text,
            compose,
            &NullTextureContext,
            resources,
        );
    let checksum =
        (output.ops.len() + output.tmesh_instances.len() + output.tmesh_geometries.len()) as f32;
    black_box(&output);
    compose.recycle_frame(&mut output);
    checksum
}

fn measure_numeric_pair(case: NumericCase) -> [BoundaryResult; 2] {
    let metrics = deadlib_present::space::metrics_for_window(854, 480);
    let fonts = font::FontMap::from_iter([("bench-numeric", numeric_font())]);
    let resources = ActorResourceArena::new(0);
    let cache_entries = match case {
        NumericCase::Combo => 1,
        NumericCase::CueCountdown => 256,
    };
    let mut text: [TextLayoutCache; 2] =
        std::array::from_fn(|_| TextLayoutCache::new(cache_entries));
    let mut compose: [ComposeScratch; 2] = std::array::from_fn(|_| ComposeScratch::default());
    let mut source = NumericScratch::new(case);
    for cache in &mut text {
        if matches!(case, NumericCase::CueCountdown) {
            for value in &source.cached_values {
                cache.prewarm_text(&fonts, "bench-numeric", value, None);
            }
            cache.lock_growth_with_reserve(source.cached_values.len());
        }
        for slot in 0..BOUNDARY_PLAYERS * numeric_run_count(case) {
            prewarm_u32_text_slot(
                cache,
                &fonts,
                "bench-numeric",
                slot as u8,
                TextAlign::Center,
            );
        }
    }
    let kinds = [BoundaryKind::WideActors, BoundaryKind::FlatDraws];
    let mut frame_index = 0usize;

    if matches!(case, NumericCase::CueCountdown) {
        // Gameplay transition warmup prepares the complete bounded countdown
        // domain. Exercise every retained shared value here as well so the
        // actor control does not measure first-use alias or mesh construction.
        for kind_index in 0..2 {
            for second in 0..60 {
                black_box(numeric_frame(
                    case,
                    kinds[kind_index],
                    second * 60,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text[kind_index],
                    &mut compose[kind_index],
                ));
            }
        }
    }

    for batch in 0..BOUNDARY_WARMUP_BATCHES {
        for offset in 0..2 {
            let kind_index = (batch + offset) % 2;
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                black_box(numeric_frame(
                    case,
                    kinds[kind_index],
                    frame_index + frame_offset,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text[kind_index],
                    &mut compose[kind_index],
                ));
            }
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [0_u64; 2];
    let mut allocated = [AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    }; 2];
    let mut samples_ns: [Vec<u64>; 2] =
        std::array::from_fn(|_| Vec::with_capacity(BOUNDARY_MEASURE_BATCHES));
    let mut checksum = [0.0_f32; 2];

    for batch in 0..BOUNDARY_MEASURE_BATCHES {
        for offset in 0..2 {
            let kind_index = (batch + offset) % 2;
            let before_alloc = ALLOC.snapshot();
            let before_cycles = read_cycles();
            let started = Instant::now();
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                checksum[kind_index] += black_box(numeric_frame(
                    case,
                    kinds[kind_index],
                    frame_index + frame_offset,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text[kind_index],
                    &mut compose[kind_index],
                ));
            }
            let sample = started.elapsed();
            elapsed[kind_index] += sample;
            cycles[kind_index] += read_cycles().saturating_sub(before_cycles);
            allocated[kind_index].add(ALLOC.snapshot().delta(before_alloc));
            samples_ns[kind_index].push((sample.as_nanos() / BOUNDARY_BATCH_FRAMES as u128) as u64);
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    for samples in &mut samples_ns {
        samples.sort_unstable();
    }
    std::array::from_fn(|index| BoundaryResult {
        elapsed: elapsed[index],
        cycles: cycles[index],
        allocated: allocated[index],
        samples_ns: std::mem::take(&mut samples_ns[index]),
        checksum: checksum[index],
    })
}

#[derive(Clone, Copy)]
enum InlineCase {
    Zmod,
    ErrorBar,
}

const fn inline_run_count(case: InlineCase) -> usize {
    match case {
        InlineCase::Zmod => HUD_TEXT_RUNS,
        InlineCase::ErrorBar => ERROR_BAR_TEXT_RUNS,
    }
}

fn inline_text_value(case: InlineCase, frame: usize, player: usize, run: usize) -> InlineText {
    if matches!(case, InlineCase::ErrorBar) {
        let text = match run {
            0 => {
                return InlineText::format(format_args!(
                    "{}.{:02}ms",
                    frame / 60 % 180,
                    frame % 60
                ))
                .expect("benchmark offset value fits inline");
            }
            1 => "Early",
            2 => "Late",
            3 => ["Fast", "Early", "Slow", "Late"][(frame / 60 + player) % 4],
            _ => unreachable!("error-bar benchmark has four bounded runs"),
        };
        return InlineText::copy_from(text).expect("benchmark error-bar label fits inline");
    }
    let value_frame = match run {
        0..=5 => frame / 120,
        6 => frame / 240,
        7 => frame,
        _ => unreachable!("ZMod HUD benchmark has eight bounded runs"),
    };
    let a = ((value_frame * (player + 3) + run * 7) % 99 + 1) as u32;
    let b = ((value_frame * (run + 5) + player * 11) % 99 + 1) as u32;
    match run {
        0 => InlineText::format(format_args!("{a}")),
        1 => InlineText::format(format_args!("({a})")),
        2..=3 => InlineText::format(format_args!("{a}/{b}")),
        4 => InlineText::format(format_args!("{b}")),
        5 => InlineText::format(format_args!("({b})")),
        6 => InlineText::format(format_args!("{}.{:02} ", a / 10, b)),
        7 => InlineText::format(format_args!("-{}.{:02}%", a / 10, b)),
        _ => unreachable!("ZMod HUD benchmark has eight bounded runs"),
    }
    .expect("benchmark HUD value fits inline")
}

fn inline_text_style(
    case: InlineCase,
    player: usize,
    run: usize,
) -> ([f32; 2], [f32; 2], TextAlign, f32, [f32; 4], f32) {
    if matches!(case, InlineCase::ErrorBar) {
        return (
            [0.5, 0.5],
            [220.0 + player as f32 * 240.0 + run as f32 * 32.0, 220.0],
            TextAlign::Center,
            [0.25, 0.7, 0.7, 0.35][run],
            [0.2 + run as f32 * 0.15, 0.8, 0.4, 0.9],
            if matches!(run, 0 | 3) { 1.0 } else { 0.0 },
        );
    }
    let mini = run == HUD_TEXT_RUNS - 1;
    (
        [if mini { 0.0 } else { 0.5 }, 0.5],
        [
            220.0 + player as f32 * 200.0 + run as f32 * 4.0,
            180.0 + run as f32 * 14.0,
        ],
        if mini {
            TextAlign::Left
        } else {
            TextAlign::Center
        },
        if mini { 0.4 } else { 0.35 },
        [0.3 + run as f32 * 0.05, 0.8, 0.4, 0.9],
        1.0,
    )
}

fn inline_text_actor(case: InlineCase, value: InlineText, player: usize, run: usize) -> Actor {
    let (align, offset, align_text, zoom, color, shadow) = inline_text_style(case, player, run);
    let mut text = TextBuilder::new();
    text.font("bench-numeric");
    let slot = (player * inline_run_count(case) + run) as u8;
    text.settext(if matches!(case, InlineCase::ErrorBar) && run != 0 {
        TextContent::Static(match value.as_str() {
            "Early" => "Early",
            "Late" => "Late",
            "Fast" => "Fast",
            "Slow" => "Slow",
            _ => unreachable!("benchmark error-bar label is from the fixed domain"),
        })
    } else {
        TextContent::frame_inline_slot(value, slot)
    });
    text.align(align[0], align[1]);
    text.xy(offset[0], offset[1]);
    text.zoom(zoom);
    text.horizalign(align_text);
    text.shadowlength(shadow);
    text.diffuse(color);
    text.z(85);
    text.build(0)
}

fn inline_text_draw(case: InlineCase, value: InlineText, player: usize, run: usize) -> FlatDraw {
    let (align, offset, align_text, zoom, color, shadow) = inline_text_style(case, player, run);
    FlatDraw::PreparedInline(FlatPreparedInline {
        align,
        offset,
        color,
        font: "bench-numeric",
        text: value,
        slot: (player * inline_run_count(case) + run) as u8,
        align_text,
        z: 85,
        scale: [zoom, zoom],
        blend: BlendMode::Alpha,
        shadow_len: [shadow, -shadow],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
    })
}

struct HudTextScratch {
    actors: [Vec<Actor>; BOUNDARY_PLAYERS],
    draws: [Vec<FlatDraw>; BOUNDARY_PLAYERS],
}

impl HudTextScratch {
    fn new() -> Self {
        Self {
            actors: std::array::from_fn(|_| Vec::with_capacity(HUD_TEXT_RUNS)),
            draws: std::array::from_fn(|_| Vec::with_capacity(HUD_TEXT_RUNS)),
        }
    }

    fn prepare(&mut self, case: InlineCase, kind: BoundaryKind, frame: usize) {
        for player in 0..BOUNDARY_PLAYERS {
            match kind {
                BoundaryKind::WideActors => {
                    self.actors[player].clear();
                    self.actors[player].extend((0..inline_run_count(case)).map(|run| {
                        inline_text_actor(
                            case,
                            inline_text_value(case, frame, player, run),
                            player,
                            run,
                        )
                    }));
                }
                BoundaryKind::FlatDraws => {
                    self.draws[player].clear();
                    self.draws[player].extend((0..inline_run_count(case)).map(|run| {
                        inline_text_draw(
                            case,
                            inline_text_value(case, frame, player, run),
                            player,
                            run,
                        )
                    }));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn hud_text_frame(
    case: InlineCase,
    kind: BoundaryKind,
    frame_index: usize,
    source: &mut HudTextScratch,
    metrics: &deadlib_present::space::Metrics,
    fonts: &font::FontMap,
    resources: &ActorResourceArena,
    text: &mut TextLayoutCache,
    compose: &mut ComposeScratch,
) -> f32 {
    source.prepare(case, kind, frame_index);
    let root_camera = Mat4::from_rotation_z(0.11);
    let tint = [0.8, 0.7, 0.6, 0.5];
    let mut segments = [ActorSegment::new(&[]); BOUNDARY_PLAYERS];
    for (player, segment) in segments.iter_mut().enumerate() {
        *segment = match kind {
            BoundaryKind::WideActors => ActorSegment::transformed(
                &source.actors[player],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            ),
            BoundaryKind::FlatDraws => ActorSegment::transformed(
                &[],
                900,
                tint,
                Some(BlendMode::Add),
                root_camera,
                Mat4::IDENTITY,
                None,
            )
            .with_flat_draws(&source.draws[player], Some(root_camera)),
        };
    }
    let mut output =
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &segments,
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            fonts,
            0.0,
            text,
            compose,
            &NullTextureContext,
            resources,
        );
    let checksum =
        (output.ops.len() + output.tmesh_instances.len() + output.tmesh_geometries.len()) as f32;
    black_box(&output);
    compose.recycle_frame(&mut output);
    checksum
}

fn prewarm_hud_text(
    case: InlineCase,
    text: &mut TextLayoutCache,
    compose: &mut ComposeScratch,
    fonts: &font::FontMap,
) {
    if matches!(case, InlineCase::ErrorBar) {
        let offset = InlineText::copy_from(".ms0123456789").expect("offset domain fits inline");
        let labels = InlineText::copy_from("EarlyLateFsSow").expect("label domain fits inline");
        for player in 0..BOUNDARY_PLAYERS {
            for run in 0..ERROR_BAR_TEXT_RUNS {
                prewarm_prepared_inline_text_slot(
                    text,
                    compose,
                    fonts,
                    "bench-numeric",
                    if run == 0 { offset } else { labels },
                    (player * ERROR_BAR_TEXT_RUNS + run) as u8,
                    TextAlign::Center,
                    BOUNDARY_PLAYERS * ERROR_BAR_TEXT_RUNS * 4,
                );
            }
        }
        for value in ["Early", "Late", "Fast", "Slow"] {
            text.prewarm_text(fonts, "bench-numeric", value, None);
        }
        text.lock_growth_with_reserve(4);
        return;
    }
    let counter = InlineText::copy_from("-/()0123456789").expect("counter domain fits inline");
    let timer = InlineText::copy_from(" .0123456789").expect("timer domain fits inline");
    let mini = InlineText::copy_from("+-.%0123456789").expect("mini domain fits inline");
    let vertex_buffers = BOUNDARY_PLAYERS * HUD_TEXT_RUNS * 4;
    for player in 0..BOUNDARY_PLAYERS {
        for run in 0..HUD_TEXT_RUNS {
            let slot = (player * HUD_TEXT_RUNS + run) as u8;
            match run {
                0..=5 => prewarm_cached_prepared_inline_text_slot(
                    text,
                    compose,
                    fonts,
                    "bench-numeric",
                    counter,
                    slot,
                    TextAlign::Center,
                    vertex_buffers,
                ),
                6 => prewarm_prepared_inline_text_slot(
                    text,
                    compose,
                    fonts,
                    "bench-numeric",
                    timer,
                    slot,
                    TextAlign::Center,
                    vertex_buffers,
                ),
                7 => prewarm_prepared_inline_text_slot(
                    text,
                    compose,
                    fonts,
                    "bench-numeric",
                    mini,
                    slot,
                    TextAlign::Left,
                    vertex_buffers,
                ),
                _ => unreachable!("ZMod HUD benchmark has eight bounded runs"),
            }
        }
    }
}

fn measure_hud_text_pair(case: InlineCase) -> [BoundaryResult; 2] {
    let metrics = deadlib_present::space::metrics_for_window(854, 480);
    let fonts = font::FontMap::from_iter([("bench-numeric", numeric_font())]);
    let resources = ActorResourceArena::new(0);
    let cache_entries = if matches!(case, InlineCase::ErrorBar) {
        32
    } else {
        1
    };
    let mut text: [TextLayoutCache; 2] =
        std::array::from_fn(|_| TextLayoutCache::new(cache_entries));
    let mut compose: [ComposeScratch; 2] = std::array::from_fn(|_| ComposeScratch::default());
    for index in 0..2 {
        prewarm_hud_text(case, &mut text[index], &mut compose[index], &fonts);
        let draw_floor = BOUNDARY_PLAYERS * inline_run_count(case) * 4;
        compose[index].retain_working_set_headroom(draw_floor, 0, draw_floor, draw_floor);
    }
    let mut source = HudTextScratch::new();
    let kinds = [BoundaryKind::WideActors, BoundaryKind::FlatDraws];
    let mut frame_index = 0usize;

    for batch in 0..BOUNDARY_WARMUP_BATCHES {
        for offset in 0..2 {
            let kind_index = (batch + offset) % 2;
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                black_box(hud_text_frame(
                    case,
                    kinds[kind_index],
                    frame_index + frame_offset,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text[kind_index],
                    &mut compose[kind_index],
                ));
            }
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [0_u64; 2];
    let mut allocated = [AllocSnapshot {
        allocs: 0,
        reallocs: 0,
        bytes: 0,
    }; 2];
    let measure_batches = if matches!(case, InlineCase::ErrorBar) {
        ERROR_BAR_MEASURE_BATCHES
    } else {
        BOUNDARY_MEASURE_BATCHES
    };
    let mut samples_ns: [Vec<u64>; 2] =
        std::array::from_fn(|_| Vec::with_capacity(measure_batches));
    let mut checksum = [0.0_f32; 2];

    for batch in 0..measure_batches {
        for offset in 0..2 {
            let kind_index = (batch + offset) % 2;
            let before_alloc = ALLOC.snapshot();
            let before_cycles = read_cycles();
            let started = Instant::now();
            for frame_offset in 0..BOUNDARY_BATCH_FRAMES {
                checksum[kind_index] += black_box(hud_text_frame(
                    case,
                    kinds[kind_index],
                    frame_index + frame_offset,
                    &mut source,
                    &metrics,
                    &fonts,
                    &resources,
                    &mut text[kind_index],
                    &mut compose[kind_index],
                ));
            }
            let sample = started.elapsed();
            elapsed[kind_index] += sample;
            cycles[kind_index] += read_cycles().saturating_sub(before_cycles);
            allocated[kind_index].add(ALLOC.snapshot().delta(before_alloc));
            samples_ns[kind_index].push((sample.as_nanos() / BOUNDARY_BATCH_FRAMES as u128) as u64);
        }
        frame_index += BOUNDARY_BATCH_FRAMES;
    }

    for samples in &mut samples_ns {
        samples.sort_unstable();
    }
    std::array::from_fn(|index| BoundaryResult {
        elapsed: elapsed[index],
        cycles: cycles[index],
        allocated: allocated[index],
        samples_ns: std::mem::take(&mut samples_ns[index]),
        checksum: checksum[index],
    })
}

fn print_hud_text_benchmark() {
    println!(
        "\nprepared ZMod HUD boundary benchmark \
         (2 players, {HUD_TEXT_RUNS} changing runs/player)"
    );
    let [wide, flat] = measure_hud_text_pair(InlineCase::Zmod);
    assert_eq!(wide.checksum, flat.checksum);
    print_boundary_result("wide text actors", &wide);
    print_boundary_result("prepared flat", &flat);
    for result in [&wide, &flat] {
        assert_zero_alloc(&BenchResult {
            elapsed: result.elapsed,
            cycles: result.cycles,
            allocated: result.allocated,
            checksum: result.checksum,
        });
    }
}

fn print_error_bar_text_benchmark() {
    println!(
        "\nprepared error-bar text boundary benchmark \
         (2 players, {ERROR_BAR_TEXT_RUNS} simultaneous runs/player)"
    );
    let [wide, flat] = measure_hud_text_pair(InlineCase::ErrorBar);
    assert_eq!(wide.checksum, flat.checksum);
    print_boundary_result("wide text actors", &wide);
    print_boundary_result("prepared flat", &flat);
    for result in [&wide, &flat] {
        assert_zero_alloc(&BenchResult {
            elapsed: result.elapsed,
            cycles: result.cycles,
            allocated: result.allocated,
            checksum: result.checksum,
        });
    }
}

fn print_cue_countdown_benchmark() {
    println!(
        "\nprepared cue-countdown boundary benchmark \
         (2 players, {CUE_COUNTDOWN_RUNS} runs/player)"
    );
    let [wide, flat] = measure_numeric_pair(NumericCase::CueCountdown);
    assert_eq!(wide.checksum, flat.checksum);
    for result in [&wide, &flat] {
        assert_zero_alloc(&BenchResult {
            elapsed: result.elapsed,
            cycles: result.cycles,
            allocated: result.allocated,
            checksum: result.checksum,
        });
    }
    print_boundary_result("cached text actors", &wide);
    print_boundary_result("prepared flat", &flat);
}

fn main() {
    if std::env::var_os("DEADSYNC_BENCH_FIELD_CAMERA_ONLY").is_some() {
        print_camera_handoff_benchmark();
        return;
    }
    if std::env::var_os("DEADSYNC_BENCH_CUE_COUNTDOWN_ONLY").is_some() {
        print_cue_countdown_benchmark();
        return;
    }
    if std::env::var_os("DEADSYNC_BENCH_ZMOD_HUD_ONLY").is_some() {
        print_hud_text_benchmark();
        return;
    }
    if std::env::var_os("DEADSYNC_BENCH_ERROR_BAR_TEXT_ONLY").is_some() {
        print_error_bar_text_benchmark();
        return;
    }
    deadlib_present::space::set_current_metrics(deadlib_present::space::metrics_for_window(
        854, 480,
    ));
    let direct = measure(benchmark_present_identity_notefield);
    assert_zero_alloc(&direct);
    black_box(direct.checksum);

    println!("dense notefield macrobenchmark");
    print_result("direct append", &direct);

    let presized_scratch = measure_peak_scratch();
    assert_zero_alloc(&presized_scratch);
    black_box(presized_scratch.checksum);
    println!(
        "\ngameplay actor scratch density-spike benchmark \
         (2 players, {PEAK_FIELD_ACTORS} field + {PEAK_HUD_ACTORS} HUD actors)"
    );
    print_peak_result("prewarmed", &presized_scratch);

    println!(
        "\nY-folded transformed player handoff benchmark \
         ({FIELD_ACTORS} field + {HUD_ACTORS} HUD mesh actors/player)"
    );
    for players in 1..=2 {
        let direct = measure_transform(players);
        assert_zero_alloc(&direct);
        black_box(direct.checksum);
        println!("{players}P");
        print_transform_result("borrowed segments", &direct, players);
    }

    println!("\nY-folded transformed player handoff + composition benchmark");
    for players in 1..=2 {
        let direct = measure_transform_compose(players);
        assert_zero_alloc(&direct);
        assert!(direct.checksum > 0.0);
        black_box(direct.checksum);
        println!("{players}P");
        print_transform_result("borrowed segments", &direct, players);
    }

    println!(
        "\ncomplete 2P tap/mine boundary benchmark \
         ({BOUNDARY_FIELD_DRAWS} field draws + {BOUNDARY_HUD_ACTORS} HUD actors/player)"
    );
    println!(
        "payload size: Actor {} bytes, FlatDraw {} bytes; lock acquisitions/frame: 0",
        std::mem::size_of::<Actor>(),
        std::mem::size_of::<FlatDraw>(),
    );
    let wide = measure_boundary(BoundaryKind::WideActors, BOUNDARY_FIELD_DRAWS, false);
    assert_zero_alloc(&BenchResult {
        elapsed: wide.elapsed,
        cycles: wide.cycles,
        allocated: wide.allocated,
        checksum: wide.checksum,
    });
    black_box(wide.checksum);
    print_boundary_result("wide actor control", &wide);
    let flat = measure_boundary(BoundaryKind::FlatDraws, BOUNDARY_FIELD_DRAWS, false);
    assert_zero_alloc(&BenchResult {
        elapsed: flat.elapsed,
        cycles: flat.cycles,
        allocated: flat.allocated,
        checksum: flat.checksum,
    });
    assert_eq!(wide.checksum, flat.checksum);
    black_box(flat.checksum);
    print_boundary_result("flat draw path", &flat);

    print_boundary_sweep(
        "feedback-scale sprite boundary sweep",
        false,
        &[2, 4, 6, 8, 16, 32, 64],
    );
    print_boundary_sweep(
        "hold-scale presentation boundary sweep",
        true,
        &[8, 16, 32, 64],
    );

    println!("\nprepared combo-number boundary benchmark (2 players, 1 run/player)");
    let [wide, flat] = measure_numeric_pair(NumericCase::Combo);
    assert_eq!(wide.checksum, flat.checksum);
    for result in [&wide, &flat] {
        assert_zero_alloc(&BenchResult {
            elapsed: result.elapsed,
            cycles: result.cycles,
            allocated: result.allocated,
            checksum: result.checksum,
        });
    }
    print_boundary_result("wide text actor", &wide);
    print_boundary_result("prepared flat", &flat);

    print_hud_text_benchmark();
    print_cue_countdown_benchmark();
    print_error_bar_text_benchmark();
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
