use deadsync_theme_simply_love::screens::gameplay::{
    SongLuaActorBuildBenchmark, SongLuaEaseBenchmark, SongLuaGraphDisplayBenchmark,
    SongLuaMessageStateBenchmark, SongLuaMultiActorEmitBenchmark, SongLuaOrderBenchmark,
    SongLuaProjectedMeshBenchmark, SongLuaProxyRequestBenchmark, SongLuaTextAttributeBenchmark,
    SongLuaTexturedGlowBenchmark, SongLuaTopologyBenchmark, SongLuaUppercaseTextBenchmark,
    SongLuaWhiteTextureKeyBenchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MESSAGE_EVENTS: usize = 2_048;
const MESSAGE_BLOCKS: usize = 512;
const FUTURE_EASES: usize = 2_048;
const ORDER_ACTORS: usize = 256;
const TOPOLOGY_GROUPS: usize = 16;
const TOPOLOGY_CHAIN_DEPTH: usize = 32;
const TOPOLOGY_REFERENCES: usize = 128;
const RGB_AFT_REFERENCES: usize = TOPOLOGY_GROUPS * 3;
const GRAPH_POINTS: usize = 128;
const WARMUP_FRAMES: usize = 1_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
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
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn measure(iterations: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.set_enabled(true);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.set_enabled(false);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1_000_000_000.0 / iterations as f64,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let frames = iterations as f64;
    println!(
        "{label:<20} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>5.2} allocs/frame  {:>8.1} bytes/frame  {:>5.2} reallocs/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_frame,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    const MESSAGE_FRAMES: usize = 20_000;
    const BLOCK_FRAMES: usize = 50_000;
    const EASE_FRAMES: usize = 100_000;
    const PROXY_FRAMES: usize = 20_000;
    const AFT_TARGET_FRAMES: usize = 2_000;
    const RGB_AFT_FRAMES: usize = 2_000;
    const ANCESTRY_FRAMES: usize = 20_000;
    const CAMERA_FRAMES: usize = 20_000;
    const ORDER_FRAMES: usize = 100_000;
    const MESH_FRAMES: usize = 50_000;
    const ACTOR_BUILD_FRAMES: usize = 100_000;
    const MULTI_ACTOR_FRAMES: usize = 100_000;
    const UPPERCASE_FRAMES: usize = 100_000;
    const TEXT_ATTRIBUTE_FRAMES: usize = 100_000;
    const TEXTURED_GLOW_FRAMES: usize = 50_000;
    const WHITE_TEXTURE_FRAMES: usize = 200_000;
    const GRAPH_FRAMES: usize = 20_000;
    const ATTRIBUTE_TEXT: &str = "Gameplay attribute colors";

    let now = MESSAGE_EVENTS as f32 * 0.125 + 1.0;
    let mut cached_messages = SongLuaMessageStateBenchmark::new(MESSAGE_EVENTS);
    let legacy_messages = SongLuaMessageStateBenchmark::new(MESSAGE_EVENTS);
    assert_eq!(
        legacy_messages.legacy_frame(now),
        cached_messages.cached_frame(now)
    );
    let legacy_message_result = measure(MESSAGE_FRAMES, || {
        legacy_messages.legacy_frame(black_box(now)).to_bits() as u64
    });
    let cached_message_result = measure(MESSAGE_FRAMES, || {
        cached_messages.cached_frame(black_box(now)).to_bits() as u64
    });

    let block_now = MESSAGE_BLOCKS as f32 * 0.01 + 1.0;
    let legacy_blocks = SongLuaMessageStateBenchmark::long_command(MESSAGE_BLOCKS);
    let mut cached_blocks = SongLuaMessageStateBenchmark::long_command(MESSAGE_BLOCKS);
    assert_eq!(
        legacy_blocks.legacy_frame(block_now),
        cached_blocks.cached_frame(block_now)
    );
    let legacy_block_result = measure(BLOCK_FRAMES, || {
        legacy_blocks.legacy_frame(black_box(block_now)).to_bits() as u64
    });
    let cached_block_result = measure(BLOCK_FRAMES, || {
        cached_blocks.cached_frame(black_box(block_now)).to_bits() as u64
    });

    let ease_benchmark = SongLuaEaseBenchmark::new(FUTURE_EASES);
    let ease_now = 0.0;
    assert_eq!(
        ease_benchmark.legacy_frame(ease_now),
        ease_benchmark.bounded_frame(ease_now)
    );
    let legacy_ease_result = measure(EASE_FRAMES, || {
        ease_benchmark.legacy_frame(black_box(ease_now)).to_bits() as u64
    });
    let bounded_ease_result = measure(EASE_FRAMES, || {
        ease_benchmark.bounded_frame(black_box(ease_now)).to_bits() as u64
    });

    let proxy_benchmark = SongLuaProxyRequestBenchmark::new(8, 16, 128);
    assert_eq!(
        proxy_benchmark.legacy_frame(),
        proxy_benchmark.indexed_frame()
    );
    let legacy_proxy_result = measure(PROXY_FRAMES, || proxy_benchmark.legacy_frame());
    let indexed_proxy_result = measure(PROXY_FRAMES, || proxy_benchmark.indexed_frame());

    let topology_benchmark =
        SongLuaTopologyBenchmark::new(TOPOLOGY_GROUPS, TOPOLOGY_CHAIN_DEPTH, TOPOLOGY_REFERENCES);
    assert_eq!(
        topology_benchmark.legacy_aft_targets(),
        topology_benchmark.indexed_aft_targets()
    );
    assert_eq!(
        topology_benchmark.legacy_aft_ancestors(),
        topology_benchmark.indexed_aft_ancestors()
    );
    assert_eq!(
        topology_benchmark.legacy_camera_states(),
        topology_benchmark.indexed_camera_states()
    );
    let legacy_aft_target_result = measure(AFT_TARGET_FRAMES, || {
        topology_benchmark.legacy_aft_targets()
    });
    let indexed_aft_target_result = measure(AFT_TARGET_FRAMES, || {
        topology_benchmark.indexed_aft_targets()
    });
    let legacy_aft_ancestor_result = measure(ANCESTRY_FRAMES, || {
        topology_benchmark.legacy_aft_ancestors()
    });
    let indexed_aft_ancestor_result = measure(ANCESTRY_FRAMES, || {
        topology_benchmark.indexed_aft_ancestors()
    });
    let legacy_camera_result = measure(CAMERA_FRAMES, || topology_benchmark.legacy_camera_states());
    let indexed_camera_result =
        measure(CAMERA_FRAMES, || topology_benchmark.indexed_camera_states());
    let mut rgb_aft_benchmark =
        SongLuaTopologyBenchmark::new(TOPOLOGY_GROUPS, TOPOLOGY_CHAIN_DEPTH, RGB_AFT_REFERENCES);
    assert_eq!(
        rgb_aft_benchmark.legacy_rgb_aft_groups(),
        rgb_aft_benchmark.indexed_rgb_aft_groups()
    );
    let legacy_rgb_aft_result =
        measure(RGB_AFT_FRAMES, || rgb_aft_benchmark.legacy_rgb_aft_groups());
    let indexed_rgb_aft_result = measure(RGB_AFT_FRAMES, || {
        rgb_aft_benchmark.indexed_rgb_aft_groups()
    });

    let mut legacy_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let mut cached_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    assert_eq!(legacy_order.legacy_frame(), cached_order.cached_frame());
    let legacy_order_result = measure(ORDER_FRAMES, || {
        legacy_order.legacy_frame().min(u64::MAX as usize) as u64
    });
    let cached_order_result = measure(ORDER_FRAMES, || {
        cached_order.cached_frame().min(u64::MAX as usize) as u64
    });

    let mut changing_tick = 0usize;
    let mut legacy_changing_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let legacy_changing_order_result = measure(ORDER_FRAMES, || {
        let checksum = legacy_changing_order.legacy_changing_frame(changing_tick);
        changing_tick = changing_tick.wrapping_add(1);
        checksum.min(u64::MAX as usize) as u64
    });
    let mut changing_tick = 0usize;
    let mut cached_changing_order = SongLuaOrderBenchmark::new(ORDER_ACTORS);
    let cached_changing_order_result = measure(ORDER_FRAMES, || {
        let checksum = cached_changing_order.cached_changing_frame(changing_tick);
        changing_tick = changing_tick.wrapping_add(1);
        checksum.min(u64::MAX as usize) as u64
    });

    let legacy_mesh = SongLuaProjectedMeshBenchmark::default();
    let mut reused_mesh = SongLuaProjectedMeshBenchmark::default();
    let legacy_vertices = legacy_mesh.legacy_frame(0.25, 0.25);
    let reused_vertices = reused_mesh.reused_frame(0.25, 0.25);
    assert_eq!(legacy_vertices.as_ref(), reused_vertices.as_slice());
    drop(reused_vertices);
    let legacy_mesh_result = measure(MESH_FRAMES, || {
        let vertices = legacy_mesh.legacy_frame(black_box(0.25), black_box(0.25));
        vertices.len() as u64
    });
    let reused_mesh_result = measure(MESH_FRAMES, || {
        let vertices = reused_mesh.reused_frame(black_box(0.25), black_box(0.25));
        vertices.len() as u64
    });

    let mut actor_build = SongLuaActorBuildBenchmark::new(96);
    assert_eq!(
        actor_build.legacy_proxy_frame(),
        actor_build.compact_proxy_frame()
    );
    assert_eq!(
        actor_build.legacy_group_frame(),
        actor_build.inline_group_frame()
    );
    assert_eq!(
        actor_build.legacy_mesh_frame(),
        actor_build.reused_mesh_frame()
    );
    let legacy_proxy_build_result =
        measure(ACTOR_BUILD_FRAMES, || actor_build.legacy_proxy_frame());
    let compact_proxy_build_result =
        measure(ACTOR_BUILD_FRAMES, || actor_build.compact_proxy_frame());
    let legacy_group_result = measure(ACTOR_BUILD_FRAMES, || actor_build.legacy_group_frame());
    let inline_group_result = measure(ACTOR_BUILD_FRAMES, || actor_build.inline_group_frame());
    let legacy_actor_mesh_result = measure(ACTOR_BUILD_FRAMES, || actor_build.legacy_mesh_frame());
    let reused_actor_mesh_result = measure(ACTOR_BUILD_FRAMES, || actor_build.reused_mesh_frame());

    let mut multi_actor = SongLuaMultiActorEmitBenchmark::new(8);
    assert_eq!(multi_actor.legacy_frame(), multi_actor.direct_frame());
    let legacy_multi_actor_result = measure(MULTI_ACTOR_FRAMES, || multi_actor.legacy_frame());
    let direct_multi_actor_result = measure(MULTI_ACTOR_FRAMES, || multi_actor.direct_frame());

    let uppercase = SongLuaUppercaseTextBenchmark::new("Mixed Straße à la carte gameplay");
    assert_eq!(uppercase.legacy_frame(), uppercase.cached_frame());
    let legacy_uppercase_result = measure(UPPERCASE_FRAMES, || uppercase.legacy_frame());
    let cached_uppercase_result = measure(UPPERCASE_FRAMES, || uppercase.cached_frame());

    let mut text_attributes = SongLuaTextAttributeBenchmark::new(ATTRIBUTE_TEXT, 8);
    assert_eq!(
        text_attributes.legacy_static_frame(),
        text_attributes.shared_static_frame()
    );
    let legacy_static_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.legacy_static_frame()
    });
    let shared_static_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.shared_static_frame()
    });
    let mut rainbow_tick = 0usize;
    let legacy_rainbow_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        let elapsed = (rainbow_tick % 70) as f32 * 0.2;
        rainbow_tick = rainbow_tick.wrapping_add(1);
        text_attributes.legacy_rainbow_frame(ATTRIBUTE_TEXT, black_box(elapsed))
    });
    let mut rainbow_tick = 0usize;
    let prewarmed_rainbow_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        let elapsed = (rainbow_tick % 70) as f32 * 0.2;
        rainbow_tick = rainbow_tick.wrapping_add(1);
        text_attributes.prewarmed_rainbow_frame(black_box(elapsed))
    });
    assert_eq!(
        text_attributes.legacy_diffuse_frame(),
        text_attributes.reused_diffuse_frame()
    );
    let legacy_diffuse_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.legacy_diffuse_frame()
    });
    let reused_diffuse_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.reused_diffuse_frame()
    });
    assert_eq!(
        text_attributes.legacy_glow_frame(),
        text_attributes.reused_glow_frame()
    );
    let legacy_glow_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.legacy_glow_frame()
    });
    let reused_glow_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.reused_glow_frame()
    });
    assert_eq!(
        text_attributes.legacy_stroke_frame(),
        text_attributes.reused_stroke_frame()
    );
    let legacy_stroke_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.legacy_stroke_frame()
    });
    let reused_stroke_attribute_result = measure(TEXT_ATTRIBUTE_FRAMES, || {
        text_attributes.reused_stroke_frame()
    });

    let mut textured_glow = SongLuaTexturedGlowBenchmark::new(96);
    assert_eq!(textured_glow.legacy_frame(), textured_glow.reused_frame());
    let legacy_textured_glow_result =
        measure(TEXTURED_GLOW_FRAMES, || textured_glow.legacy_frame());
    let reused_textured_glow_result =
        measure(TEXTURED_GLOW_FRAMES, || textured_glow.reused_frame());

    assert_eq!(
        SongLuaWhiteTextureKeyBenchmark::legacy_frame(),
        SongLuaWhiteTextureKeyBenchmark::shared_frame()
    );
    let legacy_white_texture_result = measure(WHITE_TEXTURE_FRAMES, || {
        SongLuaWhiteTextureKeyBenchmark::legacy_frame()
    });
    let shared_white_texture_result = measure(WHITE_TEXTURE_FRAMES, || {
        SongLuaWhiteTextureKeyBenchmark::shared_frame()
    });

    let mut graph_display = SongLuaGraphDisplayBenchmark::new(GRAPH_POINTS);
    assert_eq!(graph_display.legacy_frame(), graph_display.reused_frame());
    let legacy_graph_result = measure(GRAPH_FRAMES, || graph_display.legacy_frame());
    let reused_graph_result = measure(GRAPH_FRAMES, || graph_display.reused_frame());

    assert_eq!(
        textured_glow.legacy_frame(),
        textured_glow.prewarmed_static_frame()
    );
    let legacy_model_glow_result = measure(TEXTURED_GLOW_FRAMES, || textured_glow.legacy_frame());
    let prewarmed_model_glow_result = measure(TEXTURED_GLOW_FRAMES, || {
        textured_glow.prewarmed_static_frame()
    });

    assert_eq!(
        legacy_message_result.checksum,
        cached_message_result.checksum
    );
    assert_eq!(legacy_block_result.checksum, cached_block_result.checksum);
    assert_eq!(legacy_ease_result.checksum, bounded_ease_result.checksum);
    assert_eq!(legacy_proxy_result.checksum, indexed_proxy_result.checksum);
    assert_eq!(
        legacy_aft_target_result.checksum,
        indexed_aft_target_result.checksum
    );
    assert_eq!(
        legacy_aft_ancestor_result.checksum,
        indexed_aft_ancestor_result.checksum
    );
    assert_eq!(
        legacy_camera_result.checksum,
        indexed_camera_result.checksum
    );
    assert_eq!(
        legacy_rgb_aft_result.checksum,
        indexed_rgb_aft_result.checksum
    );
    assert_eq!(legacy_order_result.checksum, cached_order_result.checksum);
    assert_eq!(
        legacy_changing_order_result.checksum,
        cached_changing_order_result.checksum
    );
    assert_eq!(legacy_mesh_result.checksum, reused_mesh_result.checksum);
    assert_eq!(
        legacy_proxy_build_result.checksum,
        compact_proxy_build_result.checksum
    );
    assert_eq!(legacy_group_result.checksum, inline_group_result.checksum);
    assert_eq!(
        legacy_actor_mesh_result.checksum,
        reused_actor_mesh_result.checksum
    );
    assert_eq!(
        legacy_multi_actor_result.checksum,
        direct_multi_actor_result.checksum
    );
    assert_eq!(
        legacy_uppercase_result.checksum,
        cached_uppercase_result.checksum
    );
    assert_eq!(
        legacy_static_attribute_result.checksum,
        shared_static_attribute_result.checksum
    );
    assert_eq!(
        legacy_rainbow_result.checksum,
        prewarmed_rainbow_result.checksum
    );
    assert_eq!(
        legacy_diffuse_attribute_result.checksum,
        reused_diffuse_attribute_result.checksum
    );
    assert_eq!(
        legacy_glow_attribute_result.checksum,
        reused_glow_attribute_result.checksum
    );
    assert_eq!(
        legacy_stroke_attribute_result.checksum,
        reused_stroke_attribute_result.checksum
    );
    assert_eq!(
        legacy_textured_glow_result.checksum,
        reused_textured_glow_result.checksum
    );
    assert_eq!(
        legacy_white_texture_result.checksum,
        shared_white_texture_result.checksum
    );
    assert_eq!(legacy_graph_result.checksum, reused_graph_result.checksum);
    assert_eq!(
        legacy_model_glow_result.checksum,
        prewarmed_model_glow_result.checksum
    );

    println!("Song Lua gameplay frame hot paths");
    println!("message state ({MESSAGE_EVENTS} prior events)");
    print_result("replay history", MESSAGE_FRAMES, &legacy_message_result);
    print_result("incremental cache", MESSAGE_FRAMES, &cached_message_result);
    println!("message command ({MESSAGE_BLOCKS} completed blocks)");
    print_result("scan blocks", BLOCK_FRAMES, &legacy_block_result);
    print_result("block cursor", BLOCK_FRAMES, &cached_block_result);
    println!("runtime ease ({FUTURE_EASES} future windows)");
    print_result("scan full range", EASE_FRAMES, &legacy_ease_result);
    print_result("stop at future", EASE_FRAMES, &bounded_ease_result);
    println!("proxy requests (8 captures, 16 children, 128 references)");
    print_result("scan topology", PROXY_FRAMES, &legacy_proxy_result);
    print_result("topology index", PROXY_FRAMES, &indexed_proxy_result);
    println!(
        "AFT target lookup ({TOPOLOGY_REFERENCES} references, {} overlays)",
        TOPOLOGY_GROUPS * TOPOLOGY_CHAIN_DEPTH
    );
    print_result(
        "scan capture names",
        AFT_TARGET_FRAMES,
        &legacy_aft_target_result,
    );
    print_result(
        "target index",
        AFT_TARGET_FRAMES,
        &indexed_aft_target_result,
    );
    println!(
        "AFT ancestry ({} overlays, depth {TOPOLOGY_CHAIN_DEPTH})",
        TOPOLOGY_GROUPS * TOPOLOGY_CHAIN_DEPTH + TOPOLOGY_REFERENCES
    );
    print_result("walk parents", ANCESTRY_FRAMES, &legacy_aft_ancestor_result);
    print_result(
        "ancestor index",
        ANCESTRY_FRAMES,
        &indexed_aft_ancestor_result,
    );
    println!(
        "camera ancestry ({} overlays, depth {TOPOLOGY_CHAIN_DEPTH})",
        TOPOLOGY_GROUPS * TOPOLOGY_CHAIN_DEPTH + TOPOLOGY_REFERENCES
    );
    print_result("walk parents", CAMERA_FRAMES, &legacy_camera_result);
    print_result("camera index", CAMERA_FRAMES, &indexed_camera_result);
    println!(
        "RGB AFT grouping ({RGB_AFT_REFERENCES} sprites, {} overlays)",
        TOPOLOGY_GROUPS * TOPOLOGY_CHAIN_DEPTH + RGB_AFT_REFERENCES
    );
    print_result("scan all overlays", RGB_AFT_FRAMES, &legacy_rgb_aft_result);
    print_result("capture peers", RGB_AFT_FRAMES, &indexed_rgb_aft_result);
    println!(
        "topology storage: {} bytes/overlay",
        rgb_aft_benchmark.topology_bytes_per_overlay(),
    );
    println!("dynamic order ({ORDER_ACTORS} actors, unchanged keys)");
    print_result("sort every frame", ORDER_FRAMES, &legacy_order_result);
    print_result("key cache", ORDER_FRAMES, &cached_order_result);
    println!("dynamic order ({ORDER_ACTORS} actors, one changed key/frame)");
    print_result(
        "sort changed frame",
        ORDER_FRAMES,
        &legacy_changing_order_result,
    );
    print_result(
        "cache changed frame",
        ORDER_FRAMES,
        &cached_changing_order_result,
    );
    println!("projected mesh (4x4 scratch grid)");
    print_result("fresh immutable", MESH_FRAMES, &legacy_mesh_result);
    print_result("reused buffer", MESH_FRAMES, &reused_mesh_result);
    println!(
        "mesh storage: {} bytes, {} buffer replacements",
        reused_mesh.storage_bytes(),
        reused_mesh.replacements(),
    );
    println!("single-segment ActorProxy");
    print_result(
        "wrapper Vec",
        ACTOR_BUILD_FRAMES,
        &legacy_proxy_build_result,
    );
    print_result(
        "direct shared",
        ACTOR_BUILD_FRAMES,
        &compact_proxy_build_result,
    );
    println!("single-draw Model/Noteskin group");
    print_result("frame Vec", ACTOR_BUILD_FRAMES, &legacy_group_result);
    print_result("inline actor", ACTOR_BUILD_FRAMES, &inline_group_result);
    println!("ActorMultiVertex (96 vertices)");
    print_result(
        "fresh immutable",
        ACTOR_BUILD_FRAMES,
        &legacy_actor_mesh_result,
    );
    print_result(
        "reused buffer",
        ACTOR_BUILD_FRAMES,
        &reused_actor_mesh_result,
    );
    println!(
        "ActorMultiVertex storage: {} bytes",
        actor_build.mesh_storage_bytes(),
    );
    println!("multi-layer Model/Noteskin output (8 actors)");
    print_result(
        "temporary SmallVec",
        MULTI_ACTOR_FRAMES,
        &legacy_multi_actor_result,
    );
    print_result(
        "direct frame Vec",
        MULTI_ACTOR_FRAMES,
        &direct_multi_actor_result,
    );
    println!(
        "multi-actor song storage: {} bytes",
        multi_actor.storage_bytes(),
    );
    println!("uppercase BitmapText (Unicode, 32 bytes)");
    print_result(
        "per-frame uppercase",
        UPPERCASE_FRAMES,
        &legacy_uppercase_result,
    );
    print_result(
        "compiled Arc<str>",
        UPPERCASE_FRAMES,
        &cached_uppercase_result,
    );
    println!(
        "uppercase song storage: {} bytes",
        uppercase.storage_bytes()
    );
    println!("static BitmapText attributes (8 spans)");
    print_result(
        "clone Vec",
        TEXT_ATTRIBUTE_FRAMES,
        &legacy_static_attribute_result,
    );
    print_result(
        "clone shared Arc",
        TEXT_ATTRIBUTE_FRAMES,
        &shared_static_attribute_result,
    );
    println!("rainbow BitmapText attributes (25 characters)");
    print_result(
        "build every frame",
        TEXT_ATTRIBUTE_FRAMES,
        &legacy_rainbow_result,
    );
    print_result(
        "seven warm phases",
        TEXT_ATTRIBUTE_FRAMES,
        &prewarmed_rainbow_result,
    );
    println!(
        "rainbow song storage: {} bytes",
        text_attributes.storage_bytes()
    );
    println!("dynamic BitmapText diffuse composition (8 spans)");
    print_result(
        "fresh Vec",
        TEXT_ATTRIBUTE_FRAMES,
        &legacy_diffuse_attribute_result,
    );
    print_result(
        "reused song buffer",
        TEXT_ATTRIBUTE_FRAMES,
        &reused_diffuse_attribute_result,
    );
    println!("BitmapText attribute glow extraction (8 spans)");
    print_result(
        "fresh Vec",
        TEXT_ATTRIBUTE_FRAMES,
        &legacy_glow_attribute_result,
    );
    print_result(
        "reused song buffer",
        TEXT_ATTRIBUTE_FRAMES,
        &reused_glow_attribute_result,
    );
    println!("BitmapText stroke-only transparency");
    print_result(
        "fresh Vec",
        TEXT_ATTRIBUTE_FRAMES,
        &legacy_stroke_attribute_result,
    );
    print_result(
        "reused song buffer",
        TEXT_ATTRIBUTE_FRAMES,
        &reused_stroke_attribute_result,
    );
    println!(
        "dynamic BitmapText storage: {} bytes, {} replacements",
        text_attributes.dynamic_storage_bytes(),
        text_attributes.replacements(),
    );
    println!("textured glow pass (96 vertices)");
    print_result(
        "copy immutable",
        TEXTURED_GLOW_FRAMES,
        &legacy_textured_glow_result,
    );
    print_result(
        "reused glow buffer",
        TEXTURED_GLOW_FRAMES,
        &reused_textured_glow_result,
    );
    println!(
        "textured glow storage: {} bytes",
        textured_glow.storage_bytes()
    );
    println!("projected/skewed Quad texture key");
    print_result(
        "allocate Arc<str>",
        WHITE_TEXTURE_FRAMES,
        &legacy_white_texture_result,
    );
    print_result(
        "clone shared Arc",
        WHITE_TEXTURE_FRAMES,
        &shared_white_texture_result,
    );
    println!("GraphDisplay ({GRAPH_POINTS} points, body + line)");
    print_result("fresh meshes/frame", GRAPH_FRAMES, &legacy_graph_result);
    print_result("reused song buffers", GRAPH_FRAMES, &reused_graph_result);
    println!(
        "GraphDisplay storage: {} bytes, {} replacements, {} growths",
        graph_display.storage_bytes(),
        graph_display.replacements(),
        graph_display.growths(),
    );
    println!("static Model glow pass (96 vertices/layer)");
    print_result(
        "copy vertices",
        TEXTURED_GLOW_FRAMES,
        &legacy_model_glow_result,
    );
    print_result(
        "clone prewarmed Arc",
        TEXTURED_GLOW_FRAMES,
        &prewarmed_model_glow_result,
    );
    println!(
        "static Model glow storage: {} bytes/layer",
        textured_glow.static_storage_bytes()
    );
}
