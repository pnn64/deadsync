use deadlib_render::{
    BlendMode, INVALID_TMESH_CACHE_KEY, ObjectType, RenderList, RenderObject, SpriteInstanceRaw,
    TexturedMeshInstanceRaw, TexturedMeshVertex, TexturedMeshVertices,
    build_ordered_render_batches, build_render_batches,
    draw_prep::{DrawScratch, prepare},
};
use glam::Mat4 as Matrix4;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SPRITES: usize = 1_536;
const HOLDS: usize = 96;
const HOLD_VERTICES: usize = 48;
const TEXT_RUNS: usize = 48;
const TEXT_VERTICES: usize = 72;
const WARMUP_FRAMES: usize = 256;
const MEASURE_FRAMES: usize = 10_000;
const BENCH_RUNS: usize = 5;

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

// SAFETY: all operations delegate to `System` with the caller-provided layout;
// the atomics only observe successful allocations and do not affect ownership.
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
        // SAFETY: `ptr` and `old` are forwarded unchanged to the system allocator.
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
    alloc: AllocSnapshot,
    ops: usize,
    staged_vertices: usize,
}

fn main() {
    let frame = gameplay_frame();
    let mut unordered_objects = frame.objects.clone();
    for (index, object) in unordered_objects.iter_mut().enumerate() {
        object.z = (index % 16) as i16;
    }
    let mut runs = Vec::with_capacity(BENCH_RUNS);
    let mut batch_runs = Vec::with_capacity(BENCH_RUNS);
    let mut order_scan_runs = Vec::with_capacity(BENCH_RUNS);
    let mut abort_runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        runs.push(run(&frame));
        batch_runs.push(run_batch_build(&frame));
        order_scan_runs.push(run_order_scan(&frame));
        abort_runs.push(run_ordered_abort(&unordered_objects));
    }
    runs.sort_unstable_by_key(|result| result.elapsed);
    batch_runs.sort_unstable();
    order_scan_runs.sort_unstable();
    abort_runs.sort_unstable();
    let result = runs.swap_remove(BENCH_RUNS / 2);
    let batch_elapsed = batch_runs[BENCH_RUNS / 2];
    let order_scan_elapsed = order_scan_runs[BENCH_RUNS / 2];
    let abort_elapsed = abort_runs[BENCH_RUNS / 2];
    let frames = MEASURE_FRAMES as f64;

    println!("draw preparation: mixed gameplay frame");
    println!("{SPRITES} sprites + {HOLDS} reusable holds x2 passes + {TEXT_RUNS} cached text runs");
    println!("median of {BENCH_RUNS} runs");
    println!(
        "{:>9.2} us/frame  {:>7.2} allocs/frame  {:>8.2} KiB/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.alloc.allocs as f64 / frames,
        result.alloc.bytes as f64 / frames / 1024.0,
        result.alloc.reallocs as f64 / frames,
    );
    println!(
        "{} draw runs, {} transient textured-mesh vertices staged/frame",
        result.ops, result.staged_vertices
    );
    println!(
        "{:>9.2} us/frame to construct composition batches",
        batch_elapsed.as_secs_f64() * 1_000_000.0 / frames,
    );
    println!(
        "{:>9.2} us/frame for the removed best-case object-order scan",
        order_scan_elapsed.as_secs_f64() * 1_000_000.0 / frames,
    );
    println!(
        "{:>9.2} us/frame combined shared pipeline work",
        (result.elapsed + batch_elapsed).as_secs_f64() * 1_000_000.0 / frames,
    );
    println!(
        "{:>9.2} us/frame unordered fast-path abort before dense-sort fallback",
        abort_elapsed.as_secs_f64() * 1_000_000.0 / frames,
    );
}

fn run_ordered_abort(objects: &[RenderObject]) -> Duration {
    let mut batches = Vec::with_capacity(objects.len());
    for _ in 0..WARMUP_FRAMES {
        assert!(!build_ordered_render_batches(objects, &mut batches));
    }
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        assert!(!build_ordered_render_batches(
            black_box(objects),
            &mut batches
        ));
    }
    started.elapsed()
}

fn run_batch_build(frame: &RenderList) -> Duration {
    let mut batches = Vec::with_capacity(frame.objects.len());
    for _ in 0..WARMUP_FRAMES {
        assert!(build_ordered_render_batches(
            black_box(&frame.objects),
            &mut batches
        ));
        black_box(&batches);
    }
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        assert!(build_ordered_render_batches(
            black_box(&frame.objects),
            &mut batches
        ));
        black_box(&batches);
    }
    started.elapsed()
}

fn run_order_scan(frame: &RenderList) -> Duration {
    for _ in 0..WARMUP_FRAMES {
        black_box(scan_object_order(black_box(&frame.objects)));
    }
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        black_box(scan_object_order(black_box(&frame.objects)));
    }
    started.elapsed()
}

fn scan_object_order(objects: &[RenderObject]) -> bool {
    let Some(first) = objects.first() else {
        return true;
    };
    let mut min_z = first.z;
    let mut max_z = min_z;
    let mut sorted_by_z = true;
    let mut sorted_by_key = true;
    let mut previous = (first.z, first.order);
    for object in &objects[1..] {
        let key = (object.z, object.order);
        sorted_by_z &= previous.0 <= object.z;
        sorted_by_key &= previous <= key;
        min_z = min_z.min(object.z);
        max_z = max_z.max(object.z);
        previous = key;
    }
    black_box((min_z, max_z, sorted_by_z));
    sorted_by_key
}

fn run(frame: &RenderList) -> BenchResult {
    let mut scratch = DrawScratch::with_capacity(0, 0, 0, frame.objects.len());
    for _ in 0..WARMUP_FRAMES {
        prepare(black_box(frame), &mut scratch, |_, _| true);
        black_box(&scratch.ops);
    }
    let before = ALLOC.snapshot();
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        prepare(black_box(frame), &mut scratch, |_, _| true);
        black_box(&scratch.ops);
    }
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        ops: scratch.ops.len(),
        staged_vertices: scratch.tmesh_vertices.len(),
    }
}

fn gameplay_frame() -> RenderList {
    let mut sprite_instances = Vec::with_capacity(SPRITES);
    let mut objects = Vec::with_capacity(SPRITES + HOLDS * 2 + TEXT_RUNS);
    for index in 0..SPRITES {
        sprite_instances.push(SpriteInstanceRaw {
            center: [index as f32, (index % 64) as f32, 0.0, 1.0],
            size: [64.0, 64.0],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        });
        objects.push(RenderObject {
            object_type: ObjectType::Sprite(index as u32),
            texture_handle: 1 + (index / 24 % 8) as u64,
            blend: BlendMode::Alpha,
            z: (index / 384) as i16,
            order: index as u32,
            camera: 0,
        });
    }

    let hold_vertices = (0..HOLDS)
        .map(|hold| Arc::new(mesh_vertices(HOLD_VERTICES, hold as f32)))
        .collect::<Vec<_>>();
    for (hold, vertices) in hold_vertices.into_iter().enumerate() {
        for pass in 0..2 {
            objects.push(tmesh_object(
                Arc::clone(&vertices),
                INVALID_TMESH_CACHE_KEY,
                20 + (hold % 4) as u64,
                pass != 0,
                objects.len() as u32,
            ));
        }
    }

    for text in 0..TEXT_RUNS {
        objects.push(tmesh_object(
            Arc::new(mesh_vertices(TEXT_VERTICES, text as f32)),
            1_000 + text as u64,
            40 + (text % 4) as u64,
            false,
            objects.len() as u32,
        ));
    }

    let mut frame = RenderList {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances,
        objects,
        batches: Vec::new(),
    };
    build_render_batches(&frame.objects, &mut frame.batches);
    frame
}

fn tmesh_object(
    vertices: Arc<Vec<TexturedMeshVertex>>,
    geom_cache_key: u64,
    texture_handle: u64,
    texture_mask: bool,
    order: u32,
) -> RenderObject {
    RenderObject {
        object_type: ObjectType::TexturedMesh {
            instance: TexturedMeshInstanceRaw::new(
                Matrix4::IDENTITY,
                [1.0; 4],
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                texture_mask,
            ),
            vertices: TexturedMeshVertices::Reusable(vertices),
            geom_cache_key,
            depth_test: false,
        },
        texture_handle,
        blend: BlendMode::Alpha,
        z: 10,
        order,
        camera: 0,
    }
}

fn mesh_vertices(len: usize, seed: f32) -> Vec<TexturedMeshVertex> {
    (0..len)
        .map(|index| TexturedMeshVertex {
            pos: [index as f32, seed, 0.0],
            uv: [0.0; 2],
            color: [1.0; 4],
            tex_matrix_scale: [1.0; 2],
        })
        .collect()
}
