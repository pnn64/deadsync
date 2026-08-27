use deadsync_core::note::NoteType;
use deadsync_rules::{
    judgment::{JudgeGrade, Judgment, TimingWindow, judgment_time_error_music_ns_from_ms},
    note::Note,
    timing::{
        ArrowTimingStats, HistogramMs, ScatterFoot, ScatterPoint, TimingStats, bench_support,
    },
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ROWS: usize = 8_192;
const STATS_OPS: usize = 5_000;
const HISTOGRAM_OPS: usize = 1_500;
const MERGE_OPS: usize = 3_000;
const ARROW_OPS: usize = 3_000;
const LANE_BUCKET_OPS: usize = 500_000;
const SCATTER_OPS: usize = 1_000;
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

fn note(row: usize, column: usize, grade: JudgeGrade, error_ms: f32) -> Note {
    let window = match grade {
        JudgeGrade::Fantastic => Some(TimingWindow::W1),
        JudgeGrade::Excellent => Some(TimingWindow::W2),
        JudgeGrade::Great => Some(TimingWindow::W3),
        JudgeGrade::Decent => Some(TimingWindow::W4),
        JudgeGrade::WayOff => Some(TimingWindow::W5),
        JudgeGrade::Miss => None,
    };
    Note {
        beat: row as f32 / 48.0,
        quantization_idx: 0,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: Some(Judgment {
            time_error_ms: error_ms,
            time_error_music_ns: judgment_time_error_music_ns_from_ms(error_ms, 1.0),
            grade,
            window,
            miss_because_held: false,
        }),
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: true,
    }
}

fn summary_notes(phase: usize) -> Vec<Note> {
    let mut notes = Vec::with_capacity(ROWS + ROWS / 4);
    for row in 0..ROWS {
        let error = ((row.wrapping_mul(37) + phase * 11) % 361) as f32 - 180.25;
        let grade = if (row + phase).is_multiple_of(97) {
            JudgeGrade::Miss
        } else {
            [
                JudgeGrade::Fantastic,
                JudgeGrade::Excellent,
                JudgeGrade::Great,
                JudgeGrade::Decent,
                JudgeGrade::WayOff,
            ][(row + phase) % 5]
        };
        notes.push(note(row, row % 4, grade, error));
        if row.is_multiple_of(4) {
            notes.push(note(row, (row + 1) % 4, grade, error * 0.5));
        }
    }
    notes
}

fn assert_stats_eq(old: TimingStats, new: TimingStats) {
    assert_eq!(old.mean_ms.to_bits(), new.mean_ms.to_bits());
    assert_eq!(old.mean_abs_ms.to_bits(), new.mean_abs_ms.to_bits());
    assert_eq!(old.max_abs_ms.to_bits(), new.max_abs_ms.to_bits());
    assert!((old.stddev_ms - new.stddev_ms).abs() <= 0.000_2);
}

fn stats_checksum(stats: TimingStats) -> u64 {
    let stddev_micros = (stats.stddev_ms * 1_000.0).round() as u32;
    u64::from(stats.mean_ms.to_bits())
        ^ u64::from(stats.mean_abs_ms.to_bits()).rotate_left(17)
        ^ u64::from(stats.max_abs_ms.to_bits()).rotate_left(33)
        ^ u64::from(stddev_micros).rotate_left(49)
}

fn assert_histogram_eq(old: &HistogramMs, new: &HistogramMs) {
    assert_eq!(old.bins, new.bins);
    assert_eq!(old.max_count, new.max_count);
    assert_eq!(
        old.worst_observed_ms.to_bits(),
        new.worst_observed_ms.to_bits()
    );
    assert_eq!(old.worst_window_ms.to_bits(), new.worst_window_ms.to_bits());
    assert_eq!(old.smoothed.len(), new.smoothed.len());
    assert!(old.smoothed.iter().zip(&new.smoothed).all(
        |(&(old_bin, old_value), &(new_bin, new_value))| {
            old_bin == new_bin && old_value.to_bits() == new_value.to_bits()
        }
    ));
}

fn histogram_checksum(histogram: &HistogramMs) -> u64 {
    let first = histogram.bins.first().copied().unwrap_or_default();
    let last = histogram.bins.last().copied().unwrap_or_default();
    histogram.bins.len() as u64
        ^ (histogram.smoothed.len() as u64).rotate_left(11)
        ^ (u64::from(first.0 as u32) << 7)
        ^ u64::from(first.1).rotate_left(23)
        ^ u64::from(last.0 as u32).rotate_left(37)
        ^ u64::from(last.1).rotate_left(51)
        ^ u64::from(histogram.max_count)
}

fn arrow_checksum(stats: &ArrowTimingStats) -> u64 {
    stats
        .per_column
        .iter()
        .chain([&stats.left_foot, &stats.right_foot])
        .fold(0u64, |checksum, bucket| {
            checksum
                .rotate_left(7)
                .wrapping_add(u64::from(bucket.count))
                ^ u64::from(bucket.stats.mean_ms.to_bits()).rotate_left(13)
                ^ u64::from(bucket.stats.mean_abs_ms.to_bits()).rotate_left(29)
                ^ u64::from(bucket.stats.stddev_ms.to_bits()).rotate_left(43)
                ^ u64::from(bucket.stats.max_abs_ms.to_bits()).rotate_left(57)
        })
}

fn assert_arrow_eq(old: &ArrowTimingStats, new: &ArrowTimingStats) {
    assert_eq!(arrow_checksum(old), arrow_checksum(new));
    assert_eq!(old.per_column.len(), new.per_column.len());
    for (old, new) in old.per_column.iter().zip(&new.per_column) {
        assert_eq!(old.count, new.count);
        assert_eq!(old.stats.mean_ms.to_bits(), new.stats.mean_ms.to_bits());
        assert_eq!(
            old.stats.mean_abs_ms.to_bits(),
            new.stats.mean_abs_ms.to_bits()
        );
        assert_eq!(old.stats.stddev_ms.to_bits(), new.stats.stddev_ms.to_bits());
        assert_eq!(
            old.stats.max_abs_ms.to_bits(),
            new.stats.max_abs_ms.to_bits()
        );
    }
}

fn scatter_checksum(scatter: &[ScatterPoint]) -> u64 {
    scatter.iter().fold(0u64, |checksum, point| {
        let foot = match point.parity_foot {
            ScatterFoot::Unknown => 0,
            ScatterFoot::Left => 1,
            ScatterFoot::Right => 2,
            ScatterFoot::Both => 3,
        };
        checksum.rotate_left(5)
            ^ u64::from(point.time_sec.to_bits())
            ^ u64::from(point.offset_ms.unwrap_or_default().to_bits()).rotate_left(11)
            ^ u64::from(point.direction_code).rotate_left(23)
            ^ (point.row_index as u64).rotate_left(37)
            ^ u64::from(point.quantization_idx).rotate_left(49)
            ^ (foot << 61)
            ^ u64::from(point.miss_because_held)
    })
}

fn assert_scatter_eq(old: &[ScatterPoint], new: &[ScatterPoint]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(old.time_sec.to_bits(), new.time_sec.to_bits());
        assert_eq!(
            old.offset_ms.map(f32::to_bits),
            new.offset_ms.map(f32::to_bits)
        );
        assert_eq!(old.direction_code, new.direction_code);
        assert_eq!(old.miss_because_held, new.miss_because_held);
        assert_eq!(old.row_index, new.row_index);
        assert_eq!(old.quantization_idx, new.quantization_idx);
        assert_eq!(old.parity_foot, new.parity_foot);
    }
}

fn main() {
    let notes_a = summary_notes(0);
    let notes_b = summary_notes(1);

    assert_stats_eq(
        bench_support::timing_stats_old(&notes_a),
        bench_support::timing_stats_new(&notes_a),
    );
    let old_stats = measure(STATS_OPS, ROWS, || {
        stats_checksum(bench_support::timing_stats_old(black_box(&notes_a)))
    });
    let new_stats = measure(STATS_OPS, ROWS, || {
        stats_checksum(bench_support::timing_stats_new(black_box(&notes_a)))
    });
    print_pair(
        "one-pass timing statistics",
        STATS_OPS,
        &old_stats,
        &new_stats,
    );

    let old_histogram_value = bench_support::histogram_old(&notes_a);
    let new_histogram_value = bench_support::histogram_new(&notes_a);
    assert_histogram_eq(&old_histogram_value, &new_histogram_value);
    let old_histogram = measure(HISTOGRAM_OPS, ROWS, || {
        let histogram = bench_support::histogram_old(black_box(&notes_a));
        black_box(&histogram);
        histogram_checksum(&histogram)
    });
    let new_histogram = measure(HISTOGRAM_OPS, ROWS, || {
        let histogram = bench_support::histogram_new(black_box(&notes_a));
        black_box(&histogram);
        histogram_checksum(&histogram)
    });
    print_pair(
        "direct timing histogram counting",
        HISTOGRAM_OPS,
        &old_histogram,
        &new_histogram,
    );

    let histograms = [
        bench_support::histogram_new(&notes_a),
        bench_support::histogram_new(&notes_b),
    ];
    let old_merge_value = bench_support::merge_old(&histograms);
    let new_merge_value = bench_support::merge_new(&histograms);
    assert_histogram_eq(&old_merge_value, &new_merge_value);
    let merged_items = histograms
        .iter()
        .flat_map(|histogram| &histogram.bins)
        .map(|&(_, count)| count as usize)
        .sum();
    let old_merge = measure(MERGE_OPS, merged_items, || {
        let histogram = bench_support::merge_old(black_box(&histograms));
        black_box(&histogram);
        histogram_checksum(&histogram)
    });
    let new_merge = measure(MERGE_OPS, merged_items, || {
        let histogram = bench_support::merge_new(black_box(&histograms));
        black_box(&histogram);
        histogram_checksum(&histogram)
    });
    print_pair(
        "direct packed-histogram merge",
        MERGE_OPS,
        &old_merge,
        &new_merge,
    );

    let evaluation_notes = summary_notes(2);
    let old_arrow_value = bench_support::arrow_stats_old(&evaluation_notes, 0, 4, None);
    let new_arrow_value = bench_support::arrow_stats_new(&evaluation_notes, 0, 4, None);
    assert_arrow_eq(&old_arrow_value, &new_arrow_value);
    let old_arrow = measure(ARROW_OPS, evaluation_notes.len(), || {
        let stats = bench_support::arrow_stats_old(black_box(&evaluation_notes), 0, 4, None);
        arrow_checksum(black_box(&stats))
    });
    let new_arrow = measure(ARROW_OPS, evaluation_notes.len(), || {
        let stats = bench_support::arrow_stats_new(black_box(&evaluation_notes), 0, 4, None);
        arrow_checksum(black_box(&stats))
    });
    print_pair(
        "fused row-selection arrow summary",
        ARROW_OPS,
        &old_arrow,
        &new_arrow,
    );

    let empty_notes: [Note; 0] = [];
    let old_empty = bench_support::arrow_stats_old(&empty_notes, 0, 4, None);
    let new_empty = bench_support::arrow_stats_new(&empty_notes, 0, 4, None);
    assert_arrow_eq(&old_empty, &new_empty);
    let old_lane_buckets = measure(LANE_BUCKET_OPS, 4, || {
        let stats = bench_support::arrow_stats_old(black_box(&empty_notes), 0, 4, None);
        arrow_checksum(black_box(&stats))
    });
    let new_lane_buckets = measure(LANE_BUCKET_OPS, 4, || {
        let stats = bench_support::arrow_stats_new(black_box(&empty_notes), 0, 4, None);
        arrow_checksum(black_box(&stats))
    });
    print_pair(
        "stack-backed lane accumulators",
        LANE_BUCKET_OPS,
        &old_lane_buckets,
        &new_lane_buckets,
    );

    let note_times: Vec<i64> = evaluation_notes
        .iter()
        .map(|note| note.row_index as i64 * 10_000_000)
        .collect();
    let row_feet: Vec<(usize, ScatterFoot)> = (0..ROWS)
        .map(|row| {
            let foot = if row.is_multiple_of(3) {
                ScatterFoot::Both
            } else if row.is_multiple_of(2) {
                ScatterFoot::Left
            } else {
                ScatterFoot::Right
            };
            (row, foot)
        })
        .collect();
    let row_foot_map: HashMap<_, _> = row_feet.iter().copied().collect();
    let old_scatter_value =
        bench_support::scatter_old(&evaluation_notes, &note_times, 0, 4, Some(&row_foot_map));
    let new_scatter_value =
        bench_support::scatter_new(&evaluation_notes, &note_times, 0, 4, Some(&row_feet));
    assert_scatter_eq(&old_scatter_value, &new_scatter_value);
    let old_scatter = measure(SCATTER_OPS, evaluation_notes.len(), || {
        let mut parity = HashMap::with_capacity(row_feet.len());
        parity.extend(black_box(row_feet.iter().copied()));
        let scatter = bench_support::scatter_old(
            black_box(&evaluation_notes),
            black_box(&note_times),
            0,
            4,
            Some(&parity),
        );
        scatter_checksum(black_box(&scatter))
    });
    let new_scatter = measure(SCATTER_OPS, evaluation_notes.len(), || {
        let mut parity = Vec::with_capacity(row_feet.len());
        parity.extend(black_box(row_feet.iter().copied()));
        let scatter = bench_support::scatter_new(
            black_box(&evaluation_notes),
            black_box(&note_times),
            0,
            4,
            Some(&parity),
        );
        scatter_checksum(black_box(&scatter))
    });
    print_pair(
        "fused scatter and compact row parity",
        SCATTER_OPS,
        &old_scatter,
        &new_scatter,
    );
}
