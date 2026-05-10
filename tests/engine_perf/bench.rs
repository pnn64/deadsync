use deadsync::assets;
use deadsync::engine::gfx::{
    self, BlendMode as GfxBlendMode, INVALID_TMESH_CACHE_KEY, MeshMode as GfxMeshMode,
    TexturedMeshVertex as GfxTexturedMeshVertex, TexturedMeshVertices as GfxTexturedMeshVertices,
};
use deadsync::engine::present::actors::{Actor, SizeSpec, SpriteSource, TextContent};
use deadsync::engine::present::{anim, compose, font};
use deadsync::test_support::compose_scenarios;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::BuildHasherDefault;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SORT_ITERS_SMALL: usize = 20_000;
const SORT_ITERS_LARGE: usize = 2_000;
const SFX_ITERS_1: usize = 120_000;
const SFX_ITERS_4: usize = 40_000;
const SFX_ITERS_16: usize = 8_000;
const TMESH_ITERS: usize = 20_000;
const INPUT_TEMP_ITERS: usize = 500_000;
const ACTIVE_SFX_ITERS: usize = 100_000;
const DRAW_PREP_RESERVE_ITERS: usize = 12_000;
const DRAW_PREP_TMESH_ITERS: usize = 25_000;
const TMESH_REPACK_ITERS: usize = 30_000;
const TEXTURE_LOOKUP_ITERS: usize = 12_000;
const COMPOSE_TEXTURE_LOOKUP_ITERS: usize = 20_000;
const COMPOSE_REGISTRY_CHURN_ITERS: usize = 1_500;
const COMPOSE_TEXT_CACHE_ITERS: usize = 800;
const TEXT_CACHE_CHURN_POOL: usize = 512;
const MARKER_SCAN_ITERS: usize = 80_000;
const SHADOW_BUILD_ITERS: usize = 12_000;
const SFX_PLAY_ITERS: usize = 80_000;
const INPUT_LOCK_ITERS: usize = 500_000;
const VIDEO_FRAME_ITERS: usize = 800;
const VIDEO_QUEUE_ITERS: usize = 1_200;
const AUDIO_CLOCK_ITERS: usize = 600_000;
const CLIP_RECYCLE_ITERS: usize = 18_000;
const CLIP_FAST_ACCEPT_ITERS: usize = 20_000;
const SHADOW_TMESH_ITERS: usize = 4_000;

struct CountingAlloc {
    alloc_calls: AtomicU64,
    dealloc_calls: AtomicU64,
    realloc_calls: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
    live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
    measure_peak_live_bytes: AtomicU64,
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    alloc_calls: u64,
    dealloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    free_bytes: u64,
    live_bytes: u64,
    measure_peak_live_bytes: u64,
}

#[derive(Clone, Copy)]
struct AllocDelta {
    alloc_calls: u64,
    dealloc_calls: u64,
    realloc_calls: u64,
    alloc_bytes: u64,
    free_bytes: u64,
    live_bytes: u64,
    peak_live_delta: u64,
}

struct BenchResult {
    name: String,
    iters: usize,
    elapsed: Duration,
    alloc: AllocDelta,
    checksum: u64,
}

#[derive(Clone)]
struct SortScratch {
    z_counts: Vec<usize>,
    z_perm: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TMeshGeomKey {
    ptr: usize,
    len: usize,
}

type TMeshGeomMap = HashMap<TMeshGeomKey, (u32, u32), BuildHasherDefault<XxHash64>>;
type FastU64Map<V> = HashMap<u64, V, BuildHasherDefault<XxHash64>>;
type FastUsizeMap<V> = HashMap<usize, V, BuildHasherDefault<XxHash64>>;

type TextureHandle = u64;

#[derive(Clone, Copy)]
enum BlendMode {
    Alpha,
}

#[derive(Clone)]
struct RenderObject {
    object_type: ObjectType,
    texture_handle: TextureHandle,
    transform: [f32; 16],
    blend: BlendMode,
    z: i16,
    order: u32,
    camera: u8,
}

#[derive(Clone)]
enum ObjectType {
    Sprite {
        center: [f32; 4],
        size: [f32; 2],
        rot_sin_cos: [f32; 2],
        tint: [f32; 4],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        local_offset: [f32; 2],
        local_offset_rot_sin_cos: [f32; 2],
        edge_fade: [f32; 4],
    },
}

#[derive(Clone, Copy)]
struct TexturedMeshVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    tex_matrix_scale: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone)]
enum ClipTMeshVertices {
    Shared(Arc<[TexturedMeshVertex]>),
    Transient(Vec<TexturedMeshVertex>),
}

#[derive(Clone)]
struct ClipTMeshObj {
    tint: [f32; 4],
    vertices: ClipTMeshVertices,
}

#[derive(Clone, Copy)]
enum PrepObj {
    Sprite { texture: TextureHandle },
    Mesh { vertices: usize },
    TMeshTransient { vertices: usize, geom_id: usize },
    TMeshCached { vertices: usize, cache_key: u64 },
}

#[derive(Default)]
struct PrepReserveScratch {
    sprite_instances: Vec<u64>,
    mesh_vertices: Vec<u64>,
    tmesh_vertices: Vec<u64>,
    tmesh_instances: Vec<u64>,
    ops: Vec<u64>,
    transient_tmesh_geom: TMeshGeomMap,
    cached_tmesh: FastU64Map<bool>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TexturedMeshVertexRaw {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    tex_matrix_scale: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TexturedMeshVertexGpu {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    tex_matrix_scale: [f32; 2],
}

#[derive(Clone, Copy)]
struct TextureSlot {
    marker: u64,
}

struct TextureLookupSim {
    handles: HashMap<String, TextureHandle, BuildHasherDefault<XxHash64>>,
    frame_handles: FastUsizeMap<TextureHandle>,
}

#[derive(Clone, Copy)]
struct SpriteSim {
    center: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    texture: TextureHandle,
}

struct ShadowActorSim {
    len: f32,
    color: [f32; 4],
    child: Box<SpriteSim>,
}

#[derive(Clone, Copy)]
struct SpriteDrawSim {
    sprite: SpriteSim,
    offset: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone)]
struct SfxCommandSim {
    data: Arc<[i16]>,
    lane: u8,
}

#[derive(Clone, Copy)]
struct MusicMapSegSim {
    stream_frame_start: i64,
    frames: i64,
    music_start_sec: f64,
    music_sec_per_frame: f64,
}

#[derive(Default)]
struct PlaybackPosMapSim {
    queue: VecDeque<MusicMapSegSim>,
    backlog_frames: i64,
}

struct QueuedVideoFrameSim {
    pts_sec: f32,
    bytes: Vec<u8>,
}

thread_local! {
    static INPUT_MAP_CACHE_SIM: RefCell<(u64, u64)> = RefCell::new((0, 0));
    static INPUT_DEBOUNCE_STATE_SIM: RefCell<u64> = const { RefCell::new(0) };
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            alloc_calls: AtomicU64::new(0),
            dealloc_calls: AtomicU64::new(0),
            realloc_calls: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
            measure_peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn begin_measurement(&self) -> AllocSnapshot {
        let live = self.live_bytes.load(Ordering::Relaxed);
        self.measure_peak_live_bytes.store(live, Ordering::Relaxed);
        self.snapshot()
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            alloc_calls: self.alloc_calls.load(Ordering::Relaxed),
            dealloc_calls: self.dealloc_calls.load(Ordering::Relaxed),
            realloc_calls: self.realloc_calls.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
            measure_peak_live_bytes: self.measure_peak_live_bytes.load(Ordering::Relaxed),
        }
    }

    fn add_live(&self, size: usize) {
        let live = self.live_bytes.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        update_peak(&self.peak_live_bytes, live);
        update_peak(&self.measure_peak_live_bytes, live);
    }

    fn sub_live(&self, size: usize) {
        let _ = self.live_bytes.fetch_sub(size as u64, Ordering::Relaxed);
    }
}

impl AllocSnapshot {
    fn diff(self, start: Self) -> AllocDelta {
        AllocDelta {
            alloc_calls: self.alloc_calls.saturating_sub(start.alloc_calls),
            dealloc_calls: self.dealloc_calls.saturating_sub(start.dealloc_calls),
            realloc_calls: self.realloc_calls.saturating_sub(start.realloc_calls),
            alloc_bytes: self.alloc_bytes.saturating_sub(start.alloc_bytes),
            free_bytes: self.free_bytes.saturating_sub(start.free_bytes),
            live_bytes: self.live_bytes.saturating_sub(start.live_bytes),
            peak_live_delta: self
                .measure_peak_live_bytes
                .saturating_sub(start.measure_peak_live_bytes),
        }
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.alloc_calls.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
            self.add_live(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        self.dealloc_calls.fetch_add(1, Ordering::Relaxed);
        self.free_bytes
            .fetch_add(layout.size() as u64, Ordering::Relaxed);
        self.sub_live(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.realloc_calls.fetch_add(1, Ordering::Relaxed);
            if new_size >= old.size() {
                let delta = new_size - old.size();
                self.alloc_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.add_live(delta);
            } else {
                let delta = old.size() - new_size;
                self.free_bytes.fetch_add(delta as u64, Ordering::Relaxed);
                self.sub_live(delta);
            }
        }
        out
    }
}

fn main() {
    println!("engine perf microbench");
    println!("synthetic targeted checks for review recommendations\n");

    bench_sorting();
    bench_draw_prep_reserves();
    bench_tmesh_repack();
    bench_texture_lookup();
    bench_compose_texture_lookup();
    bench_compose_registry_churn();
    bench_text_layout_cache_churn();
    bench_marker_scan();
    bench_shadow_build();
    bench_sfx_play_path();
    bench_input_locks();
    bench_audio_clock_map();
    bench_video_frame_alloc();
    bench_video_queue_handoff();
    bench_clip_vertex_recycle();
    bench_sfx_mix();
    bench_transient_tmesh();
    bench_active_sfx_growth();
    bench_input_temp_vecs();
}

fn bench_sorting() {
    println!("render object sort");
    run_sort_pair(
        "sorted 512",
        make_sort_objects(512, SortPattern::Sorted),
        SORT_ITERS_SMALL,
    );
    run_sort_pair(
        "dense z shuffled 512",
        make_sort_objects(512, SortPattern::DenseShuffled),
        SORT_ITERS_SMALL,
    );
    run_sort_pair(
        "same z shuffled order 512",
        make_sort_objects(512, SortPattern::SameZShuffledOrder),
        SORT_ITERS_SMALL,
    );
    run_sort_pair(
        "sparse z shuffled 512",
        make_sort_objects(512, SortPattern::SparseShuffled),
        SORT_ITERS_SMALL,
    );
    run_sort_pair(
        "dense z shuffled 4096",
        make_sort_objects(4096, SortPattern::DenseShuffled),
        SORT_ITERS_LARGE,
    );
    println!();
}

fn bench_sfx_mix() {
    println!("audio SFX mix");
    run_sfx_pair("1 active sfx", 1, SFX_ITERS_1);
    run_sfx_pair("4 active sfx", 4, SFX_ITERS_4);
    run_sfx_pair("16 active sfx", 16, SFX_ITERS_16);
    println!();
}

fn bench_draw_prep_reserves() {
    println!("draw-prep scratch reserve policy");
    run_prep_reserve_pair(
        "sprite-only 4096 cold",
        &make_prep_shape(4096, PrepShape::SpriteOnly),
        true,
    );
    run_prep_reserve_pair(
        "sprite-only 4096 retained",
        &make_prep_shape(4096, PrepShape::SpriteOnly),
        false,
    );
    run_prep_reserve_pair(
        "mixed 512 retained",
        &make_prep_shape(512, PrepShape::Mixed),
        false,
    );
    run_prep_reserve_pair(
        "cached tmesh 1024 retained",
        &make_prep_shape(1024, PrepShape::CachedTMesh),
        false,
    );
    println!();
}

fn bench_tmesh_repack() {
    println!("cached textured mesh upload packing");
    run_tmesh_repack_pair("cached geom 256 verts", 256);
    run_tmesh_repack_pair("cached geom 2048 verts", 2048);
    println!();
}

fn bench_texture_lookup() {
    println!("texture handle lookup");
    run_texture_lookup_pair("64 textures, 4096 draw ops", 64, 4096);
    run_texture_lookup_pair("1024 textures, 4096 draw ops", 1024, 4096);
    println!();
}

fn bench_compose_texture_lookup() {
    println!("compose texture key lookup");
    run_compose_texture_lookup_pair("64 textures, 4096 sprites", 64, 4096);
    run_compose_texture_lookup_pair("1024 textures, 4096 sprites", 1024, 4096);
    println!();
}

fn bench_compose_registry_churn() {
    println!("compose texture registry generation churn");
    let scenario = compose_scenarios::build_scenario("perf-texture-lookup")
        .expect("perf-texture-lookup scenario should exist");
    assets::register_texture_dims("__engine_perf_video_frame", 1280, 720);

    let mut stable_cache = compose::TextLayoutCache::default();
    let mut stable_scratch = compose::ComposeScratch::default();
    let stable = bench(
        "stable registry retained compose",
        COMPOSE_REGISTRY_CHURN_ITERS,
        || {
            let render = compose::build_screen_cached_with_scratch(
                black_box(&scenario.actors),
                scenario.clear_color,
                &scenario.metrics,
                &scenario.fonts,
                scenario.total_elapsed,
                &mut stable_cache,
                &mut stable_scratch,
            );
            checksum_gfx_render(black_box(&render))
        },
    );

    let mut churn_cache = compose::TextLayoutCache::default();
    let mut churn_scratch = compose::ComposeScratch::default();
    let churn = bench(
        "same dims register before compose",
        COMPOSE_REGISTRY_CHURN_ITERS,
        || {
            assets::register_texture_dims("__engine_perf_video_frame", 1280, 720);
            let render = compose::build_screen_cached_with_scratch(
                black_box(&scenario.actors),
                scenario.clear_color,
                &scenario.metrics,
                &scenario.fonts,
                scenario.total_elapsed,
                &mut churn_cache,
                &mut churn_scratch,
            );
            checksum_gfx_render(black_box(&render))
        },
    );

    print_result(&stable);
    print_result(&churn);
    print_ratio("stable registry vs repeated register_dims", &churn, &stable);
    println!();
}

fn bench_text_layout_cache_churn() {
    println!("compose text layout cache churn");
    let stable_scenario = compose_scenarios::build_scenario("perf-text-plain")
        .expect("perf-text-plain scenario should exist");
    let mut stable_cache = compose::TextLayoutCache::default();
    let mut stable_scratch = compose::ComposeScratch::default();
    let stable = bench(
        "stable shared text retained cache",
        COMPOSE_TEXT_CACHE_ITERS,
        || {
            stable_cache.begin_frame_stats();
            let render = compose::build_screen_cached_with_scratch(
                black_box(&stable_scenario.actors),
                stable_scenario.clear_color,
                &stable_scenario.metrics,
                &stable_scenario.fonts,
                stable_scenario.total_elapsed,
                &mut stable_cache,
                &mut stable_scratch,
            );
            let stats = stable_cache.frame_stats();
            checksum_gfx_render(black_box(&render))
                .wrapping_add(u64::from(stats.shared_hits))
                .wrapping_add((u64::from(stats.misses)) << 16)
                .wrapping_add((u64::from(stats.built_glyphs)) << 32)
        },
    );

    let mut churn_scenario = compose_scenarios::build_scenario("perf-text-plain")
        .expect("perf-text-plain scenario should exist");
    let text_pool = make_text_pool(TEXT_CACHE_CHURN_POOL);
    let mut churn_cache = compose::TextLayoutCache::new(64);
    let mut churn_scratch = compose::ComposeScratch::default();
    let mut frame = 0usize;
    let churn = bench(
        "rotating shared text, cache capped 64",
        COMPOSE_TEXT_CACHE_ITERS,
        || {
            let text_count = retarget_text_contents(&mut churn_scenario.actors, &text_pool, frame);
            frame = frame.wrapping_add(1);
            churn_cache.begin_frame_stats();
            let render = compose::build_screen_cached_with_scratch(
                black_box(&churn_scenario.actors),
                churn_scenario.clear_color,
                &churn_scenario.metrics,
                &churn_scenario.fonts,
                churn_scenario.total_elapsed,
                &mut churn_cache,
                &mut churn_scratch,
            );
            let stats = churn_cache.frame_stats();
            checksum_gfx_render(black_box(&render))
                .wrapping_add(text_count as u64)
                .wrapping_add((u64::from(stats.shared_hits)) << 8)
                .wrapping_add((u64::from(stats.misses)) << 24)
                .wrapping_add((u64::from(stats.built_glyphs)) << 40)
        },
    );

    print_result(&stable);
    print_result(&churn);
    print_ratio(
        "stable shared text vs rotating capped cache",
        &churn,
        &stable,
    );
    println!();
}

fn bench_marker_scan() {
    println!("text marker replacement scan");
    let plain = make_marker_texts(128, false);
    let marked = make_marker_texts(128, true);

    let current_plain = bench(
        "plain text: always replace_markers",
        MARKER_SCAN_ITERS,
        || replace_markers_current(black_box(&plain)),
    );
    let skip_plain = bench(
        "plain text: contains('&') fast path",
        MARKER_SCAN_ITERS,
        || replace_markers_skip_plain(black_box(&plain)),
    );
    let current_marked = bench(
        "marked text: always replace_markers",
        MARKER_SCAN_ITERS,
        || replace_markers_current(black_box(&marked)),
    );
    let skip_marked = bench(
        "marked text: contains('&') fast path",
        MARKER_SCAN_ITERS,
        || replace_markers_skip_plain(black_box(&marked)),
    );

    print_result(&current_plain);
    print_result(&skip_plain);
    print_ratio("plain fast path vs current", &current_plain, &skip_plain);
    print_result(&current_marked);
    print_result(&skip_marked);
    print_ratio("marked fast path vs current", &current_marked, &skip_marked);
    println!();
}

fn bench_shadow_build() {
    println!("shadow actor build");
    run_real_shadow_build_pair("real Actor 256 shadowed sprites", 256);
    run_shadow_build_pair("64 shadowed sprites", 64);
    run_shadow_build_pair("256 shadowed sprites", 256);
    run_shadow_transient_mesh_pair("128 transient meshes x 96 verts", 128, 96);
    run_shadow_transient_mesh_pair("512 transient meshes x 24 verts", 512, 24);
    println!();
}

fn bench_sfx_play_path() {
    println!("SFX play dispatch path");
    let data: Arc<[i16]> = Arc::from(make_i16_samples(2048));
    let mut map = HashMap::new();
    map.insert("assist_tick".to_string(), Arc::clone(&data));
    map.insert("effect".to_string(), Arc::clone(&data));
    let cache = Mutex::new(map);
    let (tx, rx) = mpsc::channel::<SfxCommandSim>();
    let paths = ["assist_tick", "effect", "assist_tick", "effect"];

    let current = bench("Mutex<HashMap<String, Arc>> + mpsc", SFX_PLAY_ITERS, || {
        let mut checksum = 0u64;
        for (idx, path) in paths.iter().enumerate() {
            let data = {
                let cache = cache.lock().unwrap();
                Arc::clone(cache.get(*path).unwrap())
            };
            tx.send(SfxCommandSim {
                data,
                lane: idx as u8,
            })
            .unwrap();
        }
        while let Ok(cmd) = rx.try_recv() {
            checksum = checksum
                .wrapping_mul(131)
                .wrapping_add(cmd.data.len() as u64)
                .wrapping_add(cmd.lane as u64);
        }
        checksum
    });

    let (sync_tx, sync_rx) = mpsc::sync_channel::<SfxCommandSim>(128);
    let bounded = bench(
        "Mutex<HashMap<String, Arc>> + bounded try_send",
        SFX_PLAY_ITERS,
        || {
            let mut checksum = 0u64;
            for (idx, path) in paths.iter().enumerate() {
                let data = {
                    let cache = cache.lock().unwrap();
                    Arc::clone(cache.get(*path).unwrap())
                };
                if sync_tx
                    .try_send(SfxCommandSim {
                        data,
                        lane: idx as u8,
                    })
                    .is_err()
                {
                    checksum = checksum.wrapping_add(1);
                }
            }
            while let Ok(cmd) = sync_rx.try_recv() {
                checksum = checksum
                    .wrapping_mul(131)
                    .wrapping_add(cmd.data.len() as u64)
                    .wrapping_add(cmd.lane as u64);
            }
            checksum
        },
    );

    let mut queue = Vec::with_capacity(8);
    let direct = bench("preloaded Arc + reused queue", SFX_PLAY_ITERS, || {
        queue.clear();
        for idx in 0..paths.len() {
            queue.push(SfxCommandSim {
                data: Arc::clone(&data),
                lane: idx as u8,
            });
        }
        checksum_sfx_commands(&queue)
    });

    print_result(&current);
    print_result(&bounded);
    print_ratio("bounded try_send vs current", &current, &bounded);
    print_result(&direct);
    print_ratio("preloaded queue vs current", &current, &direct);
    println!();
}

fn bench_input_locks() {
    println!("input map/debounce locking");
    let keymap = RwLock::new(0x9e37_79b9_u64);
    let keymap_generation = AtomicU64::new(1);
    let debounce_current = Mutex::new(0u64);
    let current = bench("RwLock read + Mutex debounce", INPUT_LOCK_ITERS, || {
        let mut checksum = 0u64;
        for event in 0..8u64 {
            let map = *keymap.read().unwrap();
            let mut state = debounce_current.lock().unwrap();
            *state = state.wrapping_mul(131).wrapping_add(map ^ event);
            checksum = checksum.wrapping_add(*state);
        }
        checksum
    });

    let debounce_cached = Mutex::new(0u64);
    let cached = bench(
        "thread-local keymap cache + Mutex debounce",
        INPUT_LOCK_ITERS,
        || {
            let mut checksum = 0u64;
            for event in 0..8u64 {
                let map = cached_input_map(&keymap, &keymap_generation);
                let mut state = debounce_cached.lock().unwrap();
                *state = state.wrapping_mul(131).wrapping_add(map ^ event);
                checksum = checksum.wrapping_add(*state);
            }
            checksum
        },
    );

    INPUT_DEBOUNCE_STATE_SIM.with(|state| *state.borrow_mut() = 0);
    let cached_debounce = bench(
        "thread-local keymap + debounce state",
        INPUT_LOCK_ITERS,
        || {
            let mut checksum = 0u64;
            for event in 0..8u64 {
                let map = cached_input_map(&keymap, &keymap_generation);
                INPUT_DEBOUNCE_STATE_SIM.with(|state| {
                    let mut state = state.borrow_mut();
                    *state = state.wrapping_mul(131).wrapping_add(map ^ event);
                    checksum = checksum.wrapping_add(*state);
                });
            }
            checksum
        },
    );

    let map = 0x9e37_79b9_u64;
    let mut state = 0u64;
    let direct = bench("direct owned input state", INPUT_LOCK_ITERS, || {
        let mut checksum = 0u64;
        for event in 0..8u64 {
            state = state.wrapping_mul(131).wrapping_add(map ^ event);
            checksum = checksum.wrapping_add(state);
        }
        checksum
    });

    print_result(&current);
    print_result(&cached);
    print_ratio("TLS keymap cache vs locks", &current, &cached);
    print_result(&cached_debounce);
    print_ratio("TLS keymap + debounce vs locks", &current, &cached_debounce);
    print_result(&direct);
    print_ratio("direct state vs locks", &current, &direct);
    println!();
}

fn bench_audio_clock_map() {
    println!("audio playback position map lookup");
    let seed = make_music_map_segments(256);
    let stream_span = seed
        .last()
        .map(|seg| seg.stream_frame_start + seg.frames)
        .unwrap_or(1) as f64;

    let map = Mutex::new(playback_map_from_segments(&seed));
    let empty_pending = Vec::new();
    let mut stream_frame = 0.0;
    let double_lock = bench("double Mutex lock, empty drain", AUDIO_CLOCK_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..4 {
            stream_frame = (stream_frame + 735.0) % stream_span;
            checksum =
                checksum.wrapping_add(lookup_music_double_lock(&map, &empty_pending, stream_frame));
        }
        checksum
    });

    let map = Mutex::new(playback_map_from_segments(&seed));
    let mut stream_frame = 0.0;
    let single_lock = bench("single Mutex lock, empty drain", AUDIO_CLOCK_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..4 {
            stream_frame = (stream_frame + 735.0) % stream_span;
            checksum =
                checksum.wrapping_add(lookup_music_single_lock(&map, &empty_pending, stream_frame));
        }
        checksum
    });

    let mut map = playback_map_from_segments(&seed);
    let mut stream_frame = 0.0;
    let owned = bench("owned map, no Mutex", AUDIO_CLOCK_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..4 {
            stream_frame = (stream_frame + 735.0) % stream_span;
            checksum =
                checksum.wrapping_add(lookup_music_owned(&mut map, &empty_pending, stream_frame));
        }
        checksum
    });

    print_result(&double_lock);
    print_result(&single_lock);
    print_ratio("single lock vs double lock", &double_lock, &single_lock);
    print_result(&owned);
    print_ratio("owned map vs double lock", &double_lock, &owned);
    println!();
}

fn bench_video_frame_alloc() {
    println!("video decoded frame buffer");
    let frame_bytes = 1280 * 720 * 4;
    let mut byte = 0u8;
    let allocate = bench("vec![byte; 1280x720 RGBA]", VIDEO_FRAME_ITERS, || {
        byte = byte.wrapping_add(1);
        let raw = vec![black_box(byte); frame_bytes];
        checksum_bytes(black_box(&raw))
    });

    let mut raw = vec![0u8; frame_bytes];
    let mut byte = 0u8;
    let reuse = bench("reused RGBA Vec fill", VIDEO_FRAME_ITERS, || {
        byte = byte.wrapping_add(1);
        black_box(raw.as_mut_slice()).fill(black_box(byte));
        checksum_bytes(black_box(&raw))
    });

    print_result(&allocate);
    print_result(&reuse);
    print_ratio("reused buffer vs allocate", &allocate, &reuse);
    println!();
}

fn bench_video_queue_handoff() {
    println!("video decoded frame queue handoff");
    let frame_bytes = 640 * 360 * 4;
    let max_frames = 4usize;
    let frames_per_iter = 8usize;

    let queue = Mutex::new(VecDeque::<QueuedVideoFrameSim>::with_capacity(max_frames));
    let mut frame_index = 0u64;
    let allocate = bench("alloc Vec + Mutex<VecDeque>", VIDEO_QUEUE_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..frames_per_iter {
            let byte = frame_index as u8;
            let mut bytes = vec![0u8; frame_bytes];
            stamp_video_frame(&mut bytes, byte);
            checksum =
                checksum.wrapping_add(push_pop_video_frame(&queue, max_frames, frame_index, bytes));
            frame_index = frame_index.wrapping_add(1);
        }
        checksum
    });

    let queue = Mutex::new(VecDeque::<QueuedVideoFrameSim>::with_capacity(max_frames));
    let pool = Mutex::new(Vec::<Vec<u8>>::with_capacity(max_frames));
    let mut frame_index = 0u64;
    let pooled = bench("pooled Vec + Mutex<VecDeque>", VIDEO_QUEUE_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..frames_per_iter {
            let mut bytes = {
                let mut pool = pool.lock().unwrap();
                pool.pop().unwrap_or_else(|| vec![0u8; frame_bytes])
            };
            if bytes.len() != frame_bytes {
                bytes.resize(frame_bytes, 0);
            }
            stamp_video_frame(&mut bytes, frame_index as u8);
            checksum = checksum.wrapping_add(push_pop_video_frame_pooled(
                &queue,
                &pool,
                max_frames,
                frame_index,
                bytes,
            ));
            frame_index = frame_index.wrapping_add(1);
        }
        checksum
    });

    let mut queue = VecDeque::<QueuedVideoFrameSim>::with_capacity(max_frames);
    let mut pool = Vec::<Vec<u8>>::with_capacity(max_frames);
    let mut frame_index = 0u64;
    let owned_ring = bench("pooled Vec + owned VecDeque", VIDEO_QUEUE_ITERS, || {
        let mut checksum = 0u64;
        for _ in 0..frames_per_iter {
            let mut bytes = pool.pop().unwrap_or_else(|| vec![0u8; frame_bytes]);
            if bytes.len() != frame_bytes {
                bytes.resize(frame_bytes, 0);
            }
            stamp_video_frame(&mut bytes, frame_index as u8);
            checksum = checksum.wrapping_add(push_pop_video_frame_owned(
                &mut queue,
                &mut pool,
                max_frames,
                frame_index,
                bytes,
            ));
            frame_index = frame_index.wrapping_add(1);
        }
        checksum
    });

    print_result(&allocate);
    print_result(&pooled);
    print_ratio("pooled Vec vs allocate", &allocate, &pooled);
    print_result(&owned_ring);
    print_ratio("owned queue vs allocate", &allocate, &owned_ring);
    println!();
}

fn bench_clip_vertex_recycle() {
    println!("clipped transient vertex buffer recycle");
    let meshes = make_clip_meshes(96, 24);

    let local = bench(
        "single-mask local recycled Vecs",
        CLIP_RECYCLE_ITERS,
        || clip_meshes_local_pool(black_box(&meshes)),
    );

    let mut pool = Vec::<Vec<TexturedMeshVertex>>::new();
    let mut outputs = Vec::<Vec<TexturedMeshVertex>>::with_capacity(meshes.len());
    let reused = bench(
        "scratch recycled Vecs across frames",
        CLIP_RECYCLE_ITERS,
        || clip_meshes_reused_pool(black_box(&meshes), &mut pool, &mut outputs),
    );

    print_result(&local);
    print_result(&reused);
    print_ratio("scratch recycle vs local pool", &local, &reused);
    run_clip_fast_accept_pair("inside transient meshes 128 x 96", 128, 96);
    run_clip_fast_accept_pair("inside transient meshes 512 x 24", 512, 24);
    println!();
}

fn bench_transient_tmesh() {
    println!("transient textured mesh draw-prep dedupe");
    run_tmesh_pair("unique 64 meshes x 24 verts", 64, 24, false);
    run_tmesh_pair("duplicate 64 meshes x 24 verts", 64, 24, true);
    run_tmesh_pair("unique 256 meshes x 6 verts", 256, 6, false);
    run_tmesh_pair("duplicate 256 meshes x 6 verts", 256, 6, true);
    run_draw_prep_tmesh_prepare_pair(
        "prepare unique transient 256 meshes x 6 verts",
        256,
        6,
        false,
    );
    run_draw_prep_tmesh_prepare_pair(
        "prepare repeated shared invalid 256 meshes x 6 verts",
        256,
        6,
        true,
    );
    println!();
}

fn bench_active_sfx_growth() {
    println!("active_sfx vector growth");
    let data: Arc<[i16]> = Arc::from(make_i16_samples(2048));
    let cold = bench("cold Vec::new push 32", ACTIVE_SFX_ITERS, || {
        let mut active = Vec::new();
        let mut checksum = 0u64;
        for lane in 0..32 {
            active.push((Arc::clone(&data), lane * 3, lane as u8));
            checksum = checksum.wrapping_add(active.len() as u64);
        }
        checksum
    });
    let presized = bench(
        "new Vec::with_capacity(32) push 32",
        ACTIVE_SFX_ITERS,
        || {
            let mut active = Vec::with_capacity(32);
            let mut checksum = 0u64;
            for lane in 0..32 {
                active.push((Arc::clone(&data), lane * 3, lane as u8));
                checksum = checksum.wrapping_add(active.len() as u64);
            }
            checksum
        },
    );
    let mut active = Vec::with_capacity(32);
    let reused = bench("reused Vec capacity 32 push 32", ACTIVE_SFX_ITERS, || {
        active.clear();
        let mut checksum = 0u64;
        for lane in 0..32 {
            active.push((Arc::clone(&data), lane * 3, lane as u8));
            checksum = checksum.wrapping_add(active.len() as u64);
        }
        checksum
    });
    print_result(&cold);
    print_result(&presized);
    print_result(&reused);
    print_ratio("presized vs cold", &cold, &presized);
    print_ratio("reused vs cold", &cold, &reused);
    println!();
}

fn bench_input_temp_vecs() {
    println!("input backend temporary vectors");
    let fresh = bench("fresh Vecs per poll loop", INPUT_TEMP_ITERS, || {
        let mut hotplug = Vec::new();
        let mut remove = Vec::new();
        let mut key_remove = Vec::new();
        for i in 0..8 {
            hotplug.push(i);
        }
        for i in 0..3 {
            remove.push(i * 2);
        }
        for i in 0..2 {
            key_remove.push(i * 3);
        }
        (hotplug.len() ^ remove.len() ^ key_remove.len()) as u64
    });

    let mut hotplug = Vec::with_capacity(16);
    let mut remove = Vec::with_capacity(16);
    let mut key_remove = Vec::with_capacity(16);
    let reused = bench("reused Vecs per poll loop", INPUT_TEMP_ITERS, || {
        hotplug.clear();
        remove.clear();
        key_remove.clear();
        for i in 0..8 {
            hotplug.push(i);
        }
        for i in 0..3 {
            remove.push(i * 2);
        }
        for i in 0..2 {
            key_remove.push(i * 3);
        }
        (hotplug.len() ^ remove.len() ^ key_remove.len()) as u64
    });

    print_result(&fresh);
    print_result(&reused);
    print_ratio("reused vs fresh", &fresh, &reused);
    println!();
}

fn run_sort_pair(name: &str, base: Vec<RenderObject>, iters: usize) {
    let mut work = Vec::with_capacity(base.len());
    let mut current_scratch = SortScratch {
        z_counts: Vec::new(),
        z_perm: Vec::new(),
    };
    let mut simple_scratch = SortScratch {
        z_counts: Vec::new(),
        z_perm: Vec::new(),
    };

    let current = bench(format!("{name}: current z-bucket"), iters, || {
        work.clear();
        work.extend_from_slice(&base);
        sort_current(black_box(&mut work), &mut current_scratch);
        checksum_objects(&work)
    });

    let simple = bench(format!("{name}: sorted-check sort"), iters, || {
        work.clear();
        work.extend_from_slice(&base);
        sort_simple(black_box(&mut work), &mut simple_scratch);
        checksum_objects(&work)
    });

    print_result(&current);
    print_result(&simple);
    print_ratio("candidate vs current", &current, &simple);
}

fn run_sfx_pair(name: &str, active_count: usize, iters: usize) {
    let samples = 1024 * 2;
    let music = make_f32_samples(samples);
    let sfx = make_i16_samples(samples + active_count * 17);
    let starts = (0..active_count)
        .map(|idx| (idx * 17) % 97)
        .collect::<Vec<_>>();
    let mut out = vec![0.0f32; samples];

    let current = bench(format!("{name}: clamp inside each add"), iters, || {
        mix_sfx_current(black_box(&music), black_box(&sfx), &starts, &mut out)
    });
    let final_clamp = bench(format!("{name}: clamp once at output"), iters, || {
        mix_sfx_final_clamp(black_box(&music), black_box(&sfx), &starts, &mut out)
    });

    print_result(&current);
    print_result(&final_clamp);
    print_ratio("final clamp vs current", &current, &final_clamp);
}

fn run_tmesh_pair(name: &str, mesh_count: usize, verts_per_mesh: usize, duplicate: bool) {
    let storage = make_tmesh_storage(mesh_count, verts_per_mesh, duplicate);
    let slices = if duplicate {
        (0..mesh_count)
            .map(|_| storage[0].as_slice())
            .collect::<Vec<_>>()
    } else {
        storage.iter().map(Vec::as_slice).collect::<Vec<_>>()
    };
    let mut map = TMeshGeomMap::default();
    let mut out = Vec::with_capacity(mesh_count * verts_per_mesh);

    let dedupe = bench(format!("{name}: HashMap dedupe"), TMESH_ITERS, || {
        copy_tmesh_dedupe(black_box(&slices), &mut map, &mut out)
    });
    let direct = bench(format!("{name}: direct append"), TMESH_ITERS, || {
        copy_tmesh_direct(black_box(&slices), &mut out)
    });

    print_result(&dedupe);
    print_result(&direct);
    print_ratio("direct vs dedupe", &dedupe, &direct);
}

fn run_draw_prep_tmesh_prepare_pair(
    name: &str,
    mesh_count: usize,
    verts_per_mesh: usize,
    shared_invalid: bool,
) {
    let render_list = make_draw_prep_tmesh_render_list(mesh_count, verts_per_mesh, shared_invalid);
    let mut scratch = gfx::draw_prep::DrawScratch::with_capacity(
        0,
        0,
        mesh_count * verts_per_mesh,
        mesh_count,
        mesh_count,
    );

    let result = bench(name, DRAW_PREP_TMESH_ITERS, || {
        let stats = gfx::draw_prep::prepare(black_box(&render_list), &mut scratch, |_, _| false);
        checksum_draw_prep_scratch(&scratch)
            .wrapping_add(stats.dynamic_upload_vertices)
            .wrapping_add(stats.cached_upload_vertices << 32)
    });

    print_result(&result);
}

fn run_prep_reserve_pair(name: &str, objects: &[PrepObj], cold: bool) {
    if cold {
        let current = bench(
            format!("{name}: current upfront reserve"),
            DRAW_PREP_RESERVE_ITERS,
            || {
                let mut scratch = PrepReserveScratch::default();
                prep_reserve_current(black_box(objects), &mut scratch)
            },
        );
        let lazy = bench(
            format!("{name}: lazy typed reserve"),
            DRAW_PREP_RESERVE_ITERS,
            || {
                let mut scratch = PrepReserveScratch::default();
                prep_reserve_lazy(black_box(objects), &mut scratch)
            },
        );
        print_result(&current);
        print_result(&lazy);
        print_ratio("lazy vs current", &current, &lazy);
        return;
    }

    let mut current_scratch = PrepReserveScratch::default();
    let mut lazy_scratch = PrepReserveScratch::default();
    prep_reserve_current(objects, &mut current_scratch);
    prep_reserve_lazy(objects, &mut lazy_scratch);

    let current = bench(
        format!("{name}: current upfront reserve"),
        DRAW_PREP_RESERVE_ITERS,
        || prep_reserve_current(black_box(objects), &mut current_scratch),
    );
    let lazy = bench(
        format!("{name}: lazy typed reserve"),
        DRAW_PREP_RESERVE_ITERS,
        || prep_reserve_lazy(black_box(objects), &mut lazy_scratch),
    );
    print_result(&current);
    print_result(&lazy);
    print_ratio("lazy vs current", &current, &lazy);
}

fn run_tmesh_repack_pair(name: &str, vertices: usize) {
    let current = make_tmesh_storage(1, vertices, false);
    let current = current[0].as_slice();
    let gpu = make_gpu_tmesh_storage(vertices);
    let mut raw = Vec::with_capacity(vertices);

    let alloc_repack = bench(
        format!("{name}: current alloc+repack"),
        TMESH_REPACK_ITERS,
        || repack_tmesh_alloc(black_box(current)),
    );
    let reused_repack = bench(
        format!("{name}: reused Vec repack"),
        TMESH_REPACK_ITERS,
        || repack_tmesh_reuse(black_box(current), &mut raw),
    );
    let direct = bench(
        format!("{name}: direct GPU layout"),
        TMESH_REPACK_ITERS,
        || checksum_gpu_tmesh(black_box(&gpu)),
    );

    print_result(&alloc_repack);
    print_result(&reused_repack);
    print_result(&direct);
    print_ratio("reused repack vs alloc", &alloc_repack, &reused_repack);
    print_ratio("direct layout vs alloc", &alloc_repack, &direct);
}

fn run_texture_lookup_pair(name: &str, texture_count: usize, op_count: usize) {
    let handles = make_texture_handles(texture_count, op_count);
    let map = make_texture_map(texture_count);
    let slots = make_texture_slots(texture_count);

    let hash = bench(format!("{name}: HashMap get"), TEXTURE_LOOKUP_ITERS, || {
        lookup_textures_hash(black_box(&handles), black_box(&map))
    });
    let dense = bench(
        format!("{name}: dense Vec index"),
        TEXTURE_LOOKUP_ITERS,
        || lookup_textures_dense(black_box(&handles), black_box(&slots)),
    );

    print_result(&hash);
    print_result(&dense);
    print_ratio("dense vs hash", &hash, &dense);
}

fn run_compose_texture_lookup_pair(name: &str, texture_count: usize, op_count: usize) {
    let (keys, handles, mut cache) = make_compose_texture_work(texture_count, op_count);
    let frame_ptr = bench(
        format!("{name}: frame ptr cache"),
        COMPOSE_TEXTURE_LOOKUP_ITERS,
        || lookup_compose_textures_frame_ptr(black_box(&keys), &mut cache),
    );
    let direct = bench(
        format!("{name}: direct TextureHandle"),
        COMPOSE_TEXTURE_LOOKUP_ITERS,
        || lookup_compose_textures_direct(black_box(&handles)),
    );

    print_result(&frame_ptr);
    print_result(&direct);
    print_ratio("direct handles vs frame ptr", &frame_ptr, &direct);
}

fn run_shadow_build_pair(name: &str, count: usize) {
    let boxed = bench(
        format!("{name}: Box<child> actor"),
        SHADOW_BUILD_ITERS,
        || build_shadow_boxed(count),
    );
    let direct = bench(
        format!("{name}: direct duplicate draws"),
        SHADOW_BUILD_ITERS,
        || build_shadow_direct(count),
    );
    print_result(&boxed);
    print_result(&direct);
    print_ratio("direct draws vs boxed actor", &boxed, &direct);
}

fn run_real_shadow_build_pair(name: &str, count: usize) {
    let boxed = bench(
        format!("{name}: Actor::Shadow Box"),
        SHADOW_BUILD_ITERS,
        || build_real_shadow_boxed(count),
    );
    let inline = bench(
        format!("{name}: inline sprite shadow"),
        SHADOW_BUILD_ITERS,
        || build_real_shadow_inline(count),
    );
    print_result(&boxed);
    print_result(&inline);
    print_ratio("inline actor shadow vs boxed", &boxed, &inline);
}

fn run_shadow_transient_mesh_pair(name: &str, mesh_count: usize, verts_per_mesh: usize) {
    let base = make_clip_tmesh_objects(mesh_count, verts_per_mesh);
    let mut out = Vec::with_capacity(mesh_count);
    let current = bench(
        format!("{name}: clone transient Vec"),
        SHADOW_TMESH_ITERS,
        || {
            let mut meshes = base.clone();
            shadow_tmesh_current(black_box(&mut meshes), &mut out)
        },
    );
    let shared = bench(
        format!("{name}: convert to shared"),
        SHADOW_TMESH_ITERS,
        || {
            let mut meshes = base.clone();
            shadow_tmesh_shared(black_box(&mut meshes), &mut out)
        },
    );
    print_result(&current);
    print_result(&shared);
    print_ratio("shared shadow vs clone", &current, &shared);
}

#[derive(Clone, Copy)]
enum PrepShape {
    SpriteOnly,
    Mixed,
    CachedTMesh,
}

fn make_prep_shape(len: usize, shape: PrepShape) -> Vec<PrepObj> {
    let mut objects = Vec::with_capacity(len);
    for i in 0..len {
        objects.push(match shape {
            PrepShape::SpriteOnly => PrepObj::Sprite {
                texture: 1 + (i % 32) as TextureHandle,
            },
            PrepShape::Mixed => match i % 4 {
                0 | 1 => PrepObj::Sprite {
                    texture: 1 + (i % 32) as TextureHandle,
                },
                2 => PrepObj::Mesh { vertices: 6 },
                _ => PrepObj::TMeshTransient {
                    vertices: 6,
                    geom_id: i,
                },
            },
            PrepShape::CachedTMesh => PrepObj::TMeshCached {
                vertices: 6,
                cache_key: 0x1000 + (i % 64) as u64,
            },
        });
    }
    objects
}

fn prep_reserve_current(objects: &[PrepObj], scratch: &mut PrepReserveScratch) -> u64 {
    let objects_len = objects.len();

    scratch.sprite_instances.clear();
    if scratch.sprite_instances.capacity() < objects_len {
        scratch.sprite_instances.reserve(reserve_gap(
            objects_len,
            scratch.sprite_instances.capacity(),
        ));
    }

    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();

    scratch.ops.clear();
    if scratch.ops.capacity() < objects_len {
        scratch
            .ops
            .reserve(reserve_gap(objects_len, scratch.ops.capacity()));
    }

    scratch.transient_tmesh_geom.clear();
    scratch.cached_tmesh.clear();

    prep_reserve_fill(objects, scratch, true)
}

fn prep_reserve_lazy(objects: &[PrepObj], scratch: &mut PrepReserveScratch) -> u64 {
    scratch.sprite_instances.clear();
    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();
    scratch.ops.clear();
    prep_reserve_fill(objects, scratch, false)
}

fn prep_reserve_fill(
    objects: &[PrepObj],
    scratch: &mut PrepReserveScratch,
    maps_cleared: bool,
) -> u64 {
    let mut maps_cleared = maps_cleared;
    let mut checksum = 0u64;
    for (i, &obj) in objects.iter().enumerate() {
        match obj {
            PrepObj::Sprite { texture } => {
                let start = scratch.sprite_instances.len() as u64;
                scratch.sprite_instances.push(texture);
                scratch.ops.push(start ^ texture);
                checksum = checksum.wrapping_mul(131).wrapping_add(start ^ texture);
            }
            PrepObj::Mesh { vertices } => {
                let start = scratch.mesh_vertices.len() as u64;
                for v in 0..vertices {
                    scratch.mesh_vertices.push((i as u64) ^ (v as u64));
                }
                scratch.ops.push(start ^ vertices as u64);
                checksum = checksum
                    .wrapping_mul(131)
                    .wrapping_add(start ^ vertices as u64);
            }
            PrepObj::TMeshTransient { vertices, geom_id } => {
                if !maps_cleared {
                    scratch.transient_tmesh_geom.clear();
                    scratch.cached_tmesh.clear();
                    maps_cleared = true;
                }
                let key = TMeshGeomKey {
                    ptr: geom_id,
                    len: vertices,
                };
                let (start, count) =
                    if let Some(&(start, count)) = scratch.transient_tmesh_geom.get(&key) {
                        (start, count)
                    } else {
                        let start = scratch.tmesh_vertices.len() as u32;
                        for v in 0..vertices {
                            scratch.tmesh_vertices.push((geom_id as u64) ^ (v as u64));
                        }
                        let count = vertices as u32;
                        scratch.transient_tmesh_geom.insert(key, (start, count));
                        (start, count)
                    };
                let instance_start = scratch.tmesh_instances.len() as u64;
                scratch.tmesh_instances.push(instance_start);
                scratch.ops.push(u64::from(start) ^ u64::from(count));
                checksum = checksum
                    .wrapping_mul(131)
                    .wrapping_add(u64::from(start) ^ u64::from(count));
            }
            PrepObj::TMeshCached {
                vertices,
                cache_key,
            } => {
                if !maps_cleared {
                    scratch.transient_tmesh_geom.clear();
                    scratch.cached_tmesh.clear();
                    maps_cleared = true;
                }
                let cached = if let Some(&cached) = scratch.cached_tmesh.get(&cache_key) {
                    cached
                } else {
                    scratch.cached_tmesh.insert(cache_key, true);
                    true
                };
                let instance_start = scratch.tmesh_instances.len() as u64;
                scratch.tmesh_instances.push(instance_start);
                scratch.ops.push(cache_key ^ vertices as u64);
                checksum = checksum
                    .wrapping_mul(131)
                    .wrapping_add(cache_key ^ vertices as u64 ^ cached as u64);
            }
        }
    }

    checksum
        .wrapping_add(scratch.sprite_instances.len() as u64)
        .wrapping_add((scratch.mesh_vertices.len() as u64) << 8)
        .wrapping_add((scratch.tmesh_vertices.len() as u64) << 16)
        .wrapping_add((scratch.tmesh_instances.len() as u64) << 24)
        .wrapping_add((scratch.ops.len() as u64) << 32)
}

#[inline(always)]
fn reserve_gap(want: usize, capacity: usize) -> usize {
    want.saturating_sub(capacity)
}

fn repack_tmesh_alloc(vertices: &[TexturedMeshVertex]) -> u64 {
    let mut raw = Vec::with_capacity(vertices.len());
    for v in vertices {
        raw.push(TexturedMeshVertexRaw {
            pos: v.pos,
            uv: v.uv,
            color: v.color,
            tex_matrix_scale: v.tex_matrix_scale,
        });
    }
    checksum_raw_tmesh(&raw)
}

fn repack_tmesh_reuse(
    vertices: &[TexturedMeshVertex],
    raw: &mut Vec<TexturedMeshVertexRaw>,
) -> u64 {
    raw.clear();
    if raw.capacity() < vertices.len() {
        raw.reserve(vertices.len() - raw.capacity());
    }
    for v in vertices {
        raw.push(TexturedMeshVertexRaw {
            pos: v.pos,
            uv: v.uv,
            color: v.color,
            tex_matrix_scale: v.tex_matrix_scale,
        });
    }
    checksum_raw_tmesh(raw)
}

fn make_gpu_tmesh_storage(len: usize) -> Vec<TexturedMeshVertexGpu> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(TexturedMeshVertexGpu {
            pos: [i as f32, (i % 17) as f32, 0.0],
            uv: [i as f32 * 0.01, (i % 31) as f32 * 0.01],
            color: [1.0, 0.5, 0.25, 1.0],
            tex_matrix_scale: [1.0, 1.0],
        });
    }
    out
}

fn checksum_raw_tmesh(vertices: &[TexturedMeshVertexRaw]) -> u64 {
    let mut out = 0u64;
    for v in vertices {
        out = out
            .wrapping_mul(131)
            .wrapping_add(v.pos[0].to_bits() as u64)
            .wrapping_add(v.uv[0].to_bits() as u64)
            .wrapping_add(v.color[3].to_bits() as u64)
            .wrapping_add(v.tex_matrix_scale[0].to_bits() as u64);
    }
    out
}

fn checksum_gpu_tmesh(vertices: &[TexturedMeshVertexGpu]) -> u64 {
    let mut out = 0u64;
    for v in vertices {
        out = out
            .wrapping_mul(131)
            .wrapping_add(v.pos[0].to_bits() as u64)
            .wrapping_add(v.uv[0].to_bits() as u64)
            .wrapping_add(v.color[3].to_bits() as u64)
            .wrapping_add(v.tex_matrix_scale[0].to_bits() as u64);
    }
    out
}

fn make_texture_handles(texture_count: usize, op_count: usize) -> Vec<TextureHandle> {
    let mut out = Vec::with_capacity(op_count);
    for i in 0..op_count {
        out.push(1 + (permute(i, texture_count) as TextureHandle));
    }
    out
}

fn make_texture_map(texture_count: usize) -> FastU64Map<TextureSlot> {
    let mut map = FastU64Map::default();
    map.reserve(texture_count);
    for i in 0..texture_count {
        let handle = 1 + i as TextureHandle;
        map.insert(
            handle,
            TextureSlot {
                marker: handle.wrapping_mul(17),
            },
        );
    }
    map
}

fn make_texture_slots(texture_count: usize) -> Vec<TextureSlot> {
    let mut slots = vec![TextureSlot { marker: 0 }; texture_count + 1];
    for i in 0..texture_count {
        let handle = 1 + i as TextureHandle;
        slots[handle as usize] = TextureSlot {
            marker: handle.wrapping_mul(17),
        };
    }
    slots
}

fn lookup_textures_hash(handles: &[TextureHandle], textures: &FastU64Map<TextureSlot>) -> u64 {
    let mut out = 0u64;
    for &handle in handles {
        if let Some(texture) = textures.get(&handle) {
            out = out.wrapping_mul(131).wrapping_add(texture.marker);
        }
    }
    out
}

fn lookup_textures_dense(handles: &[TextureHandle], textures: &[TextureSlot]) -> u64 {
    let mut out = 0u64;
    for &handle in handles {
        out = out
            .wrapping_mul(131)
            .wrapping_add(textures[handle as usize].marker);
    }
    out
}

impl TextureLookupSim {
    fn new(texture_count: usize) -> Self {
        let mut handles = HashMap::with_capacity_and_hasher(
            texture_count,
            BuildHasherDefault::<XxHash64>::default(),
        );
        for i in 0..texture_count {
            handles.insert(format!("tex_{i:04}"), 1 + i as TextureHandle);
        }
        Self {
            handles,
            frame_handles: FastUsizeMap::with_capacity_and_hasher(
                texture_count,
                BuildHasherDefault::default(),
            ),
        }
    }

    fn begin_frame(&mut self) {
        self.frame_handles.clear();
    }

    fn handle_with_ptr(&mut self, key: &str) -> TextureHandle {
        let key_ptr = key.as_ptr() as usize;
        if let Some(&handle) = self.frame_handles.get(&key_ptr) {
            return handle;
        }
        let handle = *self.handles.get(key).unwrap_or(&0);
        self.frame_handles.insert(key_ptr, handle);
        handle
    }
}

fn make_compose_texture_work(
    texture_count: usize,
    op_count: usize,
) -> (Vec<Arc<str>>, Vec<TextureHandle>, TextureLookupSim) {
    let atoms = (0..texture_count)
        .map(|i| Arc::<str>::from(format!("tex_{i:04}")))
        .collect::<Vec<_>>();
    let mut keys = Vec::with_capacity(op_count);
    let mut handles = Vec::with_capacity(op_count);
    for i in 0..op_count {
        let idx = permute(i, texture_count);
        keys.push(Arc::clone(&atoms[idx]));
        handles.push(1 + idx as TextureHandle);
    }
    (keys, handles, TextureLookupSim::new(texture_count))
}

fn lookup_compose_textures_frame_ptr(keys: &[Arc<str>], cache: &mut TextureLookupSim) -> u64 {
    cache.begin_frame();
    let mut out = 0u64;
    for key in keys {
        out = out
            .wrapping_mul(131)
            .wrapping_add(cache.handle_with_ptr(key.as_ref()));
    }
    out
}

fn lookup_compose_textures_direct(handles: &[TextureHandle]) -> u64 {
    let mut out = 0u64;
    for &handle in handles {
        out = out.wrapping_mul(131).wrapping_add(handle);
    }
    out
}

fn make_marker_texts(count: usize, marked: bool) -> Vec<String> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        if marked {
            out.push(format!("score {i:03} &#9654; combo &#9733;"));
        } else {
            out.push(format!("score {i:03} combo clear timing detail"));
        }
    }
    out
}

fn replace_markers_current(texts: &[String]) -> u64 {
    let mut out = 0u64;
    for text in texts {
        let replaced = font::replace_markers(text);
        out = out
            .wrapping_mul(131)
            .wrapping_add(replaced.as_ref().len() as u64);
    }
    out
}

fn replace_markers_skip_plain(texts: &[String]) -> u64 {
    let mut out = 0u64;
    for text in texts {
        let len = if text.as_bytes().contains(&b'&') {
            font::replace_markers(text).as_ref().len()
        } else {
            text.len()
        };
        out = out.wrapping_mul(131).wrapping_add(len as u64);
    }
    out
}

fn build_shadow_boxed(count: usize) -> u64 {
    let mut actors = Vec::with_capacity(count);
    for i in 0..count {
        let sprite = SpriteSim {
            center: [i as f32, i as f32 * 0.5],
            size: [32.0, 32.0],
            color: [1.0; 4],
            texture: 1 + (i % 16) as TextureHandle,
        };
        actors.push(ShadowActorSim {
            len: 2.0,
            color: [0.0, 0.0, 0.0, 0.5],
            child: Box::new(sprite),
        });
    }
    checksum_shadow_actors(&actors)
}

fn build_shadow_direct(count: usize) -> u64 {
    let mut draws = Vec::with_capacity(count * 2);
    for i in 0..count {
        let sprite = SpriteSim {
            center: [i as f32, i as f32 * 0.5],
            size: [32.0, 32.0],
            color: [1.0; 4],
            texture: 1 + (i % 16) as TextureHandle,
        };
        draws.push(SpriteDrawSim {
            sprite,
            offset: [2.0, 2.0],
            color: [0.0, 0.0, 0.0, 0.5],
        });
        draws.push(SpriteDrawSim {
            sprite,
            offset: [0.0, 0.0],
            color: sprite.color,
        });
    }
    checksum_sprite_draws(&draws)
}

fn build_real_shadow_boxed(count: usize) -> u64 {
    let mut actors = Vec::with_capacity(count);
    for i in 0..count {
        actors.push(Actor::Shadow {
            len: [2.0, -2.0],
            color: [0.0, 0.0, 0.0, 0.5],
            child: Box::new(real_sprite_actor(i, [0.0, 0.0])),
        });
    }
    checksum_real_actors(&actors)
}

fn build_real_shadow_inline(count: usize) -> u64 {
    let mut actors = Vec::with_capacity(count);
    for i in 0..count {
        actors.push(real_sprite_actor(i, [2.0, -2.0]));
    }
    checksum_real_actors(&actors)
}

fn real_sprite_actor(i: usize, shadow_len: [f32; 2]) -> Actor {
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [i as f32, i as f32 * 0.5],
        world_z: 0.0,
        size: [SizeSpec::Px(32.0), SizeSpec::Px(32.0)],
        source: SpriteSource::TextureStatic("bench_tex"),
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
        blend: GfxBlendMode::Alpha,
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
        shadow_len,
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        effect: anim::EffectState::default(),
    }
}

fn checksum_shadow_actors(actors: &[ShadowActorSim]) -> u64 {
    let mut out = 0u64;
    for actor in actors {
        out = out
            .wrapping_mul(131)
            .wrapping_add(actor.len.to_bits() as u64)
            .wrapping_add(actor.color[3].to_bits() as u64)
            .wrapping_add(actor.child.center[0].to_bits() as u64)
            .wrapping_add(actor.child.size[0].to_bits() as u64)
            .wrapping_add(actor.child.texture);
    }
    out
}

fn checksum_real_actors(actors: &[Actor]) -> u64 {
    let mut out = 0u64;
    for actor in actors {
        out = out
            .wrapping_mul(131)
            .wrapping_add(checksum_real_actor(actor));
    }
    out
}

fn checksum_real_actor(actor: &Actor) -> u64 {
    match actor {
        Actor::Sprite {
            offset,
            size,
            tint,
            shadow_len,
            shadow_color,
            ..
        } => {
            let size_bits = match size[0] {
                SizeSpec::Px(value) => value.to_bits() as u64,
                SizeSpec::Fill => 1,
            };
            (offset[0].to_bits() as u64)
                .wrapping_add(size_bits)
                .wrapping_add(tint[3].to_bits() as u64)
                .wrapping_add(shadow_len[0].to_bits() as u64)
                .wrapping_add(shadow_color[3].to_bits() as u64)
        }
        Actor::Shadow { len, color, child } => (len[0].to_bits() as u64)
            .wrapping_add(color[3].to_bits() as u64)
            .wrapping_add(checksum_real_actor(child)),
        _ => 0,
    }
}

fn checksum_sprite_draws(draws: &[SpriteDrawSim]) -> u64 {
    let mut out = 0u64;
    for draw in draws {
        out = out
            .wrapping_mul(131)
            .wrapping_add(draw.sprite.center[0].to_bits() as u64)
            .wrapping_add(draw.sprite.size[0].to_bits() as u64)
            .wrapping_add(draw.sprite.texture)
            .wrapping_add(draw.offset[0].to_bits() as u64)
            .wrapping_add(draw.color[3].to_bits() as u64);
    }
    out
}

fn checksum_sfx_commands(queue: &[SfxCommandSim]) -> u64 {
    let mut out = 0u64;
    for cmd in queue {
        out = out
            .wrapping_mul(131)
            .wrapping_add(cmd.data.len() as u64)
            .wrapping_add(cmd.lane as u64);
    }
    out
}

impl PlaybackPosMapSim {
    fn insert(&mut self, seg: MusicMapSegSim) {
        if seg.frames <= 0
            || !seg.music_start_sec.is_finite()
            || !seg.music_sec_per_frame.is_finite()
        {
            return;
        }
        if let Some(last) = self.queue.back_mut() {
            let contiguous_stream = last.stream_frame_start + last.frames == seg.stream_frame_start;
            let ratio_match = (last.music_sec_per_frame - seg.music_sec_per_frame).abs() <= 1e-9;
            let expected_music_start =
                last.music_start_sec + last.music_sec_per_frame * last.frames as f64;
            let music_contiguous = (expected_music_start - seg.music_start_sec).abs()
                <= seg.music_sec_per_frame.abs().max(1e-9);
            if contiguous_stream && ratio_match && music_contiguous {
                last.frames += seg.frames;
                self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
                return;
            }
        }
        self.backlog_frames = self.backlog_frames.saturating_add(seg.frames);
        self.queue.push_back(seg);
    }

    fn search(&self, stream_frame: f64) -> Option<(f64, f64)> {
        if self.queue.is_empty() || !stream_frame.is_finite() {
            return None;
        }
        let mut closest = None;
        let mut closest_dist = f64::INFINITY;
        for seg in &self.queue {
            let start = seg.stream_frame_start as f64;
            let end = start + seg.frames as f64;
            if stream_frame >= start && stream_frame < end {
                let diff = stream_frame - start;
                return Some((
                    seg.music_start_sec + diff * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let start_dist = (stream_frame - start).abs();
            if start_dist < closest_dist {
                closest_dist = start_dist;
                closest = Some((
                    seg.music_start_sec + (stream_frame - start) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
            let end_music = seg.music_start_sec + seg.music_sec_per_frame * seg.frames as f64;
            let end_dist = (stream_frame - end).abs();
            if end_dist < closest_dist {
                closest_dist = end_dist;
                closest = Some((
                    end_music + (stream_frame - end) * seg.music_sec_per_frame,
                    seg.music_sec_per_frame,
                ));
            }
        }
        closest
    }
}

fn make_music_map_segments(count: usize) -> Vec<MusicMapSegSim> {
    let mut out = Vec::with_capacity(count);
    let mut stream_frame_start = 0i64;
    let mut music_start_sec = 0.0f64;
    for i in 0..count {
        let frames = 256 + (i % 5) as i64 * 64;
        let music_sec_per_frame = if i % 23 == 0 {
            1.25 / 48_000.0
        } else {
            1.0 / 48_000.0
        };
        out.push(MusicMapSegSim {
            stream_frame_start,
            frames,
            music_start_sec,
            music_sec_per_frame,
        });
        stream_frame_start += frames;
        music_start_sec += frames as f64 * music_sec_per_frame;
    }
    out
}

fn playback_map_from_segments(segments: &[MusicMapSegSim]) -> PlaybackPosMapSim {
    let mut map = PlaybackPosMapSim::default();
    map.queue.reserve(segments.len());
    for &seg in segments {
        map.insert(seg);
    }
    map
}

fn checksum_music_pos(pos: Option<(f64, f64)>) -> u64 {
    pos.map_or(0, |(music_sec, sec_per_frame)| {
        music_sec
            .to_bits()
            .wrapping_add(sec_per_frame.to_bits().rotate_left(17))
    })
}

fn lookup_music_double_lock(
    map: &Mutex<PlaybackPosMapSim>,
    pending: &[MusicMapSegSim],
    stream_frame: f64,
) -> u64 {
    {
        let mut map = map.lock().unwrap();
        for &seg in pending {
            map.insert(seg);
        }
    }
    let map = map.lock().unwrap();
    checksum_music_pos(map.search(stream_frame))
}

fn lookup_music_single_lock(
    map: &Mutex<PlaybackPosMapSim>,
    pending: &[MusicMapSegSim],
    stream_frame: f64,
) -> u64 {
    let mut map = map.lock().unwrap();
    for &seg in pending {
        map.insert(seg);
    }
    checksum_music_pos(map.search(stream_frame))
}

fn lookup_music_owned(
    map: &mut PlaybackPosMapSim,
    pending: &[MusicMapSegSim],
    stream_frame: f64,
) -> u64 {
    for &seg in pending {
        map.insert(seg);
    }
    checksum_music_pos(map.search(stream_frame))
}

fn stamp_video_frame(bytes: &mut [u8], byte: u8) {
    if bytes.is_empty() {
        return;
    }
    let mid = bytes.len() / 2;
    bytes[0] = byte;
    bytes[mid] = byte.wrapping_mul(3);
    let last = bytes.len() - 1;
    bytes[last] = byte.wrapping_mul(7);
}

fn push_pop_video_frame(
    queue: &Mutex<VecDeque<QueuedVideoFrameSim>>,
    max_frames: usize,
    frame_index: u64,
    bytes: Vec<u8>,
) -> u64 {
    let pts_sec = frame_index as f32 * (1.0 / 60.0);
    let target = pts_sec;
    let mut queue = queue.lock().unwrap();
    while queue.len() >= max_frames {
        let _ = queue.pop_front();
    }
    queue.push_back(QueuedVideoFrameSim { pts_sec, bytes });
    let mut checksum = 0u64;
    while let Some(frame) = queue.front() {
        if frame.pts_sec > target {
            break;
        }
        let frame = queue.pop_front().expect("front checked");
        checksum = checksum.wrapping_add(checksum_bytes(&frame.bytes));
    }
    checksum
}

fn push_pop_video_frame_pooled(
    queue: &Mutex<VecDeque<QueuedVideoFrameSim>>,
    pool: &Mutex<Vec<Vec<u8>>>,
    max_frames: usize,
    frame_index: u64,
    bytes: Vec<u8>,
) -> u64 {
    let pts_sec = frame_index as f32 * (1.0 / 60.0);
    let target = pts_sec;
    let mut recycled = Vec::new();
    let mut checksum = 0u64;
    {
        let mut queue = queue.lock().unwrap();
        while queue.len() >= max_frames {
            if let Some(frame) = queue.pop_front() {
                recycled.push(frame.bytes);
            }
        }
        queue.push_back(QueuedVideoFrameSim { pts_sec, bytes });
        while let Some(frame) = queue.front() {
            if frame.pts_sec > target {
                break;
            }
            let frame = queue.pop_front().expect("front checked");
            checksum = checksum.wrapping_add(checksum_bytes(&frame.bytes));
            recycled.push(frame.bytes);
        }
    }
    let mut pool = pool.lock().unwrap();
    for bytes in recycled {
        if pool.len() < max_frames {
            pool.push(bytes);
        }
    }
    checksum
}

fn push_pop_video_frame_owned(
    queue: &mut VecDeque<QueuedVideoFrameSim>,
    pool: &mut Vec<Vec<u8>>,
    max_frames: usize,
    frame_index: u64,
    bytes: Vec<u8>,
) -> u64 {
    let pts_sec = frame_index as f32 * (1.0 / 60.0);
    let target = pts_sec;
    while queue.len() >= max_frames {
        if let Some(frame) = queue.pop_front()
            && pool.len() < max_frames
        {
            pool.push(frame.bytes);
        }
    }
    queue.push_back(QueuedVideoFrameSim { pts_sec, bytes });
    let mut checksum = 0u64;
    while let Some(frame) = queue.front() {
        if frame.pts_sec > target {
            break;
        }
        let frame = queue.pop_front().expect("front checked");
        checksum = checksum.wrapping_add(checksum_bytes(&frame.bytes));
        if pool.len() < max_frames {
            pool.push(frame.bytes);
        }
    }
    checksum
}

fn make_clip_meshes(mesh_count: usize, verts_per_mesh: usize) -> Vec<Vec<TexturedMeshVertex>> {
    let mut meshes = Vec::with_capacity(mesh_count);
    for mesh in 0..mesh_count {
        let mut vertices = Vec::with_capacity(verts_per_mesh);
        for i in 0..verts_per_mesh {
            let x = ((mesh * 17 + i * 3) % 127) as f32 - 32.0;
            let y = ((mesh * 11 + i * 5) % 113) as f32 - 28.0;
            vertices.push(TexturedMeshVertex {
                pos: [x, y, 0.0],
                uv: [i as f32 * 0.01, mesh as f32 * 0.01],
                tex_matrix_scale: [1.0, 1.0],
                color: [1.0, 0.5, 0.25, 1.0],
            });
        }
        meshes.push(vertices);
    }
    meshes
}

fn clip_meshes_local_pool(meshes: &[Vec<TexturedMeshVertex>]) -> u64 {
    let mut pool = Vec::<Vec<TexturedMeshVertex>>::new();
    let mut outputs = Vec::<Vec<TexturedMeshVertex>>::with_capacity(meshes.len());
    clip_meshes_into(meshes, &mut pool, &mut outputs)
}

fn clip_meshes_reused_pool(
    meshes: &[Vec<TexturedMeshVertex>],
    pool: &mut Vec<Vec<TexturedMeshVertex>>,
    outputs: &mut Vec<Vec<TexturedMeshVertex>>,
) -> u64 {
    recycle_clip_outputs(outputs, pool);
    clip_meshes_into(meshes, pool, outputs)
}

fn recycle_clip_outputs(
    outputs: &mut Vec<Vec<TexturedMeshVertex>>,
    pool: &mut Vec<Vec<TexturedMeshVertex>>,
) {
    for mut vertices in outputs.drain(..) {
        vertices.clear();
        if pool.len() < 512 {
            pool.push(vertices);
        }
    }
}

fn clip_meshes_into(
    meshes: &[Vec<TexturedMeshVertex>],
    pool: &mut Vec<Vec<TexturedMeshVertex>>,
    outputs: &mut Vec<Vec<TexturedMeshVertex>>,
) -> u64 {
    let mut checksum = 0u64;
    for mesh in meshes {
        let mut out = pool.pop().unwrap_or_default();
        out.clear();
        out.reserve(mesh.len().min(48));
        for vertex in mesh {
            if vertex.pos[0] >= -24.0
                && vertex.pos[0] <= 72.0
                && vertex.pos[1] >= -16.0
                && vertex.pos[1] <= 80.0
            {
                let mut clipped = *vertex;
                clipped.pos[0] = clipped.pos[0].clamp(-24.0, 72.0);
                clipped.pos[1] = clipped.pos[1].clamp(-16.0, 80.0);
                out.push(clipped);
            }
        }
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(out.len() as u64)
            .wrapping_add(out.capacity() as u64);
        outputs.push(out);
    }
    checksum
}

fn run_clip_fast_accept_pair(name: &str, mesh_count: usize, verts_per_mesh: usize) {
    let mut current = make_clip_tmesh_objects(mesh_count, verts_per_mesh);
    let mut keep = make_clip_tmesh_objects(mesh_count, verts_per_mesh);
    let current = bench(
        format!("{name}: current clone"),
        CLIP_FAST_ACCEPT_ITERS,
        || clip_inside_current(black_box(&mut current)),
    );
    let keep = bench(
        format!("{name}: keep in place"),
        CLIP_FAST_ACCEPT_ITERS,
        || clip_inside_keep(black_box(&mut keep)),
    );
    print_result(&current);
    print_result(&keep);
    print_ratio("keep vs clone", &current, &keep);
}

fn make_clip_tmesh_objects(mesh_count: usize, verts_per_mesh: usize) -> Vec<ClipTMeshObj> {
    make_clip_meshes(mesh_count, verts_per_mesh)
        .into_iter()
        .map(|vertices| ClipTMeshObj {
            tint: [1.0; 4],
            vertices: ClipTMeshVertices::Transient(vertices),
        })
        .collect()
}

fn clip_inside_current(meshes: &mut [ClipTMeshObj]) -> u64 {
    let mut checksum = 0u64;
    for mesh in meshes {
        let clipped = mesh.clone();
        *mesh = clipped;
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(clip_tmesh_len(mesh) as u64);
    }
    checksum
}

fn clip_inside_keep(meshes: &mut [ClipTMeshObj]) -> u64 {
    let mut checksum = 0u64;
    for mesh in meshes {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(clip_tmesh_len(mesh) as u64);
    }
    checksum
}

fn shadow_tmesh_current(meshes: &mut [ClipTMeshObj], out: &mut Vec<ClipTMeshObj>) -> u64 {
    out.clear();
    for mesh in meshes {
        let mut shadow = mesh.clone();
        shadow.tint = [0.0, 0.0, 0.0, mesh.tint[3] * 0.5];
        out.push(shadow);
    }
    checksum_clip_tmeshes(out)
}

fn shadow_tmesh_shared(meshes: &mut [ClipTMeshObj], out: &mut Vec<ClipTMeshObj>) -> u64 {
    out.clear();
    for mesh in meshes {
        let shadow_vertices = match &mut mesh.vertices {
            ClipTMeshVertices::Shared(vertices) => ClipTMeshVertices::Shared(Arc::clone(vertices)),
            ClipTMeshVertices::Transient(vertices) => {
                let shared =
                    Arc::<[TexturedMeshVertex]>::from(std::mem::take(vertices).into_boxed_slice());
                let shadow_vertices = ClipTMeshVertices::Shared(Arc::clone(&shared));
                mesh.vertices = ClipTMeshVertices::Shared(shared);
                shadow_vertices
            }
        };
        out.push(ClipTMeshObj {
            tint: [0.0, 0.0, 0.0, mesh.tint[3] * 0.5],
            vertices: shadow_vertices,
        });
    }
    checksum_clip_tmeshes(out)
}

fn clip_tmesh_len(mesh: &ClipTMeshObj) -> usize {
    match &mesh.vertices {
        ClipTMeshVertices::Shared(vertices) => vertices.len(),
        ClipTMeshVertices::Transient(vertices) => vertices.len(),
    }
}

fn checksum_clip_tmeshes(meshes: &[ClipTMeshObj]) -> u64 {
    let mut checksum = 0u64;
    for mesh in meshes {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(clip_tmesh_len(mesh) as u64)
            .wrapping_add(mesh.tint[3].to_bits() as u64);
    }
    checksum
}

fn cached_input_map(keymap: &RwLock<u64>, generation: &AtomicU64) -> u64 {
    let current_generation = generation.load(Ordering::Acquire);
    INPUT_MAP_CACHE_SIM.with(|cache| {
        let (cached_generation, value) = *cache.borrow();
        if cached_generation == current_generation {
            return value;
        }
        let value = *keymap.read().unwrap();
        *cache.borrow_mut() = (current_generation, value);
        value
    })
}

fn checksum_bytes(bytes: &[u8]) -> u64 {
    let mid = bytes.len() / 2;
    bytes.len() as u64
        ^ u64::from(bytes[0])
        ^ (u64::from(bytes[mid]) << 8)
        ^ (u64::from(bytes[bytes.len() - 1]) << 16)
}

fn checksum_gfx_render(render: &deadsync::engine::gfx::RenderList) -> u64 {
    let mut out = render.objects.len() as u64 ^ ((render.cameras.len() as u64) << 32);
    for obj in &render.objects {
        out = out
            .wrapping_mul(131)
            .wrapping_add(obj.texture_handle)
            .wrapping_add(obj.z as u64)
            .wrapping_add((obj.order as u64) << 16)
            .wrapping_add((obj.camera as u64) << 48);
        match &obj.object_type {
            deadsync::engine::gfx::ObjectType::Sprite {
                center, size, tint, ..
            } => {
                out = out
                    .wrapping_add(center[0].to_bits() as u64)
                    .wrapping_add(center[1].to_bits() as u64)
                    .wrapping_add(size[0].to_bits() as u64)
                    .wrapping_add((tint[3].to_bits() as u64) << 1);
            }
            deadsync::engine::gfx::ObjectType::Mesh { tint, vertices, .. } => {
                out = out
                    .wrapping_add(vertices.len() as u64)
                    .wrapping_add(tint[3].to_bits() as u64);
            }
            deadsync::engine::gfx::ObjectType::TexturedMesh { tint, vertices, .. } => {
                out = out
                    .wrapping_add(vertices.len() as u64)
                    .wrapping_add(tint[3].to_bits() as u64);
            }
        }
    }
    out
}

fn make_text_pool(count: usize) -> Vec<Arc<str>> {
    (0..count)
        .map(|idx| Arc::<str>::from(format!("W1 {idx:04}  offset {:+.3}", idx as f32 * 0.001)))
        .collect()
}

fn retarget_text_contents(actors: &mut [Actor], texts: &[Arc<str>], frame: usize) -> usize {
    let mut index = 0usize;
    for actor in actors {
        retarget_text_actor(actor, texts, frame, &mut index);
    }
    index
}

fn retarget_text_actor(actor: &mut Actor, texts: &[Arc<str>], frame: usize, index: &mut usize) {
    match actor {
        Actor::Text { content, .. } => {
            let text = &texts[(frame.wrapping_mul(251) + (*index).wrapping_mul(17)) % texts.len()];
            *content = TextContent::Shared(Arc::clone(text));
            *index += 1;
        }
        Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
            for child in children {
                retarget_text_actor(child, texts, frame, index);
            }
        }
        Actor::Shadow { child, .. } => retarget_text_actor(child, texts, frame, index),
        _ => {}
    }
}

fn bench<F>(name: impl Into<String>, iters: usize, mut f: F) -> BenchResult
where
    F: FnMut() -> u64,
{
    let name = name.into();
    let mut checksum = 0u64;
    for _ in 0..32 {
        checksum = checksum.wrapping_add(black_box(f()));
    }

    let start_alloc = ALLOC.begin_measurement();
    let started = Instant::now();
    for _ in 0..iters {
        checksum = checksum.wrapping_add(black_box(f()));
    }
    BenchResult {
        name,
        iters,
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().diff(start_alloc),
        checksum,
    }
}

fn print_result(result: &BenchResult) {
    let total_ms = result.elapsed.as_secs_f64() * 1000.0;
    let per_iter_us = result.elapsed.as_secs_f64() * 1_000_000.0 / result.iters as f64;
    println!(
        "{:<42} {:>9.3} ms total {:>10.3} us/iter alloc {:>6} dealloc {:>6} realloc {:>4} bytes {:>10} freed {:>10} live {:>8} peak {:>9} checksum {}",
        result.name,
        total_ms,
        per_iter_us,
        result.alloc.alloc_calls,
        result.alloc.dealloc_calls,
        result.alloc.realloc_calls,
        result.alloc.alloc_bytes,
        result.alloc.free_bytes,
        result.alloc.live_bytes,
        result.alloc.peak_live_delta,
        result.checksum
    );
}

fn print_ratio(label: &str, base: &BenchResult, candidate: &BenchResult) {
    let base_us = base.elapsed.as_secs_f64() * 1_000_000.0 / base.iters as f64;
    let candidate_us = candidate.elapsed.as_secs_f64() * 1_000_000.0 / candidate.iters as f64;
    println!("{label}: {:.2}x", base_us / candidate_us);
}

fn update_peak(slot: &AtomicU64, value: u64) {
    let mut observed = slot.load(Ordering::Relaxed);
    while value > observed {
        match slot.compare_exchange_weak(observed, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => observed = actual,
        }
    }
}

#[derive(Clone, Copy)]
enum SortPattern {
    Sorted,
    DenseShuffled,
    SameZShuffledOrder,
    SparseShuffled,
}

fn make_sort_objects(len: usize, pattern: SortPattern) -> Vec<RenderObject> {
    let mut objects = Vec::with_capacity(len);
    for i in 0..len {
        let (z, order) = match pattern {
            SortPattern::Sorted => ((i / 64) as i16, i as u32),
            SortPattern::DenseShuffled => {
                let idx = permute(i, len);
                ((idx % 32) as i16, idx as u32)
            }
            SortPattern::SameZShuffledOrder => (0, permute(i, len) as u32),
            SortPattern::SparseShuffled => {
                let idx = permute(i, len);
                (((idx * 257) % 30_000) as i16, idx as u32)
            }
        };
        objects.push(RenderObject {
            object_type: ObjectType::Sprite {
                center: [i as f32, 0.0, 0.5, 0.5],
                size: [32.0, 32.0],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
            },
            texture_handle: 1 + (i % 16) as TextureHandle,
            transform: [
                1.0, 0.0, 0.0, 0.0, //
                0.0, 1.0, 0.0, 0.0, //
                0.0, 0.0, 1.0, 0.0, //
                0.0, 0.0, 0.0, 1.0,
            ],
            blend: BlendMode::Alpha,
            z,
            order,
            camera: 0,
        });
    }
    objects
}

fn permute(i: usize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    i.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) % len
}

fn sort_current(objects: &mut [RenderObject], scratch: &mut SortScratch) {
    if objects.len() < 2 {
        return;
    }

    let mut min_z = objects[0].z;
    let mut max_z = min_z;
    let mut sorted_by_z = true;
    let mut sorted_by_key = true;
    let mut prev_key = (min_z, objects[0].order);
    for obj in &objects[1..] {
        let key = (obj.z, obj.order);
        sorted_by_z &= prev_key.0 <= obj.z;
        sorted_by_key &= prev_key <= key;
        min_z = min_z.min(obj.z);
        max_z = max_z.max(obj.z);
        prev_key = key;
    }
    if sorted_by_key {
        return;
    }
    if sorted_by_z {
        objects.sort_unstable_by_key(|o| (o.z, o.order));
        return;
    }

    let range = (i32::from(max_z) - i32::from(min_z) + 1) as usize;
    let dense_range_limit = objects.len().saturating_mul(8).max(256);
    if range > dense_range_limit {
        objects.sort_unstable_by_key(|o| (o.z, o.order));
        return;
    }

    scratch.z_counts.clear();
    scratch.z_counts.resize(range, 0);
    scratch.z_perm.clear();
    scratch.z_perm.resize(objects.len(), 0);

    let min_z_i = i32::from(min_z);
    for obj in objects.iter() {
        scratch.z_counts[(i32::from(obj.z) - min_z_i) as usize] += 1;
    }

    let mut next = 0usize;
    for count in &mut scratch.z_counts {
        let bucket_len = *count;
        *count = next;
        next += bucket_len;
    }

    for (old_idx, obj) in objects.iter().enumerate() {
        let bucket = (i32::from(obj.z) - min_z_i) as usize;
        let new_idx = scratch.z_counts[bucket];
        scratch.z_counts[bucket] = new_idx + 1;
        scratch.z_perm[old_idx] = new_idx;
    }

    for start in 0..objects.len() {
        let current = start;
        while scratch.z_perm[current] != current {
            let next = scratch.z_perm[current];
            objects.swap(current, next);
            scratch.z_perm.swap(current, next);
        }
    }

    if objects
        .windows(2)
        .any(|pair| (pair[0].z, pair[0].order) > (pair[1].z, pair[1].order))
    {
        objects.sort_unstable_by_key(|o| (o.z, o.order));
    }
}

fn sort_simple(objects: &mut [RenderObject], _scratch: &mut SortScratch) {
    if objects.len() < 2 {
        return;
    }

    let mut prev_key = (objects[0].z, objects[0].order);
    for obj in &objects[1..] {
        let key = (obj.z, obj.order);
        if prev_key > key {
            objects.sort_unstable_by_key(|o| (o.z, o.order));
            return;
        }
        prev_key = key;
    }
}

fn checksum_objects(objects: &[RenderObject]) -> u64 {
    let mut out = 0u64;
    for obj in objects {
        let sprite_bits = match &obj.object_type {
            ObjectType::Sprite {
                center,
                size,
                rot_sin_cos,
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
            } => {
                center[0].to_bits() as u64
                    ^ size[0].to_bits() as u64
                    ^ rot_sin_cos[1].to_bits() as u64
                    ^ tint[3].to_bits() as u64
                    ^ uv_scale[0].to_bits() as u64
                    ^ uv_offset[0].to_bits() as u64
                    ^ local_offset[0].to_bits() as u64
                    ^ local_offset_rot_sin_cos[1].to_bits() as u64
                    ^ edge_fade[0].to_bits() as u64
            }
        };
        let blend_bits = match obj.blend {
            BlendMode::Alpha => 1,
        };
        out = out
            .wrapping_mul(131)
            .wrapping_add(obj.z as i64 as u64)
            .wrapping_add(obj.order as u64)
            .wrapping_add(obj.texture_handle)
            .wrapping_add(obj.transform[0].to_bits() as u64)
            .wrapping_add(blend_bits)
            .wrapping_add(obj.camera as u64)
            .wrapping_add(sprite_bits);
    }
    out
}

fn make_f32_samples(len: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let raw = pseudo(i) as i16;
        out.push(f32::from(raw) / 65536.0);
    }
    out
}

fn make_i16_samples(len: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(pseudo(i.wrapping_mul(17)) as i16);
    }
    out
}

fn pseudo(i: usize) -> u32 {
    (i as u32)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223)
        .rotate_left((i % 31) as u32)
}

fn mix_sfx_current(music: &[f32], sfx: &[i16], starts: &[usize], out: &mut [f32]) -> u64 {
    out.copy_from_slice(music);
    for &start in starts {
        for (dst, &src) in out.iter_mut().zip(&sfx[start..]) {
            let sample = f32::from(src) * (0.65 / 32768.0);
            *dst = (*dst + sample).clamp(-1.0, 1.0);
        }
    }
    checksum_f32(out)
}

fn mix_sfx_final_clamp(music: &[f32], sfx: &[i16], starts: &[usize], out: &mut [f32]) -> u64 {
    out.copy_from_slice(music);
    for &start in starts {
        for (dst, &src) in out.iter_mut().zip(&sfx[start..]) {
            *dst += f32::from(src) * (0.65 / 32768.0);
        }
    }
    for sample in out.iter_mut() {
        *sample = sample.clamp(-1.0, 1.0);
    }
    checksum_f32(out)
}

fn checksum_f32(samples: &[f32]) -> u64 {
    let mut out = 0u64;
    for &sample in samples {
        out = out.wrapping_mul(131).wrapping_add(sample.to_bits() as u64);
    }
    out
}

fn make_tmesh_storage(
    mesh_count: usize,
    verts_per_mesh: usize,
    duplicate: bool,
) -> Vec<Vec<TexturedMeshVertex>> {
    let storage_len = if duplicate { 1 } else { mesh_count };
    let mut storage = Vec::with_capacity(storage_len);
    for mesh in 0..storage_len {
        let mut vertices = Vec::with_capacity(verts_per_mesh);
        for i in 0..verts_per_mesh {
            vertices.push(TexturedMeshVertex {
                pos: [i as f32, mesh as f32, 0.0],
                uv: [i as f32 * 0.01, mesh as f32 * 0.01],
                tex_matrix_scale: [1.0, 1.0],
                color: [1.0, 0.5, 0.25, 1.0],
            });
        }
        storage.push(vertices);
    }
    storage
}

fn make_draw_prep_tmesh_render_list(
    mesh_count: usize,
    verts_per_mesh: usize,
    shared_invalid: bool,
) -> gfx::RenderList {
    let shared = shared_invalid.then(|| {
        Arc::<[GfxTexturedMeshVertex]>::from(
            make_gfx_tmesh_vertices(0, verts_per_mesh).into_boxed_slice(),
        )
    });
    let mut objects = Vec::with_capacity(mesh_count);
    for mesh in 0..mesh_count {
        let vertices = match &shared {
            Some(vertices) => GfxTexturedMeshVertices::Shared(Arc::clone(vertices)),
            None => {
                GfxTexturedMeshVertices::Transient(make_gfx_tmesh_vertices(mesh, verts_per_mesh))
            }
        };
        objects.push(gfx::RenderObject {
            object_type: gfx::ObjectType::TexturedMesh {
                tint: [1.0, 1.0, 1.0, 1.0],
                vertices,
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                mode: GfxMeshMode::Triangles,
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                uv_tex_shift: [0.0, 0.0],
                texture_mask: false,
                depth_test: false,
            },
            texture_handle: 1,
            transform: glam::Mat4::IDENTITY,
            blend: GfxBlendMode::Alpha,
            z: 0,
            order: mesh as u32,
            camera: 0,
        });
    }
    gfx::RenderList {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![glam::Mat4::IDENTITY],
        objects,
    }
}

fn make_gfx_tmesh_vertices(mesh: usize, verts_per_mesh: usize) -> Vec<GfxTexturedMeshVertex> {
    let mut vertices = Vec::with_capacity(verts_per_mesh);
    for i in 0..verts_per_mesh {
        vertices.push(GfxTexturedMeshVertex {
            pos: [i as f32, mesh as f32, 0.0],
            uv: [i as f32 * 0.01, mesh as f32 * 0.01],
            color: [1.0, 0.5, 0.25, 1.0],
            tex_matrix_scale: [1.0, 1.0],
        });
    }
    vertices
}

fn checksum_draw_prep_scratch(scratch: &gfx::draw_prep::DrawScratch) -> u64 {
    let mut out = scratch
        .tmesh_vertices
        .len()
        .wrapping_add(scratch.tmesh_instances.len() << 8)
        .wrapping_add(scratch.ops.len() << 16) as u64;
    for op in scratch.ops.iter() {
        if let gfx::draw_prep::DrawOp::TexturedMesh(run) = op {
            out = out
                .wrapping_mul(131)
                .wrapping_add(run.source.vertex_start() as u64)
                .wrapping_add(run.source.vertex_count() as u64)
                .wrapping_add(run.instance_count as u64)
                .wrapping_add(run.texture_handle);
        }
    }
    out.wrapping_add(checksum_gfx_tmesh_tail(&scratch.tmesh_vertices))
}

fn checksum_gfx_tmesh_tail(vertices: &[GfxTexturedMeshVertex]) -> u64 {
    let Some(v) = vertices.last() else {
        return 0;
    };
    v.pos[0].to_bits() as u64
        ^ v.pos[1].to_bits() as u64
        ^ v.uv[0].to_bits() as u64
        ^ v.tex_matrix_scale[0].to_bits() as u64
}

fn copy_tmesh_dedupe(
    meshes: &[&[TexturedMeshVertex]],
    map: &mut TMeshGeomMap,
    out: &mut Vec<TexturedMeshVertex>,
) -> u64 {
    map.clear();
    out.clear();
    let mut checksum = 0u64;
    for vertices in meshes {
        let key = TMeshGeomKey {
            ptr: vertices.as_ptr() as usize,
            len: vertices.len(),
        };
        let (start, count) = if let Some(&(start, count)) = map.get(&key) {
            (start, count)
        } else {
            let start = out.len() as u32;
            out.extend_from_slice(vertices);
            let count = vertices.len() as u32;
            map.insert(key, (start, count));
            (start, count)
        };
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(start as u64)
            .wrapping_add(count as u64);
    }
    checksum
        .wrapping_add(out.len() as u64)
        .wrapping_add(checksum_tmesh_tail(out))
}

fn copy_tmesh_direct(meshes: &[&[TexturedMeshVertex]], out: &mut Vec<TexturedMeshVertex>) -> u64 {
    out.clear();
    let mut checksum = 0u64;
    for vertices in meshes {
        let start = out.len() as u32;
        out.extend_from_slice(vertices);
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(start as u64)
            .wrapping_add(vertices.len() as u64);
    }
    checksum
        .wrapping_add(out.len() as u64)
        .wrapping_add(checksum_tmesh_tail(out))
}

fn checksum_tmesh_tail(vertices: &[TexturedMeshVertex]) -> u64 {
    let Some(v) = vertices.last() else {
        return 0;
    };
    v.pos[0].to_bits() as u64
        ^ v.pos[1].to_bits() as u64
        ^ v.uv[0].to_bits() as u64
        ^ v.tex_matrix_scale[0].to_bits() as u64
        ^ v.color[3].to_bits() as u64
}
