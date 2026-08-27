use deadsync_rules::timing::{
    HistogramMs, ScatterFoot, ScatterPoint, TimingStats, TimingStatsAccum, merge_histograms_ms,
    merge_histograms_ms_iter, timing_stats_from_offsets,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const STAGES: usize = 12;
const POINTS_PER_STAGE: usize = 2_048;
const LIFE_PER_STAGE: usize = 256;
const TIMING_OPS: usize = 3_000;
const HISTOGRAM_OPS: usize = 1_000;
const GRAPH_OPS: usize = 2_000;
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

#[derive(Clone)]
struct GraphStage {
    offset: f32,
    scatter: Vec<ScatterPoint>,
    life: Vec<(f32, f32)>,
}

fn graph_stages() -> Vec<GraphStage> {
    (0..STAGES)
        .map(|stage| GraphStage {
            offset: stage as f32 * 90.0,
            scatter: (0..POINTS_PER_STAGE)
                .map(|point| ScatterPoint {
                    time_sec: point as f32 / 32.0,
                    offset_ms: (!point.is_multiple_of(97))
                        .then_some(((point * 37 + stage * 11) % 361) as f32 - 180.25),
                    direction_code: (point % 4 + 1) as u8,
                    miss_because_held: point.is_multiple_of(389),
                    row_index: point,
                    quantization_idx: (point % 9) as u8,
                    parity_foot: if point.is_multiple_of(2) {
                        ScatterFoot::Left
                    } else {
                        ScatterFoot::Right
                    },
                })
                .collect(),
            life: (0..LIFE_PER_STAGE)
                .map(|sample| (sample as f32 / 4.0, 1.0 - sample as f32 / 512.0))
                .collect(),
        })
        .collect()
}

fn histograms() -> Vec<HistogramMs> {
    (0..STAGES)
        .map(|stage| {
            let bins: Vec<(i32, u32)> = (-180i32..=180)
                .map(|bin| (bin, ((bin.unsigned_abs() as usize + stage) % 7 + 1) as u32))
                .collect();
            let max_count = bins.iter().map(|&(_, count)| count).max().unwrap_or(0);
            HistogramMs {
                smoothed: bins
                    .iter()
                    .map(|&(bin, count)| (bin, count as f32))
                    .collect(),
                bins,
                max_count,
                worst_observed_ms: 180.0,
                worst_window_ms: 180.25,
            }
        })
        .collect()
}

fn timing_old(scatter: &[ScatterPoint]) -> TimingStats {
    let mut offsets = Vec::new();
    for point in scatter {
        if let Some(offset_ms) = point.offset_ms {
            offsets.push(offset_ms);
        }
    }
    timing_stats_from_offsets(offsets)
}

fn timing_new(scatter: &[ScatterPoint]) -> TimingStats {
    let mut stats = TimingStatsAccum::default();
    for point in scatter {
        if let Some(offset_ms) = point.offset_ms {
            stats.record(offset_ms);
        }
    }
    stats.finish()
}

fn timing_checksum(stats: TimingStats) -> u64 {
    u64::from(stats.mean_abs_ms.to_bits())
        ^ u64::from(stats.mean_ms.to_bits()).rotate_left(17)
        ^ u64::from(stats.stddev_ms.to_bits()).rotate_left(33)
        ^ u64::from(stats.max_abs_ms.to_bits()).rotate_left(49)
}

fn histogram_old(histograms: &[HistogramMs]) -> HistogramMs {
    let mut owned = Vec::new();
    for histogram in histograms {
        owned.push(histogram.clone());
    }
    merge_histograms_ms(&owned)
}

fn histogram_new(histograms: &[HistogramMs]) -> HistogramMs {
    merge_histograms_ms_iter(histograms.iter())
}

fn histogram_checksum(histogram: &HistogramMs) -> u64 {
    histogram.bins.iter().fold(
        histogram.smoothed.len() as u64,
        |checksum, &(bin, count)| {
            checksum.rotate_left(5) ^ u64::from(bin as u32) ^ u64::from(count).rotate_left(29)
        },
    ) ^ u64::from(histogram.max_count).rotate_left(47)
        ^ u64::from(histogram.worst_observed_ms.to_bits())
        ^ u64::from(histogram.worst_window_ms.to_bits()).rotate_left(13)
}

fn graph_old(stages: &[GraphStage]) -> (Vec<ScatterPoint>, Vec<(f32, f32)>) {
    let mut scatter = Vec::new();
    let mut life = Vec::new();
    for stage in stages {
        scatter.reserve(stage.scatter.len());
        for point in &stage.scatter {
            let mut shifted = *point;
            shifted.time_sec += stage.offset;
            scatter.push(shifted);
        }
        life.reserve(stage.life.len());
        for &(time, value) in &stage.life {
            life.push((time + stage.offset, value));
        }
    }
    (scatter, life)
}

fn graph_new(stages: &[GraphStage]) -> (Vec<ScatterPoint>, Vec<(f32, f32)>) {
    let (scatter_capacity, life_capacity) =
        stages
            .iter()
            .fold((0usize, 0usize), |(scatter, life), stage| {
                (
                    scatter.saturating_add(stage.scatter.len()),
                    life.saturating_add(stage.life.len()),
                )
            });
    let mut scatter = Vec::with_capacity(scatter_capacity);
    let mut life = Vec::with_capacity(life_capacity);
    for stage in stages {
        for point in &stage.scatter {
            let mut shifted = *point;
            shifted.time_sec += stage.offset;
            scatter.push(shifted);
        }
        for &(time, value) in &stage.life {
            life.push((time + stage.offset, value));
        }
    }
    (scatter, life)
}

fn graph_checksum((scatter, life): &(Vec<ScatterPoint>, Vec<(f32, f32)>)) -> u64 {
    let scatter_checksum = scatter.iter().fold(0u64, |checksum, point| {
        checksum.rotate_left(7)
            ^ u64::from(point.time_sec.to_bits())
            ^ u64::from(point.offset_ms.unwrap_or_default().to_bits()).rotate_left(31)
            ^ (point.row_index as u64).rotate_left(47)
    });
    life.iter()
        .fold(scatter_checksum, |checksum, &(time, value)| {
            checksum.rotate_left(11)
                ^ u64::from(time.to_bits())
                ^ u64::from(value.to_bits()).rotate_left(37)
        })
}

fn main() {
    let stages = graph_stages();
    let scatter: Vec<ScatterPoint> = stages
        .iter()
        .flat_map(|stage| stage.scatter.iter().copied())
        .collect();
    assert_eq!(
        timing_checksum(timing_old(&scatter)),
        timing_checksum(timing_new(&scatter))
    );
    let old_timing = measure(TIMING_OPS, scatter.len(), || {
        timing_checksum(timing_old(black_box(&scatter)))
    });
    let new_timing = measure(TIMING_OPS, scatter.len(), || {
        timing_checksum(timing_new(black_box(&scatter)))
    });
    print_pair(
        "streamed course timing statistics",
        TIMING_OPS,
        &old_timing,
        &new_timing,
    );

    let histograms = histograms();
    let old_histogram_value = histogram_old(&histograms);
    let new_histogram_value = histogram_new(&histograms);
    assert_eq!(old_histogram_value.bins, new_histogram_value.bins);
    assert_eq!(old_histogram_value.smoothed, new_histogram_value.smoothed);
    assert_eq!(
        histogram_checksum(&old_histogram_value),
        histogram_checksum(&new_histogram_value)
    );
    let histogram_items = histograms
        .iter()
        .map(|histogram| histogram.bins.len())
        .sum();
    let old_histogram = measure(HISTOGRAM_OPS, histogram_items, || {
        let histogram = histogram_old(black_box(&histograms));
        histogram_checksum(black_box(&histogram))
    });
    let new_histogram = measure(HISTOGRAM_OPS, histogram_items, || {
        let histogram = histogram_new(black_box(&histograms));
        histogram_checksum(black_box(&histogram))
    });
    print_pair(
        "borrowed course histogram merge",
        HISTOGRAM_OPS,
        &old_histogram,
        &new_histogram,
    );

    let old_graph_value = graph_old(&stages);
    let new_graph_value = graph_new(&stages);
    assert_eq!(old_graph_value.0.len(), new_graph_value.0.len());
    assert_eq!(old_graph_value.1, new_graph_value.1);
    assert_eq!(
        graph_checksum(&old_graph_value),
        graph_checksum(&new_graph_value)
    );
    let graph_items = STAGES * (POINTS_PER_STAGE + LIFE_PER_STAGE);
    let old_graph = measure(GRAPH_OPS, graph_items, || {
        let graph = graph_old(black_box(&stages));
        graph_checksum(black_box(&graph))
    });
    let new_graph = measure(GRAPH_OPS, graph_items, || {
        let graph = graph_new(black_box(&stages));
        graph_checksum(black_box(&graph))
    });
    print_pair(
        "pre-sized course graph concatenation",
        GRAPH_OPS,
        &old_graph,
        &new_graph,
    );
}
