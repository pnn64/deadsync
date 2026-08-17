use deadlib_present::actors::{
    Actor, ActorResourceArena, FlatDraw, FlatSprite, SizeSpec, SpriteSource,
};
use deadlib_present::compose::{
    ActorSegment, ComposeScratch, NullTextureContext, TextLayoutCache,
    build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources,
};
use deadlib_present::font;
use deadlib_render_core::{BlendMode, MeshVertex};
use deadsync_theme_simply_love::screens::gameplay::{
    BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY, BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    benchmark_present_identity_notefield, benchmark_present_transformed_notefield,
};
use glam::{Mat4, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
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
        }
    }

    fn prepare(&mut self, kind: BoundaryKind, frame: usize) {
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
                    for index in 0..BOUNDARY_FIELD_DRAWS {
                        field.push(boundary_actor(base + index));
                    }
                }
                BoundaryKind::FlatDraws => {
                    let field = &mut self.flat_fields[player];
                    field.clear();
                    for index in 0..BOUNDARY_FIELD_DRAWS {
                        field.push(boundary_flat_draw(base + index));
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
    source: &mut BoundaryScratch,
    metrics: &deadlib_present::space::Metrics,
    fonts: &font::FontMap,
    resources: &ActorResourceArena,
    text: &mut TextLayoutCache,
    compose: &mut ComposeScratch,
) -> f32 {
    source.prepare(kind, frame_index);
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

fn measure_boundary(kind: BoundaryKind) -> BoundaryResult {
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

fn print_boundary_result(label: &str, result: &BoundaryResult) {
    let frames = (BOUNDARY_MEASURE_BATCHES * BOUNDARY_BATCH_FRAMES) as f64;
    let p99_index = (result.samples_ns.len() * 99)
        .div_ceil(100)
        .saturating_sub(1);
    let p99_ns = result.samples_ns[p99_index];
    let worst_ns = result.samples_ns.last().copied().unwrap_or_default();
    println!(
        "{label:<17} mean {:>8.2} us  p99 {:>8.2} us  worst {:>8.2} us  \
         {:>8.0} cycles/frame  {} alloc  {} realloc  {} bytes",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        p99_ns as f64 / 1_000.0,
        worst_ns as f64 / 1_000.0,
        result.cycles as f64 / frames,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.bytes,
    );
}

fn main() {
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
    let wide = measure_boundary(BoundaryKind::WideActors);
    assert_zero_alloc(&BenchResult {
        elapsed: wide.elapsed,
        cycles: wide.cycles,
        allocated: wide.allocated,
        checksum: wide.checksum,
    });
    black_box(wide.checksum);
    print_boundary_result("wide actor control", &wide);
    let flat = measure_boundary(BoundaryKind::FlatDraws);
    assert_zero_alloc(&BenchResult {
        elapsed: flat.elapsed,
        cycles: flat.cycles,
        allocated: flat.allocated,
        checksum: flat.checksum,
    });
    assert_eq!(wide.checksum, flat.checksum);
    black_box(flat.checksum);
    print_boundary_result("flat draw path", &flat);
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
