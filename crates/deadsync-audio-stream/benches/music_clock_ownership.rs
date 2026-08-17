use deadlib_audio_core::{
    activate_music_track, mark_music_track_started, reset_music_stream_clock_state,
    seed_music_stream_clock, stop_music_track,
};
use deadsync_audio_stream::MusicClock;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 131_072;
const SAMPLES: usize = 512;
const FRAMES_PER_SAMPLE: usize = 2_048;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation calls delegate unchanged to `System`; relaxed atomics
// only count successful calls while this single-threaded benchmark measures.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct ResultSet {
    mean_ns: f64,
    p99_ns: f64,
    worst_ns: f64,
    cycles_per_frame: Option<f64>,
    allocations: AllocSnapshot,
    locks_per_frame: f64,
    checksum: u64,
}

fn measure(locks_per_frame: f64, mut frame: impl FnMut(usize) -> u64) -> ResultSet {
    for index in 0..WARMUP_FRAMES {
        black_box(frame(index));
    }

    let mut samples = Vec::with_capacity(SAMPLES);
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let mut checksum = 0u64;
    for sample in 0..SAMPLES {
        let started = Instant::now();
        for frame_index in 0..FRAMES_PER_SAMPLE {
            checksum =
                checksum.wrapping_add(black_box(frame(sample * FRAMES_PER_SAMPLE + frame_index)));
        }
        samples.push(started.elapsed());
    }
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocations = ALLOC.snapshot().delta(before);

    samples.sort_unstable();
    let frame_count = (SAMPLES * FRAMES_PER_SAMPLE) as f64;
    let elapsed = samples.iter().copied().sum::<Duration>();
    let sample_ns = |duration: Duration| duration.as_secs_f64() * 1e9 / FRAMES_PER_SAMPLE as f64;
    ResultSet {
        mean_ns: elapsed.as_secs_f64() * 1e9 / frame_count,
        p99_ns: sample_ns(samples[(SAMPLES * 99 / 100).min(SAMPLES - 1)]),
        worst_ns: sample_ns(*samples.last().expect("at least one sample")),
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / frame_count),
        allocations,
        locks_per_frame,
        checksum,
    }
}

fn print_result(label: &str, result: &ResultSet) {
    println!(
        "{label:<22} mean={:>8.2} ns  p99={:>8.2} ns  worst={:>8.2} ns  cycles={:>8.2}  alloc={}/{}/{}  locks={:.0}/frame  checksum={:016x}",
        result.mean_ns,
        result.p99_ns,
        result.worst_ns,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.allocations.allocs,
        result.allocations.reallocs,
        result.allocations.bytes,
        result.locks_per_frame,
        result.checksum,
    );
}

fn main() {
    reset_music_stream_clock_state();
    seed_music_stream_clock(2.0, 1.25);
    activate_music_track();
    mark_music_track_started(0);

    let locked = Mutex::new(MusicClock::without_audio());
    let legacy = measure(1.0, |_| {
        let mut clock = locked
            .lock()
            .expect("benchmark clock mutex is not poisoned");
        let snapshot = black_box(clock.snapshot());
        snapshot.music_nanos as u64 ^ u64::from(snapshot.music_seconds_per_second.to_bits())
    });

    let mut owned = MusicClock::without_audio();
    let direct = measure(0.0, |_| {
        let snapshot = black_box(owned.snapshot());
        snapshot.music_nanos as u64 ^ u64::from(snapshot.music_seconds_per_second.to_bits())
    });
    stop_music_track();
    reset_music_stream_clock_state();

    assert_eq!(legacy.checksum, direct.checksum);
    for result in [&legacy, &direct] {
        assert_eq!(result.allocations.allocs, 0);
        assert_eq!(result.allocations.reallocs, 0);
        assert_eq!(result.allocations.bytes, 0);
    }

    println!(
        "music clock ownership (matched active snapshot, {SAMPLES}x{FRAMES_PER_SAMPLE} frames)"
    );
    print_result("global mutex control", &legacy);
    print_result("App-owned reader", &direct);
    println!(
        "change: mean={:+.2}% p99={:+.2}% worst={:+.2}% cycles={:+.2}%",
        percent_change(legacy.mean_ns, direct.mean_ns),
        percent_change(legacy.p99_ns, direct.p99_ns),
        percent_change(legacy.worst_ns, direct.worst_ns),
        percent_change(
            legacy.cycles_per_frame.unwrap_or(f64::NAN),
            direct.cycles_per_frame.unwrap_or(f64::NAN),
        ),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new - old) * 100.0 / old
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: `_rdtsc` reads the architectural timestamp counter and has no
    // memory-safety preconditions.
    Some(unsafe { core::arch::x86_64::_rdtsc() })
}

#[cfg(not(target_arch = "x86_64"))]
fn cycle_counter() -> Option<u64> {
    None
}
