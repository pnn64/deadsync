use deadsync_theme_simply_love::screens::components::shared::qr_code::{
    QrCacheBenchFixture, QrRenderBenchFixture, qr_mesh_build_for_bench,
    qr_mesh_build_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SIZE: f32 = 192.0;

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

// SAFETY: allocation operations are forwarded unchanged to `System`; the
// independent atomics only observe successful operations.
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
        // SAFETY: the caller supplies the allocation's original layout.
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
    let contents = (0_usize..64)
        .map(|index| {
            format!(
                "https://scores.deadsync.example/events/2026/player/{index:04}/chart/{:016x}",
                index.wrapping_mul(0x9E37_79B9)
            )
        })
        .collect::<Vec<_>>();
    let cache = QrCacheBenchFixture::new(contents.clone(), SIZE);
    let render = QrRenderBenchFixture::new(&contents[..16], SIZE);

    for index in 0..contents.len() {
        assert_eq!(cache.legacy_hit(index), cache.optimized_hit(index));
        assert_eq!(
            qr_mesh_build_legacy_for_bench(&contents[index], SIZE),
            qr_mesh_build_for_bench(&contents[index], SIZE)
        );
    }
    assert_eq!(render.legacy_traversal(), render.optimized_traversal());

    let old_vertices = render.legacy_vertices();
    let new_vertices = render.optimized_vertices();
    println!(
        "QR render geometry: {old_vertices} -> {new_vertices} vertices ({:.1}% reduction)",
        reduction(old_vertices as u64, new_vertices as u64)
    );

    let old_index = Cell::new(0);
    let new_index = Cell::new(0);
    run_case(
        "warm cache lookup",
        100_000,
        || {
            let index = old_index.get();
            old_index.set(index + 1);
            cache.legacy_hit(index)
        },
        || {
            let index = new_index.get();
            new_index.set(index + 1);
            cache.optimized_hit(index)
        },
    );

    let old_index = Cell::new(0);
    let new_index = Cell::new(0);
    run_case(
        "QR mesh construction",
        400,
        || {
            let index = old_index.get();
            old_index.set(index + 1);
            qr_mesh_build_legacy_for_bench(&contents[index % contents.len()], SIZE)
        },
        || {
            let index = new_index.get();
            new_index.set(index + 1);
            qr_mesh_build_for_bench(&contents[index % contents.len()], SIZE)
        },
    );

    run_case(
        "render-geometry traversal",
        1_000,
        || render.legacy_traversal(),
        || render.optimized_traversal(),
    );
}

fn run_case(
    label: &str,
    sample_operations: usize,
    mut legacy: impl FnMut() -> u64,
    mut optimized: impl FnMut() -> u64,
) {
    const SAMPLES: usize = 5;
    for _ in 0..100 {
        black_box(legacy());
        black_box(optimized());
    }
    let mut old_samples = Vec::with_capacity(SAMPLES);
    let mut new_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (old, new) = if sample % 2 == 0 {
            (
                measure(sample_operations, &mut legacy),
                measure(sample_operations, &mut optimized),
            )
        } else {
            let new = measure(sample_operations, &mut optimized);
            let old = measure(sample_operations, &mut legacy);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        old_samples.push(old);
        new_samples.push(new);
    }
    old_samples.sort_unstable_by_key(|result| result.elapsed);
    new_samples.sort_unstable_by_key(|result| result.elapsed);
    let old = &old_samples[SAMPLES / 2];
    let new = &new_samples[SAMPLES / 2];

    println!("{label} ({SAMPLES} x {sample_operations} operation samples, median)");
    print_result("old", sample_operations, old);
    print_result("new", sample_operations, new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn measure(operations: usize, operation: &mut impl FnMut() -> u64) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for index in 0..operations {
        checksum = checksum
            .rotate_left(5)
            .wrapping_add(black_box(operation()))
            .wrapping_add(index as u64);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, operations: usize, result: &BenchResult) {
    let operations = operations as f64;
    println!(
        "  {label:<4} {:>8.1} ns/op {:>8.1} cycles/op {:>7.2} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per op, {:.1} bytes/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
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
