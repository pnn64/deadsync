use deadlib_render_core::{
    BlendMode, DenseSlotMap, DrawOp, RenderFrame, TexturedMeshGeometry, TexturedMeshRun,
    TexturedMeshUploads, TexturedMeshVertex, TexturedMeshVertices, resolve_textured_meshes,
    resolve_textured_meshes_legacy,
};
use rustc_hash::FxBuildHasher;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const GEOMETRIES: usize = 128;
const DRAWS: usize = 512;
const WARMUP: usize = 5_000;
const SAMPLES: usize = 100;
const OPS_PER_SAMPLE: usize = 1_000;
const ALLOC_OPS: usize = 10_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; the counters only
// observe successful allocator calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Measurement {
    ns_per_op: f64,
    cycles_per_op: Option<f64>,
    p95_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_group<const N: usize>(mut ops: [&mut dyn FnMut() -> u64; N]) -> [Measurement; N] {
    assert!(N > 0);
    for round in 0..WARMUP {
        for offset in 0..N {
            black_box(ops[(round + offset) % N]());
        }
    }

    let mut elapsed = [Duration::ZERO; N];
    let mut cycles = [Some(0u64); N];
    let mut checksums = [0u64; N];
    let mut samples: [Vec<Duration>; N] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
    for sample in 0..SAMPLES {
        for offset in 0..N {
            let index = (sample + offset) % N;
            let cycle_start = cycle_counter();
            let started = Instant::now();
            let mut checksum = 0u64;
            for _ in 0..OPS_PER_SAMPLE {
                checksum = checksum.wrapping_add(black_box(ops[index]()));
            }
            let sample_elapsed = started.elapsed();
            let cycle_end = cycle_counter();
            elapsed[index] += sample_elapsed;
            samples[index].push(sample_elapsed);
            checksums[index] = checksums[index].wrapping_add(checksum);
            cycles[index] = cycles[index]
                .zip(cycle_start.zip(cycle_end))
                .map(|(total, (start, end))| total.wrapping_add(end.wrapping_sub(start)));
        }
    }

    let allocated: [AllocSnapshot; N] = std::array::from_fn(|index| {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        for _ in 0..ALLOC_OPS {
            black_box(ops[index]());
        }
        ALLOC.enabled.store(false, Ordering::Relaxed);
        ALLOC.snapshot().delta(before)
    });

    let operations = (SAMPLES * OPS_PER_SAMPLE) as f64;
    std::array::from_fn(|index| {
        samples[index].sort_unstable();
        Measurement {
            ns_per_op: elapsed[index].as_secs_f64() * 1_000_000_000.0 / operations,
            cycles_per_op: cycles[index].map(|cycles| cycles as f64 / operations),
            p95_ns: samples[index][SAMPLES * 95 / 100].as_secs_f64() * 1_000_000_000.0
                / OPS_PER_SAMPLE as f64,
            allocated: allocated[index],
            checksum: checksums[index],
        }
    })
}

type LegacySlots = HashMap<u64, u64, FxBuildHasher>;

struct LegacyPipeline {
    frame: RenderFrame,
    cache: LegacySlots,
    uploads: TexturedMeshUploads,
}

impl LegacyPipeline {
    fn frame(&mut self) -> u64 {
        let cache = &self.cache;
        resolve_textured_meshes_legacy(&self.frame, &mut self.uploads, |key, _| {
            cache.contains_key(&key).then_some(key)
        });
        record_hash(&self.frame.ops, &self.uploads, cache)
    }
}

struct MemoPipeline {
    frame: RenderFrame,
    cache: LegacySlots,
    uploads: TexturedMeshUploads,
}

impl MemoPipeline {
    fn frame(&mut self) -> u64 {
        let cache = &self.cache;
        resolve_textured_meshes(&self.frame, &mut self.uploads, |key, _| {
            cache.contains_key(&key).then_some(key)
        });
        record_hash(&self.frame.ops, &self.uploads, cache)
    }
}

struct DensePipeline {
    frame: RenderFrame,
    cache: DenseSlotMap<u64>,
    uploads: TexturedMeshUploads,
}

struct LegacyDensePipeline {
    frame: RenderFrame,
    cache: DenseSlotMap<u64>,
    uploads: TexturedMeshUploads,
}

impl LegacyDensePipeline {
    fn frame(&mut self) -> u64 {
        let cache = &self.cache;
        resolve_textured_meshes_legacy(&self.frame, &mut self.uploads, |key, _| {
            cache.get(key).map(|(slot, _)| slot)
        });
        record_dense(&self.frame.ops, &self.uploads, &self.cache)
    }
}

impl DensePipeline {
    fn frame(&mut self) -> u64 {
        let cache = &self.cache;
        resolve_textured_meshes(&self.frame, &mut self.uploads, |key, _| {
            cache.get(key).map(|(slot, _)| slot)
        });
        record_dense(&self.frame.ops, &self.uploads, &self.cache)
    }
}

fn pipelines() -> (
    LegacyPipeline,
    MemoPipeline,
    LegacyDensePipeline,
    DensePipeline,
) {
    let geometries = (0..GEOMETRIES)
        .map(|_| TexturedMeshGeometry {
            vertices: TexturedMeshVertices::Shared(Arc::from([TexturedMeshVertex::default(); 6])),
            cache_key: 0,
        })
        .enumerate()
        .map(|(index, mut geometry)| {
            geometry.cache_key = mesh_key(index);
            geometry
        })
        .collect::<Vec<_>>();
    let ops = (0..DRAWS)
        .map(|draw| {
            DrawOp::TexturedMesh(TexturedMeshRun {
                geometry: ((draw * 37 + draw / 5) % GEOMETRIES) as u32,
                instance_start: draw as u32,
                instance_count: 1,
                blend: BlendMode::Alpha,
                texture_handle: 1,
                camera: 0,
                depth_test: false,
            })
        })
        .collect::<Vec<_>>();
    let render_frame = || RenderFrame {
        clear_color: [0.0; 4],
        render_targets: Vec::new(),
        cameras: Vec::new(),
        sprite_instances: Vec::new(),
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: geometries.clone(),
        ops: ops.clone(),
    };
    let mut legacy = LegacySlots::with_capacity_and_hasher(GEOMETRIES, Default::default());
    let mut memo = LegacySlots::with_capacity_and_hasher(GEOMETRIES, Default::default());
    let mut dense = DenseSlotMap::with_capacity(GEOMETRIES);
    let mut legacy_dense = DenseSlotMap::with_capacity(GEOMETRIES);
    for index in 0..GEOMETRIES {
        let key = mesh_key(index);
        let value = (index as u64 + 1).wrapping_mul(0x9e37_79b9);
        legacy.insert(key, value);
        memo.insert(key, value);
        legacy_dense.insert(key, value);
        dense.insert(key, value);
    }
    (
        LegacyPipeline {
            frame: render_frame(),
            cache: legacy,
            uploads: TexturedMeshUploads::with_capacity(0, GEOMETRIES),
        },
        MemoPipeline {
            frame: render_frame(),
            cache: memo,
            uploads: TexturedMeshUploads::with_capacity(0, GEOMETRIES),
        },
        LegacyDensePipeline {
            frame: render_frame(),
            cache: legacy_dense,
            uploads: TexturedMeshUploads::with_capacity(0, GEOMETRIES),
        },
        DensePipeline {
            frame: render_frame(),
            cache: dense,
            uploads: TexturedMeshUploads::with_capacity(0, GEOMETRIES),
        },
    )
}

const fn mesh_key(index: usize) -> u64 {
    (index as u64 + 0x1000_0001)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        | 1
}

fn record_hash(ops: &[DrawOp], uploads: &TexturedMeshUploads, cache: &LegacySlots) -> u64 {
    let mut checksum = 0u64;
    let mut last_buffer = None;
    for op in ops {
        let DrawOp::TexturedMesh(run) = op else {
            continue;
        };
        let source = uploads.source(run.geometry).expect("resolved geometry");
        let Some(key) = source.buffer_key() else {
            continue;
        };
        if last_buffer != Some(key) {
            checksum = checksum.wrapping_add(cache[&key]);
            last_buffer = Some(key);
        }
        checksum = checksum.wrapping_add(u64::from(source.vertex_count()));
    }
    checksum
}

fn record_dense(ops: &[DrawOp], uploads: &TexturedMeshUploads, cache: &DenseSlotMap<u64>) -> u64 {
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
            checksum = checksum.wrapping_add(*cache.get_slot(buffer_key).expect("cached slot"));
            last_buffer = Some(buffer_key);
        }
        checksum = checksum.wrapping_add(u64::from(source.vertex_count()));
    }
    checksum
}

#[derive(Clone, Copy)]
struct PendingBatch {
    frame: u8,
    payload: u64,
}

struct RetirementBench {
    batches: Vec<PendingBatch>,
}

impl RetirementBench {
    fn new() -> Self {
        Self {
            batches: (1..=32)
                .map(|frame| PendingBatch {
                    frame,
                    payload: u64::from(frame).wrapping_mul(0x9e37_79b9),
                })
                .collect(),
        }
    }

    fn legacy_pending(&mut self) -> u64 {
        let mut pending = Vec::with_capacity(self.batches.len());
        for batch in std::mem::take(&mut self.batches) {
            if batch.frame != 0 {
                pending.push(batch);
            }
        }
        self.batches = pending;
        pending_checksum(&self.batches)
    }

    fn in_place_pending(&mut self) -> u64 {
        self.batches.retain_mut(|batch| batch.frame != 0);
        pending_checksum(&self.batches)
    }
}

fn pending_checksum(batches: &[PendingBatch]) -> u64 {
    batches
        .iter()
        .fold(0, |checksum, batch| checksum.rotate_left(5) ^ batch.payload)
}

fn main() {
    let (mut legacy, mut memo, mut legacy_dense, mut dense) = pipelines();
    assert_eq!(legacy.frame(), memo.frame());
    assert_eq!(legacy.frame(), legacy_dense.frame());
    assert_eq!(legacy.frame(), dense.frame());
    let mut legacy_op = || legacy.frame();
    let mut memo_op = || memo.frame();
    let mut legacy_dense_op = || legacy_dense.frame();
    let mut dense_op = || dense.frame();
    let [
        legacy_result,
        memo_result,
        legacy_dense_result,
        dense_result,
    ] = measure_group([
        &mut legacy_op,
        &mut memo_op,
        &mut legacy_dense_op,
        &mut dense_op,
    ]);
    assert_eq!(legacy_result.checksum, memo_result.checksum);
    assert_eq!(legacy_result.checksum, legacy_dense_result.checksum);
    assert_eq!(legacy_result.checksum, dense_result.checksum);
    assert_zero_alloc(&legacy_result);
    assert_zero_alloc(&memo_result);
    assert_zero_alloc(&legacy_dense_result);
    assert_zero_alloc(&dense_result);

    println!(
        "graphics textured-mesh resolution + recording ({GEOMETRIES} geometries, {DRAWS} draws)"
    );
    print_result("old: hash prepare + record", &legacy_result, DRAWS);
    print_result("memoized preparation", &memo_result, DRAWS);
    print_result("dense recording", &legacy_dense_result, DRAWS);
    print_result("memo + dense recording", &dense_result, DRAWS);
    print_change("old -> memo", &legacy_result, &memo_result);
    print_change("old -> dense", &legacy_result, &legacy_dense_result);
    print_change("old -> memo+dense", &legacy_result, &dense_result);
    print_change("dense -> memo+dense", &legacy_dense_result, &dense_result);

    let (mut legacy_mixed, mut memo_mixed, _, _) = pipelines();
    legacy_mixed.frame.tmesh_geometries[GEOMETRIES - 1].cache_key = 0;
    memo_mixed.frame.tmesh_geometries[GEOMETRIES - 1].cache_key = 0;
    assert_eq!(legacy_mixed.frame(), memo_mixed.frame());
    let mut legacy_mixed_op = || legacy_mixed.frame();
    let mut memo_mixed_op = || memo_mixed.frame();
    let [legacy_mixed_result, memo_mixed_result] =
        measure_group([&mut legacy_mixed_op, &mut memo_mixed_op]);
    assert_eq!(legacy_mixed_result.checksum, memo_mixed_result.checksum);
    assert_zero_alloc(&legacy_mixed_result);
    assert_zero_alloc(&memo_mixed_result);
    println!("\nmixed fallback (127 cached geometries, one transient)");
    print_result("old resolver", &legacy_mixed_result, DRAWS);
    print_result("new fallback", &memo_mixed_result, DRAWS);
    print_change("old -> fallback", &legacy_mixed_result, &memo_mixed_result);

    let mut old_retirement = RetirementBench::new();
    let mut new_retirement = RetirementBench::new();
    assert_eq!(
        old_retirement.legacy_pending(),
        new_retirement.in_place_pending()
    );
    let mut old_retirement_op = || old_retirement.legacy_pending();
    let mut new_retirement_op = || new_retirement.in_place_pending();
    let [old_retirement_result, new_retirement_result] =
        measure_group([&mut old_retirement_op, &mut new_retirement_op]);
    assert_eq!(
        old_retirement_result.checksum,
        new_retirement_result.checksum
    );
    assert_zero_alloc(&new_retirement_result);

    println!("\nVulkan pending upload-batch scan (32 in-flight batches)");
    print_result("old: rebuild pending vec", &old_retirement_result, 32);
    print_result("new: retain in place", &new_retirement_result, 32);
    print_change(
        "old -> in-place",
        &old_retirement_result,
        &new_retirement_result,
    );
}

fn print_result(label: &str, result: &Measurement, items: usize) {
    let ops = ALLOC_OPS as f64;
    println!(
        "  {label:<27} {:>9.2} ns/op {:>9.2} cycles/op {:>9.2} ns p95 \
         {:>8.2} Mitem/s {:>5.2} alloc {:>5.2} realloc {:>5.2} free {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        items as f64 * 1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.frees as f64 / ops,
        result.allocated.churn_bytes() as f64 / ops,
    );
}

fn print_change(label: &str, old: &Measurement, new: &Measurement) {
    println!(
        "  {label:<27} {:>8.2}% latency {:>8.2}% cycles {:>8.2}% p95 {:>8.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn assert_zero_alloc(result: &Measurement) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.frees, 0);
    assert_eq!(result.allocated.churn_bytes(), 0);
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
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
