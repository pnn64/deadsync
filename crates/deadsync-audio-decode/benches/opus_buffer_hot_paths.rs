use deadsync_audio_decode::opus::bench_support::direct_target;
use deadsync_audio_decode::resample::drop_front_samples;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKET_SAMPLES: usize = 5_760 * 2;
const SEEK_PACKETS: usize = 8;
const DIRECT_RUNS: usize = 10_000;
const SEEK_RUNS: usize = 2_000;
const TAIL_RUNS: usize = 10_000;
const WORST_SAMPLES: usize = 32;

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

// SAFETY: every allocator operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_unit: f64,
    cycles_per_unit: Option<f64>,
    units_per_second: f64,
    worst_ns_per_unit: f64,
    allocated: AllocSnapshot,
    checksum: u64,
    runs: usize,
}

fn measure(runs: usize, units_per_run: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    let sample_runs = (runs / 20).max(1);
    for _ in 0..sample_runs {
        black_box(operation());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..runs {
        checksum = checksum.rotate_left(5) ^ black_box(operation());
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..runs {
        black_box(operation());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    let mut worst_ns_per_unit = 0.0f64;
    for _ in 0..WORST_SAMPLES {
        let started = Instant::now();
        for _ in 0..sample_runs {
            black_box(operation());
        }
        worst_ns_per_unit = worst_ns_per_unit.max(
            started.elapsed().as_secs_f64() * 1_000_000_000.0
                / (sample_runs * units_per_run) as f64,
        );
    }

    let units = (runs * units_per_run) as f64;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_unit: seconds * 1_000_000_000.0 / units,
        cycles_per_unit: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / units),
        units_per_second: units / seconds,
        worst_ns_per_unit,
        allocated,
        checksum,
        runs,
    }
}

fn print_pair(name: &str, unit: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", unit, old);
    print_result("new", unit, new);
    assert_eq!(new.checksum, old.checksum, "{name} output diverged");
    assert_eq!(new.allocated.operations(), 0, "{name} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{name} new path churned");
}

fn print_result(label: &str, unit: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>9.3} ns/{unit}  {:>9.3} cycles/{unit}  {:>9.3} M{unit}/s  \
         worst {:>9.3} ns  {:>5.2} alloc/run  {:>5.2} realloc/run  \
         {:>5.2} free/run  {:>10.1} churn B/run  {:016x}",
        result.ns_per_unit,
        result.cycles_per_unit.unwrap_or(f64::NAN),
        result.units_per_second / 1_000_000.0,
        result.worst_ns_per_unit,
        result.allocated.allocs as f64 / result.runs as f64,
        result.allocated.reallocs as f64 / result.runs as f64,
        result.allocated.frees as f64 / result.runs as f64,
        result.allocated.churn_bytes() as f64 / result.runs as f64,
        result.checksum,
    );
}

fn output_checksum(output: &[i16]) -> u64 {
    let output = black_box(output);
    output.len() as u64
        ^ output.first().copied().unwrap_or_default() as u16 as u64
        ^ ((output.get(output.len() / 2).copied().unwrap_or_default() as u16 as u64) << 16)
        ^ ((output.last().copied().unwrap_or_default() as u16 as u64) << 32)
}

fn handoff_old(decoded: &[u16], out: &mut Vec<i16>) {
    out.clear();
    out.reserve(decoded.len());
    out.extend(decoded.iter().map(|sample| *sample as i16));
}

fn main() {
    let source = (0..PACKET_SAMPLES)
        .map(|index| index.wrapping_mul(25_173) as u16)
        .collect::<Vec<_>>();
    let signed_source = source
        .iter()
        .map(|sample| *sample as i16)
        .collect::<Vec<_>>();

    let mut old_decode = vec![0u16; PACKET_SAMPLES];
    let mut old_out = Vec::with_capacity(PACKET_SAMPLES);
    let mut new_out = Vec::with_capacity(PACKET_SAMPLES);
    let old = measure(DIRECT_RUNS, PACKET_SAMPLES, || {
        old_decode.copy_from_slice(black_box(&source));
        handoff_old(&old_decode, &mut old_out);
        output_checksum(&old_out)
    });
    let new = measure(DIRECT_RUNS, PACKET_SAMPLES, || {
        direct_target(&mut new_out, PACKET_SAMPLES).copy_from_slice(black_box(&source));
        output_checksum(&new_out)
    });
    print_pair("direct Opus decode output", "sample", &old, &new);

    let old = measure(SEEK_RUNS, 1, || {
        let mut checksum = 0u64;
        for _ in 0..SEEK_PACKETS {
            let mut decoded = Vec::new();
            handoff_old(black_box(&source), &mut decoded);
            checksum = checksum.rotate_left(7) ^ output_checksum(&decoded);
        }
        checksum
    });
    let mut seek_scratch = Vec::with_capacity(PACKET_SAMPLES);
    let new = measure(SEEK_RUNS, 1, || {
        let mut checksum = 0u64;
        for _ in 0..SEEK_PACKETS {
            direct_target(&mut seek_scratch, PACKET_SAMPLES).copy_from_slice(black_box(&source));
            checksum = checksum.rotate_left(7) ^ output_checksum(&seek_scratch);
        }
        checksum
    });
    print_pair("eight discarded seek packets", "seek", &old, &new);

    let old = measure(TAIL_RUNS, 1, || {
        let tail = black_box(&signed_source)[..].to_vec();
        output_checksum(&tail)
    });
    let mut retained_tail = Vec::with_capacity(PACKET_SAMPLES);
    let new = measure(TAIL_RUNS, 1, || {
        retained_tail.clear();
        retained_tail.extend_from_slice(black_box(&signed_source));
        drop_front_samples(&mut retained_tail, 0);
        output_checksum(&retained_tail)
    });
    print_pair("zero-offset accepted seek tail", "seek", &old, &new);
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
