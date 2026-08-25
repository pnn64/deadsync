use rayon::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 40;
const SAMPLES: usize = 60;
const OPS_PER_SAMPLE: usize = 20;
const ALLOC_OPS: usize = 1_000;
const WIDTH: usize = 1_920;
const STRIPES: usize = 34;
const HEIGHT: usize = STRIPES * 32;
const PIXELS: usize = WIDTH * HEIGHT;
const TRIANGLES: usize = 2_048;
const WRAPS: usize = 16_384;
const COLORS: usize = 16_384;

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

// SAFETY: calls delegate unchanged to `System`; the atomics only observe
// successful allocator activity while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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

fn draw_overlay(stripe: &mut [u32], stripe_index: usize) {
    for (row_index, row) in stripe.as_chunks_mut::<WIDTH>().0.iter_mut().enumerate() {
        let x = (stripe_index * 97 + row_index * 43) % WIDTH;
        row[x] = 0xff00_0000 | ((stripe_index as u32) << 8) | row_index as u32;
    }
}

fn framebuffer_checksum(buffer: &[u32]) -> u64 {
    [0, WIDTH - 1, PIXELS / 2, PIXELS - WIDTH, PIXELS - 1]
        .into_iter()
        .fold(0u64, |sum, index| {
            sum.rotate_left(7) ^ u64::from(buffer[index])
        })
}

fn clear_old(buffer: &mut [u32], pool: &rayon::ThreadPool) -> u64 {
    buffer.fill(0xff12_3456);
    pool.install(|| {
        buffer
            .par_chunks_mut(WIDTH * 32)
            .enumerate()
            .for_each(|(stripe_index, stripe)| draw_overlay(stripe, stripe_index));
    });
    framebuffer_checksum(buffer)
}

fn clear_new(buffer: &mut [u32], pool: &rayon::ThreadPool) -> u64 {
    pool.install(|| {
        buffer
            .par_chunks_mut(WIDTH * 32)
            .enumerate()
            .for_each(|(stripe_index, stripe)| {
                stripe.fill(0xff12_3456);
                draw_overlay(stripe, stripe_index);
            });
    });
    framebuffer_checksum(buffer)
}

#[derive(Clone, Copy)]
struct Triangle {
    row_start: u16,
    row_end: u16,
    id: u32,
}

fn triangles() -> [Triangle; TRIANGLES] {
    std::array::from_fn(|index| {
        let center = (index * 101) % HEIGHT;
        let half = 4 + index % 17;
        Triangle {
            row_start: center.saturating_sub(half) as u16,
            row_end: (center + half + 1).min(HEIGHT) as u16,
            id: index as u32 + 1,
        }
    })
}

fn triangles_old(triangles: &[Triangle; TRIANGLES]) -> u64 {
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = (stripe * 32) as u16;
        let end = start + 32;
        for triangle in triangles {
            if triangle.row_start < end && triangle.row_end > start {
                checksum = checksum.rotate_left(5) ^ u64::from(triangle.id);
            }
        }
    }
    checksum
}

#[derive(Default)]
struct TriangleBins {
    offsets: Vec<u32>,
    cursors: Vec<u32>,
    indices: Vec<u32>,
}

impl TriangleBins {
    fn build(&mut self, triangles: &[Triangle; TRIANGLES]) {
        self.offsets.clear();
        self.offsets.resize(STRIPES + 1, 0);
        for triangle in triangles {
            let first = usize::from(triangle.row_start) / 32;
            let end = usize::from(triangle.row_end).div_ceil(32).min(STRIPES);
            for stripe in first.min(STRIPES)..end {
                self.offsets[stripe + 1] += 1;
            }
        }
        for stripe in 0..STRIPES {
            self.offsets[stripe + 1] += self.offsets[stripe];
        }
        self.indices.clear();
        self.indices.resize(self.offsets[STRIPES] as usize, 0);
        self.cursors.clear();
        self.cursors.extend_from_slice(&self.offsets[..STRIPES]);
        for (index, triangle) in triangles.iter().enumerate() {
            let first = usize::from(triangle.row_start) / 32;
            let end = usize::from(triangle.row_end).div_ceil(32).min(STRIPES);
            for stripe in first.min(STRIPES)..end {
                let slot = self.cursors[stripe] as usize;
                self.indices[slot] = index as u32;
                self.cursors[stripe] += 1;
            }
        }
    }
}

fn triangles_new(triangles: &[Triangle; TRIANGLES], bins: &mut TriangleBins) -> u64 {
    bins.build(triangles);
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = bins.offsets[stripe] as usize;
        let end = bins.offsets[stripe + 1] as usize;
        for &index in &bins.indices[start..end] {
            checksum = checksum.rotate_left(5) ^ u64::from(triangles[index as usize].id);
        }
    }
    checksum
}

#[inline(always)]
fn wrap_old(i: i32, max: usize) -> usize {
    let max = max as i32;
    let mut value = i % max;
    if value < 0 {
        value += max;
    }
    value as usize
}

#[inline(always)]
fn wrap_new(i: i32, max: usize) -> usize {
    if max.is_power_of_two() {
        i as usize & (max - 1)
    } else {
        wrap_old(i, max)
    }
}

fn wrap_inputs() -> Vec<(i32, usize)> {
    const SIZES: [usize; 5] = [256, 512, 255, 1_024, 300];
    (0..WRAPS)
        .map(|index| {
            let max = SIZES[index % SIZES.len()];
            let value = (index as i32).wrapping_mul(1_664_525) % 4_097 - 2_048;
            (value, max)
        })
        .collect()
}

fn wrap_batch(inputs: &[(i32, usize)], fast: bool) -> u64 {
    inputs.iter().fold(0u64, |sum, &(value, max)| {
        let wrapped = if fast {
            wrap_new(black_box(value), black_box(max))
        } else {
            wrap_old(black_box(value), black_box(max))
        };
        sum.rotate_left(7) ^ wrapped as u64
    })
}

#[inline(always)]
fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[inline(always)]
fn pack_old(rgb: [f32; 3]) -> u32 {
    let r = clamp01(rgb[0]).mul_add(255.0, 0.5) as u32;
    let g = clamp01(rgb[1]).mul_add(255.0, 0.5) as u32;
    let b = clamp01(rgb[2]).mul_add(255.0, 0.5) as u32;
    let a = clamp01(1.0).mul_add(255.0, 0.5) as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

#[inline(always)]
fn pack_new(rgb: [f32; 3]) -> u32 {
    let r = rgb[0].mul_add(255.0, 0.5) as u32;
    let g = rgb[1].mul_add(255.0, 0.5) as u32;
    let b = rgb[2].mul_add(255.0, 0.5) as u32;
    0xff00_0000 | (r << 16) | (g << 8) | b
}

fn colors() -> Vec<[f32; 3]> {
    (0..COLORS)
        .map(|index| {
            [
                ((index * 17) & 0xff) as f32 / 255.0,
                ((index * 43 + 7) & 0xff) as f32 / 255.0,
                ((index * 97 + 11) & 0xff) as f32 / 255.0,
            ]
        })
        .collect()
}

fn pack_batch(colors: &[[f32; 3]], fast: bool) -> u64 {
    colors.iter().fold(0u64, |sum, &rgb| {
        let packed = if fast { pack_new(rgb) } else { pack_old(rgb) };
        sum.rotate_left(7) ^ u64::from(packed)
    })
}

fn main() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .expect("benchmark worker pool builds");
    let mut old_buffer = vec![0; PIXELS];
    let mut new_buffer = vec![0; PIXELS];
    assert_eq!(
        clear_old(&mut old_buffer, &pool),
        clear_new(&mut new_buffer, &pool)
    );
    assert_eq!(old_buffer, new_buffer);
    let [old_clear, new_clear] = measure_pair(
        || clear_old(&mut old_buffer, &pool),
        || clear_new(&mut new_buffer, &pool),
    );
    println!("parallel framebuffer clear ({WIDTH}x{HEIGHT}, 4 workers)");
    print_result("old: serial clear then draw", &old_clear, PIXELS);
    print_result("new: clear inside workers", &new_clear, PIXELS);
    print_change(&old_clear, &new_clear);
    assert_eq!(old_clear.checksum, new_clear.checksum);

    let triangles = triangles();
    let mut bins = TriangleBins::default();
    assert_eq!(
        triangles_old(&triangles),
        triangles_new(&triangles, &mut bins)
    );
    let [old_triangles, new_triangles] = measure_pair(
        || triangles_old(&triangles),
        || triangles_new(&triangles, &mut bins),
    );
    assert_pair(&old_triangles, &new_triangles);
    println!("\ntriangle stripe dispatch ({TRIANGLES} triangles x {STRIPES} stripes)");
    print_result(
        "old: scan all triangles",
        &old_triangles,
        TRIANGLES * STRIPES,
    );
    print_result(
        "new: retained triangle index",
        &new_triangles,
        TRIANGLES * STRIPES,
    );
    print_change(&old_triangles, &new_triangles);

    let wraps = wrap_inputs();
    assert_eq!(wrap_batch(&wraps, false), wrap_batch(&wraps, true));
    let [old_wrap, new_wrap] =
        measure_pair(|| wrap_batch(&wraps, false), || wrap_batch(&wraps, true));
    assert_pair(&old_wrap, &new_wrap);
    println!("\nrepeat texture wrapping ({WRAPS} mixed power-of-two/non-power-of-two indices)");
    print_result("old: signed remainder", &old_wrap, WRAPS);
    print_result("new: power-of-two mask", &new_wrap, WRAPS);
    print_change(&old_wrap, &new_wrap);

    let colors = colors();
    assert_eq!(pack_batch(&colors, false), pack_batch(&colors, true));
    let [old_pack, new_pack] =
        measure_pair(|| pack_batch(&colors, false), || pack_batch(&colors, true));
    assert_pair(&old_pack, &new_pack);
    println!("\nopaque pixel packing ({COLORS} already-clamped RGB values)");
    print_result("old: reclamp RGBA", &old_pack, COLORS);
    print_result("new: direct opaque pack", &new_pack, COLORS);
    print_change(&old_pack, &new_pack);
}

fn assert_pair(old: &Measurement, new: &Measurement) {
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(old);
    assert_zero_alloc(new);
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
    assert_eq!(result.allocated.allocs, 0, "unexpected allocations");
    assert_eq!(result.allocated.reallocs, 0, "unexpected reallocations");
    assert_eq!(result.allocated.frees, 0, "unexpected frees");
    assert_eq!(result.allocated.churn_bytes(), 0, "unexpected byte churn");
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
