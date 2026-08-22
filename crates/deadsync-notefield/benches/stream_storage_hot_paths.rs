use deadsync_gameplay::partition_point_from_hint;
use deadsync_notefield::{BrokenRunLookup, StreamProgressLookup};
use deadsync_rules::stream::{StreamSegment, stream_sequences_threshold};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MEASURE_COUNT: usize = 16_384;
const SEGMENT_OPS: usize = 3_000;
const LOOKUP_OPS: usize = 10_000;
const SAMPLE_BATCHES: usize = 100;
const QUERY_COUNT: usize = 64;

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
// only observe successful calls while the single-threaded benchmark gate is on.
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
    p95_sample_ns: f64,
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
    let mut sample_ns = [0.0f64; SAMPLE_BATCHES];
    for sample in &mut sample_ns {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        *sample = sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / batch as f64;
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
    sample_ns.sort_unstable_by(f64::total_cmp);
    BenchResult {
        ns_per_op: seconds * 1_000_000_000.0 / iterations as f64,
        p95_sample_ns: sample_ns[SAMPLE_BATCHES * 95 / 100],
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
        percent_change(old.p95_sample_ns, new.p95_sample_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let count = iterations as f64;
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} p95 ns    \
         {:>8.2} Mitem/s  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>10.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_sample_ns,
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

#[derive(Clone, Copy)]
struct OldStreamSegment {
    start: u32,
    end: u32,
    is_break: bool,
}

#[derive(Default)]
struct OldStreamRuns {
    start: Option<usize>,
    end: usize,
}

impl OldStreamRuns {
    fn record(&mut self, index: usize, output: &mut Vec<OldStreamSegment>) {
        match self.start {
            None => {
                if index >= 2 {
                    output.push(old_segment(0, index, true));
                }
                self.start = Some(index);
                self.end = index + 1;
            }
            Some(_) if index == self.end => self.end += 1,
            Some(start) => {
                output.push(old_segment(start, self.end, false));
                if index >= self.end + 2 {
                    output.push(old_segment(self.end, index, true));
                }
                self.start = Some(index);
                self.end = index + 1;
            }
        }
    }

    fn finish(self, measure_count: usize, output: &mut Vec<OldStreamSegment>) {
        let Some(start) = self.start else { return };
        output.push(old_segment(start, self.end, false));
        if measure_count >= self.end + 2 {
            output.push(old_segment(self.end, measure_count, true));
        }
    }
}

fn old_segment(start: usize, end: usize, is_break: bool) -> OldStreamSegment {
    OldStreamSegment {
        start: u32::try_from(start).expect("benchmark segment start exceeds u32"),
        end: u32::try_from(end).expect("benchmark segment end exceeds u32"),
        is_break,
    }
}

fn old_stream_segments(measures: &[u8], threshold: usize) -> Vec<OldStreamSegment> {
    let mut output = Vec::new();
    let mut runs = OldStreamRuns::default();
    for (index, &density) in measures.iter().enumerate() {
        if usize::from(density) >= threshold {
            runs.record(index, &mut output);
        }
    }
    runs.finish(measures.len(), &mut output);
    output
}

fn old_segment_checksum(segments: &[OldStreamSegment]) -> u64 {
    segments.iter().fold(0, |checksum, segment| {
        checksum.rotate_left(9)
            ^ u64::from(segment.start)
            ^ u64::from(segment.end).rotate_left(23)
            ^ u64::from(segment.is_break).rotate_left(47)
    })
}

fn segment_checksum(segments: &[StreamSegment]) -> u64 {
    segments.iter().fold(0, |checksum, segment| {
        checksum.rotate_left(9)
            ^ u64::from(segment.start())
            ^ u64::from(segment.end()).rotate_left(23)
            ^ u64::from(segment.is_break()).rotate_left(47)
    })
}

#[derive(Clone, Copy)]
struct OldProgressEntry {
    stream_before: f64,
    start: f32,
    end: f32,
    is_break: bool,
}

struct OldProgressLookup {
    segments: Box<[OldProgressEntry]>,
    cursor: Cell<usize>,
}

impl OldProgressLookup {
    fn new(segments: &[StreamSegment]) -> Self {
        let mut stream_before = 0.0;
        let segments = segments
            .iter()
            .map(|segment| {
                let entry = OldProgressEntry {
                    stream_before,
                    start: segment.start() as f32,
                    end: segment.end() as f32,
                    is_break: segment.is_break(),
                };
                if !segment.is_break() {
                    stream_before += f64::from(segment.end() - segment.start());
                }
                entry
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            segments,
            cursor: Cell::new(0),
        }
    }

    fn completion_for_beat(&self, total_stream_measures: f64, beat_floor: f32) -> Option<f64> {
        if total_stream_measures <= 0.0 || self.segments.is_empty() {
            return None;
        }
        let current = if beat_floor.is_finite() {
            (beat_floor / 4.0).ceil().max(0.0)
        } else {
            0.0
        };
        let index = partition_point_from_hint(&self.segments, self.cursor.get(), |segment| {
            current >= segment.end
        });
        self.cursor.set(index);
        let Some(segment) = self.segments.get(index) else {
            let completed = self.segments.last().map_or(0.0, |last| {
                last.stream_before
                    + if last.is_break {
                        0.0
                    } else {
                        f64::from(last.end - last.start)
                    }
            });
            return Some((completed / total_stream_measures).clamp(0.0, 1.0));
        };
        let partial = if segment.is_break || current <= segment.start {
            0.0
        } else {
            f64::from(current.min(segment.end) - segment.start)
        };
        Some(((segment.stream_before + partial) / total_stream_measures).clamp(0.0, 1.0))
    }
}

#[derive(Clone, Copy)]
struct OldBrokenRunSpan {
    end: f32,
    segment_index: usize,
    broken_end: i32,
    broken: bool,
}

struct OldBrokenRunLookup {
    spans: Box<[OldBrokenRunSpan]>,
    cursor: Cell<usize>,
}

impl OldBrokenRunLookup {
    fn new(segments: &[StreamSegment]) -> Self {
        let mut spans = Vec::with_capacity(segments.len());
        let mut index = 0;
        while let Some(segment) = segments.get(index).copied() {
            if segment.is_break() {
                spans.push(OldBrokenRunSpan {
                    end: segment.end() as f32,
                    segment_index: index,
                    broken_end: segment.end() as i32,
                    broken: false,
                });
                index += 1;
                continue;
            }
            let (broken_end, broken, next_index) = old_broken_run(segments, index);
            spans.push(OldBrokenRunSpan {
                end: broken_end as f32,
                segment_index: index,
                broken_end,
                broken,
            });
            index = next_index.max(index + 1);
        }
        Self {
            spans: spans.into_boxed_slice(),
            cursor: Cell::new(0),
        }
    }

    fn segment(&self, current_measure: f32) -> Option<(usize, i32, bool)> {
        if current_measure.is_nan() {
            return None;
        }
        let index = partition_point_from_hint(&self.spans, self.cursor.get(), |span| {
            current_measure >= span.end
        });
        self.cursor.set(index);
        self.spans
            .get(index)
            .map(|span| (span.segment_index, span.broken_end, span.broken))
    }
}

fn old_broken_run(segments: &[StreamSegment], start_index: usize) -> (i32, bool, usize) {
    let Some(first) = segments.get(start_index).copied() else {
        return (0, false, segments.len());
    };
    if first.is_break() {
        return (first.end() as i32, false, start_index.saturating_add(1));
    }
    let last_index = segments.len().saturating_sub(1);
    let mut end = first.end();
    let mut broken = false;
    let mut index = start_index + 1;
    while index < segments.len() {
        let segment = segments[index];
        let len = segment.end() - segment.start();
        if segment.is_break() {
            if len < 4 && index != last_index {
                end += len;
                broken = true;
                index += 1;
                continue;
            }
            break;
        }
        broken = true;
        end += len;
        if !segments[index - 1].is_break() {
            end += 1;
        }
        index += 1;
    }
    (end as i32, broken, index)
}

fn measures() -> Vec<u8> {
    (0..MEASURE_COUNT)
        .map(|index| match index % 14 {
            0..=2 | 7..=9 => 24,
            _ => 4,
        })
        .collect()
}

fn queries() -> [f32; QUERY_COUNT] {
    std::array::from_fn(|index| {
        -16.0 + (MEASURE_COUNT as f32 * 4.0 + 32.0) * index as f32 / (QUERY_COUNT - 1) as f32
    })
}

fn total_stream(segments: &[StreamSegment]) -> f64 {
    segments
        .iter()
        .filter(|segment| !segment.is_break())
        .map(|segment| f64::from(segment.end() - segment.start()))
        .sum()
}

fn old_progress_checksum(lookup: &OldProgressLookup, total_stream: f64, queries: &[f32]) -> u64 {
    queries.iter().fold(0, |checksum, &query| {
        checksum.rotate_left(7)
            ^ lookup
                .completion_for_beat(total_stream, query)
                .unwrap_or_default()
                .to_bits()
    })
}

fn progress_checksum(lookup: &StreamProgressLookup, total_stream: f64, queries: &[f32]) -> u64 {
    queries.iter().fold(0, |checksum, &query| {
        checksum.rotate_left(7)
            ^ lookup
                .completion_for_beat(total_stream, query)
                .unwrap_or_default()
                .to_bits()
    })
}

fn old_broken_checksum(lookup: &OldBrokenRunLookup, queries: &[f32]) -> u64 {
    queries.iter().fold(0, |checksum, &query| {
        let (index, end, broken) = lookup.segment(query).unwrap_or_default();
        checksum.rotate_left(7)
            ^ index as u64
            ^ (end as u64).rotate_left(21)
            ^ u64::from(broken).rotate_left(47)
    })
}

fn broken_checksum(lookup: &BrokenRunLookup, queries: &[f32]) -> u64 {
    queries.iter().fold(0, |checksum, &query| {
        let (index, end, broken) = lookup.segment(query).unwrap_or_default();
        checksum.rotate_left(7)
            ^ index as u64
            ^ (end as u64).rotate_left(21)
            ^ u64::from(broken).rotate_left(47)
    })
}

fn main() {
    let measures = measures();
    let old_segments_value = old_stream_segments(&measures, 20);
    let segments = stream_sequences_threshold(&measures, 20);
    assert_eq!(
        old_segment_checksum(&old_segments_value),
        segment_checksum(&segments)
    );
    assert_eq!(std::mem::size_of::<OldStreamSegment>(), 12);
    assert_eq!(std::mem::size_of::<StreamSegment>(), 8);
    let old_segments = measure(SEGMENT_OPS, MEASURE_COUNT, || {
        old_segment_checksum(&old_stream_segments(black_box(&measures), 20))
    });
    let new_segments = measure(SEGMENT_OPS, MEASURE_COUNT, || {
        segment_checksum(&stream_sequences_threshold(black_box(&measures), 20))
    });
    print_pair(
        "packed stream segment flags",
        SEGMENT_OPS,
        &old_segments,
        &new_segments,
    );

    let total_stream = total_stream(&segments);
    let queries = queries();
    let old_progress_value = OldProgressLookup::new(&segments);
    let progress_value = StreamProgressLookup::new(&segments);
    assert_eq!(
        old_progress_checksum(&old_progress_value, total_stream, &queries),
        progress_checksum(&progress_value, total_stream, &queries)
    );
    assert_eq!(std::mem::size_of::<OldProgressEntry>(), 24);
    assert_eq!(progress_value.storage_bytes(), segments.len() * 16);
    let old_progress = measure(LOOKUP_OPS, segments.len() + QUERY_COUNT, || {
        let lookup = OldProgressLookup::new(black_box(&segments));
        old_progress_checksum(&lookup, total_stream, &queries)
    });
    let new_progress = measure(LOOKUP_OPS, segments.len() + QUERY_COUNT, || {
        let lookup = StreamProgressLookup::new(black_box(&segments));
        progress_checksum(&lookup, total_stream, &queries)
    });
    print_pair(
        "compact stream progress entries",
        LOOKUP_OPS,
        &old_progress,
        &new_progress,
    );

    let old_broken_value = OldBrokenRunLookup::new(&segments);
    let broken_value = BrokenRunLookup::new(&segments);
    assert_eq!(
        old_broken_checksum(&old_broken_value, &queries),
        broken_checksum(&broken_value, &queries)
    );
    assert_eq!(std::mem::size_of::<OldBrokenRunSpan>(), 24);
    assert_eq!(
        broken_value.storage_bytes(),
        old_broken_value.spans.len() * 8
    );
    let old_broken = measure(LOOKUP_OPS, segments.len() + QUERY_COUNT, || {
        let lookup = OldBrokenRunLookup::new(black_box(&segments));
        old_broken_checksum(&lookup, &queries)
    });
    let new_broken = measure(LOOKUP_OPS, segments.len() + QUERY_COUNT, || {
        let lookup = BrokenRunLookup::new(black_box(&segments));
        broken_checksum(&lookup, &queries)
    });
    print_pair(
        "packed broken-run spans",
        LOOKUP_OPS,
        &old_broken,
        &new_broken,
    );
}
