use glam::Mat4;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PROJECTION_COUNT: usize = 32;
const PROJECTION_STRIDE: usize = 256;
const TEXTURE_WIDTH: usize = 853;
const TEXTURE_HEIGHT: usize = 480;
const TEXTURE_PACKED_ROW: usize = TEXTURE_WIDTH * 4;
const TEXTURE_ALIGNED_ROW: usize = TEXTURE_PACKED_ROW.next_multiple_of(256);
const STAGING_SIZES: [usize; 4] = [64 * 1024, 96 * 1024, 160 * 1024, 256 * 1024];
const FRAMES_IN_FLIGHT: usize = 3;

#[derive(Clone, Copy)]
struct BenchConfig {
    warmup: usize,
    samples: usize,
    ops_per_sample: usize,
    alloc_ops: usize,
}

const PROJECTION_CONFIG: BenchConfig = BenchConfig {
    warmup: 5_000,
    samples: 100,
    ops_per_sample: 1_000,
    alloc_ops: 10_000,
};
const COPY_CONFIG: BenchConfig = BenchConfig {
    warmup: 32,
    samples: 50,
    ops_per_sample: 16,
    alloc_ops: 32,
};

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

fn measure_pair(
    config: BenchConfig,
    mut old: impl FnMut() -> u64,
    mut new: impl FnMut() -> u64,
) -> [Measurement; 2] {
    let mut ops: [&mut dyn FnMut() -> u64; 2] = [&mut old, &mut new];
    for round in 0..config.warmup {
        black_box(ops[round % 2]());
        black_box(ops[(round + 1) % 2]());
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [Some(0u64); 2];
    let mut checksums = [0u64; 2];
    let mut samples: [Vec<Duration>; 2] =
        std::array::from_fn(|_| Vec::with_capacity(config.samples));
    for sample in 0..config.samples {
        for offset in 0..2 {
            let index = (sample + offset) % 2;
            let cycle_start = cycle_counter();
            let started = Instant::now();
            let mut checksum = 0u64;
            for _ in 0..config.ops_per_sample {
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

    let allocated: [AllocSnapshot; 2] = std::array::from_fn(|index| {
        let before = ALLOC.snapshot();
        ALLOC.enabled.store(true, Ordering::Relaxed);
        for _ in 0..config.alloc_ops {
            black_box(ops[index]());
        }
        ALLOC.enabled.store(false, Ordering::Relaxed);
        ALLOC.snapshot().delta(before)
    });
    let operations = (config.samples * config.ops_per_sample) as f64;
    std::array::from_fn(|index| {
        samples[index].sort_unstable();
        Measurement {
            ns_per_op: elapsed[index].as_secs_f64() * 1_000_000_000.0 / operations,
            cycles_per_op: cycles[index].map(|value| value as f64 / operations),
            p95_ns: samples[index][config.samples * 95 / 100].as_secs_f64() * 1_000_000_000.0
                / config.ops_per_sample as f64,
            allocated: allocated[index],
            checksum: checksums[index],
        }
    })
}

struct OldProjection {
    upload: Vec<u8>,
    keys: Vec<[u32; 16]>,
    cameras: Vec<Mat4>,
    fallback: Mat4,
    frame: usize,
}

struct NewProjection {
    upload: Vec<u8>,
    cameras: Vec<Mat4>,
    fallback: Mat4,
    frame: usize,
}

fn projection_cameras() -> Vec<Mat4> {
    (0..PROJECTION_COUNT)
        .map(|index| Mat4::from_translation([index as f32, index as f32 * 0.5, 0.0].into()))
        .collect()
}

impl OldProjection {
    fn frame(&mut self) -> u64 {
        update_fallback(&mut self.fallback, self.frame);
        self.frame += 1;
        let changed = old_stage_projection(
            &mut self.upload,
            &mut self.keys,
            &self.cameras,
            self.fallback,
        );
        upload_checksum(&self.upload, changed)
    }
}

impl NewProjection {
    fn frame(&mut self) -> u64 {
        update_fallback(&mut self.fallback, self.frame);
        self.frame += 1;
        let changed = new_stage_projection(&mut self.upload, &self.cameras, self.fallback);
        upload_checksum(&self.upload, changed)
    }
}

fn update_fallback(fallback: &mut Mat4, frame: usize) {
    if frame.is_multiple_of(8) {
        fallback.w_axis.x = if fallback.w_axis.x == 0.25 { 0.5 } else { 0.25 };
    }
}

fn old_stage_projection(
    upload: &mut Vec<u8>,
    keys: &mut Vec<[u32; 16]>,
    cameras: &[Mat4],
    fallback: Mat4,
) -> bool {
    let needed = cameras.len() + 1;
    let mut changed = keys.len() != needed;
    keys.resize(needed, [0; 16]);
    for (slot, matrix) in keys
        .iter_mut()
        .zip(cameras.iter().copied().chain(std::iter::once(fallback)))
    {
        let key = matrix.to_cols_array().map(f32::to_bits);
        if *slot != key {
            *slot = key;
            changed = true;
        }
    }
    if !changed {
        return false;
    }
    upload.resize(needed * PROJECTION_STRIDE, 0);
    for (index, key) in keys.iter().enumerate() {
        let bytes = bytemuck::cast_slice(std::slice::from_ref(key));
        let offset = index * PROJECTION_STRIDE;
        upload[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    true
}

fn new_stage_projection(upload: &mut Vec<u8>, cameras: &[Mat4], fallback: Mat4) -> bool {
    let needed = cameras.len() + 1;
    let mut changed = upload.len() != needed * PROJECTION_STRIDE;
    upload.resize(needed * PROJECTION_STRIDE, 0);
    for (index, matrix) in cameras.iter().chain(std::iter::once(&fallback)).enumerate() {
        let columns = matrix.to_cols_array();
        let bytes = bytemuck::cast_slice(std::slice::from_ref(&columns));
        let offset = index * PROJECTION_STRIDE;
        let slot = &mut upload[offset..offset + bytes.len()];
        if slot != bytes {
            slot.copy_from_slice(bytes);
            changed = true;
        }
    }
    changed
}

fn upload_checksum(upload: &[u8], changed: bool) -> u64 {
    u64::from(changed)
        .wrapping_add(upload.len() as u64)
        .wrapping_add(u64::from(upload[PROJECTION_COUNT * PROJECTION_STRIDE]))
}

struct OldRows {
    source: Vec<u8>,
}

struct NewRows {
    source: Vec<u8>,
    scratch: Vec<u8>,
}

fn texture_source() -> Vec<u8> {
    (0..TEXTURE_PACKED_ROW * TEXTURE_HEIGHT)
        .map(|index| index.wrapping_mul(37) as u8)
        .collect()
}

impl OldRows {
    fn stage(&self) -> u64 {
        let staged = old_stage_rows(
            &self.source,
            TEXTURE_PACKED_ROW,
            TEXTURE_ALIGNED_ROW,
            TEXTURE_HEIGHT,
        );
        row_checksum(&staged)
    }
}

impl NewRows {
    fn stage(&mut self) -> u64 {
        let staged = new_stage_rows(
            &self.source,
            TEXTURE_PACKED_ROW,
            TEXTURE_ALIGNED_ROW,
            TEXTURE_HEIGHT,
            &mut self.scratch,
        );
        row_checksum(staged)
    }
}

fn old_stage_rows(source: &[u8], packed_row: usize, aligned_row: usize, rows: usize) -> Vec<u8> {
    let mut padded = vec![0; aligned_row * rows];
    for (source, destination) in source
        .chunks_exact(packed_row)
        .zip(padded.chunks_exact_mut(aligned_row))
    {
        destination[..packed_row].copy_from_slice(source);
    }
    padded
}

fn new_stage_rows<'a>(
    source: &[u8],
    packed_row: usize,
    aligned_row: usize,
    rows: usize,
    scratch: &'a mut Vec<u8>,
) -> &'a [u8] {
    scratch.resize(aligned_row * rows, 0);
    for (source, destination) in source
        .chunks_exact(packed_row)
        .zip(scratch.chunks_exact_mut(aligned_row))
    {
        destination[..packed_row].copy_from_slice(source);
        destination[packed_row..].fill(0);
    }
    scratch
}

fn row_checksum(staged: &[u8]) -> u64 {
    staged.len() as u64
        + u64::from(staged[0])
        + u64::from(staged[TEXTURE_PACKED_ROW - 1])
        + u64::from(staged[TEXTURE_ALIGNED_ROW])
        + u64::from(staged[staged.len() - 1])
}

struct StagingResource {
    bytes: Vec<u8>,
}

impl StagingResource {
    fn new(size: usize, source: &[u8]) -> Self {
        let mut bytes = vec![0; size];
        bytes[..size].copy_from_slice(&source[..size]);
        Self { bytes }
    }

    fn fill(&mut self, size: usize, source: &[u8]) {
        self.bytes[..size].copy_from_slice(&source[..size]);
    }
}

struct OldStaging {
    source: Vec<u8>,
    submitted: [Vec<StagingResource>; FRAMES_IN_FLIGHT],
    frame: usize,
}

struct NewStaging {
    source: Vec<u8>,
    submitted: [Vec<StagingResource>; FRAMES_IN_FLIGHT],
    pool: Vec<StagingResource>,
    frame: usize,
}

impl OldStaging {
    fn cycle(&mut self) -> u64 {
        self.submitted[self.frame].clear();
        let mut checksum = 0u64;
        for size in rotated_staging_sizes(self.frame) {
            let staging = StagingResource::new(size, &self.source);
            checksum = staging_checksum(checksum, &staging, size);
            self.submitted[self.frame].push(staging);
        }
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        checksum
    }
}

impl NewStaging {
    fn cycle(&mut self) -> u64 {
        self.pool.append(&mut self.submitted[self.frame]);
        let mut checksum = 0u64;
        for size in rotated_staging_sizes(self.frame) {
            let index = self
                .pool
                .iter()
                .enumerate()
                .filter(|(_, staging)| staging.bytes.len() >= size)
                .min_by_key(|(_, staging)| staging.bytes.len())
                .map(|(index, _)| index);
            let mut staging = index.map_or_else(
                || StagingResource::new(size, &self.source),
                |index| self.pool.swap_remove(index),
            );
            staging.fill(size, &self.source);
            checksum = staging_checksum(checksum, &staging, size);
            self.submitted[self.frame].push(staging);
        }
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        checksum
    }
}

fn rotated_staging_sizes(frame: usize) -> impl Iterator<Item = usize> {
    (0..STAGING_SIZES.len()).map(move |index| STAGING_SIZES[(index + frame) % STAGING_SIZES.len()])
}

fn staging_checksum(checksum: u64, staging: &StagingResource, size: usize) -> u64 {
    checksum
        .rotate_left(7)
        .wrapping_add(size as u64)
        .wrapping_add(u64::from(staging.bytes[0]))
        .wrapping_add(u64::from(staging.bytes[size - 1]))
}

fn print_result(label: &str, result: &Measurement, items: usize, alloc_ops: usize) {
    println!(
        "  {label:<27} {:>10.2} ns/op {:>10.2} cycles/op {:>10.2} ns p95 \
         {:>8.2} Mitem/s {:>5.2} alloc {:>5.2} realloc {:>5.2} free {:>12.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        items as f64 * 1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / alloc_ops as f64,
        result.allocated.reallocs as f64 / alloc_ops as f64,
        result.allocated.frees as f64 / alloc_ops as f64,
        result.allocated.churn_bytes() as f64 / alloc_ops as f64,
    );
}

fn print_change(old: &Measurement, new: &Measurement) {
    println!(
        "  old -> new                  {:>9.2}% latency {:>9.2}% cycles {:>9.2}% p95 {:>9.2}% churn",
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

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 && new == 0.0 {
        return 0.0;
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

fn main() {
    let cameras = projection_cameras();
    let mut old_projection = OldProjection {
        upload: Vec::new(),
        keys: Vec::new(),
        cameras: cameras.clone(),
        fallback: Mat4::IDENTITY,
        frame: 0,
    };
    let mut new_projection = NewProjection {
        upload: Vec::new(),
        cameras,
        fallback: Mat4::IDENTITY,
        frame: 0,
    };
    for _ in 0..32 {
        assert_eq!(old_projection.frame(), new_projection.frame());
        assert_eq!(old_projection.upload, new_projection.upload);
    }
    let [old_projection_result, new_projection_result] = measure_pair(
        PROJECTION_CONFIG,
        || old_projection.frame(),
        || new_projection.frame(),
    );
    assert_eq!(
        old_projection_result.checksum,
        new_projection_result.checksum
    );
    println!(
        "wgpu projection staging ({} cameras, one change every 8 frames)",
        PROJECTION_COUNT
    );
    print_result(
        "old: keys then full copy",
        &old_projection_result,
        PROJECTION_COUNT + 1,
        PROJECTION_CONFIG.alloc_ops,
    );
    print_result(
        "new: compare/copy slots",
        &new_projection_result,
        PROJECTION_COUNT + 1,
        PROJECTION_CONFIG.alloc_ops,
    );
    print_change(&old_projection_result, &new_projection_result);

    let source = texture_source();
    let expected = old_stage_rows(
        &source,
        TEXTURE_PACKED_ROW,
        TEXTURE_ALIGNED_ROW,
        TEXTURE_HEIGHT,
    );
    let mut parity_scratch = Vec::new();
    assert_eq!(
        expected,
        new_stage_rows(
            &source,
            TEXTURE_PACKED_ROW,
            TEXTURE_ALIGNED_ROW,
            TEXTURE_HEIGHT,
            &mut parity_scratch,
        )
    );
    let old_rows = OldRows {
        source: source.clone(),
    };
    let mut new_rows = NewRows {
        source,
        scratch: Vec::new(),
    };
    let [old_rows_result, new_rows_result] =
        measure_pair(COPY_CONFIG, || old_rows.stage(), || new_rows.stage());
    assert_eq!(old_rows_result.checksum, new_rows_result.checksum);
    println!(
        "\nMetal row staging ({}x{} RGBA, {}-byte aligned rows)",
        TEXTURE_WIDTH, TEXTURE_HEIGHT, TEXTURE_ALIGNED_ROW
    );
    print_result(
        "old: allocate padded rows",
        &old_rows_result,
        TEXTURE_HEIGHT,
        COPY_CONFIG.alloc_ops,
    );
    print_result(
        "new: reuse row scratch",
        &new_rows_result,
        TEXTURE_HEIGHT,
        COPY_CONFIG.alloc_ops,
    );
    print_change(&old_rows_result, &new_rows_result);

    let staging_source = (0..*STAGING_SIZES.iter().max().unwrap())
        .map(|index| index.wrapping_mul(13) as u8)
        .collect::<Vec<_>>();
    let mut old_staging = OldStaging {
        source: staging_source.clone(),
        submitted: std::array::from_fn(|_| Vec::with_capacity(STAGING_SIZES.len())),
        frame: 0,
    };
    let mut new_staging = NewStaging {
        source: staging_source,
        submitted: std::array::from_fn(|_| Vec::with_capacity(STAGING_SIZES.len())),
        pool: Vec::with_capacity(STAGING_SIZES.len() * FRAMES_IN_FLIGHT),
        frame: 0,
    };
    for _ in 0..FRAMES_IN_FLIGHT * 2 {
        assert_eq!(old_staging.cycle(), new_staging.cycle());
    }
    let [old_staging_result, new_staging_result] =
        measure_pair(COPY_CONFIG, || old_staging.cycle(), || new_staging.cycle());
    assert_eq!(old_staging_result.checksum, new_staging_result.checksum);
    println!(
        "\nVulkan staging reuse ({} updates, {} frames in flight)",
        STAGING_SIZES.len(),
        FRAMES_IN_FLIGHT
    );
    print_result(
        "old: allocate each update",
        &old_staging_result,
        STAGING_SIZES.len(),
        COPY_CONFIG.alloc_ops,
    );
    print_result(
        "new: best-fit fence pool",
        &new_staging_result,
        STAGING_SIZES.len(),
        COPY_CONFIG.alloc_ops,
    );
    print_change(&old_staging_result, &new_staging_result);
}
