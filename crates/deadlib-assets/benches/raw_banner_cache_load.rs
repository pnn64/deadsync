use deadlib_assets::dynamic::{
    load_raw_cached_banner_image_direct_for_bench, load_raw_cached_banner_image_legacy_for_bench,
    save_raw_cached_banner_image,
};
use image::{Rgba, RgbaImage};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 2_000;
const BANNER_WIDTH: u32 = 418;
const BANNER_HEIGHT: u32 = 164;

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
// independent atomics only observe successful allocation operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let output = unsafe { System.alloc(layout) };
        if !output.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        output
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let output = unsafe { System.realloc(ptr, old, new_size) };
        if !output.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        output
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
    let fixture = BannerFixture::new();
    let old_image = load_raw_cached_banner_image_legacy_for_bench(&fixture.path)
        .expect("legacy loader should read fixture");
    let new_image = load_raw_cached_banner_image_direct_for_bench(&fixture.path)
        .expect("direct loader should read fixture");
    assert_eq!(old_image, new_image, "pixel output changed");

    let old = measure(&fixture.path, load_raw_cached_banner_image_legacy_for_bench);
    let new = measure(&fixture.path, load_raw_cached_banner_image_direct_for_bench);
    assert_eq!(old.checksum, new.checksum, "loaded banner output changed");

    println!("raw banner cache load ({BANNER_WIDTH}x{BANNER_HEIGHT}, {RUNS} runs)");
    print_result("before: read + split", &old);
    print_result("after: direct payload", &new);
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

fn measure(path: &Path, load: fn(&Path) -> Option<RgbaImage>) -> BenchResult {
    for _ in 0..32 {
        black_box(load(path).expect("warmup load"));
    }
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..RUNS {
        let image = black_box(load(black_box(path)).expect("measured load"));
        checksum = checksum.rotate_left(5) ^ image_checksum(&image);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn image_checksum(image: &RgbaImage) -> u64 {
    let bytes = image.as_raw();
    u64::from(bytes[0])
        ^ (u64::from(bytes[bytes.len() / 2]) << 8)
        ^ (u64::from(bytes[bytes.len() - 1]) << 16)
        ^ ((image.width() as u64) << 32)
        ^ image.height() as u64
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<25} {:>8.2} us/load {:>10.0} cycles/load {:>8.0} loads/s",
        result.elapsed.as_secs_f64() * 1.0e6 / RUNS as f64,
        result.cycles as f64 / RUNS as f64,
        RUNS as f64 / result.elapsed.as_secs_f64(),
    );
    println!(
        "  {:<25} allocs={:.2} reallocs={:.2} bytes={:.0} per load",
        "memory",
        result.alloc.allocs as f64 / RUNS as f64,
        result.alloc.reallocs as f64 / RUNS as f64,
        result.alloc.bytes as f64 / RUNS as f64,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

struct BannerFixture {
    root: PathBuf,
    path: PathBuf,
}

impl BannerFixture {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-raw-banner-bench-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create benchmark fixture directory");
        let path = root.join("banner.rgba");
        let image = RgbaImage::from_fn(BANNER_WIDTH, BANNER_HEIGHT, |x, y| {
            let seed = x.wrapping_mul(31).wrapping_add(y.wrapping_mul(17));
            Rgba([
                seed as u8,
                seed.rotate_left(7) as u8,
                seed.rotate_left(13) as u8,
                255,
            ])
        });
        assert!(save_raw_cached_banner_image(&path, &image));
        Self { root, path }
    }
}

impl Drop for BannerFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
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
