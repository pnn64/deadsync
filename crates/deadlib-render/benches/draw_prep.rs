use deadlib_render::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, MeshVertices, ObjectType, RenderBatchKind,
    RenderList, RenderObject, SpriteInstanceRaw, TexturedMeshInstanceRaw, TexturedMeshVertex,
    TexturedMeshVertices, build_ordered_render_batches, build_render_batches,
    build_sorted_render_batches,
    draw_prep::{
        __benchmark_prepare_adjacent_tmesh_reuse, __benchmark_prepare_unreserved_tmesh_instances,
        DrawOp, DrawScratch, MeshRun, prepare,
    },
};
use glam::{Mat4 as Matrix4, Vec3, Vec4 as Vector4};
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
const GAP_ORDER_STRIDE: u32 = 4;
const DENSITY_PLAYERS: usize = 2;
const DENSITY_POINTS: usize = 961;
const DENSITY_VERTICES_PER_PLAYER: usize = (DENSITY_POINTS - 1) * 6 + DENSITY_POINTS * 12;
const DENSITY_WARMUP_FRAMES: usize = 128;
const DENSITY_MEASURE_FRAMES: usize = 2_000;
const REUSED_TMESH_GEOMS: usize = 32;
const REUSED_TMESH_PASSES: usize = 8;
const REUSED_TMESH_VERTICES: usize = 96;
const COLD_TMESH_INSTANCES: usize = 4_096;
const COLD_BENCH_RUNS: usize = 1_000;

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
    println!(
        "render layout: object={} bytes, object type={} bytes, batch={} bytes, draw op={} bytes",
        std::mem::size_of::<RenderObject>(),
        std::mem::size_of::<ObjectType>(),
        std::mem::size_of::<deadlib_render::RenderBatch>(),
        std::mem::size_of::<DrawOp>(),
    );
    let frame = gameplay_frame();
    let mut unordered_objects = frame.objects.clone();
    for (index, object) in unordered_objects.iter_mut().enumerate() {
        object.z = (index % 16) as i16;
    }
    let mut runs = Vec::with_capacity(BENCH_RUNS);
    let mut batch_runs = Vec::with_capacity(BENCH_RUNS);
    let mut general_batch_runs = Vec::with_capacity(BENCH_RUNS);
    let mut order_scan_runs = Vec::with_capacity(BENCH_RUNS);
    let mut abort_runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        runs.push(run(&frame));
        batch_runs.push(run_batch_build(&frame));
        general_batch_runs.push(run_general_batch_build(&frame));
        order_scan_runs.push(run_order_scan(&frame));
        abort_runs.push(run_ordered_abort(&unordered_objects));
    }
    runs.sort_unstable_by_key(|result| result.elapsed);
    batch_runs.sort_unstable();
    general_batch_runs.sort_unstable();
    order_scan_runs.sort_unstable();
    abort_runs.sort_unstable();
    let result = runs.swap_remove(BENCH_RUNS / 2);
    let batch_elapsed = batch_runs[BENCH_RUNS / 2];
    let general_batch_elapsed = general_batch_runs[BENCH_RUNS / 2];
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
        "{:>9.2} us/frame to construct sorted composition batches",
        batch_elapsed.as_secs_f64() * 1_000_000.0 / frames,
    );
    println!(
        "{:>9.2} us/frame through general builder on sorted objects",
        general_batch_elapsed.as_secs_f64() * 1_000_000.0 / frames,
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

    let (gap_legacy_frame, gap_current_frame) = order_gap_frames(&frame);
    assert_order_gap_prepare_parity(&gap_legacy_frame, &gap_current_frame);
    let mut gap_legacy_runs = Vec::with_capacity(BENCH_RUNS);
    let mut gap_current_runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        gap_legacy_runs.push(run_order_gap_prepare(&gap_legacy_frame));
        gap_current_runs.push(run_order_gap_prepare(&gap_current_frame));
    }
    gap_legacy_runs.sort_unstable_by_key(|result| result.elapsed);
    gap_current_runs.sort_unstable_by_key(|result| result.elapsed);
    let gap_legacy = gap_legacy_runs.swap_remove(BENCH_RUNS / 2);
    let gap_current = gap_current_runs.swap_remove(BENCH_RUNS / 2);
    assert_eq!(gap_legacy.checksum, gap_current.checksum);
    println!(
        "\nlayer-interleaved gameplay draw preparation \
         (global order stride {GAP_ORDER_STRIDE}, median of {BENCH_RUNS} runs)"
    );
    print_order_gap_result("consecutive-only", &gap_legacy);
    print_order_gap_result("sorted-adjacent", &gap_current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | draw runs {} -> {}",
        gap_legacy.elapsed.as_secs_f64() / gap_current.elapsed.as_secs_f64(),
        100.0 * (1.0 - gap_current.cycles as f64 / gap_legacy.cycles as f64),
        gap_legacy.ops,
        gap_current.ops,
    );

    let density_frame = density_mesh_frame();
    assert_density_prepare_parity(&density_frame);
    let mut density_legacy_runs = Vec::with_capacity(BENCH_RUNS);
    let mut density_fast_runs = Vec::with_capacity(BENCH_RUNS);
    for _ in 0..BENCH_RUNS {
        density_legacy_runs.push(run_density_mesh(
            &density_frame,
            prepare_density_mesh_legacy,
        ));
        density_fast_runs.push(run_density_mesh(
            &density_frame,
            prepare_density_mesh_current,
        ));
    }
    density_legacy_runs.sort_unstable_by_key(|result| result.elapsed);
    density_fast_runs.sort_unstable_by_key(|result| result.elapsed);
    let density_legacy = density_legacy_runs.swap_remove(BENCH_RUNS / 2);
    let density_fast = density_fast_runs.swap_remove(BENCH_RUNS / 2);
    assert_eq!(density_legacy.checksum, density_fast.checksum);
    println!(
        "\ngameplay density mesh draw preparation \
         ({DENSITY_PLAYERS} players, {DENSITY_POINTS} points, \
         {} vertices/frame, median of {BENCH_RUNS} runs)",
        DENSITY_PLAYERS * DENSITY_VERTICES_PER_PLAYER,
    );
    print_density_result("full matrix", &density_legacy);
    print_density_result("translate+flip", &density_fast);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        density_legacy.elapsed.as_secs_f64() / density_fast.elapsed.as_secs_f64(),
        100.0 * (1.0 - density_fast.cycles as f64 / density_legacy.cycles as f64),
    );

    benchmark_non_adjacent_tmesh_reuse();
    benchmark_tmesh_instance_reserve();
}

struct TMeshReuseResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    staged_vertices: usize,
    staged_bytes: usize,
    checksum: u64,
}

fn benchmark_non_adjacent_tmesh_reuse() {
    let frame = non_adjacent_tmesh_frame();
    assert_non_adjacent_tmesh_parity(&frame);
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = run_tmesh_reuse(&frame, prepare_tmesh_frame_current);
            let old = run_tmesh_reuse(&frame, prepare_tmesh_frame_legacy);
            (old, new)
        } else {
            let old = run_tmesh_reuse(&frame, prepare_tmesh_frame_legacy);
            let new = run_tmesh_reuse(&frame, prepare_tmesh_frame_current);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        for result in [&old, &new] {
            assert_eq!(result.allocated.allocs, 0);
            assert_eq!(result.allocated.reallocs, 0);
            assert_eq!(result.allocated.bytes, 0);
        }
        legacy.push(old);
        current.push(new);
    }
    legacy.sort_unstable_by_key(|result| result.elapsed);
    current.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let current = current.swap_remove(BENCH_RUNS / 2);
    println!(
        "\nnon-adjacent shared textured-mesh staging \
         ({REUSED_TMESH_GEOMS} geometries x {REUSED_TMESH_PASSES} passes, \
         {REUSED_TMESH_VERTICES} vertices each, median of {BENCH_RUNS} runs)"
    );
    print_tmesh_reuse("adjacent only", &legacy);
    print_tmesh_reuse("frame table", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | staged vertices {} -> {} ({:.1}% fewer)",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.staged_vertices,
        current.staged_vertices,
        100.0 * (1.0 - current.staged_vertices as f64 / legacy.staged_vertices as f64),
    );
}

fn run_tmesh_reuse(
    frame: &RenderList,
    prepare_frame: fn(&RenderList, &mut DrawScratch),
) -> TMeshReuseResult {
    let mut scratch = DrawScratch::with_capacity(
        0,
        0,
        REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES,
        REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES,
    );
    for _ in 0..WARMUP_FRAMES {
        prepare_frame(black_box(frame), &mut scratch);
        black_box(&scratch);
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        prepare_frame(black_box(frame), &mut scratch);
        black_box(&scratch);
    }
    let elapsed = started.elapsed();
    TMeshReuseResult {
        elapsed,
        cycles: read_cycles().saturating_sub(cycles_before),
        allocated: ALLOC.snapshot().delta(before),
        staged_vertices: scratch.tmesh_vertices.len(),
        staged_bytes: scratch
            .tmesh_vertices
            .capacity()
            .saturating_mul(std::mem::size_of::<TexturedMeshVertex>()),
        checksum: canonical_tmesh_checksum(&scratch),
    }
}

fn print_tmesh_reuse(label: &str, result: &TMeshReuseResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<14} {:>8.2} us/frame  {:>9.0} cycles/frame  \
         {:>8.1} M source-vertices/s  {:>5.2} allocs/frame  {:>7.1} KiB staged",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * (REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES * REUSED_TMESH_VERTICES) as f64
            / result.elapsed.as_secs_f64()
            / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.staged_bytes as f64 / 1024.0,
    );
}

struct ColdTMeshResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn benchmark_tmesh_instance_reserve() {
    let frame = cold_tmesh_frame();
    assert_cold_tmesh_parity(&frame);
    let mut legacy = Vec::with_capacity(BENCH_RUNS);
    let mut current = Vec::with_capacity(BENCH_RUNS);
    for run in 0..BENCH_RUNS {
        let (old, new) = if run % 2 == 0 {
            let new = measure_cold_tmesh(&frame, prepare_tmesh_frame_current);
            let old = measure_cold_tmesh(&frame, prepare_tmesh_frame_unreserved);
            (old, new)
        } else {
            let old = measure_cold_tmesh(&frame, prepare_tmesh_frame_unreserved);
            let new = measure_cold_tmesh(&frame, prepare_tmesh_frame_current);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        legacy.push(old);
        current.push(new);
    }
    legacy.sort_unstable_by_key(|result| result.elapsed);
    current.sort_unstable_by_key(|result| result.elapsed);
    let legacy = legacy.swap_remove(BENCH_RUNS / 2);
    let current = current.swap_remove(BENCH_RUNS / 2);

    println!(
        "\ncold textured-mesh instance staging \
         ({COLD_TMESH_INSTANCES} instances, median of {BENCH_RUNS} x {COLD_BENCH_RUNS} frames)"
    );
    print_cold_tmesh("push growth", &legacy);
    print_cold_tmesh("batch reserve", &current);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | reallocations/frame {:.1} -> {:.1}",
        legacy.elapsed.as_secs_f64() / current.elapsed.as_secs_f64(),
        100.0 * (1.0 - current.cycles as f64 / legacy.cycles as f64),
        legacy.allocated.reallocs as f64 / COLD_BENCH_RUNS as f64,
        current.allocated.reallocs as f64 / COLD_BENCH_RUNS as f64,
    );
}

fn measure_cold_tmesh(
    frame: &RenderList,
    prepare_frame: fn(&RenderList, &mut DrawScratch),
) -> ColdTMeshResult {
    let mut scratches = (0..COLD_BENCH_RUNS)
        .map(|_| DrawScratch::with_capacity(0, REUSED_TMESH_VERTICES, 0, 1))
        .collect::<Vec<_>>();
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for scratch in &mut scratches {
        prepare_frame(black_box(frame), scratch);
        let instance = scratch
            .tmesh_instances
            .last()
            .expect("the cold fixture has textured-mesh instances");
        checksum = checksum.rotate_left(7)
            ^ scratch.tmesh_instances.len() as u64
            ^ u64::from(instance.tint[3].to_bits());
        black_box(&*scratch);
    }
    ColdTMeshResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        allocated: ALLOC.snapshot().delta(before),
        checksum: black_box(checksum),
    }
}

fn print_cold_tmesh(label: &str, result: &ColdTMeshResult) {
    let frames = COLD_BENCH_RUNS as f64;
    println!(
        "  {label:<14} {:>8.2} us/frame  {:>10.0} cycles/frame  \
         {:>8.1} M instances/s  {:>4.1} allocs/frame  {:>4.1} reallocs/frame  {:>7.1} KiB/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames * COLD_TMESH_INSTANCES as f64 / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.reallocs as f64 / frames,
        result.allocated.bytes as f64 / frames / 1024.0,
    );
}

fn prepare_tmesh_frame_legacy(frame: &RenderList, scratch: &mut DrawScratch) {
    __benchmark_prepare_adjacent_tmesh_reuse(frame, scratch, |_, _| false);
}

fn prepare_tmesh_frame_current(frame: &RenderList, scratch: &mut DrawScratch) {
    prepare(frame, scratch, |_, _| false);
}

fn prepare_tmesh_frame_unreserved(frame: &RenderList, scratch: &mut DrawScratch) {
    __benchmark_prepare_unreserved_tmesh_instances(frame, scratch, |_, _| false);
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
        build_sorted_render_batches(black_box(&frame.objects), &mut batches);
        black_box(&batches);
    }
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        build_sorted_render_batches(black_box(&frame.objects), &mut batches);
        black_box(&batches);
    }
    started.elapsed()
}

fn run_general_batch_build(frame: &RenderList) -> Duration {
    let mut batches = Vec::with_capacity(frame.objects.len());
    for _ in 0..WARMUP_FRAMES {
        build_render_batches(black_box(&frame.objects), &mut batches);
        black_box(&batches);
    }
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        build_render_batches(black_box(&frame.objects), &mut batches);
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

struct OrderGapBenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    ops: usize,
    checksum: u64,
}

fn order_gap_frames(source: &RenderList) -> (RenderList, RenderList) {
    let mut legacy = source.clone();
    for object in &mut legacy.objects {
        object.order = object.order.saturating_mul(GAP_ORDER_STRIDE);
    }
    build_render_batches(&legacy.objects, &mut legacy.batches);

    let mut current = legacy.clone();
    build_sorted_render_batches(&current.objects, &mut current.batches);
    (legacy, current)
}

fn run_order_gap_prepare(frame: &RenderList) -> OrderGapBenchResult {
    let mut scratch = DrawScratch::with_capacity(0, 0, 0, frame.objects.len());
    for _ in 0..WARMUP_FRAMES {
        prepare(black_box(frame), &mut scratch, |_, _| true);
        black_box(&scratch);
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    for _ in 0..MEASURE_FRAMES {
        prepare(black_box(frame), &mut scratch, |_, _| true);
        black_box(&scratch);
    }
    let checksum = prepared_checksum(&scratch);
    OrderGapBenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        ops: scratch.ops.len(),
        checksum,
    }
}

fn print_order_gap_result(label: &str, result: &OrderGapBenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {label:<16} {:>9.2} us/frame  {:>9.0} cycles/frame  \
         {:>8.0} frames/s  {:>5.2} allocs/frame  {:>5.2} reallocs/frame  \
         {:>7.1} bytes/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
        result.allocated.allocs as f64 / frames,
        result.allocated.reallocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
    );
}

fn assert_order_gap_prepare_parity(legacy_frame: &RenderList, current_frame: &RenderList) {
    assert_eq!(legacy_frame.objects.len(), current_frame.objects.len());
    assert_eq!(
        legacy_frame.sprite_instances,
        current_frame.sprite_instances
    );

    let mut legacy = DrawScratch::default();
    let mut current = DrawScratch::default();
    prepare(legacy_frame, &mut legacy, |_, _| true);
    prepare(current_frame, &mut current, |_, _| true);

    assert_eq!(legacy.tmesh_vertices, current.tmesh_vertices);
    assert_eq!(legacy.tmesh_instances, current.tmesh_instances);
    assert_eq!(legacy.mesh_vertices.len(), current.mesh_vertices.len());
    for (legacy, current) in legacy.mesh_vertices.iter().zip(&current.mesh_vertices) {
        assert_eq!(legacy.pos, current.pos);
        assert_eq!(legacy.color, current.color);
    }
    assert_eq!(
        canonical_draw_sequence(&legacy.ops),
        canonical_draw_sequence(&current.ops)
    );
    assert!(
        current.ops.len() < legacy.ops.len(),
        "order-gap batching should reduce draw runs"
    );
}

#[derive(Debug, PartialEq, Eq)]
enum CanonicalDraw {
    Sprite {
        instance: u32,
        blend: BlendMode,
        texture: u64,
        camera: u8,
    },
    Mesh {
        vertex: u32,
        blend: BlendMode,
        camera: u8,
    },
    TexturedMesh {
        source: deadlib_render::draw_prep::TexturedMeshSource,
        instance: u32,
        blend: BlendMode,
        texture: u64,
        camera: u8,
        depth_test: bool,
    },
}

fn canonical_draw_sequence(ops: &[DrawOp]) -> Vec<CanonicalDraw> {
    let mut sequence = Vec::new();
    for op in ops {
        match *op {
            DrawOp::Sprite(run) => {
                for instance in run.instance_start..run.instance_start + run.instance_count {
                    sequence.push(CanonicalDraw::Sprite {
                        instance,
                        blend: run.blend,
                        texture: run.texture_handle,
                        camera: run.camera,
                    });
                }
            }
            DrawOp::Mesh(run) => {
                for vertex in run.vertex_start..run.vertex_start + run.vertex_count {
                    sequence.push(CanonicalDraw::Mesh {
                        vertex,
                        blend: run.blend,
                        camera: run.camera,
                    });
                }
            }
            DrawOp::TexturedMesh(run) => {
                for instance in run.instance_start..run.instance_start + run.instance_count {
                    sequence.push(CanonicalDraw::TexturedMesh {
                        source: run.source,
                        instance,
                        blend: run.blend,
                        texture: run.texture_handle,
                        camera: run.camera,
                        depth_test: run.depth_test,
                    });
                }
            }
        }
    }
    sequence
}

fn prepared_checksum(scratch: &DrawScratch) -> u64 {
    let draw_units = scratch
        .ops
        .iter()
        .map(|op| match op {
            DrawOp::Sprite(run) => u64::from(run.instance_count),
            DrawOp::Mesh(run) => u64::from(run.vertex_count),
            DrawOp::TexturedMesh(run) => u64::from(run.instance_count),
        })
        .sum::<u64>();
    let mut checksum = draw_units
        ^ ((scratch.tmesh_vertices.len() as u64) << 16)
        ^ ((scratch.tmesh_instances.len() as u64) << 32);
    for instance in scratch.tmesh_instances.iter().step_by(17) {
        checksum = checksum.rotate_left(7) ^ u64::from(instance.model_col3[0].to_bits());
        checksum = checksum.rotate_left(11) ^ u64::from(instance.tint[3].to_bits());
    }
    black_box(checksum)
}

struct DensityBenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn run_density_mesh(
    frame: &RenderList,
    prepare_frame: fn(&RenderList, &mut DrawScratch),
) -> DensityBenchResult {
    let mut scratch =
        DrawScratch::with_capacity(DENSITY_PLAYERS * DENSITY_VERTICES_PER_PLAYER, 0, 0, 1);
    for _ in 0..DENSITY_WARMUP_FRAMES {
        prepare_frame(black_box(frame), &mut scratch);
        black_box(&scratch);
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for frame_index in 0..DENSITY_MEASURE_FRAMES {
        prepare_frame(black_box(frame), &mut scratch);
        checksum ^= mesh_checksum(&scratch.mesh_vertices, frame_index);
        black_box(&scratch);
    }
    DensityBenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_density_result(label: &str, result: &DensityBenchResult) {
    let frames = DENSITY_MEASURE_FRAMES as f64;
    let vertices = (DENSITY_PLAYERS * DENSITY_VERTICES_PER_PLAYER) as f64;
    println!(
        "  {label:<14} {:>9.2} us/frame  {:>9.0} cycles/frame  \
         {:>8.1} Mvertices/s  {:>5.2} allocs/frame  {:>7.1} bytes/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        vertices * frames / result.elapsed.as_secs_f64() / 1_000_000.0,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
    );
}

fn prepare_density_mesh_current(frame: &RenderList, scratch: &mut DrawScratch) {
    prepare(frame, scratch, |_, _| true);
}

fn prepare_density_mesh_legacy(frame: &RenderList, scratch: &mut DrawScratch) {
    scratch.mesh_vertices.clear();
    scratch.tmesh_vertices.clear();
    scratch.tmesh_instances.clear();
    scratch.ops.clear();

    for batch in &frame.batches {
        let RenderBatchKind::Mesh {
            object_start,
            object_count,
            blend,
            camera,
        } = batch.kind
        else {
            continue;
        };
        let vertex_start = scratch.mesh_vertices.len() as u32;
        for object in &frame.objects[object_start as usize..(object_start + object_count) as usize]
        {
            let ObjectType::Mesh {
                transform,
                tint,
                vertices,
            } = &object.object_type
            else {
                continue;
            };
            scratch.mesh_vertices.reserve(vertices.len());
            for vertex in vertices.iter() {
                let pos = *transform * Vector4::new(vertex.pos[0], vertex.pos[1], 0.0, 1.0);
                scratch.mesh_vertices.push(MeshVertex {
                    pos: [pos.x, pos.y],
                    color: [
                        vertex.color[0] * tint[0],
                        vertex.color[1] * tint[1],
                        vertex.color[2] * tint[2],
                        vertex.color[3] * tint[3],
                    ],
                });
            }
        }
        let vertex_count = scratch.mesh_vertices.len() as u32 - vertex_start;
        if vertex_count != 0 {
            scratch.ops.push(DrawOp::Mesh(MeshRun {
                vertex_start,
                vertex_count,
                blend,
                camera,
            }));
        }
    }
}

fn assert_density_prepare_parity(frame: &RenderList) {
    let mut legacy = DrawScratch::default();
    let mut current = DrawScratch::default();
    prepare_density_mesh_legacy(frame, &mut legacy);
    prepare_density_mesh_current(frame, &mut current);
    assert_eq!(legacy.ops, current.ops);
    assert_eq!(legacy.mesh_vertices.len(), current.mesh_vertices.len());
    for (legacy, current) in legacy.mesh_vertices.iter().zip(&current.mesh_vertices) {
        assert_eq!(legacy.pos, current.pos);
        assert_eq!(legacy.color, current.color);
    }
}

fn mesh_checksum(vertices: &[MeshVertex], frame: usize) -> u64 {
    let mut checksum = vertices.len() as u64 ^ frame as u64;
    for vertex in vertices.iter().step_by(257) {
        checksum = checksum.rotate_left(7) ^ u64::from(vertex.pos[0].to_bits());
        checksum = checksum.rotate_left(11) ^ u64::from(vertex.pos[1].to_bits());
        checksum = checksum.rotate_left(13) ^ u64::from(vertex.color[3].to_bits());
    }
    black_box(checksum)
}

fn density_mesh_frame() -> RenderList {
    let vertices = Arc::new(
        (0..DENSITY_VERTICES_PER_PLAYER)
            .map(|index| {
                let point = index / 18;
                MeshVertex {
                    pos: [
                        point as f32 * 512.0 / (DENSITY_POINTS - 1) as f32,
                        (point % 101) as f32,
                    ],
                    color: [0.25, 0.5, 0.75, 1.0],
                }
            })
            .collect::<Vec<_>>(),
    );
    let objects = (0..DENSITY_PLAYERS)
        .map(|player| RenderObject {
            object_type: ObjectType::Mesh {
                transform: Matrix4::from_translation(Vec3::new(
                    64.0 + player as f32 * 640.0,
                    700.0,
                    0.0,
                )) * Matrix4::from_scale(Vec3::new(1.0, -1.0, 1.0)),
                tint: [1.0; 4],
                vertices: MeshVertices::Reusable(Arc::clone(&vertices)),
            },
            texture_handle: 0,
            blend: BlendMode::Alpha,
            z: 61,
            order: player as u32,
            camera: 0,
        })
        .collect();
    let mut frame = RenderList {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances: Vec::new(),
        objects,
        batches: Vec::new(),
    };
    build_render_batches(&frame.objects, &mut frame.batches);
    frame
}

fn non_adjacent_tmesh_frame() -> RenderList {
    let geometries = (0..REUSED_TMESH_GEOMS)
        .map(|geometry| Arc::new(mesh_vertices(REUSED_TMESH_VERTICES, geometry as f32)))
        .collect::<Vec<_>>();
    let mut objects = Vec::with_capacity(REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES);
    for pass in 0..REUSED_TMESH_PASSES {
        for (geometry, vertices) in geometries.iter().enumerate() {
            objects.push(tmesh_object(
                Arc::clone(vertices),
                INVALID_TMESH_CACHE_KEY,
                100 + geometry as u64,
                pass % 2 != 0,
                objects.len() as u32,
            ));
        }
    }
    let mut frame = RenderList {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances: Vec::new(),
        objects,
        batches: Vec::new(),
    };
    build_render_batches(&frame.objects, &mut frame.batches);
    assert_eq!(
        frame.batches.len(),
        REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES
    );
    frame
}

fn cold_tmesh_frame() -> RenderList {
    let vertices = Arc::new(mesh_vertices(REUSED_TMESH_VERTICES, 1.0));
    let objects = (0..COLD_TMESH_INSTANCES)
        .map(|order| {
            tmesh_object(
                Arc::clone(&vertices),
                INVALID_TMESH_CACHE_KEY,
                100,
                order % 2 != 0,
                order as u32,
            )
        })
        .collect::<Vec<_>>();
    let mut frame = RenderList {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances: Vec::new(),
        objects,
        batches: Vec::new(),
    };
    build_render_batches(&frame.objects, &mut frame.batches);
    assert_eq!(frame.batches.len(), 1);
    frame
}

fn assert_cold_tmesh_parity(frame: &RenderList) {
    let mut legacy = DrawScratch::default();
    let mut current = DrawScratch::default();
    prepare_tmesh_frame_unreserved(frame, &mut legacy);
    prepare_tmesh_frame_current(frame, &mut current);
    assert_eq!(legacy.ops, current.ops);
    assert_eq!(legacy.tmesh_vertices, current.tmesh_vertices);
    assert_eq!(legacy.tmesh_instances, current.tmesh_instances);
    assert_eq!(current.tmesh_instances.len(), COLD_TMESH_INSTANCES);
}

fn assert_non_adjacent_tmesh_parity(frame: &RenderList) {
    let mut legacy = DrawScratch::default();
    let mut current = DrawScratch::default();
    prepare_tmesh_frame_legacy(frame, &mut legacy);
    prepare_tmesh_frame_current(frame, &mut current);
    assert_eq!(legacy.ops.len(), current.ops.len());
    assert_eq!(legacy.tmesh_instances, current.tmesh_instances);
    assert_eq!(
        canonical_tmesh_checksum(&legacy),
        canonical_tmesh_checksum(&current)
    );
    assert_eq!(
        legacy.tmesh_vertices.len(),
        REUSED_TMESH_GEOMS * REUSED_TMESH_PASSES * REUSED_TMESH_VERTICES,
    );
    assert_eq!(
        current.tmesh_vertices.len(),
        REUSED_TMESH_GEOMS * REUSED_TMESH_VERTICES,
    );
}

fn canonical_tmesh_checksum(scratch: &DrawScratch) -> u64 {
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    for op in &scratch.ops {
        let DrawOp::TexturedMesh(run) = op else {
            continue;
        };
        checksum = checksum.rotate_left(7) ^ run.texture_handle;
        checksum = checksum.rotate_left(7) ^ u64::from(run.camera);
        checksum = checksum.rotate_left(7) ^ u64::from(run.depth_test);
        let start = run.source.vertex_start() as usize;
        let end = start + run.source.vertex_count() as usize;
        for vertex in &scratch.tmesh_vertices[start..end] {
            for value in vertex
                .pos
                .into_iter()
                .chain(vertex.uv)
                .chain(vertex.color)
                .chain(vertex.tex_matrix_scale)
            {
                checksum = checksum.rotate_left(5) ^ u64::from(value.to_bits());
            }
        }
        let start = run.instance_start as usize;
        let end = start + run.instance_count as usize;
        for instance in &scratch.tmesh_instances[start..end] {
            checksum = checksum.rotate_left(11) ^ u64::from(instance.tint[3].to_bits());
            checksum = checksum.rotate_left(11) ^ u64::from(instance.texture_mask.to_bits());
        }
    }
    black_box(checksum)
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
