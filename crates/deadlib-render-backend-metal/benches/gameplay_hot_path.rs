#[path = "../src/encoder_cache.rs"]
mod encoder_cache;

use deadlib_render_core::{
    BlendMode, DrawOp, RenderFrame, TexturedMeshGeometry, TexturedMeshRun, TexturedMeshSource,
    TexturedMeshUploads, TexturedMeshVertex, TexturedMeshVertices, resolve_textured_meshes,
};
use encoder_cache::{BufferUpdate, CullMode, DrawKind, EncoderCache};
use rustc_hash::FxBuildHasher;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 10_000;
const MEASURE_FRAMES: usize = 100_000;
const GEOMETRIES: usize = 128;
const MESH_RUNS: usize = 512;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; the atomics only
// observe successful allocation activity while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` came from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
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

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct Measurement {
    elapsed: Duration,
    cycles: Option<u64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(mut frame: impl FnMut() -> u64) -> Measurement {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Measurement {
        elapsed,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start)),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
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

fn print_measurement(label: &str, result: &Measurement, items_per_frame: usize) {
    let frames = MEASURE_FRAMES as f64;
    let seconds = result.elapsed.as_secs_f64();
    println!(
        "{label:<28} {:>9.1} ns/frame  {:>9.1} cycles/frame  {:>7.2} Mitem/s  \
         {:>4.2} alloc  {:>4.2} realloc  {:>4.2} free  {:>7.1} B/frame  {:016x}",
        seconds * 1_000_000_000.0 / frames,
        result
            .cycles
            .map_or(f64::NAN, |cycles| cycles as f64 / frames),
        frames * items_per_frame as f64 / seconds / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.reallocs as f64 / frames,
        result.allocated.deallocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.checksum,
    );
}

fn assert_zero_alloc(result: &Measurement) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.deallocs, 0);
    assert_eq!(result.allocated.bytes, 0);
}

type SlotMap = HashMap<u64, u32, FxBuildHasher>;

struct MeshLookupBench {
    frame: RenderFrame,
    slots: SlotMap,
    values: Vec<u64>,
    legacy_sources: Vec<TexturedMeshSource>,
    uploads: TexturedMeshUploads,
}

impl MeshLookupBench {
    fn new() -> Self {
        let mut slots = SlotMap::with_capacity_and_hasher(GEOMETRIES, Default::default());
        let mut values = Vec::with_capacity(GEOMETRIES);
        let geometries = (0..GEOMETRIES)
            .map(|index| {
                let key = mesh_key(index);
                slots.insert(key, index as u32);
                values.push((index as u64 + 1).wrapping_mul(0x9e37_79b9));
                TexturedMeshGeometry {
                    vertices: TexturedMeshVertices::Shared(Arc::from(
                        [TexturedMeshVertex::default(); 6],
                    )),
                    cache_key: key,
                }
            })
            .collect();
        let ops = (0..MESH_RUNS)
            .map(|run| {
                DrawOp::TexturedMesh(TexturedMeshRun {
                    geometry: ((run * 37 + run / 5) % GEOMETRIES) as u32,
                    instance_start: run as u32,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: 1,
                    camera: 0,
                    depth_test: false,
                })
            })
            .collect();
        Self {
            frame: RenderFrame {
                clear_color: [0.0; 4],
                cameras: Vec::new(),
                sprite_instances: Vec::new(),
                mesh_vertices: Vec::new(),
                tmesh_instances: Vec::new(),
                tmesh_geometries: geometries,
                ops,
            },
            slots,
            values,
            legacy_sources: Vec::with_capacity(GEOMETRIES),
            uploads: TexturedMeshUploads::with_capacity(0, GEOMETRIES),
        }
    }

    fn legacy_frame(&mut self) -> u64 {
        self.legacy_sources.clear();
        for geometry in &self.frame.tmesh_geometries {
            let source = if self.slots.contains_key(&geometry.cache_key) {
                TexturedMeshSource::cached(geometry.cache_key, geometry.vertices.len() as u32)
            } else {
                TexturedMeshSource::transient(0, geometry.vertices.len() as u32)
            };
            self.legacy_sources.push(source);
        }
        record_legacy_sources(
            &self.frame.ops,
            &self.legacy_sources,
            &self.slots,
            &self.values,
        )
    }

    fn dense_frame(&mut self) -> u64 {
        let slots = &self.slots;
        resolve_textured_meshes(&self.frame, &mut self.uploads, |key, _| {
            slots.get(&key).map(|slot| u64::from(*slot) + 1)
        });
        record_dense_sources(&self.frame.ops, &self.uploads, &self.values)
    }
}

fn mesh_key(index: usize) -> u64 {
    (index as u64 + 0x1000_0001)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        | 1
}

fn record_legacy_sources(
    ops: &[DrawOp],
    sources: &[TexturedMeshSource],
    slots: &SlotMap,
    values: &[u64],
) -> u64 {
    let mut checksum = 0u64;
    let mut last_buffer = None;
    for op in ops {
        let DrawOp::TexturedMesh(run) = op else {
            continue;
        };
        let source = sources[run.geometry as usize];
        let Some(key) = source.buffer_key() else {
            continue;
        };
        if last_buffer != Some(key) {
            let slot = slots[&key] as usize;
            checksum = checksum.wrapping_add(values[slot]);
            last_buffer = Some(key);
        }
        checksum = checksum.wrapping_add(u64::from(source.vertex_count()));
    }
    checksum
}

fn record_dense_sources(ops: &[DrawOp], uploads: &TexturedMeshUploads, values: &[u64]) -> u64 {
    let mut checksum = 0u64;
    let mut last_buffer = None;
    for op in ops {
        let DrawOp::TexturedMesh(run) = op else {
            continue;
        };
        let source = uploads.source(run.geometry).expect("resolved geometry");
        let Some(buffer_key) = source.buffer_key() else {
            continue;
        };
        if last_buffer != Some(buffer_key) {
            checksum = checksum.wrapping_add(values[(buffer_key - 1) as usize]);
            last_buffer = Some(buffer_key);
        }
        checksum = checksum.wrapping_add(u64::from(source.vertex_count()));
    }
    checksum
}

#[derive(Clone, Copy)]
struct DesiredState {
    kind: DrawKind,
    blend: u8,
    camera: u8,
    texture: u64,
    repeat: bool,
    depth: bool,
    cull: CullMode,
}

#[derive(Clone, Copy, Default)]
struct EncodeResult {
    work: u64,
    semantic: u64,
    calls: u32,
    full_binds: u32,
}

fn encoder_ops() -> Vec<DesiredState> {
    let mut ops = Vec::with_capacity(512);
    for block in 0..16 {
        for run in 0..8 {
            ops.push(DesiredState {
                kind: DrawKind::Sprite,
                blend: (run == 7) as u8,
                camera: (block % 2) as u8,
                texture: 1 + (run / 2) as u64,
                repeat: false,
                depth: false,
                cull: CullMode::Back,
            });
        }
        ops.push(DesiredState {
            kind: DrawKind::Mesh,
            blend: 0,
            camera: (block % 2) as u8,
            texture: 0,
            repeat: false,
            depth: false,
            cull: CullMode::None,
        });
        for run in 0..12 {
            let depth = run >= 10;
            ops.push(DesiredState {
                kind: DrawKind::TexturedMesh,
                blend: (run == 9) as u8,
                camera: (block % 2) as u8,
                texture: 4,
                repeat: true,
                depth,
                cull: if depth {
                    CullMode::Back
                } else {
                    CullMode::None
                },
            });
        }
        ops.push(DesiredState {
            kind: DrawKind::Mesh,
            blend: 0,
            camera: (block % 2) as u8,
            texture: 0,
            repeat: false,
            depth: false,
            cull: CullMode::None,
        });
    }
    ops
}

fn legacy_encode(ops: &[DesiredState]) -> EncodeResult {
    let mut out = EncodeResult::default();
    let mut kind = None;
    let mut blend = None;
    let mut camera = None;
    let mut texture = None;
    let mut depth = None;
    for op in ops {
        if kind != Some(op.kind) {
            match op.kind {
                DrawKind::Sprite | DrawKind::Mesh => {
                    emit(&mut out, 2);
                    if op.kind == DrawKind::Mesh {
                        emit_full_bind(&mut out);
                    }
                    depth = Some(false);
                }
                DrawKind::TexturedMesh => depth = None,
            }
            kind = Some(op.kind);
            blend = None;
            camera = None;
            texture = None;
        }
        if blend != Some(op.blend) {
            emit(&mut out, 1);
            blend = Some(op.blend);
        }
        if camera != Some(op.camera) {
            emit(&mut out, 1);
            camera = Some(op.camera);
        }
        if op.texture != 0 && texture != Some((op.texture, op.repeat)) {
            emit(&mut out, 2);
            texture = Some((op.texture, op.repeat));
        }
        if op.kind == DrawKind::TexturedMesh && depth != Some(op.depth) {
            emit(&mut out, 2);
            depth = Some(op.depth);
        }
        if matches!(op.kind, DrawKind::Sprite | DrawKind::TexturedMesh) {
            emit_full_bind(&mut out);
        }
        emit(&mut out, 1);
        out.semantic = semantic_checksum(out.semantic, *op);
    }
    out
}

fn cached_encode(ops: &[DesiredState]) -> EncodeResult {
    let mut out = EncodeResult::default();
    let mut cache = EncoderCache::default();
    for op in ops {
        match op.kind {
            DrawKind::Sprite | DrawKind::TexturedMesh => match cache.instance_buffer(op.kind) {
                BufferUpdate::Bind => {
                    emit_full_bind(&mut out);
                }
                BufferUpdate::Offset => emit(&mut out, 1),
            },
            DrawKind::Mesh => {
                if cache.kind_changed(DrawKind::Mesh) {
                    emit_full_bind(&mut out);
                }
            }
        }
        if cache.pipeline_changed(op.kind, op.blend) {
            emit(&mut out, 1);
        }
        if cache.depth_changed(op.depth) {
            emit(&mut out, 1);
        }
        if cache.cull_changed(op.cull) {
            emit(&mut out, 1);
        }
        let camera_slot = usize::from(op.kind == DrawKind::TexturedMesh);
        if cache.camera_changed(camera_slot, op.camera) {
            emit(&mut out, 1);
        }
        if op.texture != 0 {
            if cache.texture_changed(op.texture) {
                emit(&mut out, 1);
            }
            if cache.sampler_changed(op.texture, op.repeat) {
                emit(&mut out, 1);
            }
        }
        emit(&mut out, 1);
        out.semantic = semantic_checksum(out.semantic, *op);
    }
    out
}

#[inline(always)]
fn emit(out: &mut EncodeResult, count: u32) {
    for _ in 0..count {
        metal_call(&mut out.work);
    }
    out.calls += count;
}

#[inline(never)]
fn metal_call(work: &mut u64) {
    *work = black_box(work.rotate_left(7).wrapping_add(0x9e37_79b9));
}

fn emit_full_bind(out: &mut EncodeResult) {
    emit(out, 1);
    out.full_binds += 1;
}

fn semantic_checksum(checksum: u64, state: DesiredState) -> u64 {
    let kind = match state.kind {
        DrawKind::Sprite => 1,
        DrawKind::Mesh => 2,
        DrawKind::TexturedMesh => 3,
    };
    checksum
        .rotate_left(9)
        .wrapping_add(kind)
        .wrapping_add(u64::from(state.blend) << 4)
        .wrapping_add(u64::from(state.camera) << 8)
        .wrapping_add(state.texture << 12)
        .wrapping_add(u64::from(state.repeat) << 48)
        .wrapping_add(u64::from(state.depth) << 49)
}

#[cfg(target_os = "macos")]
fn benchmark_render_pass() {
    use metal::{
        Device, MTLClearColor, MTLLoadAction, MTLPixelFormat, MTLStoreAction, MTLTextureType,
        MTLTextureUsage, RenderPassDescriptor, TextureDescriptor,
    };
    use objc::rc::autoreleasepool;

    let device = Device::system_default().expect("Metal device");
    let color_desc = TextureDescriptor::new();
    color_desc.set_texture_type(MTLTextureType::D2);
    color_desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    color_desc.set_width(64);
    color_desc.set_height(64);
    color_desc.set_usage(MTLTextureUsage::RenderTarget);
    let color = device.new_texture(&color_desc);
    let legacy = measure(|| {
        autoreleasepool(|| {
            let pass = RenderPassDescriptor::new();
            let attachment = pass.color_attachments().object_at(0).expect("attachment 0");
            attachment.set_load_action(MTLLoadAction::Clear);
            attachment.set_store_action(MTLStoreAction::Store);
            attachment.set_texture(Some(&color));
            attachment.set_clear_color(MTLClearColor::new(0.1, 0.2, 0.3, 1.0));
            black_box(pass as *const _ as usize as u64)
        })
    });
    let pass = autoreleasepool(|| RenderPassDescriptor::new().to_owned());
    let attachment = pass
        .color_attachments()
        .object_at(0)
        .expect("attachment 0")
        .to_owned();
    let retained = measure(|| {
        autoreleasepool(|| {
            attachment.set_texture(Some(&color));
            attachment.set_clear_color(MTLClearColor::new(0.1, 0.2, 0.3, 1.0));
            attachment.set_texture(None);
            black_box(&pass);
            black_box(&attachment);
            1
        })
    });

    println!("\nrender-pass descriptor setup (actual Metal objects)");
    print_measurement("legacy descriptor/frame", &legacy, 1);
    print_measurement("retained pass + attachment", &retained, 1);
    println!("Objective-C descriptor creates/frame: legacy 1, retained 0");
    println!("Objective-C attachment getter messages/frame: legacy 2, retained 0");
}

#[cfg(not(target_os = "macos"))]
fn benchmark_render_pass() {
    println!("\nrender-pass descriptor setup: run this benchmark on macOS for Metal timings");
    println!("Objective-C descriptor creates/frame: legacy 1, retained 0");
    println!("Objective-C attachment getter messages/frame: legacy 2, retained 0");
}

fn main() {
    let mut legacy_mesh = MeshLookupBench::new();
    let mut dense_mesh = MeshLookupBench::new();
    assert_eq!(legacy_mesh.legacy_frame(), dense_mesh.dense_frame());
    let legacy_lookup = measure(|| legacy_mesh.legacy_frame());
    let dense_lookup = measure(|| dense_mesh.dense_frame());
    assert_zero_alloc(&legacy_lookup);
    assert_zero_alloc(&dense_lookup);

    println!("Metal gameplay CPU hot paths");
    println!("\nretained textured-mesh resolution and recording");
    print_measurement("legacy double hash lookup", &legacy_lookup, MESH_RUNS);
    print_measurement("dense cache slot", &dense_lookup, MESH_RUNS);

    let ops = encoder_ops();
    let legacy_counts = legacy_encode(&ops);
    let cached_counts = cached_encode(&ops);
    assert_eq!(legacy_counts.semantic, cached_counts.semantic);
    let legacy_encoder = measure(|| {
        let result = legacy_encode(&ops);
        result.work ^ result.semantic
    });
    let cached_encoder = measure(|| {
        let result = cached_encode(&ops);
        result.work ^ result.semantic
    });
    assert_zero_alloc(&legacy_encoder);
    assert_zero_alloc(&cached_encoder);

    println!("\nMetal encoder command-planning model");
    print_measurement("legacy kind-local cache", &legacy_encoder, ops.len());
    print_measurement("persistent state cache", &cached_encoder, ops.len());
    println!(
        "Metal calls/frame: {} -> {}; full buffer binds/frame: {} -> {}",
        legacy_counts.calls,
        cached_counts.calls,
        legacy_counts.full_binds,
        cached_counts.full_binds,
    );

    benchmark_render_pass();
}
