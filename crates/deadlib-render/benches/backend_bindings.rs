use deadlib_render::{
    BlendMode, ObjectType, RenderObject, SpriteInstanceRaw, TextureHandle, TextureHandleMap,
    TexturedMeshInstanceRaw, draw_prep::TexturedMeshSource,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hash::Hash;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const TRANSIENT_GEOMETRIES: usize = 96;
const TRANSIENT_PASSES: usize = 2;
const CACHED_GEOMETRIES: usize = 48;
const WARMUP_FRAMES: usize = 1_000;
const MEASURE_FRAMES: usize = 100_000;
const BENCH_RUNS: usize = 7;
const KEY_WARMUP_FRAMES: usize = 250;
const KEY_MEASURE_FRAMES: usize = 25_000;
const SPRITE_RUNS: usize = 64;
const TMESH_RUNS: usize = 144;
const HOT_OBJECTS: usize = 4_096;
const HOT_SCAN_FRAMES: usize = 20_000;
const TEXTURE_LOOKUPS: usize = 2_048;
const TEXTURE_LOOKUP_FRAMES: usize = 40_000;

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

// SAFETY: all allocation operations delegate to `System` with the original
// pointer and layout; the counters only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees this is the layout used to allocate `ptr`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `ptr` and `old` identify a live allocation from `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
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
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
    binds: usize,
}

fn main() {
    let sources = gameplay_sources();
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);

    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure(&sources, plan_current);
            let old = measure(&sources, plan_legacy);
            (old, new)
        } else {
            let old = measure(&sources, plan_legacy);
            let new = measure(&sources, plan_current);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.binds, 144);
        assert_eq!(new.binds, 49);
        for result in [&old, &new] {
            assert_eq!(result.alloc.allocs, 0);
            assert_eq!(result.alloc.reallocs, 0);
            assert_eq!(result.alloc.bytes, 0);
        }
        legacy.push(old);
        current.push(new);
    }

    legacy.sort_unstable_by_key(|result| result.elapsed);
    current.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let current = current.swap_remove(BENCH_RUNS / 2);

    println!(
        "OpenGL/Vulkan/WGPU textured-mesh binding plan ({} runs, median of {BENCH_RUNS})",
        sources.len()
    );
    print_result("draw-range key", &legacy);
    print_result("buffer key", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | GPU buffer binds/frame {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.binds,
        current.binds,
        100.0 * (1.0 - current.binds as f64 / legacy.binds as f64),
    );

    benchmark_source_layout(&sources);
    benchmark_frame_key_layout();
    benchmark_gl_instance_submission();
    benchmark_render_object_layout();
    benchmark_texture_slots();
}

fn benchmark_frame_key_layout() {
    let legacy_keys = legacy_frame_keys();
    let compact_keys = compact_frame_keys();
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut compact = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_frame_keys(&compact_keys);
            let old = measure_frame_keys(&legacy_keys);
            (old, new)
        } else {
            let old = measure_frame_keys(&legacy_keys);
            let new = measure_frame_keys(&compact_keys);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.binds, new.binds);
        for result in [&old, &new] {
            assert_eq!(result.alloc.allocs, 0);
            assert_eq!(result.alloc.reallocs, 0);
            assert_eq!(result.alloc.bytes, 0);
        }
        legacy.push(old);
        compact.push(new);
    }
    legacy.sort_unstable_by_key(|result| result.elapsed);
    compact.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let compact = compact.swap_remove(BENCH_RUNS / 2);

    println!("\nframe geometry hash key (32 geometries x 8 passes, median of {BENCH_RUNS})");
    print_scan_result("24-byte key", &legacy, legacy_keys.len());
    print_scan_result("16-byte key", &compact, compact_keys.len());
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | hash-entry key bytes {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / compact.elapsed.as_secs_f64(),
        100.0 * (1.0 - compact.cycles as f64 / legacy.cycles as f64),
        std::mem::size_of::<LegacyFrameKey>(),
        std::mem::size_of::<CompactFrameKey>(),
        100.0
            * (1.0
                - std::mem::size_of::<CompactFrameKey>() as f64
                    / std::mem::size_of::<LegacyFrameKey>() as f64),
    );
}

fn benchmark_source_layout(sources: &[TexturedMeshSource]) {
    let legacy_sources = gameplay_legacy_sources();
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut packed = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure(sources, plan_current);
            let old = measure(&legacy_sources, plan_legacy_metadata);
            (old, new)
        } else {
            let old = measure(&legacy_sources, plan_legacy_metadata);
            let new = measure(sources, plan_current);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.binds, new.binds);
        for result in [&old, &new] {
            assert_eq!(result.alloc.allocs, 0);
            assert_eq!(result.alloc.reallocs, 0);
            assert_eq!(result.alloc.bytes, 0);
        }
        legacy.push(old);
        packed.push(new);
    }
    legacy.sort_unstable_by_key(|result| result.elapsed);
    packed.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let packed = packed.swap_remove(BENCH_RUNS / 2);

    println!(
        "\ntextured-mesh source layout ({} runs, median of {BENCH_RUNS})",
        sources.len()
    );
    print_result("24-byte enum", &legacy);
    print_result("16-byte packed", &packed);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | metadata bytes/run {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / packed.elapsed.as_secs_f64(),
        100.0 * (1.0 - packed.cycles as f64 / legacy.cycles as f64),
        std::mem::size_of::<LegacyTexturedMeshSource>(),
        std::mem::size_of::<TexturedMeshSource>(),
        100.0
            * (1.0
                - std::mem::size_of::<TexturedMeshSource>() as f64
                    / std::mem::size_of::<LegacyTexturedMeshSource>() as f64),
    );
}

fn benchmark_gl_instance_submission() {
    // A headless benchmark cannot call a real GL driver. This measures the
    // CPU-side dispatch count with gameplay-shaped runs; the reported GL-call
    // reduction is exact, while driver/GPU time remains hardware-dependent.
    let runs = gameplay_instance_runs();
    let mut pointer = Vec::with_capacity(BENCH_RUNS);
    let mut base = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_instances(&runs, plan_base_instance);
            let old = measure_instances(&runs, plan_instance_pointers);
            (old, new)
        } else {
            let old = measure_instances(&runs, plan_instance_pointers);
            let new = measure_instances(&runs, plan_base_instance);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_eq!(old.binds, SPRITE_RUNS * 11 + TMESH_RUNS * 10);
        assert_eq!(new.binds, runs.len());
        for result in [&old, &new] {
            assert_eq!(result.alloc.allocs, 0);
            assert_eq!(result.alloc.reallocs, 0);
            assert_eq!(result.alloc.bytes, 0);
        }
        pointer.push(old);
        base.push(new);
    }
    pointer.sort_unstable_by_key(|result| result.elapsed);
    base.sort_unstable_by_key(|result| result.elapsed);
    let pointer = pointer.swap_remove(BENCH_RUNS / 2);
    let base = base.swap_remove(BENCH_RUNS / 2);

    println!(
        "\nOpenGL instance submission command dispatch ({} runs, median of {BENCH_RUNS})",
        runs.len()
    );
    print_instance_result("attrib pointers", &pointer, runs.len());
    print_instance_result("base instance", &base, runs.len());
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | GL calls/frame {} -> {} ({:.1}% fewer)",
        pointer.elapsed.as_secs_f64() / base.elapsed.as_secs_f64(),
        100.0 * (1.0 - base.cycles as f64 / pointer.cycles as f64),
        pointer.binds,
        base.binds,
        100.0 * (1.0 - base.binds as f64 / pointer.binds as f64),
    );
}

#[repr(C)]
struct LegacyRenderObject {
    object_type: ObjectType,
    texture_handle: TextureHandle,
    order: u32,
    z: i16,
    blend: BlendMode,
    camera: u8,
}

fn benchmark_render_object_layout() {
    let current = (0..HOT_OBJECTS)
        .map(|index| RenderObject {
            texture_handle: (index % 64 + 1) as u64,
            order: index as u32,
            z: (index % 12) as i16 - 6,
            blend: if index % 7 == 0 {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            },
            camera: (index % 3) as u8,
            object_type: ObjectType::Sprite(index as u32),
        })
        .collect::<Vec<_>>();
    let legacy = (0..HOT_OBJECTS)
        .map(|index| LegacyRenderObject {
            object_type: ObjectType::Sprite(index as u32),
            texture_handle: (index % 64 + 1) as u64,
            order: index as u32,
            z: (index % 12) as i16 - 6,
            blend: if index % 7 == 0 {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            },
            camera: (index % 3) as u8,
        })
        .collect::<Vec<_>>();
    assert_eq!(std::mem::size_of::<LegacyRenderObject>(), 160);
    assert_eq!(std::mem::size_of::<RenderObject>(), 160);

    let mut old_runs = Vec::with_capacity(BENCH_RUNS);
    let mut new_runs = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_hot(&current, HOT_SCAN_FRAMES, scan_render_headers);
            let old = measure_hot(&legacy, HOT_SCAN_FRAMES, scan_legacy_headers);
            (old, new)
        } else {
            let old = measure_hot(&legacy, HOT_SCAN_FRAMES, scan_legacy_headers);
            let new = measure_hot(&current, HOT_SCAN_FRAMES, scan_render_headers);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_zero_alloc(&old);
        assert_zero_alloc(&new);
        old_runs.push(old);
        new_runs.push(new);
    }
    old_runs.sort_unstable_by_key(|result| result.elapsed);
    new_runs.sort_unstable_by_key(|result| result.elapsed);
    let old = old_runs.swap_remove(BENCH_RUNS / 2);
    let new = new_runs.swap_remove(BENCH_RUNS / 2);

    println!("\nrender-object hot-header scan ({HOT_OBJECTS} sprites, median of {BENCH_RUNS})");
    print_hot_result("payload first", &old, HOT_SCAN_FRAMES, HOT_OBJECTS);
    print_hot_result("header first", &new, HOT_SCAN_FRAMES, HOT_OBJECTS);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | header/payload distance {} -> {} bytes",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
        std::mem::offset_of!(LegacyRenderObject, texture_handle),
        std::mem::offset_of!(RenderObject, object_type),
    );
}

fn scan_render_headers(objects: &[RenderObject]) -> u64 {
    let mut checksum = 0u64;
    for object in objects {
        let ObjectType::Sprite(instance) = object.object_type else {
            continue;
        };
        checksum = observe_header(
            checksum,
            instance,
            object.texture_handle,
            object.order,
            object.z,
            object.blend,
            object.camera,
        );
    }
    black_box(checksum)
}

fn scan_legacy_headers(objects: &[LegacyRenderObject]) -> u64 {
    let mut checksum = 0u64;
    for object in objects {
        let ObjectType::Sprite(instance) = object.object_type else {
            continue;
        };
        checksum = observe_header(
            checksum,
            instance,
            object.texture_handle,
            object.order,
            object.z,
            object.blend,
            object.camera,
        );
    }
    black_box(checksum)
}

#[inline(always)]
fn observe_header(
    checksum: u64,
    instance: u32,
    texture: u64,
    order: u32,
    z: i16,
    blend: BlendMode,
    camera: u8,
) -> u64 {
    let blend = match blend {
        BlendMode::Alpha => 0,
        BlendMode::Add => 1,
        BlendMode::Multiply => 2,
        BlendMode::Subtract => 3,
    };
    checksum.rotate_left(7)
        ^ texture
        ^ (u64::from(instance) << 32)
        ^ u64::from(order)
        ^ ((i64::from(z) as u64) << 17)
        ^ (blend << 8)
        ^ u64::from(camera)
}

struct LegacyTextureMap<V> {
    slots: Vec<Option<V>>,
}

impl<V> LegacyTextureMap<V> {
    fn insert(&mut self, handle: TextureHandle, value: V) {
        let slot = (handle - 1) as usize;
        if slot >= self.slots.len() {
            self.slots.resize_with(slot + 1, || None);
        }
        self.slots[slot] = Some(value);
    }

    #[inline(always)]
    fn get(&self, handle: TextureHandle) -> Option<&V> {
        let slot = handle.checked_sub(1)? as usize;
        self.slots.get(slot)?.as_ref()
    }
}

fn benchmark_texture_slots() {
    let mut legacy = LegacyTextureMap { slots: Vec::new() };
    let mut current = TextureHandleMap::default();
    for handle in 1..=TEXTURE_LOOKUPS as u64 {
        legacy.insert(handle, handle.wrapping_mul(3));
        current.insert(handle, handle.wrapping_mul(3));
    }
    let handles = (0..TEXTURE_LOOKUPS)
        .map(|index| ((index * 40_503 + 17) % TEXTURE_LOOKUPS + 1) as u64)
        .collect::<Vec<_>>();

    let mut old_runs = Vec::with_capacity(BENCH_RUNS);
    let mut new_runs = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_texture_current(&current, &handles);
            let old = measure_texture_legacy(&legacy, &handles);
            (old, new)
        } else {
            let old = measure_texture_legacy(&legacy, &handles);
            let new = measure_texture_current(&current, &handles);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_zero_alloc(&old);
        assert_zero_alloc(&new);
        old_runs.push(old);
        new_runs.push(new);
    }
    old_runs.sort_unstable_by_key(|result| result.elapsed);
    new_runs.sort_unstable_by_key(|result| result.elapsed);
    let old = old_runs.swap_remove(BENCH_RUNS / 2);
    let new = new_runs.swap_remove(BENCH_RUNS / 2);

    println!(
        "\ntexture-handle lookup ({TEXTURE_LOOKUPS} shuffled handles, median of {BENCH_RUNS})"
    );
    print_hot_result(
        "subtract slot",
        &old,
        TEXTURE_LOOKUP_FRAMES,
        TEXTURE_LOOKUPS,
    );
    print_hot_result("direct slot", &new, TEXTURE_LOOKUP_FRAMES, TEXTURE_LOOKUPS);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | warmed allocations remain zero",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn measure_texture_legacy(
    textures: &LegacyTextureMap<u64>,
    handles: &[TextureHandle],
) -> BenchResult {
    measure_hot(handles, TEXTURE_LOOKUP_FRAMES, |handles| {
        scan_texture_legacy(textures, handles)
    })
}

fn measure_texture_current(
    textures: &TextureHandleMap<u64>,
    handles: &[TextureHandle],
) -> BenchResult {
    measure_hot(handles, TEXTURE_LOOKUP_FRAMES, |handles| {
        scan_texture_current(textures, handles)
    })
}

fn scan_texture_legacy(textures: &LegacyTextureMap<u64>, handles: &[TextureHandle]) -> u64 {
    let mut checksum = 0u64;
    for &handle in handles {
        checksum = checksum.rotate_left(5) ^ textures.get(handle).copied().unwrap_or_default();
    }
    black_box(checksum)
}

fn scan_texture_current(textures: &TextureHandleMap<u64>, handles: &[TextureHandle]) -> u64 {
    let mut checksum = 0u64;
    for &handle in handles {
        checksum = checksum.rotate_left(5) ^ textures.get(&handle).copied().unwrap_or_default();
    }
    black_box(checksum)
}

fn measure_hot<T, F>(items: &[T], frames: usize, mut scan: F) -> BenchResult
where
    F: FnMut(&[T]) -> u64,
{
    for _ in 0..256 {
        black_box(scan(black_box(items)));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..frames {
        checksum = checksum.rotate_left(9) ^ scan(black_box(items));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        binds: items.len(),
    }
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.alloc.allocs, 0);
    assert_eq!(result.alloc.reallocs, 0);
    assert_eq!(result.alloc.bytes, 0);
}

fn print_hot_result(label: &str, result: &BenchResult, frames: usize, items: usize) {
    let frames = frames as f64;
    println!(
        "  {label:<14} {:>8.2} us/frame  {:>10.0} cycles/frame  \
         {:>8.1} M items/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * items as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.alloc.allocs as f64,
        result.alloc.reallocs as f64,
        result.alloc.bytes as f64,
    );
}

fn measure<T>(sources: &[T], plan: fn(&[T]) -> (usize, u64)) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(plan(black_box(sources)));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut binds = 0;
    for _ in 0..MEASURE_FRAMES {
        let (frame_binds, frame_checksum) = plan(black_box(sources));
        binds = frame_binds;
        checksum = checksum.rotate_left(9) ^ frame_checksum;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        binds,
    }
}

#[inline(never)]
fn observe_bind(source: TexturedMeshSource, binds: &mut usize) {
    *binds += 1;
    black_box(source);
}

fn plan_legacy(sources: &[TexturedMeshSource]) -> (usize, u64) {
    let mut last: Option<TexturedMeshSource> = None;
    let mut binds = 0;
    let mut checksum = 0_u64;
    for &source in sources {
        if last != Some(source) {
            observe_bind(source, &mut binds);
            last = Some(source);
        }
        checksum = observe_draw(checksum, source);
    }
    (binds, black_box(checksum))
}

fn plan_current(sources: &[TexturedMeshSource]) -> (usize, u64) {
    let mut last: Option<TexturedMeshSource> = None;
    let mut binds = 0;
    let mut checksum = 0_u64;
    for &source in sources {
        if last.is_none_or(|bound| !bound.shares_vertex_buffer(source)) {
            observe_bind(source, &mut binds);
            last = Some(source);
        }
        checksum = observe_draw(checksum, source);
    }
    (binds, black_box(checksum))
}

#[inline(always)]
fn observe_draw(checksum: u64, source: TexturedMeshSource) -> u64 {
    checksum.rotate_left(7)
        ^ u64::from(source.vertex_start())
        ^ (u64::from(source.vertex_count()) << 32)
}

fn gameplay_sources() -> Vec<TexturedMeshSource> {
    let mut sources =
        Vec::with_capacity(TRANSIENT_GEOMETRIES * TRANSIENT_PASSES + CACHED_GEOMETRIES);
    for geometry in 0..TRANSIENT_GEOMETRIES {
        let vertex_start = geometry as u32 * 48;
        let source = TexturedMeshSource::transient(vertex_start, 48);
        sources.extend(std::iter::repeat_n(source, TRANSIENT_PASSES));
    }
    for geometry in 0..CACHED_GEOMETRIES {
        sources.push(TexturedMeshSource::cached(1_000 + geometry as u64, 72));
    }
    sources
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum LegacyFrameKey {
    Cached(u64),
    Shared { ptr: usize, len: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CompactFrameKey {
    Cached(u64),
    Shared(usize),
}

fn legacy_frame_keys() -> Vec<LegacyFrameKey> {
    let mut keys = Vec::with_capacity(32 * 8);
    for pass in 0..8 {
        for geometry in 0..32 {
            let ptr = 0x1000 + geometry * 0x100;
            keys.push(if pass % 4 == 3 {
                LegacyFrameKey::Cached(1_000 + geometry as u64)
            } else {
                LegacyFrameKey::Shared { ptr, len: 96 }
            });
        }
    }
    keys
}

fn compact_frame_keys() -> Vec<CompactFrameKey> {
    let mut keys = Vec::with_capacity(32 * 8);
    for pass in 0..8 {
        for geometry in 0..32 {
            let ptr = 0x1000 + geometry * 0x100;
            keys.push(if pass % 4 == 3 {
                CompactFrameKey::Cached(1_000 + geometry as u64)
            } else {
                CompactFrameKey::Shared(ptr)
            });
        }
    }
    keys
}

fn measure_frame_keys<K>(keys: &[K]) -> BenchResult
where
    K: Copy + Eq + Hash,
{
    let mut table = HashMap::<K, u32, rustc_hash::FxBuildHasher>::with_capacity_and_hasher(
        128,
        rustc_hash::FxBuildHasher,
    );
    for _ in 0..KEY_WARMUP_FRAMES {
        black_box(scan_frame_keys(keys, &mut table));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut entries = 0;
    for _ in 0..KEY_MEASURE_FRAMES {
        let (frame_entries, frame_checksum) = scan_frame_keys(black_box(keys), &mut table);
        entries = frame_entries;
        checksum = checksum.rotate_left(9) ^ frame_checksum;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        binds: entries,
    }
}

fn scan_frame_keys<K>(
    keys: &[K],
    table: &mut HashMap<K, u32, rustc_hash::FxBuildHasher>,
) -> (usize, u64)
where
    K: Copy + Eq + Hash,
{
    table.clear();
    let mut checksum = 0_u64;
    for (index, &key) in keys.iter().enumerate() {
        let value = if let Some(&value) = table.get(&key) {
            value
        } else {
            let value = index as u32;
            table.insert(key, value);
            value
        };
        checksum = checksum.rotate_left(5) ^ u64::from(value);
    }
    (table.len(), black_box(checksum))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LegacyTexturedMeshSource {
    Transient {
        vertex_start: u32,
        vertex_count: u32,
        geom_key: u64,
    },
    Cached {
        cache_key: u64,
        vertex_count: u32,
    },
}

impl LegacyTexturedMeshSource {
    #[inline(always)]
    fn shares_vertex_buffer(self, other: Self) -> bool {
        match (self, other) {
            (Self::Transient { .. }, Self::Transient { .. }) => true,
            (
                Self::Cached {
                    cache_key: left, ..
                },
                Self::Cached {
                    cache_key: right, ..
                },
            ) => left == right,
            _ => false,
        }
    }

    #[inline(always)]
    fn vertex_start(self) -> u32 {
        match self {
            Self::Transient { vertex_start, .. } => vertex_start,
            Self::Cached { .. } => 0,
        }
    }

    #[inline(always)]
    fn vertex_count(self) -> u32 {
        match self {
            Self::Transient { vertex_count, .. } | Self::Cached { vertex_count, .. } => {
                vertex_count
            }
        }
    }
}

fn gameplay_legacy_sources() -> Vec<LegacyTexturedMeshSource> {
    let mut sources =
        Vec::with_capacity(TRANSIENT_GEOMETRIES * TRANSIENT_PASSES + CACHED_GEOMETRIES);
    for geometry in 0..TRANSIENT_GEOMETRIES {
        let vertex_start = geometry as u32 * 48;
        let source = LegacyTexturedMeshSource::Transient {
            vertex_start,
            vertex_count: 48,
            geom_key: (u64::from(vertex_start) << 32) | 48,
        };
        sources.extend(std::iter::repeat_n(source, TRANSIENT_PASSES));
    }
    for geometry in 0..CACHED_GEOMETRIES {
        sources.push(LegacyTexturedMeshSource::Cached {
            cache_key: 1_000 + geometry as u64,
            vertex_count: 72,
        });
    }
    sources
}

fn plan_legacy_metadata(sources: &[LegacyTexturedMeshSource]) -> (usize, u64) {
    let mut last: Option<LegacyTexturedMeshSource> = None;
    let mut binds = 0;
    let mut checksum = 0_u64;
    for &source in sources {
        if last.is_none_or(|bound| !bound.shares_vertex_buffer(source)) {
            observe_legacy_bind(source, &mut binds);
            last = Some(source);
        }
        checksum = checksum.rotate_left(7)
            ^ u64::from(source.vertex_start())
            ^ (u64::from(source.vertex_count()) << 32);
    }
    (binds, black_box(checksum))
}

#[inline(never)]
fn observe_legacy_bind(source: LegacyTexturedMeshSource, binds: &mut usize) {
    *binds += 1;
    black_box(source);
}

#[derive(Clone, Copy)]
enum InstanceRunKind {
    Sprite,
    TexturedMesh,
}

#[derive(Clone, Copy)]
struct InstanceRun {
    kind: InstanceRunKind,
    start: u32,
    count: u32,
}

fn gameplay_instance_runs() -> Vec<InstanceRun> {
    let mut runs = Vec::with_capacity(SPRITE_RUNS + TMESH_RUNS);
    for run in 0..SPRITE_RUNS {
        runs.push(InstanceRun {
            kind: InstanceRunKind::Sprite,
            start: (run * 24) as u32,
            count: 24,
        });
    }
    for run in 0..TMESH_RUNS {
        runs.push(InstanceRun {
            kind: InstanceRunKind::TexturedMesh,
            start: run as u32,
            count: 1,
        });
    }
    runs
}

fn measure_instances(
    runs: &[InstanceRun],
    plan: fn(&[InstanceRun]) -> (usize, u64),
) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(plan(black_box(runs)));
    }

    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut calls = 0;
    for _ in 0..MEASURE_FRAMES {
        let (frame_calls, frame_checksum) = plan(black_box(runs));
        calls = frame_calls;
        checksum = checksum.rotate_left(9) ^ frame_checksum;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
        binds: calls,
    }
}

fn plan_instance_pointers(runs: &[InstanceRun]) -> (usize, u64) {
    const SPRITE_OFFSETS: [u32; 10] = [0, 16, 24, 32, 48, 56, 64, 72, 80, 96];
    const TMESH_OFFSETS: [u32; 9] = [0, 16, 32, 48, 64, 80, 88, 96, 104];
    let mut calls = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        let (stride, offsets) = match run.kind {
            InstanceRunKind::Sprite => (
                std::mem::size_of::<SpriteInstanceRaw>() as u32,
                SPRITE_OFFSETS.as_slice(),
            ),
            InstanceRunKind::TexturedMesh => (
                std::mem::size_of::<TexturedMeshInstanceRaw>() as u32,
                TMESH_OFFSETS.as_slice(),
            ),
        };
        let base = run.start * stride;
        for (attribute, offset) in offsets.iter().copied().enumerate() {
            observe_gl_call(attribute as u32, base + offset, &mut calls);
        }
        observe_gl_call(u32::MAX, run.start, &mut calls);
        checksum = instance_checksum(checksum, run);
    }
    (calls, black_box(checksum))
}

fn plan_base_instance(runs: &[InstanceRun]) -> (usize, u64) {
    let mut calls = 0;
    let mut checksum = 0_u64;
    for &run in runs {
        observe_gl_call(u32::MAX, run.start, &mut calls);
        checksum = instance_checksum(checksum, run);
    }
    (calls, black_box(checksum))
}

#[inline(never)]
fn observe_gl_call(attribute: u32, offset: u32, calls: &mut usize) {
    *calls += 1;
    black_box((attribute, offset));
}

#[inline(always)]
fn instance_checksum(checksum: u64, run: InstanceRun) -> u64 {
    let kind = matches!(run.kind, InstanceRunKind::TexturedMesh) as u64;
    checksum.rotate_left(7) ^ u64::from(run.start) ^ (u64::from(run.count) << 32) ^ kind
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<14} {:>7.2} ns/frame  {:>8.1} cycles/frame  \
         {:>7.1} M runs/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * (TRANSIENT_GEOMETRIES * TRANSIENT_PASSES + CACHED_GEOMETRIES) as f64
            / result.elapsed.as_secs_f64()
            / 1_000_000.0,
        result.alloc.allocs as f64,
        result.alloc.reallocs as f64,
        result.alloc.bytes as f64,
    );
}

fn print_instance_result(label: &str, result: &BenchResult, runs: usize) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<15} {:>7.2} ns/frame  {:>8.1} cycles/frame  \
         {:>7.1} M runs/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * runs as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.alloc.allocs as f64,
        result.alloc.reallocs as f64,
        result.alloc.bytes as f64,
    );
}

fn print_scan_result(label: &str, result: &BenchResult, runs: usize) {
    let frames = KEY_MEASURE_FRAMES as f64;
    println!(
        "  {label:<14} {:>7.2} ns/frame  {:>8.1} cycles/frame  \
         {:>7.1} M runs/s  {:>4.1} allocs  {:>4.1} reallocs  {:>5.1} bytes",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * runs as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.alloc.allocs as f64,
        result.alloc.reallocs as f64,
        result.alloc.bytes as f64,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter without
    // dereferencing memory.
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
