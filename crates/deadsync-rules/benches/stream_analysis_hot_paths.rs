use deadsync_rules::stream::{
    StreamSegment, measure_densities, stream_sequences_threshold, zmod_stream_totals_full_measures,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MEASURES: usize = 1_024;
const ROWS_PER_MEASURE: usize = 64;
const STREAM_MEASURES: usize = 16_384;
const DENSITY_OPS: usize = 1_000;
const SEGMENT_OPS: usize = 2_000;
const ZMOD_OPS: usize = 1_000;
const SAMPLE_BATCHES: usize = 100;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while the benchmark gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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

struct BenchResult {
    ns_per_op: f64,
    worst_sample_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(
    iterations: usize,
    items_per_op: usize,
    mut operation: impl FnMut() -> u64,
) -> BenchResult {
    for _ in 0..(iterations / 20).max(1) {
        black_box(operation());
    }

    let batch = (iterations / SAMPLE_BATCHES).max(1);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..iterations / batch {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1_000_000_000.0 / iterations as f64,
        worst_sample_ns,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        items_per_second: iterations as f64 * items_per_op as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% sample tail  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
        percent_change(old.worst_sample_ns, new.worst_sample_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} worst ns  \
         {:>8.2} Mitem/s  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>10.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        result.items_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
    );
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

fn note_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(MEASURES * ROWS_PER_MEASURE * 5);
    for measure in 0..MEASURES {
        for row in 0..ROWS_PER_MEASURE {
            let column = (row + measure) % 4;
            let symbol = if row % 4 == 0 {
                b'1'
            } else if row % 19 == 0 {
                b'M'
            } else {
                b'0'
            };
            let mut cells = [b'0'; 4];
            cells[column] = symbol;
            data.extend_from_slice(&cells);
            data.push(b'\n');
        }
        data.extend_from_slice(if measure + 1 == MEASURES {
            b";\n"
        } else {
            b",\n"
        });
    }
    data
}

const ROW_ZERO: u8 = 1;
const ROW_STEP: u8 = 1 << 1;

fn measure_densities_old(data: &[u8], lanes: usize) -> Vec<usize> {
    match lanes {
        8 => measure_densities_old_impl::<8>(data),
        _ => measure_densities_old_impl::<4>(data),
    }
}

fn measure_densities_old_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * 4) + 1);
    let mut measure = Vec::with_capacity(64);
    let mut measure_steps = 0usize;
    let mut done = false;
    for raw in data.split(|&byte| byte == b'\n') {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        let line = line.trim_ascii_start();
        if line.is_empty() || line[0] == b'/' {
            continue;
        }
        match line[0] {
            b',' => push_density_old(&mut measure, &mut measure_steps, &mut densities),
            b';' => {
                push_density_old(&mut measure, &mut measure_steps, &mut densities);
                done = true;
                break;
            }
            _ if line.len() >= LANES => {
                let mut all_zero = true;
                let mut has_step = false;
                for &byte in &line[..LANES] {
                    all_zero &= byte == b'0';
                    has_step |= matches!(byte, b'1' | b'2' | b'4');
                }
                let flags = u8::from(all_zero) | (u8::from(has_step) << 1);
                measure_steps += usize::from(flags & ROW_STEP != 0);
                measure.push(flags);
            }
            _ => {}
        }
    }
    if !done {
        push_density_old(&mut measure, &mut measure_steps, &mut densities);
    }
    densities
}

fn push_density_old(measure: &mut Vec<u8>, measure_steps: &mut usize, densities: &mut Vec<usize>) {
    if measure.is_empty() {
        densities.push(0);
        return;
    }
    let mut shift = 0usize;
    let mut step = 2usize;
    if measure.len() >= 2 {
        for _ in 0..measure.len().trailing_zeros() {
            let mut idx = step / 2;
            while idx < measure.len() {
                if measure[idx] & ROW_ZERO == 0 {
                    break;
                }
                idx += step;
            }
            if idx < measure.len() {
                break;
            }
            shift += 1;
            step <<= 1;
        }
    }
    let density = if shift == 0 {
        *measure_steps
    } else {
        let step = 1usize << shift;
        (0..measure.len() >> shift)
            .map(|idx| usize::from(measure[idx * step] & ROW_STEP != 0))
            .sum()
    };
    densities.push(density);
    measure.clear();
    *measure_steps = 0;
}

fn stream_sequences_old(measures: &[usize], threshold: usize) -> Vec<StreamSegment> {
    let streams: Vec<_> = measures
        .iter()
        .enumerate()
        .filter(|(_, density)| **density >= threshold)
        .map(|(idx, _)| idx + 1)
        .collect();
    if streams.is_empty() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let first_break = streams[0].saturating_sub(1);
    if first_break >= 2 {
        segments.push(StreamSegment {
            start: 0,
            end: first_break,
            is_break: true,
        });
    }
    let (mut count, mut end) = (1usize, None);
    for (idx, &current) in streams.iter().enumerate() {
        let next = streams.get(idx + 1).copied().unwrap_or(usize::MAX);
        if current + 1 == next {
            count += 1;
            end = Some(current + 1);
            continue;
        }
        let stream_end = end.unwrap_or(current);
        segments.push(StreamSegment {
            start: stream_end - count,
            end: stream_end,
            is_break: false,
        });
        let break_end = if next == usize::MAX {
            measures.len()
        } else {
            next - 1
        };
        if break_end >= current + 2 {
            segments.push(StreamSegment {
                start: current,
                end: break_end,
                is_break: true,
            });
        }
        count = 1;
        end = None;
    }
    segments
}

fn stream_measures() -> Vec<usize> {
    (0..STREAM_MEASURES)
        .map(|idx| if idx % 32 < 20 { 16 } else { 4 })
        .collect()
}

fn zmod_measures() -> Vec<usize> {
    (0..STREAM_MEASURES)
        .map(|idx| match idx % 64 {
            0..=3 => 32,
            4..=7 => 24,
            8..=11 => 20,
            12..=27 => 16,
            _ => 4,
        })
        .collect()
}

fn materialized_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
    let segments = stream_sequences_threshold(measures, threshold);
    if segments.is_empty() {
        return 0.0;
    }
    let mut total_stream = 0.0_f32;
    let mut total_measures = 0.0_f32;
    for segment in segments {
        let len = ((segment.end.saturating_sub(segment.start)) as f32 * multiplier).floor();
        if len <= 0.0 {
            continue;
        }
        if !segment.is_break {
            total_stream += len;
        }
        total_measures += len;
    }
    if total_measures <= 0.0 {
        0.0
    } else {
        total_stream / total_measures
    }
}

fn zmod_old_probes(measures: &[usize]) -> (Vec<StreamSegment>, f32, f32) {
    let mut threshold = 32usize;
    let mut multiplier = 2.0_f32;
    if materialized_density(measures, threshold, multiplier) < 0.2 {
        threshold = 24;
        multiplier = 1.5;
        if materialized_density(measures, threshold, multiplier) < 0.2 {
            threshold = 20;
            multiplier = 1.25;
            if materialized_density(measures, threshold, multiplier) < 0.2 {
                threshold = 16;
                multiplier = 1.0;
            }
        }
    }
    let segments = stream_sequences_threshold(measures, threshold);
    let (stream, breaks) = stream_break_totals(&segments);
    (segments, stream * multiplier, breaks * multiplier)
}

fn stream_break_totals(segments: &[StreamSegment]) -> (f32, f32) {
    let mut total_stream = 0.0_f32;
    let mut total_break = 0.0_f32;
    let mut edge_break = 0.0_f32;
    let mut last_stream = false;
    for (idx, segment) in segments.iter().enumerate() {
        let len = segment.end.saturating_sub(segment.start) as f32;
        if len <= 0.0 {
            continue;
        }
        if segment.is_break && idx > 0 && idx + 1 < segments.len() {
            total_break += len;
            last_stream = false;
        } else if segment.is_break {
            edge_break += len;
            last_stream = false;
        } else {
            if last_stream {
                total_break += 1.0;
            }
            total_stream += len;
            last_stream = true;
        }
    }
    if total_stream + total_break < 10.0 || total_stream + total_break < edge_break {
        total_break += edge_break;
    }
    (total_stream, total_break)
}

fn usize_checksum(values: &[usize]) -> u64 {
    values
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (idx, value)| {
            checksum.rotate_left(7) ^ (*value as u64).rotate_left(idx as u32)
        })
}

fn segment_checksum(segments: &[StreamSegment]) -> u64 {
    segments.iter().fold(0u64, |checksum, segment| {
        checksum.rotate_left(9)
            ^ (segment.start as u64)
            ^ (segment.end as u64).rotate_left(23)
            ^ u64::from(segment.is_break).rotate_left(47)
    })
}

fn totals_checksum(value: &(Vec<StreamSegment>, f32, f32)) -> u64 {
    segment_checksum(&value.0)
        ^ u64::from(value.1.to_bits()).rotate_left(17)
        ^ u64::from(value.2.to_bits()).rotate_left(41)
}

fn main() {
    let data = note_data();
    let old_density_value = measure_densities_old(&data, 4);
    let new_density_value = measure_densities(&data, 4);
    assert_eq!(old_density_value, new_density_value);
    let old_density = measure(DENSITY_OPS, MEASURES * ROWS_PER_MEASURE, || {
        usize_checksum(&measure_densities_old(black_box(&data), 4))
    });
    let new_density = measure(DENSITY_OPS, MEASURES * ROWS_PER_MEASURE, || {
        usize_checksum(&measure_densities(black_box(&data), 4))
    });
    print_pair(
        "scratch-free measure density scan",
        DENSITY_OPS,
        &old_density,
        &new_density,
    );

    let measures = stream_measures();
    let old_segment_value = stream_sequences_old(&measures, 16);
    let new_segment_value = stream_sequences_threshold(&measures, 16);
    assert_eq!(old_segment_value, new_segment_value);
    let old_segments = measure(SEGMENT_OPS, measures.len(), || {
        segment_checksum(&stream_sequences_old(black_box(&measures), 16))
    });
    let new_segments = measure(SEGMENT_OPS, measures.len(), || {
        segment_checksum(&stream_sequences_threshold(black_box(&measures), 16))
    });
    print_pair(
        "direct stream segment construction",
        SEGMENT_OPS,
        &old_segments,
        &new_segments,
    );

    let measures = zmod_measures();
    let old_zmod_value = zmod_old_probes(&measures);
    let new_zmod_value = zmod_stream_totals_full_measures(&measures, true);
    assert_eq!(old_zmod_value.0, new_zmod_value.0);
    assert_eq!(
        totals_checksum(&old_zmod_value),
        totals_checksum(&new_zmod_value)
    );
    let old_zmod = measure(ZMOD_OPS, measures.len(), || {
        totals_checksum(&zmod_old_probes(black_box(&measures)))
    });
    let new_zmod = measure(ZMOD_OPS, measures.len(), || {
        totals_checksum(&zmod_stream_totals_full_measures(
            black_box(&measures),
            true,
        ))
    });
    print_pair(
        "single-pass allocation-free ZMod probes",
        ZMOD_OPS,
        &old_zmod,
        &new_zmod,
    );
}
