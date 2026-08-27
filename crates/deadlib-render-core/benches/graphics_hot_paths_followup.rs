use deadlib_render_core::{SamplerCache, SamplerDesc, SamplerFilter, SamplerWrap};
use rustc_hash::FxBuildHasher;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, hash_map::Entry};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const GEOMETRY_KEYS: usize = 128;
const GEOMETRY_CALLS: usize = 256;
const DRAWS: usize = 512;
const FRAMES_IN_FLIGHT: usize = 3;
const STAGING_PER_BATCH: usize = 32;
const RETIRED_PER_BATCH: usize = 8;
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum GeometryIdentity {
    Cached(u64),
    Shared(usize),
}

type GeometryMap = HashMap<GeometryIdentity, u32, FxBuildHasher>;

struct LegacyGeometryBench {
    map: GeometryMap,
    keys: Vec<GeometryIdentity>,
}

struct EntryGeometryBench {
    map: GeometryMap,
    keys: Vec<GeometryIdentity>,
}

impl LegacyGeometryBench {
    fn frame(&mut self) -> u64 {
        self.map.clear();
        let mut next_geometry = 0u32;
        let mut checksum = 0u64;
        for &identity in &self.keys {
            let geometry = if let Some(&geometry) = self.map.get(&identity) {
                geometry
            } else {
                let geometry = next_geometry;
                next_geometry += 1;
                if self.map.len() < GEOMETRY_KEYS {
                    self.map.insert(identity, geometry);
                }
                geometry
            };
            checksum = checksum.rotate_left(5) ^ u64::from(geometry);
        }
        checksum ^ self.map.len() as u64
    }
}

impl EntryGeometryBench {
    fn frame(&mut self) -> u64 {
        self.map.clear();
        let mut next_geometry = 0u32;
        let mut checksum = 0u64;
        for &identity in &self.keys {
            let admit = self.map.len() < GEOMETRY_KEYS;
            let geometry = match self.map.entry(identity) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let geometry = next_geometry;
                    next_geometry += 1;
                    if admit {
                        entry.insert(geometry);
                    }
                    geometry
                }
            };
            checksum = checksum.rotate_left(5) ^ u64::from(geometry);
        }
        checksum ^ self.map.len() as u64
    }
}

fn geometry_benches() -> (LegacyGeometryBench, EntryGeometryBench) {
    let keys = (0..GEOMETRY_CALLS)
        .map(|call| (call * 37) % GEOMETRY_KEYS)
        .map(|key| {
            if key % 2 == 0 {
                GeometryIdentity::Cached(key as u64 + 1)
            } else {
                GeometryIdentity::Shared((key + 1) * 0x1000)
            }
        })
        .collect::<Vec<_>>();
    (
        LegacyGeometryBench {
            map: GeometryMap::with_capacity_and_hasher(GEOMETRY_KEYS, FxBuildHasher::default()),
            keys: keys.clone(),
        },
        EntryGeometryBench {
            map: GeometryMap::with_capacity_and_hasher(GEOMETRY_KEYS, FxBuildHasher::default()),
            keys,
        },
    )
}

const SAMPLERS: [SamplerDesc; 8] = [
    SamplerDesc {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Clamp,
        mipmaps: false,
    },
    SamplerDesc {
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Clamp,
        mipmaps: false,
    },
    SamplerDesc {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Repeat,
        mipmaps: false,
    },
    SamplerDesc {
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Repeat,
        mipmaps: false,
    },
    SamplerDesc {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Clamp,
        mipmaps: true,
    },
    SamplerDesc {
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Clamp,
        mipmaps: true,
    },
    SamplerDesc {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Repeat,
        mipmaps: true,
    },
    SamplerDesc {
        filter: SamplerFilter::Nearest,
        wrap: SamplerWrap::Repeat,
        mipmaps: true,
    },
];

struct LegacySamplerBench {
    samplers: HashMap<SamplerDesc, u64>,
    cursor: usize,
}

struct FixedSamplerBench {
    samplers: SamplerCache<u64>,
    cursor: usize,
}

impl LegacySamplerBench {
    fn lookups(&mut self) -> u64 {
        let mut checksum = 0u64;
        for lookup in 0..DRAWS {
            let desc = SAMPLERS[(self.cursor + lookup * 5) & 7];
            checksum = checksum.wrapping_add(*self.samplers.get(&desc).expect("warmed sampler"));
        }
        self.cursor = (self.cursor + 1) & 7;
        checksum
    }
}

impl FixedSamplerBench {
    fn lookups(&mut self) -> u64 {
        let mut checksum = 0u64;
        for lookup in 0..DRAWS {
            let desc = SAMPLERS[(self.cursor + lookup * 5) & 7];
            checksum = checksum.wrapping_add(*self.samplers.get(desc).expect("warmed sampler"));
        }
        self.cursor = (self.cursor + 1) & 7;
        checksum
    }
}

fn sampler_benches() -> (LegacySamplerBench, FixedSamplerBench) {
    let mut old = HashMap::new();
    let mut new = SamplerCache::default();
    for desc in SAMPLERS {
        let value = (desc.slot() as u64 + 1).wrapping_mul(0x9e37_79b9);
        old.insert(desc, value);
        new.insert(desc, value);
    }
    (
        LegacySamplerBench {
            samplers: old,
            cursor: 0,
        },
        FixedSamplerBench {
            samplers: new,
            cursor: 0,
        },
    )
}

#[derive(Default)]
struct UploadBatch {
    staging: Vec<u64>,
    retired: Vec<u64>,
}

struct LegacyUploadBench {
    pending: UploadBatch,
    submitted: [UploadBatch; FRAMES_IN_FLIGHT],
    frame: usize,
    sequence: u64,
}

struct RecycledUploadBench {
    pending: UploadBatch,
    recycled: UploadBatch,
    submitted: [UploadBatch; FRAMES_IN_FLIGHT],
    frame: usize,
    sequence: u64,
}

impl LegacyUploadBench {
    fn cycle(&mut self) -> u64 {
        fill_pending(&mut self.pending, self.sequence);
        let submitted = std::mem::take(&mut self.pending);
        let retired = std::mem::replace(&mut self.submitted[self.frame], submitted);
        let checksum = batch_checksum(&self.submitted[self.frame]) ^ batch_checksum(&retired);
        self.advance();
        checksum
    }

    const fn advance(&mut self) {
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        self.sequence = self.sequence.wrapping_add(1);
    }
}

impl RecycledUploadBench {
    fn cycle(&mut self) -> u64 {
        fill_pending(&mut self.pending, self.sequence);
        let mut retired = std::mem::take(&mut self.submitted[self.frame]);
        let retired_checksum = batch_checksum(&retired);
        retired.staging.clear();
        retired.retired.clear();
        recycle_empty_vec(&mut self.recycled.staging, &mut retired.staging);
        recycle_empty_vec(&mut self.recycled.retired, &mut retired.retired);

        let submitted = UploadBatch {
            staging: take_with_recycled(&mut self.pending.staging, &mut self.recycled.staging),
            retired: take_with_recycled(&mut self.pending.retired, &mut self.recycled.retired),
        };
        self.submitted[self.frame] = submitted;
        let checksum = batch_checksum(&self.submitted[self.frame]) ^ retired_checksum;
        self.frame = (self.frame + 1) % FRAMES_IN_FLIGHT;
        self.sequence = self.sequence.wrapping_add(1);
        checksum
    }
}

fn fill_pending(pending: &mut UploadBatch, sequence: u64) {
    debug_assert!(pending.staging.is_empty());
    debug_assert!(pending.retired.is_empty());
    for index in 0..STAGING_PER_BATCH {
        pending
            .staging
            .push(sequence.wrapping_mul(131).wrapping_add(index as u64));
    }
    for index in 0..RETIRED_PER_BATCH {
        pending
            .retired
            .push(sequence.wrapping_mul(17).wrapping_add(index as u64));
    }
}

fn batch_checksum(batch: &UploadBatch) -> u64 {
    batch
        .staging
        .iter()
        .chain(&batch.retired)
        .fold(0, |checksum, value| checksum.rotate_left(3) ^ value)
}

const fn recycle_empty_vec<T>(recycled: &mut Vec<T>, candidate: &mut Vec<T>) {
    if candidate.capacity() > recycled.capacity() {
        std::mem::swap(recycled, candidate);
    }
}

fn take_with_recycled<T>(active: &mut Vec<T>, recycled: &mut Vec<T>) -> Vec<T> {
    std::mem::replace(active, std::mem::take(recycled))
}

fn upload_benches() -> (LegacyUploadBench, RecycledUploadBench) {
    (
        LegacyUploadBench {
            pending: UploadBatch::default(),
            submitted: std::array::from_fn(|_| UploadBatch::default()),
            frame: 0,
            sequence: 0,
        },
        RecycledUploadBench {
            pending: UploadBatch::default(),
            recycled: UploadBatch::default(),
            submitted: std::array::from_fn(|_| UploadBatch::default()),
            frame: 0,
            sequence: 0,
        },
    )
}

fn cold_sampler_alloc(fixed: bool) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    if fixed {
        let mut cache = SamplerCache::default();
        for desc in SAMPLERS {
            cache.insert(desc, desc.slot() as u64);
        }
        black_box(cache);
    } else {
        let mut cache = HashMap::new();
        for desc in SAMPLERS {
            cache.insert(desc, desc.slot() as u64);
        }
        black_box(cache);
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn main() {
    let (mut old_geometry, mut new_geometry) = geometry_benches();
    assert_eq!(old_geometry.frame(), new_geometry.frame());
    let mut old_geometry_op = || old_geometry.frame();
    let mut new_geometry_op = || new_geometry.frame();
    let [old_geometry_result, new_geometry_result] =
        measure_group([&mut old_geometry_op, &mut new_geometry_op]);
    assert_eq!(old_geometry_result.checksum, new_geometry_result.checksum);
    assert_zero_alloc(&old_geometry_result);
    assert_zero_alloc(&new_geometry_result);

    println!("textured-geometry dedup ({GEOMETRY_CALLS} calls, {GEOMETRY_KEYS} unique identities)");
    print_result("old: get then insert", &old_geometry_result, GEOMETRY_CALLS);
    print_result(
        "new: single entry probe",
        &new_geometry_result,
        GEOMETRY_CALLS,
    );
    print_change("old -> entry", &old_geometry_result, &new_geometry_result);

    let (mut old_sampler, mut new_sampler) = sampler_benches();
    assert_eq!(old_sampler.lookups(), new_sampler.lookups());
    let mut old_sampler_op = || old_sampler.lookups();
    let mut new_sampler_op = || new_sampler.lookups();
    let [old_sampler_result, new_sampler_result] =
        measure_group([&mut old_sampler_op, &mut new_sampler_op]);
    assert_eq!(old_sampler_result.checksum, new_sampler_result.checksum);
    assert_zero_alloc(&old_sampler_result);
    assert_zero_alloc(&new_sampler_result);
    let old_cold = cold_sampler_alloc(false);
    let new_cold = cold_sampler_alloc(true);

    println!("\nsampler lookup ({DRAWS} warmed lookups over all eight descriptions)");
    print_result("old: randomized hash map", &old_sampler_result, DRAWS);
    print_result("new: fixed slot table", &new_sampler_result, DRAWS);
    print_change("old -> fixed", &old_sampler_result, &new_sampler_result);
    println!(
        "  cold table storage          {} alloc, {} free, {} churn B -> {} alloc, {} free, {} churn B",
        old_cold.allocs,
        old_cold.frees,
        old_cold.churn_bytes(),
        new_cold.allocs,
        new_cold.frees,
        new_cold.churn_bytes(),
    );

    let (mut old_upload, mut new_upload) = upload_benches();
    for _ in 0..FRAMES_IN_FLIGHT * 2 {
        assert_eq!(old_upload.cycle(), new_upload.cycle());
    }
    let mut old_upload_op = || old_upload.cycle();
    let mut new_upload_op = || new_upload.cycle();
    let [old_upload_result, new_upload_result] =
        measure_group([&mut old_upload_op, &mut new_upload_op]);
    assert_eq!(old_upload_result.checksum, new_upload_result.checksum);
    assert_zero_alloc(&new_upload_result);

    println!(
        "\nVulkan upload storage cycle ({STAGING_PER_BATCH} staging, {RETIRED_PER_BATCH} retired, {FRAMES_IN_FLIGHT} frames in flight)"
    );
    print_result(
        "old: take fresh vectors",
        &old_upload_result,
        STAGING_PER_BATCH,
    );
    print_result(
        "new: recycle completed",
        &new_upload_result,
        STAGING_PER_BATCH,
    );
    print_change("old -> recycled", &old_upload_result, &new_upload_result);
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
