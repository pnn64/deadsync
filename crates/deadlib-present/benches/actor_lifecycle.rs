use deadlib_present::actors::{Actor, ActorResourceArena, SizeSpec, SpriteSource};
use deadlib_present::anim::{self, Step};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_present::runtime;
use deadlib_render::BlendMode;
use smallvec::SmallVec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SOURCES: usize = 16;
const SPRITES: usize = 512;
const WARMUP_FRAMES: usize = 64;
const MEASURE_FRAMES: usize = 20_000;
const TWEEN_SITE_BASE: u64 = 0x5457_4545_4E42_454E;

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
    transient_owners_per_key: usize,
    checksum: usize,
}

fn main() {
    println!(
        "tween layout: Step={} B, SpriteBuilder={} B, TextBuilder={} B",
        size_of::<Step>(),
        size_of::<SpriteBuilder>(),
        size_of::<TextBuilder>(),
    );

    let sources: Vec<Arc<str>> = (0..SOURCES)
        .map(|source| Arc::from(format!("noteskin/source/{source}")))
        .collect();
    let owned = run_owned(&sources);
    let arena = run_arena(&sources);
    assert_eq!(owned.checksum, arena.checksum);

    println!("transient actor texture ownership microbenchmark");
    println!("{SPRITES} sprites/frame across {SOURCES} song-stable texture keys");
    print_result("owned Arc<str>", &owned);
    print_result("arena texture ID", &arena);
    println!(
        "arena IDs: {:.2}x throughput, {} -> {} transient Arc owners/key",
        owned.elapsed.as_secs_f64() / arena.elapsed.as_secs_f64(),
        owned.transient_owners_per_key,
        arena.transient_owners_per_key,
    );

    let tweened = run_tweened_builder();
    println!("tweened actor builder microbenchmark");
    print_result("tweened builder", &tweened);
}

fn run_tweened_builder() -> BenchResult {
    runtime::clear_all();
    let mut actors = Vec::with_capacity(SPRITES);
    for _ in 0..WARMUP_FRAMES {
        black_box(tweened_frame(&mut actors));
    }

    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(tweened_frame(&mut actors)));
    }
    let elapsed = started.elapsed();
    let alloc = ALLOC.snapshot().delta(before);
    assert_eq!(alloc.allocs, 0, "tween builders allocated after warmup");
    assert_eq!(alloc.reallocs, 0, "tween builders reallocated after warmup");
    runtime::clear_all();
    BenchResult {
        elapsed,
        alloc,
        transient_owners_per_key: 0,
        checksum,
    }
}

fn tweened_frame(actors: &mut Vec<Actor>) -> usize {
    runtime::tick(1.0 / 60.0);
    actors.clear();
    for sprite in 0..SPRITES {
        let mut builder = SpriteBuilder::solid();
        builder.tweensalt(sprite as u64);
        let mut steps = SmallVec::<[Step; 4]>::new();
        steps.push(
            anim::linear(0.25)
                .xy(sprite as f32, 32.0)
                .diffuse(0.8, 0.6, 0.4, 1.0)
                .build(),
        );
        steps.push(anim::accelerate(0.2).zoom(1.2, 0.8).rotationz(15.0).build());
        steps.push(
            anim::decelerate(0.3)
                .xy(0.0, 0.0)
                .glow(1.0, 1.0, 1.0, 0.0)
                .build(),
        );
        steps.push(anim::sleep(0.1));
        builder.set_tween(steps);
        actors.push(builder.build(TWEEN_SITE_BASE));
    }
    black_box(&*actors);
    actors.len()
}

fn run_owned(sources: &[Arc<str>]) -> BenchResult {
    let mut actors = Vec::with_capacity(SPRITES);
    for _ in 0..WARMUP_FRAMES {
        black_box(owned_frame(&mut actors, sources));
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(owned_frame(&mut actors, sources)));
    }
    let elapsed = started.elapsed();
    let alloc = ALLOC.snapshot().delta(before);
    let transient_owners_per_key = Arc::strong_count(&sources[0]).saturating_sub(1);
    BenchResult {
        elapsed,
        alloc,
        transient_owners_per_key,
        checksum,
    }
}

fn run_arena(sources: &[Arc<str>]) -> BenchResult {
    let arena = ActorResourceArena::new(SOURCES);
    let cached: Vec<AtomicU64> = (0..SOURCES).map(|_| AtomicU64::new(0)).collect();
    let mut actors = Vec::with_capacity(SPRITES);
    for _ in 0..WARMUP_FRAMES {
        black_box(arena_frame(&mut actors, sources, &cached, &arena));
    }
    assert_eq!(arena.stats().texture_misses as usize, SOURCES);
    arena.lock_growth();
    arena.reset_stats();

    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(arena_frame(
            &mut actors,
            sources,
            &cached,
            &arena,
        )));
    }
    let elapsed = started.elapsed();
    let alloc = ALLOC.snapshot().delta(before);
    let transient_owners_per_key = Arc::strong_count(&sources[0]).saturating_sub(2);
    assert_eq!(arena.stats().texture_misses, 0);
    assert_eq!(arena.stats().texture_saturated, 0);
    BenchResult {
        elapsed,
        alloc,
        transient_owners_per_key,
        checksum,
    }
}

fn owned_frame(actors: &mut Vec<Actor>, sources: &[Arc<str>]) -> usize {
    actors.clear();
    for sprite in 0..SPRITES {
        let source = &sources[sprite % sources.len()];
        actors.push(sprite_actor(SpriteSource::TextureHandle {
            key: Arc::clone(source),
            handle: sprite as u64 % SOURCES as u64 + 1,
            generation: 1,
        }));
    }
    black_box(&*actors);
    actors.len()
}

fn arena_frame(
    actors: &mut Vec<Actor>,
    sources: &[Arc<str>],
    cached: &[AtomicU64],
    arena: &ActorResourceArena,
) -> usize {
    actors.clear();
    for sprite in 0..SPRITES {
        let source = sprite % sources.len();
        actors.push(sprite_actor(arena.texture_source(
            &sources[source],
            source as u64 + 1,
            1,
            &cached[source],
        )));
    }
    black_box(&*actors);
    actors.len()
}

fn sprite_actor(source: SpriteSource) -> Actor {
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(64.0), SizeSpec::Px(64.0)],
        source,
        tint: [1.0; 4],
        glow: [0.0; 4],
        z: 0,
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

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let actors = (MEASURE_FRAMES * SPRITES) as f64;
    println!(
        "{name:<18} {:>8.2} us/frame  {:>6.2} ns/actor  {:>5.1} allocs/frame  \
         {:>6.1} B/frame  {:>4.1} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / actors,
        result.alloc.allocs as f64 / frames,
        result.alloc.bytes as f64 / frames,
        result.alloc.reallocs as f64 / frames,
    );
}
