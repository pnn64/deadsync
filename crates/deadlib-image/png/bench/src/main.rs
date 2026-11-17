#[cfg(not(feature = "allocations"))]
use std::time::Instant;
use std::{
    hint::black_box,
    io::Cursor,
    path::{Path, PathBuf},
};

#[cfg(feature = "allocations")]
mod allocations {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    };
    pub static ENABLED: AtomicBool = AtomicBool::new(false);
    pub static CALLS: AtomicUsize = AtomicUsize::new(0);
    pub static BYTES: AtomicUsize = AtomicUsize::new(0);
    pub struct Allocator;
    // Measurement only: every operation forwards the unchanged System contract.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(l.size(), Relaxed);
            }
            unsafe { System.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(l.size(), Relaxed);
            }
            unsafe { System.alloc_zeroed(l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(n, Relaxed);
            }
            unsafe { System.realloc(p, l, n) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
    }
}
#[cfg(feature = "allocations")]
#[global_allocator]
static ALLOCATOR: allocations::Allocator = allocations::Allocator;

#[cfg(windows)]
mod cycles {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        #[cfg(not(feature = "allocations"))]
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    }
    pub fn pin() {
        assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) }, 0);
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
        assert_ne!(
            unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
            0
        );
        value
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn pin() {}
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        0
    }
}

fn reference(bytes: &[u8]) -> (png::OutputInfo, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut out).unwrap();
    reader.finish().unwrap();
    out.truncate(info.buffer_size());
    (info, out)
}

fn verify(bytes: &[u8]) {
    let native = deadlib_image_png::decode(bytes).unwrap();
    let (info, expected) = reference(bytes);
    assert_eq!(
        (native.image.width, native.image.height),
        (info.width, info.height)
    );
    assert_eq!(native.image.color_type as u8, info.color_type as u8);
    assert_eq!(native.image.bit_depth, info.bit_depth as u8);
    match &native.image.samples {
        deadlib_image_png::Samples::U16(v) => {
            assert!(
                v.iter().copied().eq(expected
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_be_bytes([b[0], b[1]])))
            );
        }
        deadlib_image_png::Samples::U8(v) if info.bit_depth == png::BitDepth::Eight => {
            assert_eq!(v, &expected)
        }
        deadlib_image_png::Samples::U8(v) => {
            let depth = native.image.bit_depth as usize;
            let width = info.width as usize;
            for (row, samples) in expected.chunks(info.line_size).zip(v.chunks(width)) {
                for (x, &sample) in samples.iter().enumerate() {
                    assert_eq!(
                        sample,
                        (row[x * depth / 8] >> (8 - depth - x * depth % 8)) & ((1 << depth) - 1)
                    );
                }
            }
        }
    }
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
    assert_eq!(
        native.to_rgba16(Default::default()).unwrap().samples,
        deadlib_image_png::Samples::U16(image.to_rgba16().into_raw())
    );
}

fn unpack_reference<const DEPTH: usize>(info: &png::OutputInfo, packed: &[u8], samples: &mut [u8]) {
    for (row, dst) in packed
        .chunks(info.line_size)
        .zip(samples.chunks_mut(info.width as usize))
    {
        for (&byte, group) in row.iter().zip(dst.chunks_mut(8 / DEPTH)) {
            for (n, sample) in group.iter_mut().enumerate() {
                *sample = (byte >> (8 - DEPTH - n * DEPTH)) & ((1 << DEPTH) - 1);
            }
        }
    }
}

fn decode(codec: usize, bytes: &[u8]) {
    match codec {
        0 => {
            black_box(deadlib_image_png::decode(black_box(bytes)).unwrap());
        }
        1 => {
            let (info, mut out) = reference(black_box(bytes));
            if info.bit_depth == png::BitDepth::Sixteen {
                black_box(
                    out.as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| u16::from_be_bytes([b[0], b[1]]))
                        .collect::<Vec<_>>(),
                );
            } else if (info.bit_depth as u8) < 8 {
                let width = info.width as usize;
                let mut samples = vec![0; width * info.height as usize];
                match info.bit_depth {
                    png::BitDepth::One => unpack_reference::<1>(&info, &out, &mut samples),
                    png::BitDepth::Two => unpack_reference::<2>(&info, &out, &mut samples),
                    png::BitDepth::Four => unpack_reference::<4>(&info, &out, &mut samples),
                    _ => unreachable!(),
                }
                black_box(samples);
            } else {
                black_box(&mut out);
            }
        }
        2 => {
            black_box(
                image::load_from_memory_with_format(black_box(bytes), image::ImageFormat::Png)
                    .unwrap(),
            );
        }
        3 => {
            black_box(reference(black_box(bytes)));
        }
        _ => unreachable!(),
    }
}

fn files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            files(&entry.unwrap().path(), out);
        }
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        out.push(path.to_owned());
    }
}

fn main() {
    let mut paths = Vec::new();
    for arg in std::env::args_os().skip(1) {
        files(Path::new(&arg), &mut paths);
    }
    paths.sort();
    assert!(!paths.is_empty(), "supply PNG files or corpus directories");
    cycles::pin();
    #[cfg(feature = "allocations")]
    {
        use std::{
            alloc::{GlobalAlloc, Layout},
            sync::atomic::Ordering::Relaxed,
        };
        allocations::ENABLED.store(true, Relaxed);
        // Exercise allocation, zeroed allocation, reallocation and freeing.
        unsafe {
            let a = black_box(ALLOCATOR.alloc(Layout::from_size_align(31, 8).unwrap()));
            assert!(!a.is_null());
            let a = black_box(ALLOCATOR.realloc(a, Layout::from_size_align(31, 8).unwrap(), 96));
            assert!(!a.is_null());
            let b = black_box(ALLOCATOR.alloc_zeroed(Layout::from_size_align(17, 8).unwrap()));
            assert!(!b.is_null());
            ALLOCATOR.dealloc(a, Layout::from_size_align(96, 8).unwrap());
            ALLOCATOR.dealloc(b, Layout::from_size_align(17, 8).unwrap());
        }
        allocations::ENABLED.store(false, Relaxed);
        assert_eq!(allocations::CALLS.load(Relaxed), 3);
        assert_eq!(allocations::BYTES.load(Relaxed), 144);
    }
    #[cfg(not(feature = "allocations"))]
    let sample_ms = std::env::var("PNG_BENCH_MS").map_or(20, |v| v.parse::<u128>().unwrap());
    println!("file\tcodec\tns\tthread_cycles\talloc_calls\trequested_bytes");
    for (file_index, path) in paths.into_iter().enumerate() {
        let bytes = std::fs::read(&path).unwrap();
        verify(&bytes);
        for offset in 0..4 {
            let codec = (offset + file_index) % 4;
            decode(codec, &bytes);
            #[cfg(feature = "allocations")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                allocations::CALLS.store(0, Relaxed);
                allocations::BYTES.store(0, Relaxed);
                allocations::ENABLED.store(true, Relaxed);
                decode(codec, &bytes);
                allocations::ENABLED.store(false, Relaxed);
                println!(
                    "{}\t{}\t0\t0\t{}\t{}",
                    path.display(),
                    ["native", "png", "image", "png-packed"][codec],
                    allocations::CALLS.load(Relaxed),
                    allocations::BYTES.load(Relaxed)
                );
            }
            #[cfg(not(feature = "allocations"))]
            {
                let mut timings = Vec::new();
                for _ in 0..5 {
                    let start_cycles = cycles::read();
                    let start = Instant::now();
                    let mut iterations = 0;
                    loop {
                        for _ in 0..8 {
                            decode(codec, &bytes);
                        }
                        iterations += 8;
                        if start.elapsed().as_millis() >= sample_ms {
                            break;
                        }
                    }
                    let elapsed = start.elapsed().as_nanos() as u64 / iterations;
                    let cycles = (cycles::read() - start_cycles) / iterations;
                    timings.push((elapsed, cycles));
                }
                let mut wall: Vec<_> = timings.iter().map(|&(ns, _)| ns).collect();
                let mut cpu: Vec<_> = timings.iter().map(|&(_, cycles)| cycles).collect();
                wall.sort_unstable();
                cpu.sort_unstable();
                let (ns, cycles) = (wall[2], cpu[2]);
                println!(
                    "{}\t{}\t{ns}\t{cycles}\t0\t0",
                    path.display(),
                    ["native", "png", "image", "png-packed"][codec]
                );
            }
        }
    }
}
