use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 500;
const SAMPLES: usize = 80;
const OPS_PER_SAMPLE: usize = 100;
const ALLOC_OPS: usize = 2_000;
const OBJECTS: usize = 384;
const STRIPES: usize = 34;
const ROWS: usize = STRIPES * 32;
const BLENDS_PER_BATCH: usize = 2_048;

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

// SAFETY: every operation delegates unchanged to `System`; counters only
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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
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

fn measure_pair(mut old: impl FnMut() -> u64, mut new: impl FnMut() -> u64) -> [Measurement; 2] {
    let mut ops: [&mut dyn FnMut() -> u64; 2] = [&mut old, &mut new];
    for round in 0..WARMUP {
        black_box(ops[round % 2]());
        black_box(ops[(round + 1) % 2]());
    }

    let mut elapsed = [Duration::ZERO; 2];
    let mut cycles = [Some(0u64); 2];
    let mut checksums = [0u64; 2];
    let mut samples: [Vec<Duration>; 2] = std::array::from_fn(|_| Vec::with_capacity(SAMPLES));
    for sample in 0..SAMPLES {
        for offset in 0..2 {
            let index = (sample + offset) % 2;
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

    let allocated: [AllocSnapshot; 2] = std::array::from_fn(|index| {
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
            cycles_per_op: cycles[index].map(|value| value as f64 / operations),
            p95_ns: samples[index][SAMPLES * 95 / 100].as_secs_f64() * 1_000_000_000.0
                / OPS_PER_SAMPLE as f64,
            allocated: allocated[index],
            checksum: checksums[index],
        }
    })
}

#[derive(Clone, Copy)]
struct ObjectBounds {
    x: [f32; 4],
    y: [f32; 4],
    id: u32,
}

fn objects() -> [ObjectBounds; OBJECTS] {
    std::array::from_fn(|index| {
        let center = ((index * 83) % ROWS) as f32;
        let half = (20 + index % 29) as f32;
        let skew = (index % 7) as f32 * 0.125;
        let x = [
            100.0 - half,
            100.0 + half + skew,
            100.0 + half,
            100.0 - half - skew,
        ];
        let y = [
            center - half,
            center - half + skew,
            center + half,
            center + half - skew,
        ];
        ObjectBounds {
            x,
            y,
            id: index as u32 + 1,
        }
    })
}

#[inline(always)]
fn triangle_inv(x0: f32, y0: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    1.0 / ((x2 - x0).mul_add(y1 - y0, -((y2 - y0) * (x1 - x0))))
}

fn calc_rows(y: [f32; 4]) -> (u16, u16) {
    let min_y = y.into_iter().fold(f32::INFINITY, f32::min).floor().max(0.0) as usize;
    let max_y = y
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((ROWS - 1) as f32) as usize;
    if min_y > max_y {
        (0, 0)
    } else {
        (min_y as u16, (max_y + 1) as u16)
    }
}

fn scan_rows_old(objects: &[ObjectBounds; OBJECTS]) -> u64 {
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let stripe_start = stripe * 32;
        let stripe_end = stripe_start + 32;
        for object in objects {
            let min_y = object
                .y
                .into_iter()
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as usize;
            let max_y = object
                .y
                .into_iter()
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .min((ROWS - 1) as f32) as usize;
            if min_y <= max_y && max_y >= stripe_start && min_y < stripe_end {
                checksum = checksum.rotate_left(5) ^ u64::from(object.id);
            }
        }
    }
    checksum
}

fn scan_rows_new(objects: &[ObjectBounds; OBJECTS]) -> u64 {
    let rows: [(u16, u16); OBJECTS] = std::array::from_fn(|index| calc_rows(objects[index].y));
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let stripe_start = (stripe * 32) as u16;
        let stripe_end = stripe_start + 32;
        for (object, &(row_start, row_end)) in objects.iter().zip(&rows) {
            if row_start < stripe_end && row_end > stripe_start {
                checksum = checksum.rotate_left(5) ^ u64::from(object.id);
            }
        }
    }
    checksum
}

fn sprite_setup_old(objects: &[ObjectBounds; OBJECTS]) -> u64 {
    let rows: [(u16, u16); OBJECTS] = std::array::from_fn(|index| calc_rows(objects[index].y));
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let stripe_start = (stripe * 32) as u16;
        let stripe_end = stripe_start + 32;
        for (object, &(row_start, row_end)) in objects.iter().zip(&rows) {
            if row_start >= stripe_end || row_end <= stripe_start {
                continue;
            }
            let first = triangle_inv(
                object.x[0],
                object.y[0],
                object.x[1],
                object.y[1],
                object.x[2],
                object.y[2],
            );
            let second = triangle_inv(
                object.x[0],
                object.y[0],
                object.x[2],
                object.y[2],
                object.x[3],
                object.y[3],
            );
            checksum = checksum.rotate_left(5)
                ^ u64::from(first.to_bits())
                ^ u64::from(second.to_bits()).rotate_left(17);
        }
    }
    checksum
}

fn sprite_setup_new(objects: &[ObjectBounds; OBJECTS]) -> u64 {
    let rows: [(u16, u16); OBJECTS] = std::array::from_fn(|index| calc_rows(objects[index].y));
    let inv_denom: [[f32; 2]; OBJECTS] = std::array::from_fn(|index| {
        let object = objects[index];
        [
            triangle_inv(
                object.x[0],
                object.y[0],
                object.x[1],
                object.y[1],
                object.x[2],
                object.y[2],
            ),
            triangle_inv(
                object.x[0],
                object.y[0],
                object.x[2],
                object.y[2],
                object.x[3],
                object.y[3],
            ),
        ]
    });
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let stripe_start = (stripe * 32) as u16;
        let stripe_end = stripe_start + 32;
        for (index, (_, &(row_start, row_end))) in objects.iter().zip(&rows).enumerate() {
            if row_start >= stripe_end || row_end <= stripe_start {
                continue;
            }
            checksum = checksum.rotate_left(5)
                ^ u64::from(inv_denom[index][0].to_bits())
                ^ u64::from(inv_denom[index][1].to_bits()).rotate_left(17);
        }
    }
    checksum
}

#[inline(always)]
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[inline(always)]
fn pack(color: [f32; 4]) -> u32 {
    let [r, g, b, a] = color.map(|value| clamp01(value).mul_add(255.0, 0.5) as u32);
    (a << 24) | (r << 16) | (g << 8) | b
}

#[inline(always)]
fn blend_old(dst: u32, src: [f32; 4]) -> u32 {
    const SCALE: f32 = 1.0 / 255.0;
    let dr = ((dst >> 16) & 0xff) as f32 * SCALE;
    let dg = ((dst >> 8) & 0xff) as f32 * SCALE;
    let db = (dst & 0xff) as f32 * SCALE;
    let da = ((dst >> 24) & 0xff) as f32 * SCALE;
    let inv = 1.0 - src[3];
    pack([
        src[0].mul_add(src[3], dr * inv),
        src[1].mul_add(src[3], dg * inv),
        src[2].mul_add(src[3], db * inv),
        src[3] + da * inv,
    ])
}

#[inline(always)]
fn blend_new(dst: u32, src: [f32; 4]) -> u32 {
    if src[3] >= 1.0 {
        pack([src[0], src[1], src[2], 1.0])
    } else {
        blend_old(dst, src)
    }
}

fn blend_inputs() -> [(u32, [f32; 4]); BLENDS_PER_BATCH] {
    std::array::from_fn(|index| {
        let dst = (index as u32).wrapping_mul(0x9e37_79b9) | 0xff00_0000;
        let src = [
            ((index * 17) & 0xff) as f32 / 255.0,
            ((index * 43 + 7) & 0xff) as f32 / 255.0,
            ((index * 97 + 11) & 0xff) as f32 / 255.0,
            1.0,
        ];
        (dst, src)
    })
}

fn blend_batch(inputs: &[(u32, [f32; 4]); BLENDS_PER_BATCH], fast: bool) -> u64 {
    inputs.iter().fold(0u64, |checksum, &(dst, src)| {
        let out = if fast {
            blend_new(dst, src)
        } else {
            blend_old(dst, src)
        };
        checksum.rotate_left(7) ^ u64::from(out)
    })
}

fn main() {
    let objects = objects();
    assert_eq!(scan_rows_old(&objects), scan_rows_new(&objects));
    let [old_rows, new_rows] = measure_pair(|| scan_rows_old(&objects), || scan_rows_new(&objects));
    assert_eq!(old_rows.checksum, new_rows.checksum);
    assert_zero_alloc(&old_rows);
    assert_zero_alloc(&new_rows);
    println!("software row visibility ({OBJECTS} objects x {STRIPES} stripes)");
    print_result("old: recompute float bounds", &old_rows, OBJECTS * STRIPES);
    print_result("new: retained row interval", &new_rows, OBJECTS * STRIPES);
    print_change(&old_rows, &new_rows);

    assert_eq!(sprite_setup_old(&objects), sprite_setup_new(&objects));
    let [old_setup, new_setup] =
        measure_pair(|| sprite_setup_old(&objects), || sprite_setup_new(&objects));
    assert_eq!(old_setup.checksum, new_setup.checksum);
    assert_zero_alloc(&old_setup);
    assert_zero_alloc(&new_setup);
    println!("\nsoftware sprite triangle setup ({OBJECTS} objects across intersecting stripes)");
    print_result("old: divide in every stripe", &old_setup, OBJECTS);
    print_result("new: retained reciprocals", &new_setup, OBJECTS);
    print_change(&old_setup, &new_setup);

    let blends = blend_inputs();
    assert_eq!(blend_batch(&blends, false), blend_batch(&blends, true));
    let [old_blend, new_blend] = measure_pair(
        || blend_batch(&blends, false),
        || blend_batch(&blends, true),
    );
    assert_eq!(old_blend.checksum, new_blend.checksum);
    assert_zero_alloc(&old_blend);
    assert_zero_alloc(&new_blend);
    println!("\nsoftware opaque source-over ({BLENDS_PER_BATCH} pixels)");
    print_result("old: unpack + blend dst", &old_blend, BLENDS_PER_BATCH);
    print_result("new: exact opaque store", &new_blend, BLENDS_PER_BATCH);
    print_change(&old_blend, &new_blend);
}

fn print_result(label: &str, result: &Measurement, items: usize) {
    println!(
        "  {label:<29} {:>9.2} ns/op {:>9.2} cycles/op {:>9.2} ns p95 \
         {:>8.2} Mitem/s {:>5.3} alloc {:>5.3} realloc {:>5.3} free {:>9.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        items as f64 * 1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ALLOC_OPS as f64,
        result.allocated.reallocs as f64 / ALLOC_OPS as f64,
        result.allocated.frees as f64 / ALLOC_OPS as f64,
        result.allocated.churn_bytes() as f64 / ALLOC_OPS as f64,
    );
}

fn print_change(old: &Measurement, new: &Measurement) {
    println!(
        "  old -> new                    {:>8.2}% latency {:>8.2}% cycles {:>8.2}% p95 {:>8.2}% churn",
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
