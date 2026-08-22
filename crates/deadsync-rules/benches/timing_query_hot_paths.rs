use deadsync_rules::timing::{
    BeatTimeCache, DelaySegment, DisplayedBeatCache, ScrollSegment, StopSegment, TimingData,
    TimingSegments, WarpSegment,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const QUERY_COUNT: usize = 4_096;
const EVENT_COUNT: usize = 64;
const OPS: usize = 200;
const SAMPLE_BATCHES: usize = 50;

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

// SAFETY: allocator operations delegate unchanged to `System`; relaxed
// counters only observe successful calls while this single-threaded bench runs.
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

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% p95  {:>7.2}% churn",
        change(old.ns_per_op, new.ns_per_op),
        change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        change(old.items_per_second, new.items_per_second),
        change(old.p95_sample_ns, new.p95_sample_ns),
        change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let count = OPS as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>12.2} cycles/op  {:>11.2} p95 ns  \
         {:>8.2} Mitem/s  {:>5.2} alloc/op  {:>5.2} realloc/op  {:>5.2} free/op  {:>9.1} churn B/op",
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

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn timing() -> TimingData {
    let segments = TimingSegments {
        bpms: (0..EVENT_COUNT)
            .map(|index| (index as f32 * 16.0, 90.0 + (index % 7) as f32 * 23.0))
            .collect(),
        stops: (0..EVENT_COUNT)
            .map(|index| StopSegment {
                beat: index as f32 * 16.0 + 4.0,
                duration: 0.01 + (index % 5) as f32 * 0.005,
            })
            .collect(),
        delays: (0..EVENT_COUNT)
            .map(|index| DelaySegment {
                beat: index as f32 * 16.0 + 8.0,
                duration: 0.005 + (index % 3) as f32 * 0.005,
            })
            .collect(),
        warps: (0..EVENT_COUNT)
            .map(|index| WarpSegment {
                beat: index as f32 * 16.0 + 12.0,
                length: 1.0,
            })
            .collect(),
        scrolls: (0..EVENT_COUNT)
            .map(|index| ScrollSegment {
                beat: index as f32 * 16.0,
                ratio: 0.5 + (index % 7) as f32 * 0.25,
            })
            .collect(),
        ..TimingSegments::default()
    };
    TimingData::from_segments(0.037, -0.011, &segments, &[])
}

#[inline(never)]
fn old_monotonic(timing: &TimingData) -> u64 {
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        sum.rotate_left(5) ^ timing.get_time_for_beat_ns(black_box(beat)) as u64
    })
}

#[inline(never)]
fn new_monotonic(timing: &TimingData) -> u64 {
    let mut cache = BeatTimeCache::new(timing);
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        sum.rotate_left(5) ^ timing.get_time_for_beat_ns_cached(black_box(beat), &mut cache) as u64
    })
}

#[inline(never)]
fn old_gameplay_timing(timing: &TimingData) -> u64 {
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        let sum = sum.rotate_left(5) ^ timing.get_time_for_beat_ns(black_box(beat)) as u64;
        if index % 8 == 0 {
            let tail = beat + 1.0 + (index % 32) as f32 * 0.25;
            sum.rotate_left(7) ^ timing.get_time_for_beat_ns(black_box(tail)) as u64
        } else {
            sum
        }
    })
}

#[inline(never)]
fn new_gameplay_timing(timing: &TimingData) -> u64 {
    let mut head_cache = BeatTimeCache::new(timing);
    let mut tail_cache = BeatTimeCache::new(timing);
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        let sum = sum.rotate_left(5)
            ^ timing.get_time_for_beat_ns_cached(black_box(beat), &mut head_cache) as u64;
        if index % 8 == 0 {
            let tail = beat + 1.0 + (index % 32) as f32 * 0.25;
            sum.rotate_left(7)
                ^ timing.get_time_for_beat_ns_cached(black_box(tail), &mut tail_cache) as u64
        } else {
            sum
        }
    })
}

#[inline(never)]
fn old_displayed(timing: &TimingData) -> u64 {
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        sum.rotate_left(5) ^ u64::from(timing.get_displayed_beat(black_box(beat)).to_bits())
    })
}

#[inline(never)]
fn new_displayed(timing: &TimingData) -> u64 {
    let mut cache = DisplayedBeatCache::new();
    (0..QUERY_COUNT).fold(0u64, |sum, index| {
        let beat = index as f32 * 0.25;
        sum.rotate_left(5)
            ^ u64::from(
                timing
                    .get_displayed_beat_cached(black_box(beat), &mut cache)
                    .to_bits(),
            )
    })
}

fn main() {
    let timing = timing();
    let mut verify_cache = BeatTimeCache::new(&timing);
    for index in 0..QUERY_COUNT {
        let beat = index as f32 * 0.25;
        assert_eq!(
            timing.get_time_for_beat_ns_cached(beat, &mut verify_cache),
            timing.get_time_for_beat_ns(beat),
            "beat-time parity at index {index}, beat {beat}",
        );
    }

    let old = measure(OPS, QUERY_COUNT, || old_monotonic(black_box(&timing)));
    let new = measure(OPS, QUERY_COUNT, || new_monotonic(black_box(&timing)));
    print_pair("monotonic complex beat times", &old, &new);

    let query_count_with_holds = QUERY_COUNT + QUERY_COUNT / 8;
    let old = measure(OPS, query_count_with_holds, || {
        old_gameplay_timing(black_box(&timing))
    });
    let new = measure(OPS, query_count_with_holds, || {
        new_gameplay_timing(black_box(&timing))
    });
    print_pair("gameplay note and hold-tail times", &old, &new);

    let old = measure(OPS, QUERY_COUNT, || old_displayed(black_box(&timing)));
    let new = measure(OPS, QUERY_COUNT, || new_displayed(black_box(&timing)));
    print_pair("monotonic displayed beats", &old, &new);
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC read the x86 timestamp counter without memory access.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC read the x86-64 timestamp counter without memory access.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
