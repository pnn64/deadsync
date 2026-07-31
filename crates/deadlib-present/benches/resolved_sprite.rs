use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadlib_present::compose::{
    ComposeScratch, TextLayoutCache, TextureContext, TextureMeta,
    build_screen_cached_with_scratch_and_texture_context,
};
use deadlib_present::{font::FontMap, space::Metrics};
use deadlib_render::BlendMode;
use std::hint::black_box;
use std::time::{Duration, Instant};

const LOGICAL_SPRITES: usize = 512;
const OUTPUT_PASSES: usize = LOGICAL_SPRITES * 2;
const WARMUP_FRAMES: usize = 256;
const MEASURE_FRAMES: usize = 10_000;

struct Textures;

impl TextureContext for Textures {
    fn texture_registry_generation(&self) -> u64 {
        1
    }

    fn texture_dims(&self, _key: &str) -> Option<TextureMeta> {
        Some(TextureMeta { w: 64, h: 64 })
    }

    fn sprite_sheet_dims(&self, _key: &str) -> (u32, u32) {
        (1, 1)
    }

    fn texture_handle(&self, _key: &str) -> deadlib_render::TextureHandle {
        1
    }
}

fn legacy_sprite(index: usize, glow: bool) -> Actor {
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [(index % 32) as f32 * 20.0, (index / 32) as f32 * 20.0],
        world_z: index as f32 * 0.001,
        size: [SizeSpec::Px(64.0), SizeSpec::Px(64.0)],
        source: SpriteSource::TextureStaticHandle {
            key: "noteskin/tap",
            handle: 1,
            generation: 1,
        },
        tint: if glow {
            [1.0, 1.0, 1.0, 0.0]
        } else {
            [0.8, 0.6, 0.4, 0.75]
        },
        glow: if glow {
            [1.0, 1.0, 1.0, 0.35]
        } else {
            [1.0, 1.0, 1.0, 0.0]
        },
        z: 140,
        cell: None,
        grid: None,
        uv_rect: Some([0.125, 0.25, 0.625, 0.75]),
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
        blend: [BlendMode::Alpha, BlendMode::Add][glow as usize],
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: (index % 8) as f32 * 5.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.1,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        effect: Default::default(),
    }
}

fn legacy_actors() -> Vec<Actor> {
    let mut actors = Vec::with_capacity(OUTPUT_PASSES);
    fill_legacy_actors(&mut actors);
    actors
}

fn fill_legacy_actors(actors: &mut Vec<Actor>) {
    actors.clear();
    for index in 0..LOGICAL_SPRITES {
        actors.push(legacy_sprite(index, false));
        actors.push(legacy_sprite(index, true));
    }
}

fn resolved_sprite(index: usize) -> Actor {
    Actor::ResolvedSprite {
        align: [0.5, 0.5],
        offset: [(index % 32) as f32 * 20.0, (index / 32) as f32 * 20.0],
        world_z: index as f32 * 0.001,
        size: [64.0, 64.0],
        source: SpriteSource::TextureStaticHandle {
            key: "noteskin/tap",
            handle: 1,
            generation: 1,
        },
        tint: [0.8, 0.6, 0.4, 0.75],
        glow: [1.0, 1.0, 1.0, 0.35],
        z: 140,
        uv_rect: [0.125, 0.25, 0.625, 0.75],
        flip_x: false,
        flip_y: false,
        blend: BlendMode::Alpha,
        glow_blend: BlendMode::Add,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: (index % 8) as f32 * 5.0,
    }
}

fn resolved_actors() -> Vec<Actor> {
    let mut actors = Vec::with_capacity(LOGICAL_SPRITES);
    fill_resolved_actors(&mut actors);
    actors
}

fn fill_resolved_actors(actors: &mut Vec<Actor>) {
    actors.clear();
    for index in 0..LOGICAL_SPRITES {
        actors.push(resolved_sprite(index));
    }
}

struct Bench {
    text_cache: TextLayoutCache,
    scratch: ComposeScratch,
    metrics: Metrics,
    fonts: FontMap,
    actors: Vec<Actor>,
}

impl Bench {
    fn new() -> Self {
        Self {
            text_cache: TextLayoutCache::default(),
            scratch: ComposeScratch::default(),
            metrics: Metrics {
                left: -320.0,
                right: 320.0,
                top: 240.0,
                bottom: -240.0,
            },
            fonts: FontMap::default(),
            actors: Vec::with_capacity(OUTPUT_PASSES),
        }
    }

    fn frame(&mut self, actors: &[Actor]) -> u64 {
        let mut render = build_screen_cached_with_scratch_and_texture_context(
            actors,
            [0.0, 0.0, 0.0, 1.0],
            &self.metrics,
            &self.fonts,
            2.5,
            &mut self.text_cache,
            &mut self.scratch,
            &Textures,
        );
        assert_eq!(render.sprite_instances.len(), OUTPUT_PASSES);
        let checksum = (render.objects.len() as u64).rotate_left(7)
            ^ (render.batches.len() as u64).rotate_left(13)
            ^ u64::from(render.sprite_instances[0].center[0].to_bits());
        self.scratch.recycle_render_list(&mut render);
        checksum
    }

    fn built_frame(&mut self, resolved: bool) -> u64 {
        if resolved {
            fill_resolved_actors(&mut self.actors);
        } else {
            fill_legacy_actors(&mut self.actors);
        }
        let mut render = build_screen_cached_with_scratch_and_texture_context(
            &self.actors,
            [0.0, 0.0, 0.0, 1.0],
            &self.metrics,
            &self.fonts,
            2.5,
            &mut self.text_cache,
            &mut self.scratch,
            &Textures,
        );
        assert_eq!(render.sprite_instances.len(), OUTPUT_PASSES);
        let checksum = (render.objects.len() as u64).rotate_left(7)
            ^ (render.batches.len() as u64).rotate_left(13)
            ^ u64::from(render.sprite_instances[0].center[0].to_bits());
        self.scratch.recycle_render_list(&mut render);
        checksum
    }
}

struct Result {
    elapsed: Duration,
    cycles: u64,
    checksum: u64,
}

fn measure(actors: &[Actor]) -> Result {
    let mut bench = Bench::new();
    for _ in 0..WARMUP_FRAMES {
        black_box(bench.frame(actors));
    }
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(3) ^ black_box(bench.frame(actors));
    }
    Result {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        checksum,
    }
}

fn measure_built(resolved: bool) -> Result {
    let mut bench = Bench::new();
    for _ in 0..WARMUP_FRAMES {
        black_box(bench.built_frame(resolved));
    }
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.rotate_left(3) ^ black_box(bench.built_frame(resolved));
    }
    Result {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        checksum,
    }
}

fn print_comparison(label: &str, legacy: &Result, resolved: &Result) {
    println!("{label}");
    print_result("legacy actors", legacy);
    print_result("resolved actors", resolved);
    println!(
        "resolved path: {:.2}x throughput, {:.1}% fewer cycles",
        legacy.elapsed.as_secs_f64() / resolved.elapsed.as_secs_f64(),
        100.0 * (1.0 - resolved.cycles as f64 / legacy.cycles as f64),
    );
}

fn print_result(label: &str, result: &Result) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<18} {:>8.2} us/frame  {:>7.2} ns/pass  {:>8.0} cycles/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames / OUTPUT_PASSES as f64,
        result.cycles as f64 / frames,
    );
}

fn main() {
    let legacy = legacy_actors();
    let resolved = resolved_actors();
    let legacy_result = measure(&legacy);
    let resolved_result = measure(&resolved);
    assert_eq!(legacy_result.checksum, resolved_result.checksum);
    let legacy_built_result = measure_built(false);
    let resolved_built_result = measure_built(true);
    assert_eq!(legacy_built_result.checksum, resolved_built_result.checksum);
    black_box((
        legacy_result.checksum,
        resolved_result.checksum,
        legacy_built_result.checksum,
        resolved_built_result.checksum,
    ));

    println!(
        "resolved sprite composition benchmark\n{LOGICAL_SPRITES} logical sprites, \
         {OUTPUT_PASSES} equal diffuse/glow passes per frame"
    );
    print_comparison("composition only", &legacy_result, &resolved_result);
    print_comparison(
        "build + composition",
        &legacy_built_result,
        &resolved_built_result,
    );
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
