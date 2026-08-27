use deadsync_shell::{BenchmarkMediaPrepDispatch, benchmark_media_completion_budget};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const COMPLETIONS: usize = 4_096;
const PAYLOAD_BYTES: usize = 1_024;
const SAMPLES: usize = 31;
const DISPATCH_SAMPLES: usize = 101;
const DISPATCH_JOBS: usize = 8;

struct Completion {
    sequence: u64,
    payload: Box<[u8]>,
}

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters only observe the single-threaded benchmark's gated interval.
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
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct DrainSample {
    ns: f64,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    processed: usize,
    checksum: u64,
}

fn completion_queue() -> Receiver<Completion> {
    let (tx, rx) = mpsc::channel();
    for sequence in 0..COMPLETIONS as u64 {
        tx.send(Completion {
            sequence,
            payload: vec![sequence as u8; PAYLOAD_BYTES].into_boxed_slice(),
        })
        .unwrap();
    }
    rx
}

fn consume(completion: Completion) -> u64 {
    completion.sequence ^ completion.payload.first().copied().map_or(0, u64::from)
}

fn drain_frame(rx: &Receiver<Completion>, limit: usize) -> (usize, u64) {
    let mut processed = 0usize;
    let mut checksum = 0u64;
    for _ in 0..limit {
        let Ok(completion) = rx.try_recv() else {
            break;
        };
        processed += 1;
        checksum = checksum.wrapping_add(black_box(consume(completion)));
    }
    (processed, checksum)
}

fn sample_first_frame(limit: usize) -> DrainSample {
    let rx = completion_queue();
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let (processed, checksum) = drain_frame(&rx, limit);
    let ns = started.elapsed().as_secs_f64() * 1e9;
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before);

    let mut eventual_processed = processed;
    let mut eventual_checksum = checksum;
    while eventual_processed < COMPLETIONS {
        let (count, next) = drain_frame(&rx, limit);
        assert!(count > 0);
        eventual_processed += count;
        eventual_checksum = eventual_checksum.wrapping_add(next);
    }
    assert_eq!(eventual_processed, COMPLETIONS);
    black_box(eventual_checksum);

    DrainSample {
        ns,
        cycles: cycle_start.zip(cycle_end).map(|(start, end)| end - start),
        alloc,
        processed,
        checksum,
    }
}

fn median_sample(limit: usize) -> DrainSample {
    let mut samples: Vec<DrainSample> = (0..SAMPLES).map(|_| sample_first_frame(limit)).collect();
    samples.sort_by(|a, b| a.ns.total_cmp(&b.ns));
    samples[SAMPLES / 2]
}

fn eventual_drain(limit: usize) -> (f64, Option<u64>, usize, u64) {
    let rx = completion_queue();
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut frames = 0usize;
    let mut processed = 0usize;
    let mut checksum = 0u64;
    while processed < COMPLETIONS {
        let (count, next) = drain_frame(&rx, limit);
        assert!(count > 0);
        processed += count;
        checksum = checksum.wrapping_add(next);
        frames += 1;
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9;
    let cycle_end = cycle_counter();
    (
        elapsed,
        cycle_start.zip(cycle_end).map(|(start, end)| end - start),
        frames,
        checksum,
    )
}

fn main() {
    benchmark_dispatch();

    let budget = benchmark_media_completion_budget();
    let old = median_sample(COMPLETIONS);
    let new = median_sample(budget);
    assert_eq!(old.processed, COMPLETIONS);
    assert_eq!(new.processed, budget);

    println!("dynamic-media completion burst ({COMPLETIONS} ready results)");
    print_first("old", old);
    print_first("new", new);
    println!(
        "  first-frame change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% churn",
        change(old.ns, new.ns),
        change(
            old.cycles.map_or(f64::NAN, |value| value as f64),
            new.cycles.map_or(f64::NAN, |value| value as f64),
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );

    let old_total = eventual_drain(COMPLETIONS);
    let new_total = eventual_drain(budget);
    assert_eq!(old_total.3, new_total.3);
    println!("  eventual drain (identical checksum)");
    print_total("old", old_total);
    print_total("new", new_total);
}

#[derive(Clone, Copy)]
struct DispatchSample {
    ns: f64,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
}

fn benchmark_dispatch() {
    let mut old = Vec::with_capacity(DISPATCH_SAMPLES);
    let mut cold = Vec::with_capacity(DISPATCH_SAMPLES);
    let mut warm = Vec::with_capacity(DISPATCH_SAMPLES);
    let mut warm_worker = BenchmarkMediaPrepDispatch::new();
    warm_worker.warm();
    for sample in 0..DISPATCH_SAMPLES {
        if sample % 2 == 0 {
            old.push(sample_old_dispatch());
            cold.push(sample_new_dispatch(&mut BenchmarkMediaPrepDispatch::new()));
            warm.push(sample_new_dispatch(&mut warm_worker));
        } else {
            warm.push(sample_new_dispatch(&mut warm_worker));
            cold.push(sample_new_dispatch(&mut BenchmarkMediaPrepDispatch::new()));
            old.push(sample_old_dispatch());
        }
    }

    println!("dynamic-media request burst ({DISPATCH_JOBS} preparations)");
    print_dispatch("old per-request threads", &old);
    print_dispatch("new cold bounded pool", &cold);
    print_dispatch("new warm bounded pool", &warm);
}

fn sample_old_dispatch() -> DispatchSample {
    let (tx, rx) = mpsc::sync_channel(DISPATCH_JOBS);
    let mut workers = Vec::with_capacity(DISPATCH_JOBS);
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    for job in 0..DISPATCH_JOBS {
        let tx = tx.clone();
        workers.push(std::thread::spawn(move || tx.send(job).unwrap()));
    }
    let ns = started.elapsed().as_secs_f64() * 1e9;
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before);
    drop(tx);
    let checksum = rx.iter().sum::<usize>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(checksum, (0..DISPATCH_JOBS).sum::<usize>());
    DispatchSample {
        ns,
        cycles: cycle_start.zip(cycle_end).map(|(start, end)| end - start),
        alloc,
    }
}

fn sample_new_dispatch(worker: &mut BenchmarkMediaPrepDispatch) -> DispatchSample {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let submitted = worker.submit_burst();
    let ns = started.elapsed().as_secs_f64() * 1e9;
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before);
    assert_eq!(submitted, DISPATCH_JOBS);
    assert_eq!(
        worker.drain_burst(),
        (0..DISPATCH_JOBS).fold(0, |sum, job| sum ^ job)
    );
    DispatchSample {
        ns,
        cycles: cycle_start.zip(cycle_end).map(|(start, end)| end - start),
        alloc,
    }
}

fn print_dispatch(label: &str, samples: &[DispatchSample]) {
    let mut ns = samples.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    ns.sort_by(f64::total_cmp);
    let mut cycles = samples
        .iter()
        .filter_map(|sample| sample.cycles)
        .collect::<Vec<_>>();
    cycles.sort_unstable();
    let median_alloc = {
        let mut alloc = samples.to_vec();
        alloc.sort_by(|a, b| a.ns.total_cmp(&b.ns));
        alloc[alloc.len() / 2].alloc
    };
    println!(
        "  {label:<23} ns p50/p95/p99/worst {:>9.2}/{:>9.2}/{:>9.2}/{:>9.2}  \
         cycles {:>8}/{:>8}/{:>8}/{:>8}  median alloc/churn {:>3}/{:>7} B",
        percentile(&ns, 50),
        percentile(&ns, 95),
        percentile(&ns, 99),
        *ns.last().unwrap(),
        percentile(&cycles, 50),
        percentile(&cycles, 95),
        percentile(&cycles, 99),
        *cycles.last().unwrap_or(&0),
        median_alloc.allocs,
        median_alloc.churn(),
    );
}

const fn percentile<T: Copy>(sorted: &[T], percentile: usize) -> T {
    sorted[(sorted.len() - 1) * percentile / 100]
}

fn print_first(label: &str, sample: DrainSample) {
    println!(
        "  {label:<3} {:>12.2} ns/frame  {:>12.2} cycles/frame  {:>7} integrated  \
         {:>7} alloc  {:>7} free  {:>12} churn B",
        sample.ns,
        sample.cycles.map_or(f64::NAN, |value| value as f64),
        sample.processed,
        sample.alloc.allocs,
        sample.alloc.frees,
        sample.alloc.churn(),
    );
    black_box(sample.checksum);
}

fn print_total(label: &str, result: (f64, Option<u64>, usize, u64)) {
    println!(
        "  {label:<3} {:>12.2} ns/burst  {:>12.2} cycles/burst  {:>7} frames  {:>8.3} Mresults/s",
        result.0,
        result.1.map_or(f64::NAN, |value| value as f64),
        result.2,
        COMPLETIONS as f64 * 1_000.0 / result.0,
    );
    black_box(result.3);
}

fn change(old: f64, new: f64) -> f64 {
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
