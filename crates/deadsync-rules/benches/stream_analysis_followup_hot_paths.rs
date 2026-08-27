use deadsync_rules::stream::bench_support::zmod_fused_growth;
use deadsync_rules::stream::{
    StreamSegment, stream_measure_densities, stream_run_progress, stream_sequences_threshold,
    zmod_stream_totals_full_measures,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const MEASURE_COUNT: usize = 16_384;
const NOTE_MEASURES: usize = 2_048;
const FAIL_MEASURE: usize = 1_023;
const FAIL_OPS: usize = 1_000;
const FOLD_OPS: usize = 2_000;
const SIZE_OPS: usize = 1_000;
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

fn note_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(NOTE_MEASURES * 32 * 5);
    for measure in 0..NOTE_MEASURES {
        let density = if (FAIL_MEASURE - 5..=FAIL_MEASURE + 6).contains(&measure) {
            20
        } else {
            (measure * 17 + measure / 11) % 18
        };
        for row in 0..32 {
            let mut cells = [b'0'; 4];
            if row < density {
                cells[(row + measure) % 4] = b'1';
            }
            data.extend_from_slice(&cells);
            data.push(b'\n');
        }
        data.extend_from_slice(if measure + 1 == NOTE_MEASURES {
            b";\n"
        } else {
            b",\n"
        });
    }
    data
}

fn folded_measures() -> Vec<u8> {
    (0..MEASURE_COUNT)
        .map(|idx| match idx % 11 {
            0..=2 | 5 | 8..=9 => (16 + idx % 7) as u8,
            _ => (idx % 13) as u8,
        })
        .collect()
}

fn sized_measures() -> Vec<u8> {
    (0..MEASURE_COUNT)
        .map(|idx| if idx % 2 == 0 { 32 } else { 0 })
        .collect()
}

fn fail_progress_old(notes: &[u8], current_measure: usize) -> Option<(usize, usize)> {
    let densities = stream_measure_densities(notes, 4);
    if densities.get(current_measure).copied().unwrap_or(0) < 16 {
        return None;
    }
    let segment = stream_sequences_threshold(&densities, 16)
        .into_iter()
        .find(|segment| {
            !segment.is_break()
                && current_measure >= segment.start() as usize
                && current_measure < segment.end() as usize
        })?;
    Some((
        current_measure - segment.start() as usize + 1,
        (segment.end() - segment.start()) as usize,
    ))
}

fn zmod_scan_reference(
    measures: &[u8],
    threshold: usize,
    multiplier: f32,
) -> (Vec<StreamSegment>, f32, f32) {
    let segments = stream_sequences_threshold(measures, threshold);
    let mut total_stream = 0.0_f32;
    let mut total_break = 0.0_f32;
    let mut edge_break = 0.0_f32;
    let mut last_stream = false;
    for (index, segment) in segments.iter().enumerate() {
        let len = segment.end().saturating_sub(segment.start()) as f32;
        if segment.is_break() && index > 0 && index + 1 < segments.len() {
            total_break += len;
            last_stream = false;
        } else if segment.is_break() {
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
    (
        segments,
        total_stream * multiplier,
        total_break * multiplier,
    )
}

fn option_checksum(value: Option<(usize, usize)>) -> u64 {
    value.map_or(0, |(run, total)| {
        (run as u64).rotate_left(17) ^ (total as u64).rotate_left(43)
    })
}

fn totals_checksum(value: &(Vec<StreamSegment>, f32, f32)) -> u64 {
    value.0.iter().fold(0u64, |checksum, segment| {
        checksum.rotate_left(9)
            ^ (segment.start() as u64)
            ^ (segment.end() as u64).rotate_left(23)
            ^ u64::from(segment.is_break()).rotate_left(47)
    }) ^ u64::from(value.1.to_bits()).rotate_left(17)
        ^ u64::from(value.2.to_bits()).rotate_left(41)
}

fn main() {
    let notes = note_data();
    assert_eq!(
        fail_progress_old(&notes, FAIL_MEASURE),
        stream_run_progress(&notes, 4, 16, FAIL_MEASURE)
    );
    let old_fail = measure(FAIL_OPS, NOTE_MEASURES, || {
        option_checksum(fail_progress_old(black_box(&notes), FAIL_MEASURE))
    });
    let new_fail = measure(FAIL_OPS, NOTE_MEASURES, || {
        option_checksum(stream_run_progress(black_box(&notes), 4, 16, FAIL_MEASURE))
    });
    print_pair(
        "allocation-free failed-stream query",
        FAIL_OPS,
        &old_fail,
        &new_fail,
    );

    let measures = folded_measures();
    let old_fold_value = zmod_scan_reference(&measures, 16, 1.0);
    let new_fold_value = zmod_fused_growth(&measures, false);
    assert_eq!(
        totals_checksum(&old_fold_value),
        totals_checksum(&new_fold_value)
    );
    let old_fold = measure(FOLD_OPS, MEASURE_COUNT, || {
        totals_checksum(&zmod_scan_reference(black_box(&measures), 16, 1.0))
    });
    let new_fold = measure(FOLD_OPS, MEASURE_COUNT, || {
        totals_checksum(&zmod_fused_growth(black_box(&measures), false))
    });
    print_pair(
        "single-pass segment total fold",
        FOLD_OPS,
        &old_fold,
        &new_fold,
    );

    let measures = sized_measures();
    let old_size_value = zmod_fused_growth(&measures, true);
    let new_size_value = zmod_stream_totals_full_measures(&measures, true);
    assert_eq!(
        totals_checksum(&old_size_value),
        totals_checksum(&new_size_value)
    );
    let old_size = measure(SIZE_OPS, MEASURE_COUNT, || {
        totals_checksum(&zmod_fused_growth(black_box(&measures), true))
    });
    let new_size = measure(SIZE_OPS, MEASURE_COUNT, || {
        totals_checksum(&zmod_stream_totals_full_measures(
            black_box(&measures),
            true,
        ))
    });
    print_pair(
        "exact constant-BPM segment sizing",
        SIZE_OPS,
        &old_size,
        &new_size,
    );
}
