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
        fn GetCurrentProcess() -> *mut c_void;
        #[cfg(not(feature = "allocations"))]
        fn QueryProcessCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
        fn SetProcessAffinityMask(process: *mut c_void, mask: usize) -> i32;
    }
    pub fn pin() {
        assert_ne!(unsafe { SetProcessAffinityMask(GetCurrentProcess(), 1) }, 0);
    }
    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
        assert_ne!(
            unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut value) },
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

use deadlib_image_jpeg as native;
use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
const CODECS: [&str; 4] = ["native", "zune", "jpeg-decoder", "image"];
fn options(rgba8: bool) -> native::DecodeOptions {
    native::DecodeOptions {
        strict: !rgba8,
        ..native::DecodeOptions::default()
    }
}
fn native_pixels(bytes: &[u8], rgba8: bool) -> Result<Vec<u8>, String> {
    let image = native::decode_with_options(bytes, options(rgba8), native::Upsampling::Linear)
        .map_err(|e| e.to_string())?;
    match image.samples {
        native_samples::Samples::U8(pixels) => Ok(pixels),
        _ => Err("8-bit comparison requires 8-bit native output".into()),
    }
}
fn into_rgba8(pixels: Vec<u8>, gray: bool) -> Vec<u8> {
    let rgba: Vec<[u8; 4]> = if gray {
        pixels.iter().map(|&v| [v, v, v, 255]).collect()
    } else {
        pixels
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| [r, g, b, 255])
            .collect()
    };
    rgba.into_flattened()
}
fn pixels(codec: usize, bytes: &[u8], gray: bool, rgba8: bool) -> Result<Vec<u8>, String> {
    let pixels = match codec {
        0 => native_pixels(bytes, rgba8),
        1 => {
            let options = DecoderOptions::default()
                .set_max_width(65535)
                .set_max_height(65535)
                .jpeg_set_out_colorspace(if gray {
                    ColorSpace::Luma
                } else {
                    ColorSpace::RGB
                });
            zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options)
                .decode()
                .map_err(|e| format!("{e:?}"))
        }
        2 => {
            let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
            let pixels = decoder.decode().map_err(|e| e.to_string())?;
            match decoder.info().unwrap().pixel_format {
                jpeg_decoder::PixelFormat::L8 | jpeg_decoder::PixelFormat::RGB24 => Ok(pixels),
                jpeg_decoder::PixelFormat::CMYK32 => {
                    let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
                    for pixel in pixels.as_chunks::<4>().0 {
                        let k = u16::from(255 - pixel[3]);
                        rgb.extend(
                            pixel[..3]
                                .iter()
                                .map(|&c| ((u16::from(255 - c) * k + 127) / 255) as u8),
                        );
                    }
                    Ok(rgb)
                }
                format => Err(format!("pixel adapter not implemented: {format:?}")),
            }
        }
        3 => {
            let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())?;
            if rgba8 {
                return Ok(image.into_rgba8().into_raw());
            }
            match image {
                image::DynamicImage::ImageLuma8(pixels) => Ok(pixels.into_raw()),
                image::DynamicImage::ImageRgb8(pixels) => Ok(pixels.into_raw()),
                image => Err(format!("unexpected output: {:?}", image.color())),
            }
        }
        _ => unreachable!(),
    }?;
    Ok(if rgba8 {
        into_rgba8(pixels, gray)
    } else {
        pixels
    })
}
struct Stats {
    total_pixels: usize,
    gray: bool,
    available: [bool; 4],
    native: Vec<u8>,
    differences: Vec<(usize, u8, f64)>,
    unavailable: Vec<(usize, String)>,
}
fn verify(bytes: &[u8], rgba8: bool) -> Result<Stats, String> {
    let coefficients = native::decode_coefficients_with_options(bytes, options(rgba8))
        .map_err(|e| e.to_string())?;
    if coefficients.frame.precision != 8 {
        return Err("higher-precision comparison pending; native output is preserved".into());
    }
    let gray = coefficients.color_space() == native::ColorSpace::Grayscale;
    let actual = pixels(0, bytes, gray, rgba8)?;
    let mut stats = Stats {
        total_pixels: coefficients.frame.width as usize * coefficients.frame.height as usize,
        gray,
        available: [true, false, false, false],
        native: actual,
        differences: Vec::new(),
        unavailable: Vec::new(),
    };
    assert_eq!(
        stats.native.len(),
        stats.total_pixels
            * if rgba8 {
                4
            } else if gray {
                1
            } else {
                3
            }
    );
    for codec in 1..CODECS.len() {
        match pixels(codec, bytes, gray, rgba8) {
            Ok(expected) => {
                assert_eq!(stats.native.len(), expected.len());
                let max = stats
                    .native
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                let square: u64 = stats
                    .native
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| u64::from(a.abs_diff(*b)).pow(2))
                    .sum();
                stats.differences.push((
                    codec,
                    max,
                    (square as f64 / expected.len() as f64).sqrt(),
                ));
                stats.available[codec] = true;
            }
            Err(error) => stats.unavailable.push((codec, error)),
        }
    }
    Ok(stats)
}
fn decode(codec: usize, bytes: &[u8], gray: bool, rgba8: bool) {
    black_box(pixels(codec, black_box(bytes), gray, rgba8).unwrap());
}
fn files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for e in std::fs::read_dir(path).unwrap() {
            files(&e.unwrap().path(), out);
        }
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
    {
        out.push(path.to_owned());
    }
}
fn main() {
    let mut paths = Vec::new();
    let mut rgba8 = false;
    let mut verify_only = false;
    let mut native_only = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--rgba8" {
            rgba8 = true;
        } else if arg == "--verify-only" {
            verify_only = true;
        } else if arg == "--native-only" {
            native_only = true;
        } else if arg == "--files-from" {
            let list = args.next().expect("--files-from needs a path");
            for line in std::fs::read_to_string(list)
                .unwrap()
                .trim_start_matches('\u{feff}')
                .lines()
            {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    files(Path::new(line), &mut paths);
                }
            }
        } else {
            assert!(
                !arg.to_string_lossy().starts_with('-'),
                "unknown option: {arg:?}"
            );
            files(Path::new(&arg), &mut paths);
        }
    }
    paths.sort();
    paths.dedup();
    assert!(!paths.is_empty(), "supply JPEG files or corpus directories");
    eprintln!(
        "MODE\t{}\tstrict={}\tfiles={}",
        if rgba8 { "rgba8" } else { "raw" },
        !rgba8,
        paths.len()
    );
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
    let sample_ms = std::env::var("JPEG_BENCH_MS").map_or(20, |v| v.parse::<u128>().unwrap());
    println!("file\tcodec\tpixels\tns\tprocess_cycles\talloc_calls\trequested_bytes");
    for (file_index, path) in paths.into_iter().enumerate() {
        let bytes = std::fs::read(&path).unwrap();
        let stats = match verify(&bytes, rgba8) {
            Ok(stats) => stats,
            Err(error) => {
                eprintln!("EXCLUDED\t{}\t{error}", path.display());
                continue;
            }
        };
        eprintln!(
            "VERIFIED\t{}\t{}\t{}",
            path.display(),
            stats.total_pixels,
            stats.native.len()
        );
        for &(codec, max, rmse) in &stats.differences {
            eprintln!(
                "PIXELS\t{}\t{}\t{max}\t{rmse:.6}",
                path.display(),
                CODECS[codec]
            );
        }
        for (codec, error) in &stats.unavailable {
            eprintln!(
                "UNAVAILABLE\t{}\t{}\t{error}",
                path.display(),
                CODECS[*codec]
            );
        }
        if verify_only {
            continue;
        }
        #[cfg(not(feature = "allocations"))]
        let group_size = if stats.total_pixels > 4_000_000 { 1 } else { 8 };
        for offset in 0..CODECS.len() {
            let codec = (offset + file_index) % CODECS.len();
            if !stats.available[codec] || (native_only && codec != 0) {
                continue;
            }
            decode(codec, &bytes, stats.gray, rgba8);
            #[cfg(feature = "allocations")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                allocations::CALLS.store(0, Relaxed);
                allocations::BYTES.store(0, Relaxed);
                allocations::ENABLED.store(true, Relaxed);
                decode(codec, &bytes, stats.gray, rgba8);
                allocations::ENABLED.store(false, Relaxed);
                println!(
                    "{}\t{}\t{}\t0\t0\t{}\t{}",
                    path.display(),
                    CODECS[codec],
                    stats.total_pixels,
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
                        for _ in 0..group_size {
                            decode(codec, &bytes, stats.gray, rgba8);
                        }
                        iterations += group_size;
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
                    "{}\t{}\t{}\t{ns}\t{cycles}\t0\t0",
                    path.display(),
                    CODECS[codec],
                    stats.total_pixels
                );
            }
        }
    }
}
