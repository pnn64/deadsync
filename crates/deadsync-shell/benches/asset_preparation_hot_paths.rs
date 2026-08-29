use deadlib_assets::dynamic::{self, BannerCacheOptions, DynamicImagePrewarmJob};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[allow(dead_code, unused_imports)]
#[path = "../src/gameplay_prewarm.rs"]
mod gameplay_prewarm;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
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
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every request is delegated unchanged to `System`; relaxed counters
// observe only this single-threaded benchmark while their gate is enabled.
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
        // SAFETY: the pointer-layout pair came from the allocator caller.
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
    items: usize,
}

fn measure(ops: usize, items: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..ops.min(8) {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut sample_checksum = 0u64;
        for _ in 0..ops {
            sample_checksum = sample_checksum.wrapping_add(black_box(op()));
        }
        let ns = started.elapsed().as_secs_f64() * 1e9 / ops as f64;
        let cycle_end = cycle_counter();
        times.push(ns);
        if let Some(sample_cycles) = cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64)
        {
            cycles.push(sample_cycles);
        }
        checksum ^= sample_checksum;
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..ops {
        black_box(op());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        ops,
        items,
    }
}

fn artwork_paths(count: usize, unique: usize, kind: &str) -> Vec<PathBuf> {
    (0..count)
        .map(|index| {
            PathBuf::from(format!(
                "benchmark/Pack{:02}/Song{:04}/{kind}.png",
                index % 32,
                index % unique
            ))
        })
        .collect()
}

fn old_artwork_plan(banners: &[PathBuf], cdtitles: &[PathBuf]) -> (usize, usize) {
    let opts = BannerCacheOptions { enabled: true };
    let total_paths = banners.len() + cdtitles.len();
    let banner_cache = Path::new("benchmark-banner-cache");
    let cdtitle_cache = Path::new("benchmark-cdtitle-cache");
    let mut counted = HashSet::<String>::with_capacity(total_paths);
    for path in banners {
        counted.insert(dynamic::dynamic_image_prewarm_dedupe_key(
            path,
            opts,
            banner_cache,
        ));
    }
    for path in cdtitles {
        counted.insert(dynamic::dynamic_image_prewarm_dedupe_key(
            path,
            opts,
            cdtitle_cache,
        ));
    }
    let count = counted.len();

    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let mut jobs = Vec::<DynamicImagePrewarmJob>::with_capacity(total_paths);
    dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        banners,
        opts,
        banner_cache,
        "Banner",
    );
    dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        cdtitles,
        opts,
        cdtitle_cache,
        "CDTitle",
    );
    (count, jobs.len())
}

fn old_unique_texture_keys(keys: &[String]) -> usize {
    let mut seen = HashSet::<String>::with_capacity(keys.len());
    keys.iter()
        .filter(|key| seen.insert((*key).clone()))
        .count()
}

fn new_unique_texture_keys(keys: &[String]) -> usize {
    let mut seen = hashbrown::HashSet::<String>::with_capacity(keys.len());
    keys.iter()
        .filter(|key| gameplay_prewarm::insert_texture_key(&mut seen, key))
        .count()
}

fn old_dedupe_dynamic_keys(keys: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::with_capacity(keys.len());
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

fn key_checksum(keys: &[String]) -> u64 {
    keys.iter().fold(keys.len() as u64, |sum, key| {
        sum.wrapping_mul(131).wrapping_add(key.len() as u64)
    })
}

fn main() {
    const ART_OPS: usize = 16;
    const KEY_OPS: usize = 256;

    let banners = artwork_paths(512, 128, "banner");
    let cdtitles = artwork_paths(256, 64, "cdtitle");
    let old_artwork = measure(ART_OPS, banners.len() + cdtitles.len(), || {
        let (counted, planned) = old_artwork_plan(black_box(&banners), black_box(&cdtitles));
        ((counted as u64) << 32) | planned as u64
    });
    let new_artwork = measure(ART_OPS, banners.len() + cdtitles.len(), || {
        let (counted, planned) = deadsync_assets::media_cache::benchmark_artwork_cache_plan(
            black_box(&banners),
            black_box(&cdtitles),
        );
        ((counted as u64) << 32) | planned as u64
    });
    assert_eq!(old_artwork.checksum, new_artwork.checksum);
    print_pair(
        "artwork job planning (768 inputs, 192 unique)",
        &old_artwork,
        &new_artwork,
    );

    let texture_keys = (0..2_048)
        .map(|index| format!("assets/noteskins/default/texture_{:03}.png", index % 128))
        .collect::<Vec<_>>();
    let old_texture = measure(KEY_OPS, texture_keys.len(), || {
        old_unique_texture_keys(black_box(&texture_keys)) as u64
    });
    let new_texture = measure(KEY_OPS, texture_keys.len(), || {
        new_unique_texture_keys(black_box(&texture_keys)) as u64
    });
    assert_eq!(old_texture.checksum, new_texture.checksum);
    print_pair(
        "gameplay texture-key ownership (2048 visits, 128 unique)",
        &old_texture,
        &new_texture,
    );

    let dynamic_keys = (0..512)
        .map(|index| format!("dynamic/media/pack/song/video_{:03}.mp4", index % 96))
        .collect::<Vec<_>>();
    let old_dynamic = measure(KEY_OPS, dynamic_keys.len(), || {
        key_checksum(&old_dedupe_dynamic_keys(black_box(dynamic_keys.clone())))
    });
    let new_dynamic = measure(KEY_OPS, dynamic_keys.len(), || {
        key_checksum(&deadlib_assets::dynamic::dedupe_dynamic_keys(black_box(
            dynamic_keys.clone(),
        )))
    });
    assert_eq!(old_dynamic.checksum, new_dynamic.checksum);
    print_pair(
        "dynamic-media release dedupe (512 inputs, 96 unique)",
        &old_dynamic,
        &new_dynamic,
    );
}

fn print_pair(name: &str, old: &Row, new: &Row) {
    println!("{name}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN)
        ),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    let allocs = row.alloc.allocs as f64 / row.ops as f64;
    let reallocs = row.alloc.reallocs as f64 / row.ops as f64;
    let frees = row.alloc.frees as f64 / row.ops as f64;
    let churn = row.alloc.churn() as f64 / row.ops as f64;
    let throughput = row.items as f64 * 1e9 / row.median_ns;
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>10.0} item/s  \
         {:>7.1} alloc  {:>6.1} realloc  {:>7.1} free  {:>12.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput,
        allocs,
        reallocs,
        frees,
        churn,
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
