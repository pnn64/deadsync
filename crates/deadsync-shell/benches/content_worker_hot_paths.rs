use deadsync_shell::{benchmark_receive_ready, benchmark_sample_progress};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PROGRESS_COUNT: usize = 4_096;
const PROGRESS_OPS: usize = 100;
const FRAME_BUDGET: usize = 8;
const SAMPLES: usize = 31;

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

// SAFETY: every request is delegated unchanged to `System`; relaxed counters
// observe only the single-threaded benchmark while its gate is enabled.
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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

struct Row {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

#[derive(Clone)]
struct ProgressEvent {
    done: usize,
    line2: String,
    line3: String,
}

fn progress_samples() -> Vec<(usize, usize, Duration)> {
    (1..=PROGRESS_COUNT)
        .map(|done| {
            (
                done,
                PROGRESS_COUNT,
                Duration::from_micros((done - 1) as u64 * 100),
            )
        })
        .collect()
}

fn event(done: usize, total: usize) -> ProgressEvent {
    ProgressEvent {
        done,
        line2: format!("Pack {:03}", done % 250),
        line3: format!("Song {done:05} of {total:05}"),
    }
}

fn event_checksum(events: &[ProgressEvent]) -> u64 {
    events.iter().fold(events.len() as u64, |sum, event| {
        sum.wrapping_mul(131)
            .wrapping_add(event.done as u64)
            .wrapping_add(event.line2.len() as u64)
            .wrapping_add(event.line3.len() as u64)
    })
}

fn measure_progress(mut op: impl FnMut() -> Vec<ProgressEvent>) -> (Row, usize, usize) {
    for _ in 0..10 {
        black_box(event_checksum(&op()));
    }
    let sample = op();
    let emitted = sample.len();
    let terminal = sample.last().map_or(0, |event| event.done);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..PROGRESS_OPS {
        checksum = checksum.wrapping_add(event_checksum(black_box(&op())));
    }
    let ns = started.elapsed().as_secs_f64() * 1e9 / PROGRESS_OPS as f64;
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..PROGRESS_OPS {
        black_box(op());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (
        Row {
            ns,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| (end - start) as f64 / PROGRESS_OPS as f64),
            alloc: ALLOC.snapshot().delta(before),
            checksum,
        },
        emitted,
        terminal,
    )
}

fn progress_queue() -> Receiver<ProgressEvent> {
    let (tx, rx) = mpsc::channel();
    for done in 1..=PROGRESS_COUNT {
        tx.send(event(done, PROGRESS_COUNT)).unwrap();
    }
    rx
}

fn drain_frame(rx: &Receiver<ProgressEvent>, max: usize) -> (usize, u64) {
    if max == usize::MAX {
        let events = rx.try_iter().collect::<Vec<_>>();
        (events.len(), event_checksum(black_box(&events)))
    } else {
        let events = benchmark_receive_ready(rx);
        (events.len(), event_checksum(black_box(&events)))
    }
}

fn sample_drain(max: usize) -> Row {
    let rx = progress_queue();
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let (processed, checksum) = drain_frame(&rx, max);
    let ns = started.elapsed().as_secs_f64() * 1e9;
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    Row {
        ns,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| (end - start) as f64),
        alloc: ALLOC.snapshot().delta(before),
        checksum: checksum ^ processed as u64,
    }
}

fn median_drain(max: usize) -> Row {
    let mut rows = (0..SAMPLES).map(|_| sample_drain(max)).collect::<Vec<_>>();
    rows.sort_by(|a, b| a.ns.total_cmp(&b.ns));
    rows.swap_remove(SAMPLES / 2)
}

fn main() {
    let samples = progress_samples();
    let (old_progress, old_events, old_terminal) = measure_progress(|| {
        samples
            .iter()
            .map(|&(done, total, _)| event(done, total))
            .collect()
    });
    let (new_progress, new_events, new_terminal) = measure_progress(|| {
        benchmark_sample_progress(black_box(&samples), |done, total| event(done, total))
    });
    assert_eq!(old_terminal, PROGRESS_COUNT);
    assert_eq!(new_terminal, old_terminal);
    assert!(new_events < old_events);
    black_box(old_progress.checksum);
    black_box(new_progress.checksum);
    println!(
        "worker progress production ({PROGRESS_COUNT} callbacks at 100 us intervals; terminal preserved)"
    );
    print_progress("old", &old_progress, old_events);
    print_progress("new", &new_progress, new_events);
    print_change(&old_progress, &new_progress);

    let old_drain = median_drain(usize::MAX);
    let new_drain = median_drain(FRAME_BUDGET);
    black_box(old_drain.checksum);
    black_box(new_drain.checksum);
    println!("ready progress burst first frame ({PROGRESS_COUNT} queued events)");
    print_drain("old", &old_drain, PROGRESS_COUNT);
    print_drain("new", &new_drain, FRAME_BUDGET);
    print_change(&old_drain, &new_drain);
}

fn print_progress(label: &str, row: &Row, emitted: usize) {
    println!(
        "  {label:<3} {:>12.2} ns/burst  {:>12.2} cycles/burst  {:>8.3} Mcallback/s  \
         {:>6} emitted  {:>8.1} alloc/burst  {:>12.1} churn B/burst",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        PROGRESS_COUNT as f64 * 1_000.0 / row.ns,
        emitted,
        row.alloc.allocs as f64 / PROGRESS_OPS as f64,
        row.alloc.churn() as f64 / PROGRESS_OPS as f64,
    );
}

fn print_drain(label: &str, row: &Row, integrated: usize) {
    println!(
        "  {label:<3} {:>12.2} ns/frame  {:>12.2} cycles/frame  {:>8.3} Mevent/s  \
         {:>6} integrated  {:>6} alloc  {:>6} free  {:>12} churn B",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        integrated as f64 * 1_000.0 / row.ns,
        integrated,
        row.alloc.allocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn print_change(old: &Row, new: &Row) {
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% churn",
        change(old.ns, new.ns),
        change(
            old.cycles.unwrap_or(f64::NAN),
            new.cycles.unwrap_or(f64::NAN)
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
