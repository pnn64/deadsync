use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP: usize = 400;
const SAMPLES: usize = 80;
const OPS_PER_SAMPLE: usize = 100;
const ALLOC_OPS: usize = 2_000;
const OBJECTS: usize = 384;
const TRIANGLES: usize = 256;
const STRIPES: usize = 34;
const ROWS: usize = STRIPES * 32;

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

// SAFETY: allocation operations delegate unchanged to `System`; the atomics
// only observe successful calls while the benchmark gate is enabled.
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
struct Object {
    rows: (u16, u16),
    id: u32,
}

fn objects() -> [Object; OBJECTS] {
    std::array::from_fn(|index| {
        let center = (index * 83) % ROWS;
        let half = 20 + index % 29;
        Object {
            rows: (
                center.saturating_sub(half) as u16,
                (center + half + 1).min(ROWS) as u16,
            ),
            id: index as u32 + 1,
        }
    })
}

fn scan_stripes_old(objects: &[Object; OBJECTS]) -> u64 {
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = (stripe * 32) as u16;
        let end = start + 32;
        for object in objects {
            if object.rows.0 < end && object.rows.1 > start {
                checksum = checksum.rotate_left(5) ^ u64::from(object.id);
            }
        }
    }
    checksum
}

#[derive(Default)]
struct StripeBins {
    offsets: Vec<u32>,
    cursors: Vec<u32>,
    indices: Vec<u32>,
}

impl StripeBins {
    fn build(&mut self, objects: &[Object; OBJECTS]) {
        self.offsets.clear();
        self.offsets.resize(STRIPES + 1, 0);
        for object in objects {
            let first = usize::from(object.rows.0) / 32;
            let end = usize::from(object.rows.1).div_ceil(32).min(STRIPES);
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
        for (index, object) in objects.iter().enumerate() {
            let first = usize::from(object.rows.0) / 32;
            let end = usize::from(object.rows.1).div_ceil(32).min(STRIPES);
            for stripe in first.min(STRIPES)..end {
                let slot = self.cursors[stripe] as usize;
                self.indices[slot] = index as u32;
                self.cursors[stripe] += 1;
            }
        }
    }
}

fn scan_stripes_new(objects: &[Object; OBJECTS], bins: &mut StripeBins) -> u64 {
    bins.build(objects);
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = bins.offsets[stripe] as usize;
        let end = bins.offsets[stripe + 1] as usize;
        for &index in &bins.indices[start..end] {
            checksum = checksum.rotate_left(5) ^ u64::from(objects[index as usize].id);
        }
    }
    checksum
}

#[derive(Clone, Copy)]
enum TextureSlot {
    Software(u64),
    Foreign,
}

struct TextureTable {
    slots: [Option<TextureSlot>; 64],
}

impl TextureTable {
    #[inline(always)]
    fn software(&self, handle: u8) -> Option<u64> {
        match self.slots[usize::from(handle)]? {
            TextureSlot::Software(value) => Some(value),
            TextureSlot::Foreign => None,
        }
    }
}

fn texture_table() -> TextureTable {
    TextureTable {
        slots: std::array::from_fn(|index| {
            if index % 13 == 0 {
                Some(TextureSlot::Foreign)
            } else if index % 11 == 0 {
                None
            } else {
                Some(TextureSlot::Software(
                    (index as u64 + 1).wrapping_mul(0x9e37_79b9),
                ))
            }
        }),
    }
}

fn texture_handles() -> [u8; OBJECTS] {
    std::array::from_fn(|index| ((index / 24) * 7 % 61) as u8)
}

fn resolve_textures_old(table: &TextureTable, handles: &[u8; OBJECTS]) -> u64 {
    handles.iter().fold(0u64, |checksum, &handle| {
        table
            .software(black_box(handle))
            .map_or(checksum.rotate_left(3), |value| {
                checksum.rotate_left(3) ^ value
            })
    })
}

fn resolve_textures_new(table: &TextureTable, handles: &[u8; OBJECTS]) -> u64 {
    let mut cached: Option<(u8, Option<u64>)> = None;
    handles.iter().fold(0u64, |checksum, &handle| {
        let value = match cached {
            Some((cached_handle, value)) if cached_handle == handle => value,
            _ => {
                let value = table.software(black_box(handle));
                cached = Some((handle, value));
                value
            }
        };
        value.map_or(checksum.rotate_left(3), |value| {
            checksum.rotate_left(3) ^ value
        })
    })
}

#[derive(Clone, Copy)]
struct Triangle {
    x: [f32; 3],
    y: [f32; 3],
    id: u32,
}

fn triangles() -> [Triangle; TRIANGLES] {
    std::array::from_fn(|index| {
        let center = ((index * 101) % ROWS) as f32;
        let half = (8 + index % 31) as f32;
        Triangle {
            x: [30.0 - half, 30.0 + half, 30.0 + (index % 9) as f32],
            y: [center - half, center + half * 0.37, center + half],
            id: index as u32 + 1,
        }
    })
}

#[derive(Clone, Copy)]
struct TriangleSetup {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    inv_denom: f32,
    id: u32,
}

#[inline(always)]
fn triangle_setup(triangle: Triangle) -> Option<TriangleSetup> {
    let min_x = triangle.x.into_iter().fold(f32::INFINITY, f32::min).floor() as i32;
    let max_x = triangle
        .x
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    let min_y = triangle
        .y
        .into_iter()
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = triangle
        .y
        .into_iter()
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((ROWS - 1) as f32) as i32;
    let denom = (triangle.x[2] - triangle.x[0]).mul_add(
        triangle.y[1] - triangle.y[0],
        -((triangle.y[2] - triangle.y[0]) * (triangle.x[1] - triangle.x[0])),
    );
    (min_x <= max_x && min_y <= max_y && denom != 0.0).then_some(TriangleSetup {
        min_x,
        max_x,
        min_y,
        max_y,
        inv_denom: 1.0 / denom,
        id: triangle.id,
    })
}

fn triangle_scan_old(triangles: &[Triangle; TRIANGLES]) -> u64 {
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = (stripe * 32) as i32;
        let end = start + 31;
        for &triangle in triangles {
            let Some(setup) = triangle_setup(triangle) else {
                continue;
            };
            if setup.max_y >= start && setup.min_y <= end {
                checksum = checksum.rotate_left(7)
                    ^ u64::from(setup.inv_denom.to_bits())
                    ^ (setup.min_x as u64).rotate_left(11)
                    ^ (setup.max_x as u64).rotate_left(19)
                    ^ u64::from(setup.id);
            }
        }
    }
    checksum
}

fn triangle_scan_new(triangles: &[Triangle; TRIANGLES], setups: &mut Vec<TriangleSetup>) -> u64 {
    setups.clear();
    setups.extend(triangles.iter().copied().filter_map(triangle_setup));
    let mut checksum = 0u64;
    for stripe in 0..STRIPES {
        let start = (stripe * 32) as i32;
        let end = start + 31;
        for setup in setups.iter() {
            if setup.max_y >= start && setup.min_y <= end {
                checksum = checksum.rotate_left(7)
                    ^ u64::from(setup.inv_denom.to_bits())
                    ^ (setup.min_x as u64).rotate_left(11)
                    ^ (setup.max_x as u64).rotate_left(19)
                    ^ u64::from(setup.id);
            }
        }
    }
    checksum
}

fn main() {
    let objects = objects();
    let mut bins = StripeBins::default();
    assert_eq!(
        scan_stripes_old(&objects),
        scan_stripes_new(&objects, &mut bins)
    );
    let [old_bins, new_bins] = measure_pair(
        || scan_stripes_old(&objects),
        || scan_stripes_new(&objects, &mut bins),
    );
    assert_pair(&old_bins, &new_bins);
    println!("software stripe dispatch ({OBJECTS} objects x {STRIPES} stripes)");
    print_result("old: scan all objects", &old_bins, OBJECTS * STRIPES);
    print_result("new: retained stripe index", &new_bins, OBJECTS * STRIPES);
    print_change(&old_bins, &new_bins);

    let table = texture_table();
    let handles = texture_handles();
    assert_eq!(
        resolve_textures_old(&table, &handles),
        resolve_textures_new(&table, &handles)
    );
    let [old_lookup, new_lookup] = measure_pair(
        || resolve_textures_old(&table, &handles),
        || resolve_textures_new(&table, &handles),
    );
    assert_pair(&old_lookup, &new_lookup);
    println!("\nsoftware texture resolution ({OBJECTS} prepared objects, 24 per run)");
    print_result("old: lookup every object", &old_lookup, OBJECTS);
    print_result("new: reuse run lookup", &new_lookup, OBJECTS);
    print_change(&old_lookup, &new_lookup);

    let triangles = triangles();
    let mut setups = Vec::with_capacity(TRIANGLES);
    assert_eq!(
        triangle_scan_old(&triangles),
        triangle_scan_new(&triangles, &mut setups)
    );
    let [old_triangles, new_triangles] = measure_pair(
        || triangle_scan_old(&triangles),
        || triangle_scan_new(&triangles, &mut setups),
    );
    assert_pair(&old_triangles, &new_triangles);
    println!("\nsoftware mesh setup ({TRIANGLES} triangles x {STRIPES} stripes)");
    print_result(
        "old: setup in every stripe",
        &old_triangles,
        TRIANGLES * STRIPES,
    );
    print_result(
        "new: retained triangle setup",
        &new_triangles,
        TRIANGLES * STRIPES,
    );
    print_change(&old_triangles, &new_triangles);
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
