use deadsync_simfile::artwork::{
    resolved_required_artwork_for_bench, resolved_required_artwork_reference_for_bench,
    song_art_hints_for_bench, song_art_hints_reference_for_bench, sort_song_art_paths_for_bench,
    sort_song_art_paths_reference_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation calls delegate unchanged to `System`; relaxed counters
// observe successful operations only while this single-threaded benchmark
// enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
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
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: f64,
    allocated: AllocSnapshot,
    checksum: u64,
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

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn measure(calls_per_sample: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..4 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(op());
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        times.push(elapsed.as_secs_f64() * 1_000_000_000.0 / calls_per_sample as f64);
        cycles.push(cycle_start.zip(cycle_end).map_or(f64::NAN, |(start, end)| {
            end.wrapping_sub(start) as f64 / calls_per_sample as f64
        }));
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: percentile(&times, 0.5),
        p95_ns: percentile(&times, 0.95),
        median_cycles: percentile(&cycles, 0.5),
        allocated: ALLOC.snapshot().delta(before),
        checksum: checksum ^ allocation_checksum,
    }
}

fn path_checksum(path: &Path) -> u64 {
    path.to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn paths_checksum<'a>(paths: impl IntoIterator<Item = &'a PathBuf>) -> u64 {
    paths
        .into_iter()
        .enumerate()
        .fold(0u64, |sum, (index, path)| {
            sum.wrapping_add(path_checksum(path).rotate_left((index % 63) as u32))
        })
}

fn hints_checksum(hints: &[Option<PathBuf>; 6]) -> u64 {
    hints.iter().enumerate().fold(0u64, |sum, (index, path)| {
        sum.wrapping_add(
            path.as_deref()
                .map_or(0, path_checksum)
                .rotate_left((index * 9) as u32),
        )
    })
}

fn print_result(label: &str, result: &BenchResult, calls: usize) {
    println!(
        "{label:<9} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>9.1} Kops/s  {:>5} alloc  {:>3} realloc  {:>5} free  {:>9} B alloc  {:>9} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles,
        1_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
    println!("          allocation counters cover {calls} calls");
}

fn reduction(new: u64, old: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

fn benchmark_pair(
    title: &str,
    calls: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) {
    assert_eq!(old_op(), new_op(), "{title} behavior diverged");
    let old = measure(calls, &mut old_op);
    let new = measure(calls, &mut new_op);
    assert_eq!(old.checksum, new.checksum, "{title} checksum diverged");

    println!("\n{title}");
    print_result("old", &old, calls);
    print_result("new", &new, calls);
    println!(
        "change    {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        100.0 * (1.0 - new.median_cycles / old.median_cycles),
        reduction(new.allocated.allocated_bytes, old.allocated.allocated_bytes),
        reduction(new.allocated.churn_bytes(), old.allocated.churn_bytes()),
    );
}

fn simfile_fixture() -> String {
    let mut data = String::with_capacity(32 * 1024);
    for index in 0..96 {
        data.push_str("#TITLE:Artwork Fast Path;#ARTIST:DeadSync;#BPMS:0.000=150.000;");
        data.push_str("#NOTES:dance-single::Hard:9:0,0,0,0,0:0000\n1000\n0000\n0000;");
        if index % 8 == 0 {
            data.push_str("#CDIMAGE:disc-art.png;#DISCIMAGE:title-art.png;");
        }
    }
    data
}

fn sort_fixture() -> Vec<PathBuf> {
    [
        "zeta background.PNG",
        "Alpha Banner.png",
        "song jacket.JPG",
        "visuals.MP4",
        "cdtitle.png",
        "song-cd.PNG",
        "song disc.png",
        "midnight background.jpeg",
        "BETA BN.png",
        "jk_album.bmp",
        "stage title.png",
        "Encore Banner.PNG",
        "final bg.jpg",
        "AlbumArt.png",
        "cover.png",
        "Éclair.png",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn hint_fixture() -> Vec<PathBuf> {
    let mut images = (0..48)
        .map(|index| PathBuf::from(format!("visual-{index:02}.png")))
        .collect::<Vec<_>>();
    images[7] = PathBuf::from("song banner.png");
    images[14] = PathBuf::from("song background.jpg");
    images[21] = PathBuf::from("jk_song.png");
    images[28] = PathBuf::from("song-cd.png");
    images[35] = PathBuf::from("song disc.png");
    images[42] = PathBuf::from("cdtitle.png");
    images
}

fn main() {
    let simfile = simfile_fixture();
    let sort_paths = sort_fixture();
    let hint_paths = hint_fixture();

    println!("artwork resolution hot paths ({SAMPLES} samples)");
    benchmark_pair(
        "resolved required art: optional tag scan vs early return",
        2_048,
        || {
            (0..2_048).fold(0u64, |sum, _| {
                sum.wrapping_add(resolved_required_artwork_reference_for_bench(black_box(
                    simfile.as_bytes(),
                )) as u64)
            })
        },
        || {
            (0..2_048).fold(0u64, |sum, _| {
                sum.wrapping_add(
                    resolved_required_artwork_for_bench(black_box(simfile.as_bytes())) as u64,
                )
            })
        },
    );
    benchmark_pair(
        "art path order: cached lowercase keys vs pooled folded keys",
        256,
        || {
            (0..256).fold(0u64, |sum, _| {
                let mut paths = black_box(sort_paths.clone());
                sort_song_art_paths_reference_for_bench(&mut paths);
                sum.wrapping_add(paths_checksum(&paths))
            })
        },
        || {
            (0..256).fold(0u64, |sum, _| {
                let mut paths = black_box(sort_paths.clone());
                sort_song_art_paths_for_bench(&mut paths);
                sum.wrapping_add(paths_checksum(&paths))
            })
        },
    );
    benchmark_pair(
        "art hints: six scans vs one borrowed-stem traversal",
        512,
        || {
            (0..512).fold(0u64, |sum, _| {
                let hints = song_art_hints_reference_for_bench(black_box(&hint_paths));
                sum.wrapping_add(hints_checksum(&hints))
            })
        },
        || {
            (0..512).fold(0u64, |sum, _| {
                let hints = song_art_hints_for_bench(black_box(&hint_paths));
                sum.wrapping_add(hints_checksum(&hints))
            })
        },
    );
}
