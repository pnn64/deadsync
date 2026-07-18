use deadlib_present::runtime;
use deadsync_theme_simply_love::screens::select_music::SelectMusicFolderStatsBench;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 256;
const MEASURE_FRAMES: usize = 5_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
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

// SAFETY: all requests are forwarded unchanged to `System`; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the live allocation and original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer and its original layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
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

struct BenchResult {
    elapsed: Duration,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn main() {
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load_for_tests("en"));
    println!(
        "settled Select Music folder stats: 512 songs, 2560 charts, {MEASURE_FRAMES} update + actor frames"
    );
    report("solo", measure(false));
    report("versus", measure(true));
}

fn measure(versus: bool) -> BenchResult {
    runtime::clear_all();
    let mut bench = SelectMusicFolderStatsBench::new(versus);
    run_frames(&mut bench, WARMUP_FRAMES);
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let checksum = black_box(run_frames(&mut bench, MEASURE_FRAMES));
    BenchResult {
        elapsed: started.elapsed(),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn run_frames(bench: &mut SelectMusicFolderStatsBench, frames: usize) -> usize {
    let mut checksum = 0usize;
    for _ in 0..frames {
        checksum = checksum.wrapping_add(black_box(bench.frame()));
    }
    checksum
}

fn report(label: &str, result: BenchResult) {
    println!(
        "{label:>6}: {:>10.2} us/frame  {:.2} allocs/frame  {:.0} B/frame  checksum={}",
        result.elapsed.as_secs_f64() * 1.0e6 / MEASURE_FRAMES as f64,
        result.alloc.allocs as f64 / MEASURE_FRAMES as f64,
        result.alloc.bytes as f64 / MEASURE_FRAMES as f64,
        result.checksum,
    );
}
