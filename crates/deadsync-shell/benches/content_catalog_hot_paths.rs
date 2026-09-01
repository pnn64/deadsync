use deadsync_chart::{SongData, SongPack, SyncPref};
use deadsync_shell::{
    benchmark_artwork_paths, benchmark_artwork_paths_reference, benchmark_progress_fallback,
    benchmark_progress_fallback_reference, benchmark_replaygain_paths,
    benchmark_replaygain_paths_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKS: usize = 64;
const SONGS_PER_PACK: usize = 128;
const SONGS: usize = PACKS * SONGS_PER_PACK;
const PROGRESS_PATHS: usize = 4_096;
const SAMPLES: usize = 31;
const WARMUPS: usize = 3;
const OPS: usize = 4;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all calls delegate unchanged to `System`; relaxed counters are
// enabled only around one single-threaded benchmark operation.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let output = unsafe { System.realloc(pointer, old, new_size) };
        if !output.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        output
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..WARMUPS {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0;
    let mut new_checksum = 0;
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample.is_multiple_of(2) {
            (timed_sample(&mut old_op), timed_sample(&mut new_op))
        } else {
            let new_sample = timed_sample(&mut new_op);
            let old_sample = timed_sample(&mut old_op);
            (old_sample, new_sample)
        };
        old_times.push(old_sample.0);
        new_times.push(new_sample.0);
        if let Some(cycles) = old_sample.1 {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_sample.1 {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sample.2;
        new_checksum ^= new_sample.2;
    }

    let old_allocated = measured_allocations(&mut old_op);
    let new_allocated = measured_allocations(&mut new_op);
    (
        bench_result(old_times, old_cycles, old_allocated, old_checksum),
        bench_result(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn timed_sample(operation: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..OPS {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9 / OPS as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS as f64);
    (elapsed, cycles, checksum)
}

fn measured_allocations(operation: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn bench_result(
    mut times: Vec<f64>,
    mut cycles: Vec<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
) -> BenchResult {
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated,
        checksum,
    }
}

fn song(pack_index: usize, song_index: usize, shared_music: usize) -> Arc<SongData> {
    let song_dir = format!("Songs/Pack {pack_index:03}/Song {song_index:04}");
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("{song_dir}/song.ssc")),
        title: String::new(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: Some(PathBuf::from(format!("{song_dir}/banner.png"))),
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: Some(PathBuf::from(format!("{song_dir}/cdtitle.png"))),
        music_path: Some(PathBuf::from(format!(
            "Songs/Shared Audio/music-{shared_music:05}.ogg"
        ))),
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts: Vec::new(),
    })
}

fn catalog_fixture() -> Vec<SongPack> {
    (0..PACKS)
        .map(|pack_index| {
            let songs = (0..SONGS_PER_PACK)
                .map(|song_index| {
                    let global = pack_index * SONGS_PER_PACK + song_index;
                    song(pack_index, song_index, global / 3)
                })
                .collect();
            SongPack {
                group_name: format!("Pack {pack_index:03}"),
                name: format!("Pack {pack_index:03}"),
                sort_title: String::new(),
                translit_title: String::new(),
                series: String::new(),
                folder_series: String::new(),
                year: 0,
                sync_pref: SyncPref::Default,
                directory: PathBuf::from(format!("Songs/Pack {pack_index:03}")),
                banner_path: Some(PathBuf::from(format!(
                    "Songs/Pack {pack_index:03}/banner.png"
                ))),
                songs,
            }
        })
        .collect()
}

fn progress_fixture() -> Vec<PathBuf> {
    (0..PROGRESS_PATHS)
        .map(|index| match index % 3 {
            0 => PathBuf::from(format!(
                "Songs/Pack {:03}/Song {index:05}/banner.png",
                index % PACKS
            )),
            1 => PathBuf::from(format!(
                "Courses/Tournament {:03}/course-{index:05}.png",
                index % 37
            )),
            _ => PathBuf::from(format!("Cache/Generated {index:05}/preview.png")),
        })
        .collect()
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

fn path_checksum(path: &Path) -> u64 {
    text_checksum(path.to_string_lossy().as_ref())
}

fn paths_checksum(paths: &[PathBuf]) -> u64 {
    paths.iter().fold(paths.len() as u64, |sum, path| {
        sum.wrapping_mul(65_537).wrapping_add(path_checksum(path))
    })
}

fn pair_checksum(pair: &(Vec<PathBuf>, Vec<PathBuf>)) -> u64 {
    paths_checksum(&pair.0).rotate_left(17) ^ paths_checksum(&pair.1).rotate_left(41)
}

fn labels_checksum(labels: &(String, String)) -> u64 {
    text_checksum(&labels.0).rotate_left(13) ^ text_checksum(&labels.1).rotate_left(37)
}

fn main() {
    let packs = catalog_fixture();
    let expected = benchmark_replaygain_paths_reference(&packs, None);
    let actual = benchmark_replaygain_paths(&packs, None);
    assert_eq!(actual, expected, "ReplayGain path behavior diverged");

    let expected = benchmark_artwork_paths_reference(&packs);
    let actual = benchmark_artwork_paths(&packs);
    assert_eq!(actual, expected, "artwork path behavior diverged");

    let progress_paths = progress_fixture();
    for path in &progress_paths {
        assert_eq!(
            benchmark_progress_fallback(path),
            benchmark_progress_fallback_reference(path),
            "progress labels diverged for {path:?}"
        );
    }

    let (old, new) = measure_pair(
        || {
            paths_checksum(&benchmark_replaygain_paths_reference(
                black_box(&packs),
                None,
            ))
        },
        || paths_checksum(&benchmark_replaygain_paths(black_box(&packs), None)),
    );
    report("dense ReplayGain path collection", SONGS, &old, &new);
    assert_improved(&old, &new);

    let (old, new) = measure_pair(
        || pair_checksum(&benchmark_artwork_paths_reference(black_box(&packs))),
        || pair_checksum(&benchmark_artwork_paths(black_box(&packs))),
    );
    report("capacity-sized artwork path collection", SONGS, &old, &new);
    assert_improved(&old, &new);

    let (old, new) = measure_pair(
        || {
            progress_paths.iter().fold(0u64, |sum, path| {
                sum.wrapping_mul(257).wrapping_add(labels_checksum(
                    &benchmark_progress_fallback_reference(black_box(path)),
                ))
            })
        },
        || {
            progress_paths.iter().fold(0u64, |sum, path| {
                sum.wrapping_mul(257)
                    .wrapping_add(labels_checksum(&benchmark_progress_fallback(black_box(
                        path,
                    ))))
            })
        },
    );
    report("streaming progress path labels", PROGRESS_PATHS, &old, &new);
    assert_improved(&old, &new);
}

fn report(label: &str, items: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{label} checksum diverged");
    println!("{label} ({items} items)");
    print_result("old", items, old);
    print_result("new", items, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% p95  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        improvement(old.median_ns, new.median_ns),
        improvement(old.p95_ns, new.p95_ns),
        improvement(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(throughput(items, old), throughput(items, new)),
        improvement(old.allocated.allocs as f64, new.allocated.allocs as f64),
        improvement(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        improvement(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );
}

fn print_result(label: &str, items: usize, result: &BenchResult) {
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>12.0} items/s  \
         {:>8} alloc  {:>5} realloc  {:>8} free  {:>12} B alloc  {:>12} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        throughput(items, result),
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn(),
    );
}

fn assert_improved(old: &BenchResult, new: &BenchResult) {
    assert!(new.median_ns < old.median_ns, "median regressed");
    assert!(new.p95_ns < old.p95_ns, "p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "cycles regressed");
    }
    assert!(new.allocated.allocs <= old.allocated.allocs);
    assert!(new.allocated.reallocs <= old.allocated.reallocs);
    assert!(new.allocated.frees <= old.allocated.frees);
    assert!(new.allocated.allocated_bytes < old.allocated.allocated_bytes);
    assert!(new.allocated.churn() < old.allocated.churn());
}

fn throughput(items: usize, result: &BenchResult) -> f64 {
    items as f64 * 1e9 / result.median_ns
}

fn improvement(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn percent_change(old: f64, new: f64) -> f64 {
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
