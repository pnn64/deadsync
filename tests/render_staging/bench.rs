use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Run with:
// rustc -O --edition=2024 tests/render_staging/bench.rs -o /tmp/render_staging_bench
// /tmp/render_staging_bench
//
// This is intentionally std-only so it can run on machines that do not have the
// Vulkan loader required by the full deadsync benchmark binaries.

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SPRITE_ITERS: usize = 18_000;
const TMESH_ITERS: usize = 8_000;

type TextureHandle = u64;
type FastMap<V> = HashMap<u64, V, BuildHasherDefault<FxHasher>>;

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

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
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

#[derive(Default)]
struct FxHasher(u64);

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = self.0.rotate_left(5) ^ u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i.wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlendMode {
    Alpha,
}

#[derive(Clone)]
enum ObjectType {
    Sprite(SpriteInstanceRaw),
    TexturedMesh {
        transform: [f32; 16],
        tint: [f32; 4],
        vertices: Arc<[TexturedMeshVertex]>,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        texture_mask: bool,
        depth_test: bool,
    },
}

#[derive(Clone)]
#[allow(dead_code)] // Keep production staging fields in the model even when a scenario ignores them.
struct RenderObject {
    object_type: ObjectType,
    texture_handle: TextureHandle,
    blend: BlendMode,
    z: i16,
    order: u32,
    camera: u8,
}

#[derive(Clone, Copy)]
struct SpriteInput {
    texture_handle: TextureHandle,
    center: [f32; 4],
    size: [f32; 2],
    tint: [f32; 4],
    order: u32,
}

#[derive(Clone)]
struct TMeshInput {
    texture_handle: TextureHandle,
    vertices: Arc<[TexturedMeshVertex]>,
    transform: [f32; 16],
    order: u32,
}

#[derive(Clone, Copy)]
struct TexturedMeshVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    tex_matrix_scale: [f32; 2],
}

#[derive(Clone, Copy)]
struct SpriteInstanceRaw {
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    edge_fade: [f32; 4],
    texture_mask: f32,
}

#[derive(Clone, Copy)]
struct TMeshInstanceRaw {
    transform: [f32; 16],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SpriteRun {
    instance_start: u32,
    instance_count: u32,
    texture_handle: TextureHandle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TMeshRun {
    vertex_start: u32,
    vertex_count: u32,
    instance_start: u32,
    instance_count: u32,
    texture_handle: TextureHandle,
    depth_test: bool,
}

enum DrawOp {
    Sprite(SpriteRun),
    TMesh(TMeshRun),
}

#[derive(Default)]
struct Scratch {
    sprites: Vec<SpriteInstanceRaw>,
    tmesh_vertices: Vec<TexturedMeshVertex>,
    tmesh_instances: Vec<TMeshInstanceRaw>,
    ops: Vec<DrawOp>,
    shared_geom: FastMap<(u32, u32)>,
}

struct BenchResult {
    name: String,
    iters: usize,
    elapsed: Duration,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    println!("render staging microbench");
    println!("std-only model of RenderObject staging vs direct prepared buffers\n");

    run_sprite_pair("sprites same texture 4096", 4096, 1);
    run_sprite_pair("sprites 64 textures 4096", 4096, 64);
    run_sprite_pair("sprites 1024 textures 4096", 4096, 1024);
    run_tmesh_pair("shared tmesh 256 x 24 verts", 256, 24, false);
    run_tmesh_pair("shared tmesh repeated geom 256 x 24 verts", 256, 24, true);
}

fn run_sprite_pair(name: &str, count: usize, textures: usize) {
    let input = make_sprites(count, textures);
    let mut objects = Vec::new();
    let mut staged = Scratch::default();
    let mut direct = Scratch::default();
    stage_sprites(&input, &mut objects);
    prepare(&objects, &mut staged);
    direct_sprites(&input, &mut direct);

    let staged = bench(
        format!("{name}: staged RenderObject + prepare"),
        SPRITE_ITERS,
        || {
            stage_sprites(black_box(&input), &mut objects);
            prepare(&objects, &mut staged);
            checksum_scratch(&staged)
        },
    );
    let direct = bench(
        format!("{name}: direct prepared buffers"),
        SPRITE_ITERS,
        || {
            direct_sprites(black_box(&input), &mut direct);
            checksum_scratch(&direct)
        },
    );

    print_result(&staged);
    print_result(&direct);
    print_ratio("direct vs staged", &staged, &direct);
    println!();
}

fn run_tmesh_pair(name: &str, count: usize, verts: usize, repeated: bool) {
    let input = make_tmeshes(count, verts, repeated);
    let mut objects = Vec::new();
    let mut staged = Scratch::default();
    let mut direct = Scratch::default();
    stage_tmeshes(&input, &mut objects);
    prepare(&objects, &mut staged);
    direct_tmeshes(&input, &mut direct);

    let staged = bench(
        format!("{name}: staged RenderObject + prepare"),
        TMESH_ITERS,
        || {
            stage_tmeshes(black_box(&input), &mut objects);
            prepare(&objects, &mut staged);
            checksum_scratch(&staged)
        },
    );
    let direct = bench(
        format!("{name}: direct prepared buffers"),
        TMESH_ITERS,
        || {
            direct_tmeshes(black_box(&input), &mut direct);
            checksum_scratch(&direct)
        },
    );

    print_result(&staged);
    print_result(&direct);
    print_ratio("direct vs staged", &staged, &direct);
    println!();
}

fn bench<F>(name: String, iters: usize, mut f: F) -> BenchResult
where
    F: FnMut() -> u64,
{
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iters {
        checksum = checksum.wrapping_add(f());
    }
    BenchResult {
        name,
        iters,
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(result: &BenchResult) {
    let ns = result.elapsed.as_nanos() as f64 / result.iters as f64;
    println!(
        "{:<54} {:>10.1} ns/iter allocs={} reallocs={} bytes={} checksum={:016x}",
        result.name,
        ns,
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.bytes,
        result.checksum
    );
}

fn print_ratio(label: &str, base: &BenchResult, new: &BenchResult) {
    let ratio = new.elapsed.as_secs_f64() / base.elapsed.as_secs_f64();
    println!("{label}: {ratio:.3}x");
}

fn make_sprites(count: usize, textures: usize) -> Vec<SpriteInput> {
    (0..count)
        .map(|i| SpriteInput {
            texture_handle: 1 + (permute(i, textures) as TextureHandle),
            center: [i as f32, (i % 97) as f32, 0.0, 0.0],
            size: [32.0 + (i % 5) as f32, 32.0],
            tint: [1.0, 0.5, 0.25, 1.0],
            order: i as u32,
        })
        .collect()
}

fn make_tmeshes(count: usize, verts: usize, repeated: bool) -> Vec<TMeshInput> {
    let shared = repeated.then(|| Arc::from(make_vertices(0, verts).into_boxed_slice()));
    (0..count)
        .map(|i| TMeshInput {
            texture_handle: 1 + (permute(i, 16) as TextureHandle),
            vertices: shared.as_ref().map_or_else(
                || Arc::from(make_vertices(i, verts).into_boxed_slice()),
                Arc::clone,
            ),
            transform: transform(i),
            order: i as u32,
        })
        .collect()
}

fn make_vertices(seed: usize, count: usize) -> Vec<TexturedMeshVertex> {
    (0..count)
        .map(|i| TexturedMeshVertex {
            pos: [i as f32, seed as f32, 0.0],
            uv: [i as f32 * 0.01, seed as f32 * 0.01],
            color: [1.0, 0.5, 0.25, 1.0],
            tex_matrix_scale: [1.0, 1.0],
        })
        .collect()
}

fn transform(i: usize) -> [f32; 16] {
    let mut out = [0.0; 16];
    out[0] = 1.0;
    out[5] = 1.0;
    out[10] = 1.0;
    out[15] = 1.0;
    out[12] = (i % 1024) as f32;
    out[13] = (i / 1024) as f32;
    out
}

fn stage_sprites(input: &[SpriteInput], objects: &mut Vec<RenderObject>) {
    objects.clear();
    objects.reserve(input.len());
    for sprite in input {
        objects.push(RenderObject {
            object_type: ObjectType::Sprite(SpriteInstanceRaw {
                center: sprite.center,
                size: sprite.size,
                rot_sin_cos: [0.0, 1.0],
                tint: sprite.tint,
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            }),
            texture_handle: sprite.texture_handle,
            blend: BlendMode::Alpha,
            z: 0,
            order: sprite.order,
            camera: 0,
        });
    }
}

fn stage_tmeshes(input: &[TMeshInput], objects: &mut Vec<RenderObject>) {
    objects.clear();
    objects.reserve(input.len());
    for mesh in input {
        objects.push(RenderObject {
            object_type: ObjectType::TexturedMesh {
                transform: mesh.transform,
                tint: [1.0; 4],
                vertices: Arc::clone(&mesh.vertices),
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                uv_tex_shift: [0.0, 0.0],
                texture_mask: false,
                depth_test: false,
            },
            texture_handle: mesh.texture_handle,
            blend: BlendMode::Alpha,
            z: 0,
            order: mesh.order,
            camera: 0,
        });
    }
}

fn prepare(objects: &[RenderObject], scratch: &mut Scratch) {
    scratch.sprites.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();
    scratch.ops.clear();
    scratch.shared_geom.clear();

    let mut sprite_run = None;
    for obj in objects {
        match &obj.object_type {
            ObjectType::Sprite(instance) => push_sprite_instance(
                &mut scratch.sprites,
                &mut scratch.ops,
                &mut sprite_run,
                obj.texture_handle,
                *instance,
            ),
            ObjectType::TexturedMesh {
                transform,
                tint,
                vertices,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                texture_mask,
                depth_test,
            } => {
                flush_sprite_run(&mut sprite_run, &mut scratch.ops);
                push_tmesh_instance(
                    scratch,
                    obj.texture_handle,
                    *transform,
                    *tint,
                    vertices,
                    *uv_scale,
                    *uv_offset,
                    *uv_tex_shift,
                    *texture_mask,
                    *depth_test,
                );
            }
        }
    }
    flush_sprite_run(&mut sprite_run, &mut scratch.ops);
}

fn direct_sprites(input: &[SpriteInput], scratch: &mut Scratch) {
    scratch.sprites.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();
    scratch.ops.clear();
    scratch.shared_geom.clear();

    let mut sprite_run = None;
    for sprite in input {
        push_sprite_instance(
            &mut scratch.sprites,
            &mut scratch.ops,
            &mut sprite_run,
            sprite.texture_handle,
            SpriteInstanceRaw {
                center: sprite.center,
                size: sprite.size,
                rot_sin_cos: [0.0, 1.0],
                tint: sprite.tint,
                uv_scale: [1.0, 1.0],
                uv_offset: [0.0, 0.0],
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            },
        );
    }
    flush_sprite_run(&mut sprite_run, &mut scratch.ops);
}

fn direct_tmeshes(input: &[TMeshInput], scratch: &mut Scratch) {
    scratch.sprites.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();
    scratch.ops.clear();
    scratch.shared_geom.clear();

    for mesh in input {
        push_tmesh_instance(
            scratch,
            mesh.texture_handle,
            mesh.transform,
            [1.0; 4],
            &mesh.vertices,
            [1.0, 1.0],
            [0.0, 0.0],
            [0.0, 0.0],
            false,
            false,
        );
    }
}

fn push_sprite_instance(
    sprites: &mut Vec<SpriteInstanceRaw>,
    ops: &mut Vec<DrawOp>,
    sprite_run: &mut Option<SpriteRun>,
    texture_handle: TextureHandle,
    instance: SpriteInstanceRaw,
) {
    let instance_start = sprites.len() as u32;
    sprites.push(instance);

    if let Some(last) = sprite_run.as_mut()
        && last.texture_handle == texture_handle
        && last.instance_start + last.instance_count == instance_start
    {
        last.instance_count += 1;
        return;
    }

    flush_sprite_run(sprite_run, ops);
    *sprite_run = Some(SpriteRun {
        instance_start,
        instance_count: 1,
        texture_handle,
    });
}

fn push_tmesh_instance(
    scratch: &mut Scratch,
    texture_handle: TextureHandle,
    transform: [f32; 16],
    tint: [f32; 4],
    vertices: &Arc<[TexturedMeshVertex]>,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
    depth_test: bool,
) {
    let geom_key = ((vertices.as_ptr() as usize as u64) << 16) ^ vertices.len() as u64;
    let (vertex_start, vertex_count) = if let Some(source) = scratch.shared_geom.get(&geom_key) {
        *source
    } else {
        let start = scratch.tmesh_vertices.len() as u32;
        scratch.tmesh_vertices.extend_from_slice(vertices.as_ref());
        let count = vertices.len() as u32;
        scratch.shared_geom.insert(geom_key, (start, count));
        (start, count)
    };

    let instance_start = scratch.tmesh_instances.len() as u32;
    scratch.tmesh_instances.push(TMeshInstanceRaw {
        transform,
        tint,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        texture_mask: texture_mask as u8 as f32,
    });

    if let Some(DrawOp::TMesh(last)) = scratch.ops.last_mut()
        && last.texture_handle == texture_handle
        && last.depth_test == depth_test
        && last.vertex_start == vertex_start
        && last.vertex_count == vertex_count
        && last.instance_start + last.instance_count == instance_start
    {
        last.instance_count += 1;
        return;
    }

    scratch.ops.push(DrawOp::TMesh(TMeshRun {
        vertex_start,
        vertex_count,
        instance_start,
        instance_count: 1,
        texture_handle,
        depth_test,
    }));
}

fn flush_sprite_run(run: &mut Option<SpriteRun>, ops: &mut Vec<DrawOp>) {
    if let Some(run) = run.take() {
        ops.push(DrawOp::Sprite(run));
    }
}

fn checksum_scratch(scratch: &Scratch) -> u64 {
    let mut out = scratch
        .sprites
        .len()
        .wrapping_add(scratch.tmesh_vertices.len() << 8)
        .wrapping_add(scratch.tmesh_instances.len() << 16)
        .wrapping_add(scratch.ops.len() << 24) as u64;
    for sprite in scratch.sprites.iter().take(8) {
        out = out
            .wrapping_mul(131)
            .wrapping_add(sprite.center[0].to_bits() as u64)
            .wrapping_add(sprite.size[0].to_bits() as u64)
            .wrapping_add(sprite.rot_sin_cos[1].to_bits() as u64)
            .wrapping_add(sprite.tint[3].to_bits() as u64)
            .wrapping_add(sprite.uv_scale[0].to_bits() as u64)
            .wrapping_add(sprite.uv_offset[0].to_bits() as u64)
            .wrapping_add(sprite.local_offset[0].to_bits() as u64)
            .wrapping_add(sprite.local_offset_rot_sin_cos[1].to_bits() as u64)
            .wrapping_add(sprite.edge_fade[0].to_bits() as u64)
            .wrapping_add(sprite.texture_mask.to_bits() as u64);
    }
    for vertex in scratch.tmesh_vertices.iter().take(8) {
        out = out
            .wrapping_mul(131)
            .wrapping_add(vertex.pos[0].to_bits() as u64)
            .wrapping_add(vertex.uv[0].to_bits() as u64)
            .wrapping_add(vertex.color[3].to_bits() as u64)
            .wrapping_add(vertex.tex_matrix_scale[0].to_bits() as u64);
    }
    for instance in scratch.tmesh_instances.iter().take(8) {
        out = out
            .wrapping_mul(131)
            .wrapping_add(instance.transform[12].to_bits() as u64)
            .wrapping_add(instance.tint[3].to_bits() as u64)
            .wrapping_add(instance.uv_scale[0].to_bits() as u64)
            .wrapping_add(instance.uv_offset[0].to_bits() as u64)
            .wrapping_add(instance.uv_tex_shift[0].to_bits() as u64)
            .wrapping_add(instance.texture_mask.to_bits() as u64);
    }
    for op in &scratch.ops {
        match op {
            DrawOp::Sprite(run) => {
                out = out
                    .wrapping_mul(131)
                    .wrapping_add(run.instance_start as u64)
                    .wrapping_add(run.instance_count as u64)
                    .wrapping_add(run.texture_handle);
            }
            DrawOp::TMesh(run) => {
                out = out
                    .wrapping_mul(131)
                    .wrapping_add(run.vertex_start as u64)
                    .wrapping_add(run.vertex_count as u64)
                    .wrapping_add(run.instance_start as u64)
                    .wrapping_add(run.instance_count as u64)
                    .wrapping_add(run.texture_handle);
            }
        }
    }
    out
}

fn permute(i: usize, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    i.wrapping_mul(1_103_515_245).wrapping_add(12_345) % n
}
