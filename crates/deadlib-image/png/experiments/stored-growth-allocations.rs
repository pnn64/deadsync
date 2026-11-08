//! Untimed allocation instrumentation; not part of the native codec.
//! All unsafe operations delegate unchanged requests to std::alloc::System.
//! Logical live-byte peaks exclude allocator overhead and realloc-internal overlap.
use deadlib_image_png::{Samples, decode, zlib};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
};

struct Meter;
#[global_allocator]
static ALLOCATOR: Meter = Meter;
static ENABLED: AtomicBool = AtomicBool::new(false);
static BAD: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static ZEROED: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCS: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn add(bytes: usize) {
    let previous = LIVE.fetch_add(bytes, Relaxed);
    let live = previous.wrapping_add(bytes);
    if live < previous {
        BAD.store(true, Relaxed);
    }
    PEAK.fetch_max(live, Relaxed);
}
fn subtract(bytes: usize) {
    if LIVE.fetch_sub(bytes, Relaxed) < bytes {
        BAD.store(true, Relaxed);
    }
}
// SAFETY: Every pointer/layout request is forwarded to System unchanged. Hooks
// use only atomics, never allocate or unwind, and do not access the allocation.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if ENABLED.load(Relaxed) {
            if ptr.is_null() {
                FAILED.fetch_add(1, Relaxed);
            } else {
                ALLOCS.fetch_add(1, Relaxed);
                ALLOCATED_BYTES.fetch_add(layout.size(), Relaxed);
                add(layout.size());
            }
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if ENABLED.load(Relaxed) {
            if ptr.is_null() {
                FAILED.fetch_add(1, Relaxed);
            } else {
                ZEROED.fetch_add(1, Relaxed);
                ALLOCATED_BYTES.fetch_add(layout.size(), Relaxed);
                add(layout.size());
            }
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        if ENABLED.load(Relaxed) {
            DEALLOCS.fetch_add(1, Relaxed);
            subtract(layout.size());
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, size) };
        if ENABLED.load(Relaxed) {
            if result.is_null() {
                FAILED.fetch_add(1, Relaxed);
            } else {
                REALLOCS.fetch_add(1, Relaxed);
                REALLOCATED_BYTES.fetch_add(size, Relaxed);
                if size >= layout.size() {
                    add(size - layout.size());
                } else {
                    subtract(layout.size() - size);
                }
            }
        }
        result
    }
}

#[derive(Debug)]
struct Stats {
    allocs: usize,
    zeroed: usize,
    reallocs: usize,
    deallocs: usize,
    failed: usize,
    allocated_bytes: usize,
    reallocated_bytes: usize,
    live: usize,
    peak: usize,
}
fn begin() {
    assert!(!ENABLED.load(Relaxed));
    for counter in [
        &ALLOCS,
        &ZEROED,
        &REALLOCS,
        &DEALLOCS,
        &FAILED,
        &ALLOCATED_BYTES,
        &REALLOCATED_BYTES,
        &LIVE,
        &PEAK,
    ] {
        counter.store(0, Relaxed);
    }
    BAD.store(false, Relaxed);
    ENABLED.store(true, Relaxed);
}
fn end() -> Stats {
    ENABLED.store(false, Relaxed);
    assert!(
        !BAD.load(Relaxed),
        "deallocation crossed a measurement boundary"
    );
    Stats {
        allocs: ALLOCS.load(Relaxed),
        zeroed: ZEROED.load(Relaxed),
        reallocs: REALLOCS.load(Relaxed),
        deallocs: DEALLOCS.load(Relaxed),
        failed: FAILED.load(Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Relaxed),
        reallocated_bytes: REALLOCATED_BYTES.load(Relaxed),
        live: LIVE.load(Relaxed),
        peak: PEAK.load(Relaxed),
    }
}
fn measure<T>(run: impl FnOnce() -> T) -> (T, Stats) {
    begin();
    let result = black_box(run());
    let stats = end();
    assert_eq!(stats.failed, 0);
    (result, stats)
}
fn calibrate() {
    let original = Layout::from_size_align(31, 8).unwrap();
    let grown = Layout::from_size_align(96, 8).unwrap();
    let zeroed = Layout::from_size_align(17, 8).unwrap();
    begin();
    // SAFETY: These direct allocator calls use nonzero valid layouts; successful
    // realloc consumes the original block, and both resulting blocks are freed
    // exactly once using their matching layouts. No pointer is dereferenced.
    unsafe {
        let ptr = ALLOCATOR.alloc(original);
        assert!(!ptr.is_null());
        let larger = ALLOCATOR.realloc(ptr, original, grown.size());
        assert!(!larger.is_null());
        let zeros = ALLOCATOR.alloc_zeroed(zeroed);
        assert!(!zeros.is_null());
        black_box((larger, zeros));
        ALLOCATOR.dealloc(zeros, zeroed);
        ALLOCATOR.dealloc(larger, grown);
    }
    let stats = end();
    assert_eq!(
        (stats.allocs, stats.zeroed, stats.reallocs, stats.deallocs),
        (1, 1, 1, 2)
    );
    assert_eq!(
        (
            stats.allocated_bytes,
            stats.reallocated_bytes,
            stats.live,
            stats.peak
        ),
        (48, 96, 0, 113)
    );
    eprintln!("Allocator calibration passed: {stats:?}");
}
fn print_stats(case: &str, operation: &str, bytes: usize, stats: &Stats) {
    println!(
        "{case}\t{operation}\t{bytes}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        stats.allocs,
        stats.zeroed,
        stats.reallocs,
        stats.deallocs,
        stats.failed,
        stats.allocated_bytes,
        stats.reallocated_bytes,
        stats.peak,
        stats.live
    );
}

fn main() {
    calibrate();
    let mut args = std::env::args_os().skip(1);
    let corpus = PathBuf::from(args.next().expect("corpus directory"));
    let output = PathBuf::from(args.next().expect("sample output directory"));
    assert!(args.next().is_none());
    std::fs::create_dir(&output).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(corpus)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    println!(
        "case\toperation\tsample_bytes\talloc_calls\tzeroed_calls\trealloc_calls\tdealloc_calls\tfailed_calls\tallocated_requested_bytes\treallocated_requested_bytes\tpeak_requested_live_bytes\tretained_requested_live_bytes"
    );
    for path in files {
        let input = std::fs::read(&path).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap();
        drop(decode(&input).unwrap());
        {
            let (result, stats) = measure(|| decode(&input));
            let png = result.unwrap();
            assert!(png.animation.is_none());
            let samples: Vec<u8> = match &png.image.samples {
                Samples::U8(values) => values.clone(),
                Samples::U16(values) => values.iter().flat_map(|v| v.to_be_bytes()).collect(),
            };
            print_stats(name, "decode", samples.len(), &stats);
            std::fs::write(output.join(format!("{name}.samples")), samples).unwrap();
        }
        let mut idat = Vec::new();
        let mut offset = 8;
        while offset < input.len() {
            let size = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
            if &input[offset + 4..offset + 8] == b"IDAT" {
                idat.extend_from_slice(&input[offset + 8..offset + 8 + size]);
            }
            offset += size + 12;
        }
        let mut reference = Vec::new();
        flate2::read::ZlibDecoder::new(idat.as_slice())
            .read_to_end(&mut reference)
            .unwrap();
        drop(zlib::decompress(&idat, reference.len()).unwrap());
        let (result, stats) = measure(|| zlib::decompress(&idat, reference.len()));
        let raw = result.unwrap();
        assert_eq!(raw, reference);
        assert_eq!(stats.live, raw.capacity());
        print_stats(name, "inflate", raw.len(), &stats);
    }
}
