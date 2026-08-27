use deadsync_shell::BenchmarkProfileImportService;
use deadsync_theme_simply_love::SimplyLoveProfileImportEvent;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const BURST_EVENTS: usize = 64;
// The production local-score import callback currently reports an empty label.
const LABEL_BYTES: usize = 0;
const SAMPLES: usize = 2_001;

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

// SAFETY: requests are delegated unchanged to `System`; counters only observe
// the benchmark's single-threaded, explicitly gated measurement interval.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
}

#[derive(Default)]
struct UiState {
    done: usize,
    total: usize,
    label: String,
}

impl UiState {
    fn apply(&mut self, events: impl IntoIterator<Item = SimplyLoveProfileImportEvent>) {
        for event in events {
            if let SimplyLoveProfileImportEvent::Progress { done, total, label } = event {
                self.done = done;
                self.total = total;
                self.label = label;
            }
        }
    }

    const fn checksum(&self) -> usize {
        self.done ^ self.total.rotate_left(7) ^ self.label.len().rotate_left(13)
    }
}

struct LegacyService {
    rx: Receiver<SimplyLoveProfileImportEvent>,
}

impl LegacyService {
    fn with_progress_burst() -> Self {
        let (tx, rx) = mpsc::sync_channel(BURST_EVENTS);
        let label = "p".repeat(LABEL_BYTES);
        for done in 1..=BURST_EVENTS {
            tx.send(SimplyLoveProfileImportEvent::Progress {
                done,
                total: BURST_EVENTS,
                label: label.clone(),
            })
            .expect("the benchmark owns the receiver");
        }
        Self { rx }
    }

    fn poll(&self) -> Vec<SimplyLoveProfileImportEvent> {
        self.rx.try_iter().collect()
    }
}

struct LegacyPublisher {
    tx: mpsc::SyncSender<SimplyLoveProfileImportEvent>,
    _rx: Receiver<SimplyLoveProfileImportEvent>,
}

impl LegacyPublisher {
    fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(BURST_EVENTS);
        Self { tx, _rx: rx }
    }

    fn publish_burst(&self) {
        for done in 1..=BURST_EVENTS {
            self.tx
                .send(SimplyLoveProfileImportEvent::Progress {
                    done,
                    total: BURST_EVENTS,
                    label: String::new(),
                })
                .expect("the benchmark owns the receiver");
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    ns: u64,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn measure_legacy() -> Sample {
    let service = LegacyService::with_progress_burst();
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    let mut state = UiState::default();
    state.apply(service.poll());
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before_alloc);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc,
        checksum: black_box(state.checksum()),
    }
}

fn measure_latest() -> Sample {
    let mut service = BenchmarkProfileImportService::with_progress_burst(BURST_EVENTS, LABEL_BYTES);
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    let mut state = UiState::default();
    state.apply(service.poll().expect("the benchmark import is active"));
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before_alloc);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc,
        checksum: black_box(state.checksum()),
    }
}

fn measure_legacy_publish() -> Sample {
    let publisher = LegacyPublisher::new();
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    publisher.publish_burst();
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before_alloc);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc,
        checksum: BURST_EVENTS,
    }
}

fn measure_latest_publish() -> Sample {
    let publisher = BenchmarkProfileImportService::active();
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    for done in 1..=BURST_EVENTS {
        publisher.publish_progress(done, BURST_EVENTS, "");
    }
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let alloc = ALLOC.snapshot().delta(before_alloc);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc,
        checksum: BURST_EVENTS,
    }
}

fn percentile(values: &mut [u64], percentile: usize) -> u64 {
    values.sort_unstable();
    values[(values.len() - 1) * percentile / 100]
}

fn report(name: &str, throughput_unit: &str, samples: &[Sample]) {
    let mut ns_p50 = samples.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut ns_p95 = ns_p50.clone();
    let mut ns_p99 = ns_p50.clone();
    let ns_mean = samples.iter().map(|sample| sample.ns as f64).sum::<f64>() / samples.len() as f64;
    let cycles = samples
        .iter()
        .filter_map(|sample| sample.cycles)
        .collect::<Vec<_>>();
    let cycle_mean =
        (!cycles.is_empty()).then(|| cycles.iter().sum::<u64>() as f64 / cycles.len() as f64);
    let allocs = samples
        .iter()
        .map(|sample| sample.alloc.allocs)
        .sum::<u64>() as f64
        / samples.len() as f64;
    let frees =
        samples.iter().map(|sample| sample.alloc.frees).sum::<u64>() as f64 / samples.len() as f64;
    let alloc_bytes = samples
        .iter()
        .map(|sample| sample.alloc.alloc_bytes)
        .sum::<u64>() as f64
        / samples.len() as f64;
    let free_bytes = samples
        .iter()
        .map(|sample| sample.alloc.free_bytes)
        .sum::<u64>() as f64
        / samples.len() as f64;
    let checksum = samples
        .iter()
        .fold(0usize, |sum, sample| sum ^ sample.checksum);
    println!(
        "{name}: mean_ns={ns_mean:.2} p50_ns={} p95_ns={} p99_ns={} worst_ns={} throughput_m{throughput_unit}_s={:.3} mean_cycles={} allocs={allocs:.2} frees={frees:.2} alloc_bytes={alloc_bytes:.2} free_bytes={free_bytes:.2} checksum={checksum}",
        percentile(&mut ns_p50, 50),
        percentile(&mut ns_p95, 95),
        percentile(&mut ns_p99, 99),
        samples.iter().map(|sample| sample.ns).max().unwrap_or(0),
        1_000.0 / ns_mean,
        cycle_mean.map_or_else(|| "n/a".to_owned(), |cycles| format!("{cycles:.2}")),
    );
}

fn main() {
    assert!(
        std::env::args().any(|arg| arg == "burst-64"),
        "pass burst-64"
    );

    for _ in 0..128 {
        black_box(measure_legacy());
        black_box(measure_latest());
        black_box(measure_legacy_publish());
        black_box(measure_latest_publish());
    }
    let mut legacy = Vec::with_capacity(SAMPLES);
    let mut latest = Vec::with_capacity(SAMPLES);
    let mut legacy_publish = Vec::with_capacity(SAMPLES);
    let mut latest_publish = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        if index % 2 == 0 {
            legacy.push(measure_legacy());
            latest.push(measure_latest());
            legacy_publish.push(measure_legacy_publish());
            latest_publish.push(measure_latest_publish());
        } else {
            latest_publish.push(measure_latest_publish());
            legacy_publish.push(measure_legacy_publish());
            latest.push(measure_latest());
            legacy.push(measure_legacy());
        }
    }

    assert!(
        legacy
            .iter()
            .all(|sample| sample.checksum == legacy[0].checksum)
    );
    assert!(
        latest
            .iter()
            .all(|sample| sample.checksum == legacy[0].checksum)
    );
    report("legacy_queue", "frame", &legacy);
    report("latest_progress", "frame", &latest);
    report("legacy_publish_64", "burst", &legacy_publish);
    report("latest_publish_64", "burst", &latest_publish);
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: the pseudo-handle is valid for the current process and the output
    // pointer refers to initialized writable storage for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}

fn cycle_delta(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    Some(after?.saturating_sub(before?))
}
