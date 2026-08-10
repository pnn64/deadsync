use deadlib_present::cache::{SharedStrCache, cached_shared_str, shared_str_cache_with_capacity};
use deadlib_present::font::{Glyph, GlyphMap};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::BuildHasher;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::LocalKey;
use std::time::{Duration, Instant};

const KEYS: usize = 1_024;
const LOOKUPS: usize = 8_000_000;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

thread_local! {
    static RANDOM_STRINGS: RefCell<SharedStrCache<RandomState>> =
        RefCell::new(HashMap::with_capacity(KEYS));
    static FAST_STRINGS: RefCell<SharedStrCache> =
        RefCell::new(shared_str_cache_with_capacity(KEYS));
}

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; the atomics only
// observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.frees.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplies the allocation's original pointer and layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure_shared<S>(
    cache: &'static LocalKey<RefCell<SharedStrCache<S>>>,
    keys: &[Box<str>],
) -> BenchResult
where
    S: BuildHasher,
{
    for key in keys {
        black_box(cached_shared_str(cache, key, KEYS));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for lookup in 0..LOOKUPS {
        let shared = cached_shared_str(cache, black_box(&keys[lookup % KEYS]), KEYS);
        checksum = checksum.wrapping_add(shared.len() as u64);
        black_box(shared);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn measure_glyphs<S>(glyphs: &HashMap<char, Glyph, S>, keys: &[char]) -> BenchResult
where
    S: BuildHasher,
{
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for lookup in 0..LOOKUPS {
        let key = black_box(keys[lookup % KEYS]);
        checksum = checksum.wrapping_add(glyphs.get(&key).unwrap().advance_i32 as u64);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(name: &str, old: &BenchResult, candidate: &BenchResult) {
    assert_eq!(old.checksum, candidate.checksum);
    println!("{name}");
    print_result("randomized", old);
    print_result("fast", candidate);
    println!(
        "  speedup {:.2}x | cycle reduction {:.1}%",
        old.elapsed.as_secs_f64() / candidate.elapsed.as_secs_f64(),
        100.0 * (1.0 - candidate.cycles as f64 / old.cycles as f64),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<10} {:>7.2} ns/lookup  {:>7.2} cycles/lookup  {:>7.1} Mlookup/s  \
         {} alloc/realloc/free  {} B",
        result.elapsed.as_secs_f64() * 1.0e9 / LOOKUPS as f64,
        result.cycles as f64 / LOOKUPS as f64,
        LOOKUPS as f64 / result.elapsed.as_secs_f64() / 1.0e6,
        result.alloc.allocs + result.alloc.reallocs + result.alloc.frees,
        result.alloc.bytes,
    );
}

fn main() {
    let strings = (0..KEYS)
        .map(|index| format!("screen-label-{index:04}-localized-value").into_boxed_str())
        .collect::<Vec<_>>();
    let glyph_keys = (0..KEYS)
        .map(|index| char::from_u32(0x4e00 + index as u32).unwrap())
        .collect::<Vec<_>>();
    let glyph = Glyph {
        texture_key: Arc::from("bench-glyph"),
        stroke_texture_key: None,
        tex_rect: [0.0; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        size: [16.0; 2],
        offset: [0.0; 2],
        advance: 16.0,
        advance_i32: 16,
    };
    let random_glyphs = HashMap::<char, Glyph>::from_iter(
        glyph_keys.iter().copied().map(|key| (key, glyph.clone())),
    );
    let fast_glyphs =
        GlyphMap::from_iter(glyph_keys.iter().copied().map(|key| (key, glyph.clone())));

    println!("internal hashing ({KEYS} warmed entries x {LOOKUPS} lookups)");
    print_pair(
        "shared actor strings",
        &measure_shared(&RANDOM_STRINGS, &strings),
        &measure_shared(&FAST_STRINGS, &strings),
    );
    print_pair(
        "non-ASCII glyphs",
        &measure_glyphs(&random_glyphs, &glyph_keys),
        &measure_glyphs(&fast_glyphs, &glyph_keys),
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
