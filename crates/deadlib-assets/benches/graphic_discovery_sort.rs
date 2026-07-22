use deadlib_assets::discover::{
    DiscoveredTexture, sort_discovered_textures_for_bench,
    sort_discovered_textures_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const TEXTURES: usize = 4_096;
const RUNS: usize = 50;
type Sorter = fn(&mut [DiscoveredTexture], bool);

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

// SAFETY: allocation requests are forwarded unchanged; atomics only observe them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
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
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let textures = fixture();
    let mut expected = textures.clone();
    sort_discovered_textures_legacy_for_bench(&mut expected, true);
    let mut actual = textures.clone();
    sort_discovered_textures_for_bench(&mut actual, true);
    assert_eq!(order_checksum(&expected), order_checksum(&actual));

    let old = measure(
        batches(&textures),
        sort_discovered_textures_legacy_for_bench,
    );
    let new = measure(batches(&textures), sort_discovered_textures_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("graphic discovery ordering ({TEXTURES} textures x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture() -> Vec<DiscoveredTexture> {
    (0..TEXTURES)
        .map(|index| {
            let source = index.wrapping_mul(2_053) % TEXTURES;
            let label = match source % 5 {
                0 => format!("JUDGMENT {source:04} Neon"),
                1 => format!("judgment {source:04} neon"),
                2 => format!("Combo {source:04} Metallic"),
                3 => format!("combo {source:04} metallic"),
                _ => "Love".to_string(),
            };
            DiscoveredTexture {
                key: format!("judgments/texture-{source:04}.png"),
                label,
                source_path: format!("assets/graphics/judgments/texture-{source:04}.png"),
            }
        })
        .collect()
}

fn batches(textures: &[DiscoveredTexture]) -> Vec<Vec<DiscoveredTexture>> {
    (0..RUNS).map(|_| textures.to_vec()).collect()
}

fn measure(mut batches: Vec<Vec<DiscoveredTexture>>, sort: Sorter) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for textures in &mut batches {
        sort(black_box(textures), true);
        checksum = checksum.rotate_left(7) ^ order_checksum(textures);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn order_checksum(textures: &[DiscoveredTexture]) -> u64 {
    textures.iter().fold(0_u64, |checksum, texture| {
        checksum.rotate_left(3) ^ texture.key.len() as u64 ^ texture.label.len() as u64
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let items = (TEXTURES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.1} ns/texture {:>8.1} cycles/texture {:>7.2} Mtextures/s",
        result.elapsed.as_secs_f64() * 1.0e9 / items,
        result.cycles as f64 / items,
        items / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per texture, {:.1} bytes/texture",
        result.alloc.allocs as f64 / items,
        result.alloc.reallocs as f64 / items,
        result.alloc.bytes as f64 / items,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
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
