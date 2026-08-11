use deadlib_audio::{MixBus, QueuedSfx, sfx_transport};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const QUEUE_CAPACITY: usize = 128;
const EMPTY_CALLBACKS: usize = 5_000_000;
const BURST_CALLBACKS: usize = 500_000;
const BURST_SIZE: usize = 8;
const SAMPLE_OPS: usize = 1_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    dealloc_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            dealloc_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            dealloc_bytes: self.dealloc_bytes.load(Ordering::Relaxed),
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
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.dealloc_bytes
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
    deallocs: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    dealloc_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            dealloc_bytes: self.dealloc_bytes - before.dealloc_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.dealloc_bytes
    }
}

struct BenchResult {
    ns_per_callback: f64,
    worst_sample_ns: f64,
    cycles_per_callback: Option<f64>,
    callbacks_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(callbacks: usize, mut callback: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(callbacks / 20).max(1) {
        black_box(callback());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..callbacks / SAMPLE_OPS {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_OPS {
            checksum = checksum.wrapping_add(black_box(callback()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_OPS as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..callbacks {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(callback()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_callback: seconds * 1_000_000_000.0 / callbacks as f64,
        worst_sample_ns,
        cycles_per_callback: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / callbacks as f64),
        callbacks_per_second: callbacks as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, callbacks: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", callbacks, old);
    print_result("new", callbacks, new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  {:>7.2}% sample tail",
        percent_change(old.ns_per_callback, new.ns_per_callback),
        percent_change(
            old.cycles_per_callback.unwrap_or(f64::NAN),
            new.cycles_per_callback.unwrap_or(f64::NAN),
        ),
        percent_change(old.callbacks_per_second, new.callbacks_per_second),
        percent_change(old.worst_sample_ns, new.worst_sample_ns),
    );
}

fn print_result(label: &str, callbacks: usize, result: &BenchResult) {
    let count = callbacks as f64;
    println!(
        "  {label:<3} {:>9.2} ns/cb  {:>9.2} cycles/cb  {:>9.2} worst ns  \
         {:>8.3} Mcb/s  {:>5.2} alloc/cb  {:>5.2} realloc/cb  {:>5.2} free/cb  {:>8.1} churn B/cb",
        result.ns_per_callback,
        result.cycles_per_callback.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        result.callbacks_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.deallocs as f64 / count,
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

fn queued(data: &Arc<[i16]>, frame: u64) -> QueuedSfx {
    QueuedSfx {
        data: Arc::clone(data),
        bus: MixBus::new(0),
        generation: frame,
        target_stream_frame: frame,
    }
}

fn main() {
    let (old_sender, old_receiver) = sync_channel::<QueuedSfx>(QUEUE_CAPACITY);
    let (mut new_sender, mut new_receiver) = sfx_transport(QUEUE_CAPACITY);
    let old_empty = measure(EMPTY_CALLBACKS, || old_receiver.try_iter().count() as u64);
    let new_empty = measure(EMPTY_CALLBACKS, || new_receiver.try_iter().count() as u64);
    print_pair(
        "empty audio-callback SFX drain",
        EMPTY_CALLBACKS,
        &old_empty,
        &new_empty,
    );

    let data: Arc<[i16]> = Arc::from([1, -1, 2, -2]);
    let mut old_frame = 0u64;
    let old_burst = measure(BURST_CALLBACKS, || {
        for offset in 0..BURST_SIZE {
            assert!(
                old_sender
                    .try_send(queued(&data, old_frame + offset as u64))
                    .is_ok(),
                "benchmark burst fits fixed queue"
            );
        }
        let checksum = old_receiver.try_iter().fold(0u64, |checksum, queued| {
            checksum.wrapping_add(queued.target_stream_frame)
        });
        old_frame = old_frame.wrapping_add(BURST_SIZE as u64);
        checksum
    });
    let mut new_frame = 0u64;
    let new_burst = measure(BURST_CALLBACKS, || {
        for offset in 0..BURST_SIZE {
            assert!(
                new_sender
                    .try_send(queued(&data, new_frame + offset as u64))
                    .is_ok(),
                "benchmark burst fits fixed queue"
            );
        }
        let checksum = new_receiver.try_iter().fold(0u64, |checksum, queued| {
            checksum.wrapping_add(queued.target_stream_frame)
        });
        new_frame = new_frame.wrapping_add(BURST_SIZE as u64);
        checksum
    });
    print_pair(
        "eight-command SFX enqueue and callback drain",
        BURST_CALLBACKS,
        &old_burst,
        &new_burst,
    );
}
