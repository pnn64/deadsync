use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 40;
const SAMPLES: usize = 60;
const OPS_PER_SAMPLE: usize = 50;
const ALLOC_OPS: usize = 2_000;
const SAMPLES_PER_OP: usize = 16_384;
const TEXTURE_BYTES: usize = 256 * 256 * 4;
const SPRITES: usize = 768;
const STRIPES: usize = 34;
const STRIPE_ROWS: i32 = 32;

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

// SAFETY: calls delegate unchanged to `System`; atomics only observe allocator
// activity while the benchmark gate is enabled.
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
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Sprite {
    vertices: [Point; 4],
}

#[derive(Clone, Copy)]
struct Setup {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    inv_denom: f32,
}

fn sprite_data() -> [Sprite; SPRITES] {
    std::array::from_fn(|index| {
        let center_x = ((index * 137) % 1_800 + 60) as f32;
        let center_y = ((index * 89) % 1_000 + 40) as f32;
        let half_x = (12 + index % 80) as f32;
        let half_y = (10 + index % 120) as f32;
        let angle = index as f32 * 0.071;
        let (sin, cos) = angle.sin_cos();
        let corner = |x: f32, y: f32| Point {
            x: center_x + cos.mul_add(x, -(sin * y)),
            y: center_y + sin.mul_add(x, cos * y),
        };
        Sprite {
            vertices: [
                corner(-half_x, -half_y),
                corner(half_x, -half_y),
                corner(half_x, half_y),
                corner(-half_x, half_y),
            ],
        }
    })
}

#[inline(always)]
fn setup(a: Point, b: Point, c: Point) -> Option<Setup> {
    let denom = (c.x - a.x).mul_add(b.y - a.y, -((c.y - a.y) * (b.x - a.x)));
    (denom != 0.0).then(|| Setup {
        min_x: a.x.min(b.x).min(c.x).floor().max(0.0) as i32,
        max_x: a.x.max(b.x).max(c.x).ceil().min(1_919.0) as i32,
        min_y: a.y.min(b.y).min(c.y).floor().max(0.0) as i32,
        max_y: a.y.max(b.y).max(c.y).ceil().min(1_087.0) as i32,
        inv_denom: 1.0 / denom,
    })
}

#[inline(always)]
fn fold_setup(sum: u64, setup: Setup, stripe: i32) -> u64 {
    let start = stripe * STRIPE_ROWS;
    let end = start + STRIPE_ROWS - 1;
    if setup.max_y < start || setup.min_y > end {
        return sum;
    }
    sum.wrapping_add(
        setup.min_x as u64
            ^ ((setup.max_x as u64) << 12)
            ^ u64::from(setup.inv_denom.to_bits())
            ^ ((stripe as u64) << 44),
    )
}

fn sprites_old(sprites: &[Sprite; SPRITES]) -> u64 {
    let mut checksum = 0;
    for sprite in sprites {
        let min_y = sprite
            .vertices
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as i32;
        let max_y = sprite
            .vertices
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(1_087.0) as i32;
        for stripe in (min_y / STRIPE_ROWS).max(0)..=(max_y / STRIPE_ROWS).min(STRIPES as i32 - 1) {
            if let Some(first) = setup(sprite.vertices[0], sprite.vertices[1], sprite.vertices[2]) {
                checksum = fold_setup(checksum, first, stripe);
            }
            if let Some(second) = setup(sprite.vertices[0], sprite.vertices[2], sprite.vertices[3])
            {
                checksum = fold_setup(checksum, second, stripe);
            }
        }
    }
    checksum
}

fn sprites_new(sprites: &[Sprite; SPRITES]) -> u64 {
    let mut checksum = 0;
    for sprite in sprites {
        for triangle in [
            setup(sprite.vertices[0], sprite.vertices[1], sprite.vertices[2]),
            setup(sprite.vertices[0], sprite.vertices[2], sprite.vertices[3]),
        ]
        .into_iter()
        .flatten()
        {
            for stripe in (triangle.min_y / STRIPE_ROWS).max(0)
                ..=(triangle.max_y / STRIPE_ROWS).min(STRIPES as i32 - 1)
            {
                checksum = fold_setup(checksum, triangle, stripe);
            }
        }
    }
    checksum
}

#[inline(always)]
fn wrap_uv(mut value: f32, repeat: bool) -> f32 {
    if repeat {
        value = value.fract();
        if value < 0.0 {
            value += 1.0;
        }
        value
    } else {
        value.clamp(0.0, 1.0)
    }
}

#[inline(always)]
fn wrap_index_old(index: i32, max: usize, repeat: bool) -> usize {
    if !repeat {
        return index.clamp(0, max.saturating_sub(1) as i32) as usize;
    }
    if max.is_power_of_two() {
        return index as usize & (max - 1);
    }
    index.rem_euclid(max as i32) as usize
}

#[inline(always)]
fn nearest_old(value: f32, max: usize, repeat: bool) -> usize {
    wrap_index_old(
        (wrap_uv(value, repeat) * max as f32).floor() as i32,
        max,
        repeat,
    )
}

#[inline(always)]
fn nearest_new(value: f32, max: usize, repeat: bool) -> usize {
    let index = (wrap_uv(value, repeat) * max as f32).floor() as usize;
    index.min(max.saturating_sub(1))
}

fn nearest_work(new: bool) -> u64 {
    let mut checksum = 0u64;
    for index in 0..SAMPLES_PER_OP {
        let value = ((index * 313 % 65_537) as f32 - 32_768.0) * (1.0 / 2_113.0);
        let max = [64, 96, 128, 257][index & 3];
        let sample = if new {
            nearest_new(value, max, index & 1 == 0)
        } else {
            nearest_old(value, max, index & 1 == 0)
        };
        checksum = checksum.rotate_left(5) ^ sample as u64;
    }
    checksum
}

fn texels() -> Vec<u8> {
    let mut out = vec![0; SAMPLES_PER_OP * 4];
    for index in 0..SAMPLES_PER_OP {
        out[index * 4] = (index * 37) as u8;
        out[index * 4 + 1] = (index * 71) as u8;
        out[index * 4 + 2] = (index * 109) as u8;
        out[index * 4 + 3] = 255;
    }
    out
}

fn texture_upload_work(source: &[u8], dest: &mut [u8], classify: bool) -> u64 {
    dest.clone_from_slice(source);
    let opaque = !classify || dest.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255);
    u64::from(dest[0])
        ^ (u64::from(dest[TEXTURE_BYTES / 2]) << 8)
        ^ (u64::from(dest[TEXTURE_BYTES - 1]) << 16)
        ^ ((opaque as u64) << 63)
}

#[inline(always)]
fn opaque_old(data: &[u8], index: usize) -> [f32; 4] {
    const SCALE: f32 = 1.0 / 255.0;
    [
        f32::from(data[index]) * SCALE,
        f32::from(data[index + 1]) * SCALE,
        f32::from(data[index + 2]) * SCALE,
        f32::from(data[index + 3]) * SCALE,
    ]
}

fn opaque_mask_work(data: &[u8], skip_sample: bool) -> u64 {
    let mut checksum = 0u64;
    for sample in 0..SAMPLES_PER_OP {
        let texel = sample * 127 % (SAMPLES_PER_OP - 130);
        let alpha = if skip_sample {
            1.0
        } else {
            linear_opaque(data, texel * 4)[3]
        };
        checksum = checksum.rotate_left(5) ^ u64::from(alpha.to_bits());
    }
    checksum
}

#[inline(always)]
fn linear_opaque(data: &[u8], base: usize) -> [f32; 4] {
    let indices = [base, base + 4, base + 512, base + 516];
    let colors = indices.map(|index| opaque_old(data, index));
    let fx = 0.37;
    let fy = 0.61;
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    std::array::from_fn(|channel| {
        lerp(
            lerp(colors[0][channel], colors[1][channel], fx),
            lerp(colors[2][channel], colors[3][channel], fx),
            fy,
        )
    })
}

fn linear_texels() -> Vec<u8> {
    let mut out = vec![0; SAMPLES_PER_OP * 4];
    for index in 0..SAMPLES_PER_OP {
        out[index * 4] = (index * 37) as u8;
        out[index * 4 + 1] = (index * 71) as u8;
        out[index * 4 + 2] = (index * 109) as u8;
        out[index * 4 + 3] = if index % 11 < 8 { 0 } else { 64 + index as u8 };
    }
    out
}

#[inline(always)]
fn linear(data: &[u8], base: usize, early: bool) -> Option<[f32; 4]> {
    let indices = [base, base + 4, base + 512, base + 516];
    if early && indices.iter().all(|index| data[index + 3] == 0) {
        return None;
    }
    const SCALE: f32 = 1.0 / 255.0;
    let mut colors = [[0.0; 4]; 4];
    for (out, index) in colors.iter_mut().zip(indices) {
        *out = [
            f32::from(data[index]) * SCALE,
            f32::from(data[index + 1]) * SCALE,
            f32::from(data[index + 2]) * SCALE,
            f32::from(data[index + 3]) * SCALE,
        ];
    }
    let fx = 0.37;
    let fy = 0.61;
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    Some(std::array::from_fn(|channel| {
        lerp(
            lerp(colors[0][channel], colors[1][channel], fx),
            lerp(colors[2][channel], colors[3][channel], fx),
            fy,
        )
    }))
}

fn transparent_linear_work(data: &[u8], early: bool) -> u64 {
    let mut checksum = 0u64;
    for sample in 0..SAMPLES_PER_OP {
        let texel = sample * 127 % (SAMPLES_PER_OP - 130);
        if let Some(rgba) = linear(data, texel * 4, early)
            && rgba[3] > 0.0
        {
            checksum = checksum.rotate_left(5)
                ^ u64::from(rgba[0].to_bits())
                ^ u64::from(rgba[1].to_bits())
                ^ u64::from(rgba[2].to_bits())
                ^ u64::from(rgba[3].to_bits());
        }
    }
    checksum
}

#[inline(always)]
fn linear_alpha(data: &[u8], base: usize) -> f32 {
    let indices = [base, base + 4, base + 512, base + 516];
    const SCALE: f32 = 1.0 / 255.0;
    let alpha = indices.map(|index| f32::from(data[index + 3]) * SCALE);
    let fx = 0.37;
    let fy = 0.61;
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    lerp(
        lerp(alpha[0], alpha[1], fx),
        lerp(alpha[2], alpha[3], fx),
        fy,
    )
}

fn texture_mask_work(data: &[u8], alpha_only: bool) -> u64 {
    let mut checksum = 0u64;
    for sample in 0..SAMPLES_PER_OP {
        let texel = sample * 127 % (SAMPLES_PER_OP - 130);
        let base = texel * 4;
        let alpha = if alpha_only {
            linear_alpha(data, base)
        } else {
            linear(data, base, false).expect("four in-range texels")[3]
        };
        if alpha > 0.0 {
            checksum = checksum.rotate_left(5) ^ u64::from(alpha.to_bits());
        }
    }
    checksum
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
    let sprites = sprite_data();
    let opaque = texels();
    let transparent = linear_texels();
    let upload_source: Vec<u8> = (0..TEXTURE_BYTES)
        .map(|index| if index & 3 == 3 { 255 } else { index as u8 })
        .collect();
    let mut old_upload = vec![0; TEXTURE_BYTES];
    let mut new_upload = vec![0; TEXTURE_BYTES];

    print_pair(
        "sprite retained triangle setup + dispatch",
        "sprite",
        SPRITES as f64,
        &measure_pair(|| sprites_old(&sprites), || sprites_new(&sprites)),
    );
    print_pair(
        "nearest normalized addressing",
        "sample",
        SAMPLES_PER_OP as f64,
        &measure_pair(|| nearest_work(false), || nearest_work(true)),
    );
    print_pair(
        "texture update + opacity classification",
        "byte",
        TEXTURE_BYTES as f64,
        &measure_pair(
            || texture_upload_work(&upload_source, &mut old_upload, false),
            || texture_upload_work(&upload_source, &mut new_upload, true),
        ),
    );
    print_pair(
        "opaque texture-mask sample elimination",
        "sample",
        SAMPLES_PER_OP as f64,
        &measure_pair(
            || opaque_mask_work(&opaque, false),
            || opaque_mask_work(&opaque, true),
        ),
    );
    print_pair(
        "texture-mask alpha-only sampling",
        "sample",
        SAMPLES_PER_OP as f64,
        &measure_pair(
            || texture_mask_work(&transparent, false),
            || texture_mask_work(&transparent, true),
        ),
    );
    print_pair(
        "transparent bilinear early reject",
        "sample",
        SAMPLES_PER_OP as f64,
        &measure_pair(
            || transparent_linear_work(&transparent, false),
            || transparent_linear_work(&transparent, true),
        ),
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
