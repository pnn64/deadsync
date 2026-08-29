use deadsync_profile::PlayerSide;
use deadsync_shell::{
    BenchmarkQrLoginService, BenchmarkScoreImportService, benchmark_qr_route,
    benchmark_qr_route_reference,
};
use deadsync_theme_simply_love::{
    SimplyLoveQrLoginEvent, SimplyLoveQrLoginService, SimplyLoveScoreImportEvent,
    SimplyLoveScoreImportProgress,
};
use smallvec::SmallVec;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const BURST_EVENTS: usize = 64;
const QR_EVENTS: usize = 4;
const SAMPLES: usize = 2_001;

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
// only observe the benchmark's single-threaded, explicitly gated interval.
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

#[derive(Clone, Copy)]
struct Sample {
    ns: u64,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(action: impl FnOnce() -> u64) -> Sample {
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    let checksum = black_box(action());
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

#[derive(Default)]
struct ScoreUi {
    done: usize,
    total: usize,
    imported: usize,
    missing: usize,
    detail_len: usize,
}

impl ScoreUi {
    fn apply(&mut self, events: impl IntoIterator<Item = SimplyLoveScoreImportEvent>) {
        for event in events {
            if let SimplyLoveScoreImportEvent::Progress(progress) = event {
                self.done = progress.processed_charts;
                self.total = progress.total_charts;
                self.imported = progress.imported_scores;
                self.missing = progress.missing_scores;
                self.detail_len = progress.detail.len();
            }
        }
    }

    const fn checksum(&self) -> u64 {
        self.done as u64
            ^ (self.total as u64).rotate_left(7)
            ^ (self.imported as u64).rotate_left(13)
            ^ (self.missing as u64).rotate_left(19)
            ^ (self.detail_len as u64).rotate_left(23)
    }
}

fn progress(done: usize) -> SimplyLoveScoreImportEvent {
    SimplyLoveScoreImportEvent::Progress(SimplyLoveScoreImportProgress {
        processed_charts: done,
        total_charts: BURST_EVENTS,
        imported_scores: done.saturating_sub(2),
        missing_scores: done.min(2),
        failed_requests: 0,
        detail: String::new(),
    })
}

struct LegacyScoreDrain {
    _tx: mpsc::SyncSender<(u64, SimplyLoveScoreImportEvent)>,
    rx: Receiver<(u64, SimplyLoveScoreImportEvent)>,
}

impl LegacyScoreDrain {
    fn empty() -> Self {
        let (tx, rx) = mpsc::sync_channel(BURST_EVENTS);
        Self { _tx: tx, rx }
    }

    fn with_progress_burst() -> Self {
        let (tx, rx) = mpsc::sync_channel(BURST_EVENTS);
        for done in 1..=BURST_EVENTS {
            tx.send((1, progress(done)))
                .expect("benchmark owns the receiver");
        }
        Self { _tx: tx, rx }
    }

    fn poll(&self) -> Vec<SimplyLoveScoreImportEvent> {
        self.rx
            .try_iter()
            .filter_map(|(job_id, event)| (job_id == 1).then_some(event))
            .collect()
    }
}

struct LegacyScorePublisher {
    tx: mpsc::SyncSender<(u64, SimplyLoveScoreImportEvent)>,
    _rx: Receiver<(u64, SimplyLoveScoreImportEvent)>,
}

impl LegacyScorePublisher {
    fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel(BURST_EVENTS);
        Self { tx, _rx: rx }
    }

    fn publish_burst(&self) {
        for done in 1..=BURST_EVENTS {
            self.tx
                .send((1, progress(done)))
                .expect("benchmark burst fits the queue");
        }
    }
}

fn measure_legacy_publish() -> Sample {
    let publisher = LegacyScorePublisher::new();
    measure(|| {
        publisher.publish_burst();
        BURST_EVENTS as u64
    })
}

fn measure_latest_publish() -> Sample {
    let publisher = BenchmarkScoreImportService::active();
    measure(|| {
        for done in 1..=BURST_EVENTS {
            publisher.publish_progress(done, BURST_EVENTS, "");
        }
        BURST_EVENTS as u64
    })
}

fn measure_legacy_drain() -> Sample {
    let service = LegacyScoreDrain::with_progress_burst();
    measure(|| {
        let mut ui = ScoreUi::default();
        ui.apply(service.poll());
        ui.checksum()
    })
}

fn measure_legacy_empty_poll() -> Sample {
    let service = LegacyScoreDrain::empty();
    measure(|| service.poll().len() as u64)
}

fn measure_latest_empty_poll() -> Sample {
    let mut service = BenchmarkScoreImportService::active();
    measure(|| service.poll().expect("benchmark import is active").len() as u64)
}

fn measure_latest_drain() -> Sample {
    let mut service = BenchmarkScoreImportService::with_progress_burst(BURST_EVENTS, 0);
    measure(|| {
        let mut ui = ScoreUi::default();
        ui.apply(service.poll().expect("benchmark import is active"));
        ui.checksum()
    })
}

fn measure_legacy_qr() -> Sample {
    let mut service = BenchmarkQrLoginService::with_started_burst(
        SimplyLoveQrLoginService::ArrowCloud,
        QR_EVENTS,
    );
    measure(|| service.drain_reference_checksum())
}

fn measure_inline_qr() -> Sample {
    let mut service = BenchmarkQrLoginService::with_started_burst(
        SimplyLoveQrLoginService::ArrowCloud,
        QR_EVENTS,
    );
    measure(|| service.drain_checksum())
}

fn qr_event(index: usize) -> SimplyLoveQrLoginEvent {
    SimplyLoveQrLoginEvent::Started {
        service: SimplyLoveQrLoginService::ArrowCloud,
        side: [PlayerSide::P1, PlayerSide::P2][index % 2],
        short_code: String::new(),
        verification_url: String::new(),
    }
}

fn measure_legacy_qr_route() -> Sample {
    let events = (0..QR_EVENTS).map(qr_event).collect::<Vec<_>>();
    measure(|| benchmark_qr_route_reference(events))
}

fn measure_direct_qr_route() -> Sample {
    let events = (0..QR_EVENTS)
        .map(qr_event)
        .collect::<SmallVec<[_; QR_EVENTS]>>();
    measure(|| benchmark_qr_route(SimplyLoveQrLoginService::ArrowCloud, &events))
}

fn percentile(values: &mut [u64], percentile: usize) -> u64 {
    values.sort_unstable();
    values[(values.len() - 1) * percentile / 100]
}

fn mean(samples: &[Sample], value: impl Fn(&Sample) -> u64) -> f64 {
    samples
        .iter()
        .map(|sample| value(sample) as f64)
        .sum::<f64>()
        / samples.len() as f64
}

fn report(name: &str, items: usize, samples: &[Sample]) {
    let mut p50 = samples.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut p95 = p50.clone();
    let ns_mean = mean(samples, |sample| sample.ns);
    let cycle_values = samples
        .iter()
        .filter_map(|sample| sample.cycles)
        .collect::<Vec<_>>();
    let cycles = (!cycle_values.is_empty())
        .then(|| cycle_values.iter().sum::<u64>() as f64 / cycle_values.len() as f64);
    let median = percentile(&mut p50, 50);
    println!(
        "  {name:<6} p50 {:>10} ns  p95 {:>10} ns  mean {:>10.1} ns  {:>10.1} cycles  {:>9.2} Mitem/s  \
         {:>5.2} alloc  {:>5.2} realloc  {:>5.2} free  {:>9.1} churn B/op",
        median,
        percentile(&mut p95, 95),
        ns_mean,
        cycles.unwrap_or(f64::NAN),
        items as f64 * 1_000.0 / ns_mean,
        mean(samples, |sample| sample.alloc.allocs),
        mean(samples, |sample| sample.alloc.reallocs),
        mean(samples, |sample| sample.alloc.frees),
        mean(samples, |sample| sample.alloc.churn_bytes()),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn report_pair(title: &str, items: usize, old: &[Sample], new: &[Sample]) {
    assert_eq!(old[0].checksum, new[0].checksum, "{title} behavior differs");
    assert!(old.iter().all(|sample| sample.checksum == old[0].checksum));
    assert!(new.iter().all(|sample| sample.checksum == new[0].checksum));
    let mut old_p50 = old.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut new_p50 = new.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut old_p95 = old_p50.clone();
    let mut new_p95 = new_p50.clone();
    let old_cycles = old.iter().filter_map(|sample| sample.cycles).sum::<u64>() as f64
        / old.iter().filter(|sample| sample.cycles.is_some()).count() as f64;
    let new_cycles = new.iter().filter_map(|sample| sample.cycles).sum::<u64>() as f64
        / new.iter().filter(|sample| sample.cycles.is_some()).count() as f64;
    println!("{title}");
    report("old", items, old);
    report("new", items, new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% churn",
        percent_change(
            percentile(&mut old_p50, 50) as f64,
            percentile(&mut new_p50, 50) as f64,
        ),
        percent_change(
            percentile(&mut old_p95, 95) as f64,
            percentile(&mut new_p95, 95) as f64,
        ),
        percent_change(old_cycles, new_cycles),
        percent_change(
            mean(old, |sample| sample.alloc.churn_bytes()),
            mean(new, |sample| sample.alloc.churn_bytes()),
        ),
    );
}

fn sample_pair(
    old_measure: fn() -> Sample,
    new_measure: fn() -> Sample,
) -> (Vec<Sample>, Vec<Sample>) {
    for _ in 0..64 {
        black_box(old_measure());
        black_box(new_measure());
    }
    let mut old = Vec::with_capacity(SAMPLES);
    let mut new = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        if index % 2 == 0 {
            old.push(old_measure());
            new.push(new_measure());
        } else {
            new.push(new_measure());
            old.push(old_measure());
        }
    }
    (old, new)
}

fn main() {
    let (old_publish, new_publish) = sample_pair(measure_legacy_publish, measure_latest_publish);
    report_pair(
        "score progress worker publication (64 updates)",
        BURST_EVENTS,
        &old_publish,
        &new_publish,
    );

    let (old_drain, new_drain) = sample_pair(measure_legacy_drain, measure_latest_drain);
    report_pair(
        "score progress frame integration (64 queued updates)",
        BURST_EVENTS,
        &old_drain,
        &new_drain,
    );

    let (old_empty, new_empty) = sample_pair(measure_legacy_empty_poll, measure_latest_empty_poll);
    report_pair(
        "score import active frame without an update",
        1,
        &old_empty,
        &new_empty,
    );

    let (old_qr, new_qr) = sample_pair(measure_legacy_qr, measure_inline_qr);
    report_pair(
        "QR login channel drain (4 events)",
        QR_EVENTS,
        &old_qr,
        &new_qr,
    );

    let (old_qr_route, new_qr_route) =
        sample_pair(measure_legacy_qr_route, measure_direct_qr_route);
    report_pair(
        "QR login provider routing (4 events)",
        QR_EVENTS,
        &old_qr_route,
        &new_qr_route,
    );
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: the pseudo-handle is valid for this process and `cycles` is
    // writable for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}

const fn cycle_delta(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    match (before, after) {
        (Some(before), Some(after)) => Some(after.wrapping_sub(before)),
        _ => None,
    }
}
