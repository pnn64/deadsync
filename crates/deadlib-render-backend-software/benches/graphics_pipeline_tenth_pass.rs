use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 40;
const SAMPLES: usize = 60;
const OPS_PER_SAMPLE: usize = 40;
const ALLOC_OPS: usize = 2_000;
const TRIANGLES: usize = 2_048;
const PIXELS: usize = 16_384;

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

// SAFETY: allocator operations delegate unchanged to `System`; atomics only
// observe successful activity while the benchmark gate is enabled.
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

#[derive(Clone, Copy)]
struct ClipVertex {
    clip: [f32; 4],
    uv: [f32; 2],
    color: [f32; 4],
}

fn clip_triangles() -> [[ClipVertex; 3]; TRIANGLES] {
    std::array::from_fn(|index| {
        let x = (index % 64) as f32 * 0.025 - 0.8;
        let y = (index / 64) as f32 * 0.04 - 0.6;
        let z = if index % 31 == 0 { -1.2 } else { -0.2 };
        std::array::from_fn(|corner| ClipVertex {
            clip: [
                x + [0.0, 0.02, 0.01][corner],
                y + [0.0, 0.0, 0.03][corner],
                if corner == 2 && index % 31 == 0 {
                    -1.2
                } else {
                    z
                },
                1.0,
            ],
            uv: [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]][corner],
            color: [0.7, 0.8, 0.9, 0.65],
        })
    })
}

#[inline(always)]
fn clip_distance(vertex: ClipVertex) -> f32 {
    vertex.clip[2] + vertex.clip[3]
}

#[inline(always)]
fn interpolate(a: ClipVertex, b: ClipVertex, t: f32) -> ClipVertex {
    ClipVertex {
        clip: std::array::from_fn(|i| (b.clip[i] - a.clip[i]).mul_add(t, a.clip[i])),
        uv: std::array::from_fn(|i| (b.uv[i] - a.uv[i]).mul_add(t, a.uv[i])),
        color: std::array::from_fn(|i| (b.color[i] - a.color[i]).mul_add(t, a.color[i])),
    }
}

fn clip_old(triangle: [ClipVertex; 3]) -> ([ClipVertex; 4], usize) {
    let distances = triangle.map(clip_distance);
    if distances.iter().all(|distance| *distance >= 0.0) {
        return ([triangle[0], triangle[1], triangle[2], triangle[0]], 3);
    }
    let mut out = [triangle[0]; 4];
    let mut len = 0;
    let mut previous = triangle[2];
    let mut previous_distance = distances[2];
    let mut previous_inside = previous_distance >= 0.0;
    for (current, current_distance) in triangle.into_iter().zip(distances) {
        let current_inside = current_distance >= 0.0;
        if current_inside != previous_inside {
            out[len] = interpolate(
                previous,
                current,
                previous_distance / (previous_distance - current_distance),
            );
            len += 1;
        }
        if current_inside {
            out[len] = current;
            len += 1;
        }
        previous = current;
        previous_distance = current_distance;
        previous_inside = current_inside;
    }
    (out, len)
}

#[inline(always)]
fn fold_projected(mut sum: u64, vertices: &[ClipVertex]) -> u64 {
    for vertex in vertices {
        let inv_w = 1.0 / vertex.clip[3];
        let x = vertex.clip[0] * inv_w;
        let y = vertex.clip[1] * inv_w;
        sum = sum.rotate_left(7)
            ^ u64::from(x.to_bits())
            ^ (u64::from(y.to_bits()) << 11)
            ^ u64::from(vertex.uv[0].to_bits())
            ^ u64::from(vertex.color[3].to_bits());
    }
    sum
}

fn clipping_old(triangles: &[[ClipVertex; 3]; TRIANGLES]) -> u64 {
    triangles.iter().fold(0, |sum, triangle| {
        let (clipped, len) = clip_old(*triangle);
        fold_projected(sum, &clipped[..len])
    })
}

fn clipping_new(triangles: &[[ClipVertex; 3]; TRIANGLES]) -> u64 {
    triangles.iter().fold(0, |sum, triangle| {
        if triangle.iter().all(|vertex| clip_distance(*vertex) >= 0.0) {
            fold_projected(sum, triangle)
        } else {
            let (clipped, len) = clip_old(*triangle);
            fold_projected(sum, &clipped[..len])
        }
    })
}

fn texture_data(alpha: u8) -> Vec<u8> {
    let mut out = vec![0; PIXELS * 4];
    for index in 0..PIXELS {
        out[index * 4] = (index * 37) as u8;
        out[index * 4 + 1] = (index * 71) as u8;
        out[index * 4 + 2] = (index * 109) as u8;
        out[index * 4 + 3] = if alpha == 255 {
            255
        } else {
            alpha.wrapping_add(index as u8)
        };
    }
    out
}

#[inline(always)]
fn alpha_linear(data: &[u8], base: usize) -> f32 {
    const SCALE: f32 = 1.0 / 255.0;
    let indices = [base, base + 4, base + 512, base + 516];
    let alpha = indices.map(|index| f32::from(data[index + 3]) * SCALE);
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    lerp(
        lerp(alpha[0], alpha[1], 0.37),
        lerp(alpha[2], alpha[3], 0.37),
        0.61,
    )
}

#[inline(always)]
fn shade_mask_old(
    data: &[u8],
    base: usize,
    texture_mask: bool,
    opaque: bool,
    tint: [f32; 4],
) -> [f32; 4] {
    let sampled = if texture_mask && opaque {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, alpha_linear(data, base)]
    };
    [
        if texture_mask {
            tint[0]
        } else {
            sampled[0] * tint[0]
        },
        if texture_mask {
            tint[1]
        } else {
            sampled[1] * tint[1]
        },
        if texture_mask {
            tint[2]
        } else {
            sampled[2] * tint[2]
        },
        sampled[3] * tint[3],
    ]
}

#[inline(always)]
fn shade_mask_opaque(tint: [f32; 4]) -> [f32; 4] {
    tint
}

#[inline(always)]
fn shade_mask_alpha(data: &[u8], base: usize, tint: [f32; 4]) -> [f32; 4] {
    [
        tint[0],
        tint[1],
        tint[2],
        alpha_linear(data, base) * tint[3],
    ]
}

fn mask_work(opaque: &[u8], alpha: &[u8], specialized: bool) -> u64 {
    let tint = [0.63, 0.72, 0.81, 0.68];
    let mut sum = 0u64;
    for sample in 0..PIXELS {
        let base = (sample * 127 % (PIXELS - 130)) * 4;
        let first = if specialized {
            shade_mask_opaque(tint)
        } else {
            shade_mask_old(opaque, base, black_box(true), black_box(true), tint)
        };
        let second = if specialized {
            shade_mask_alpha(alpha, base, tint)
        } else {
            shade_mask_old(alpha, base, black_box(true), black_box(false), tint)
        };
        for channel in 0..4 {
            sum = sum.rotate_left(5)
                ^ u64::from(first[channel].to_bits())
                ^ u64::from(second[channel].to_bits());
        }
    }
    sum
}

#[inline(always)]
fn rgba_linear_old(data: &[u8], base: usize, opaque: bool) -> [f32; 4] {
    const SCALE: f32 = 1.0 / 255.0;
    let indices = [base, base + 4, base + 512, base + 516];
    let colors = indices.map(|index| {
        [
            f32::from(data[index]) * SCALE,
            f32::from(data[index + 1]) * SCALE,
            f32::from(data[index + 2]) * SCALE,
            if opaque {
                1.0
            } else {
                f32::from(data[index + 3]) * SCALE
            },
        ]
    });
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    std::array::from_fn(|channel| {
        lerp(
            lerp(colors[0][channel], colors[1][channel], 0.37),
            lerp(colors[2][channel], colors[3][channel], 0.37),
            0.61,
        )
    })
}

#[inline(always)]
fn rgba_linear_opaque(data: &[u8], base: usize) -> [f32; 4] {
    const SCALE: f32 = 1.0 / 255.0;
    let indices = [base, base + 4, base + 512, base + 516];
    let colors = indices.map(|index| {
        [
            f32::from(data[index]) * SCALE,
            f32::from(data[index + 1]) * SCALE,
            f32::from(data[index + 2]) * SCALE,
        ]
    });
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    let rgb = std::array::from_fn::<_, 3, _>(|channel| {
        lerp(
            lerp(colors[0][channel], colors[1][channel], 0.37),
            lerp(colors[2][channel], colors[3][channel], 0.37),
            0.61,
        )
    });
    [rgb[0], rgb[1], rgb[2], 1.0]
}

fn opaque_work(data: &[u8], specialized: bool) -> u64 {
    let mut sum = 0u64;
    for sample in 0..PIXELS {
        let base = (sample * 127 % (PIXELS - 130)) * 4;
        let color = if specialized {
            rgba_linear_opaque(data, base)
        } else {
            rgba_linear_old(data, base, black_box(true))
        };
        for channel in color {
            sum = sum.rotate_left(5) ^ u64::from(channel.to_bits());
        }
    }
    sum
}

#[inline(always)]
fn wrap_old(index: i32, max: usize) -> usize {
    if max.is_power_of_two() {
        index as usize & (max - 1)
    } else {
        index.rem_euclid(max as i32) as usize
    }
}

#[inline(always)]
fn wrap_new(index: i32, max: usize, mask: usize) -> usize {
    if mask != 0 {
        index as usize & mask
    } else {
        index.rem_euclid(max as i32) as usize
    }
}

fn wrap_work(precomputed: bool) -> u64 {
    let dimensions = [64usize, 128, 256, 96];
    let masks = [63usize, 127, 255, 0];
    let mut sum = 0u64;
    for sample in 0..PIXELS {
        let max = dimensions[sample & 3];
        let mask = masks[sample & 3];
        let index = (sample as i32 * 313).wrapping_sub(2_000_000);
        for offset in [-1, 0, 1, 2] {
            let wrapped = if precomputed {
                wrap_new(index + offset, max, mask)
            } else {
                wrap_old(index + offset, max)
            };
            sum = sum.rotate_left(5) ^ wrapped as u64;
        }
    }
    sum
}

fn print_pair(name: &str, units: &str, work_per_op: f64, pair: &[Measurement; 2]) {
    assert_eq!(
        pair[0].checksum, pair[1].checksum,
        "{name} behavior changed"
    );
    println!("\n{name} ({units}/operation)");
    for (label, measurement) in ["old", "new"].into_iter().zip(pair) {
        let cycles = measurement
            .cycles_per_op
            .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}"));
        println!(
            "  {label}: {:.2} ns, {cycles} cycles, {:.2} M{units}/s, p95 {:.2} ns, alloc/realloc/free {}/{}/{}, churn {} B",
            measurement.ns_per_op,
            work_per_op * 1_000.0 / measurement.ns_per_op,
            measurement.p95_ns,
            measurement.allocated.allocs,
            measurement.allocated.reallocs,
            measurement.allocated.frees,
            measurement.allocated.churn_bytes(),
        );
    }
    println!(
        "  delta: {:.1}% cycles, {:+.1}% throughput, {:.1}% p95",
        (pair[1].cycles_per_op.unwrap_or(0.0) / pair[0].cycles_per_op.unwrap_or(1.0) - 1.0) * 100.0,
        (pair[0].ns_per_op / pair[1].ns_per_op - 1.0) * 100.0,
        (pair[1].p95_ns / pair[0].p95_ns - 1.0) * 100.0,
    );
}

fn main() {
    let triangles = clip_triangles();
    let opaque = texture_data(255);
    let alpha = texture_data(37);
    print_pair(
        "visible near-clip fast path",
        "triangle",
        TRIANGLES as f64,
        &measure_pair(|| clipping_old(&triangles), || clipping_new(&triangles)),
    );
    print_pair(
        "texture-mask raster specialization",
        "pixel",
        (PIXELS * 2) as f64,
        &measure_pair(
            || mask_work(&opaque, &alpha, false),
            || mask_work(&opaque, &alpha, true),
        ),
    );
    print_pair(
        "opaque RGBA specialization",
        "sample",
        PIXELS as f64,
        &measure_pair(
            || opaque_work(&opaque, false),
            || opaque_work(&opaque, true),
        ),
    );
    print_pair(
        "precomputed repeat masks",
        "index",
        (PIXELS * 4) as f64,
        &measure_pair(|| wrap_work(false), || wrap_work(true)),
    );
}

#[inline(always)]
fn cycle_counter() -> Option<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: `_rdtsc` reads the timestamp counter and has no memory effects.
        Some(unsafe { core::arch::x86_64::_rdtsc() })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        None
    }
}
