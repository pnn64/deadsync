//! Encode already-owned, opaque RGBA8 pixels; input preparation and verification
//! are outside measurement. Every mode returns and destroys a fresh PNG Vec.

use deadlib_image_png::{ColorType, Image, Png, Samples};
use image::ImageEncoder;
#[cfg(not(feature = "allocations"))]
use std::time::{Duration, Instant};
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

    // Measurement only: all pointers, layouts, and sizes are forwarded unchanged
    // to System; the counters never allocate or change the allocator contract.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size(), Relaxed);
            }
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(layout.size(), Relaxed);
            }
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            if ENABLED.load(Relaxed) {
                CALLS.fetch_add(1, Relaxed);
                BYTES.fetch_add(size, Relaxed);
            }
            unsafe { System.realloc(ptr, layout, size) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
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
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
        #[cfg(not(feature = "allocations"))]
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }

    pub fn pin() {
        // The pseudo-handle belongs to this thread; CPU 0 is in its group.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) },
            0,
            "could not pin the benchmark thread to logical CPU 0"
        );
    }

    #[cfg(not(feature = "allocations"))]
    pub fn read() -> u64 {
        let mut value = 0;
        // The output pointer is valid for the duration of this synchronous call.
        assert_ne!(
            unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
            0,
            "QueryThreadCycleTime failed"
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

const MODES: [&str; 10] = [
    "native-buffered",
    "native-writer",
    "image-default",
    "png-fast-adaptive",
    "png-balanced-adaptive",
    "native-rle-buffered",
    "native-rle-writer",
    "native-rle-up-buffered",
    "native-rle-up-writer",
    "png-fast-up",
];

fn pixels(png: &Png) -> &[u8] {
    match &png.image.samples {
        Samples::U8(bytes) => bytes,
        Samples::U16(_) => unreachable!("benchmark inputs are RGBA8"),
    }
}

fn encode(mode: usize, input: &Png) -> Vec<u8> {
    let width = input.image.width;
    let height = input.image.height;
    if mode == 0 {
        return deadlib_image_png::encode(input, &Default::default())
            .expect("native buffered encoding failed");
    }
    if mode == 5 || mode == 7 {
        return deadlib_image_png::encode(
            input,
            &deadlib_image_png::EncodeOptions {
                compression: deadlib_image_png::zlib::Compression::Rle,
                filter: if mode == 7 {
                    deadlib_image_png::Filter::Up
                } else {
                    deadlib_image_png::Filter::Adaptive
                },
                ..Default::default()
            },
        )
        .expect("native Rle buffered encoding failed");
    }
    let mut output = Vec::new();
    match mode {
        1 => {
            deadlib_image_png::encode_writer(input, &Default::default(), &mut output)
                .expect("native writer encoding failed");
        }
        2 => {
            // image 0.25.10's ImageBuffer::save_with_format(Png) selects this
            // encoder: Fast compression and Adaptive filtering. Its filesystem,
            // BufWriter, and boxed format dispatch are outside this core workload.
            image::codecs::png::PngEncoder::new(&mut output)
                .write_image(
                    pixels(input),
                    width,
                    height,
                    image::ExtendedColorType::Rgba8,
                )
                .expect("image default PNG encoding failed");
        }
        3 | 4 | 9 => {
            let mut encoder = png::Encoder::new(&mut output, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(if mode == 4 {
                png::Compression::Balanced
            } else {
                png::Compression::Fast
            });
            encoder.set_filter(if mode == 9 {
                png::Filter::Up
            } else {
                png::Filter::Adaptive
            });
            let mut writer = encoder.write_header().expect("png header encoding failed");
            writer
                .write_image_data(pixels(input))
                .expect("png pixel encoding failed");
            writer.finish().expect("png finalization failed");
        }
        6 | 8 => {
            deadlib_image_png::encode_writer(
                input,
                &deadlib_image_png::EncodeOptions {
                    compression: deadlib_image_png::zlib::Compression::Rle,
                    filter: if mode == 8 {
                        deadlib_image_png::Filter::Up
                    } else {
                        deadlib_image_png::Filter::Adaptive
                    },
                    ..Default::default()
                },
                &mut output,
            )
            .expect("native Rle writer encoding failed");
        }
        _ => unreachable!("unknown encoder mode"),
    }
    output
}

fn run(mode: usize, input: &Png) {
    drop(black_box(encode(mode, black_box(input))));
}

fn verify(name: &str, mode: usize, input: &Png) -> (usize, u64) {
    let bytes = encode(mode, input);
    let mut reader = png::Decoder::new(Cursor::new(&bytes))
        .read_info()
        .unwrap_or_else(|error| panic!("{name}: {} png header: {error}", MODES[mode]));
    let mut decoded = vec![
        0;
        reader
            .output_buffer_size()
            .expect("PNG output size overflow")
    ];
    let info = reader
        .next_frame(&mut decoded)
        .unwrap_or_else(|error| panic!("{name}: {} png pixels: {error}", MODES[mode]));
    reader
        .finish()
        .unwrap_or_else(|error| panic!("{name}: {} png trailer: {error}", MODES[mode]));
    assert_eq!(
        (info.width, info.height, info.color_type, info.bit_depth),
        (
            input.image.width,
            input.image.height,
            png::ColorType::Rgba,
            png::BitDepth::Eight
        ),
        "{name}: {} PNG dimensions or format differ",
        MODES[mode]
    );
    assert!(
        &decoded[..info.buffer_size()] == pixels(input),
        "{name}: {} pixels differ in png decoder",
        MODES[mode]
    );
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap_or_else(|error| panic!("{name}: {} image decode: {error}", MODES[mode]))
        .into_rgba8();
    assert_eq!(
        decoded.dimensions(),
        (input.image.width, input.image.height),
        "{name}: {} image dimensions differ",
        MODES[mode]
    );
    assert!(
        decoded.as_raw().as_slice() == pixels(input),
        "{name}: {} pixels differ in image decoder",
        MODES[mode]
    );
    // Stable FNV-1a fingerprint of the encoded bytes, never part of timing or
    // allocation counting. This is a comparison aid, not a cryptographic digest.
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    eprintln!(
        "VERIFIED\t{name}\t{}\t{}x{}\t{} bytes\t{hash:016x}",
        MODES[mode],
        input.image.width,
        input.image.height,
        bytes.len()
    );
    (bytes.len(), hash)
}

fn rgba8(width: u32, height: u32, mut bytes: Vec<u8>) -> Png {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    let image = Image {
        width,
        height,
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: Samples::U8(bytes),
    };
    image.validate().expect("invalid prepared RGBA8 input");
    Png::new(image)
}

fn load(path: &Path) -> Png {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .into_rgba8();
    rgba8(image.width(), image.height(), image.into_raw())
}

fn generated(pattern: &str, width: u32, height: u32) -> Png {
    let size = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .expect("generated RGBA8 byte count overflow");
    let mut bytes = vec![0; size];
    let mut random = 0x6d2b_79f5_u32 ^ width.wrapping_mul(0x9e37_79b9) ^ height;
    for (index, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (index % width as usize) as u32;
        let y = (index / width as usize) as u32;
        let rgb = match pattern {
            "flat" => [34, 40, 52],
            "gradient" => {
                let r = (u64::from(x) * 255 / u64::from((width - 1).max(1))) as u8;
                let g = (u64::from(y) * 255 / u64::from((height - 1).max(1))) as u8;
                [r, g, ((u16::from(r) + u16::from(g)) / 2) as u8]
            }
            "noise" => {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                [random as u8, (random >> 8) as u8, (random >> 16) as u8]
            }
            "ui" => {
                // Resolution-independent panels, text-like strokes, and chart
                // bars produce a deterministic screenshot-shaped workload.
                let sx = (u64::from(x) * 1280 / u64::from(width)) as u32;
                let sy = (u64::from(y) * 720 / u64::from(height)) as u32;
                let cx = sx.saturating_sub(196) % 260;
                let cy = sy.saturating_sub(64) % 212;
                if sy < 48 {
                    if (19..29).contains(&sy) && sx % 120 < 72 {
                        [190, 198, 212]
                    } else {
                        [34, 39, 49]
                    }
                } else if sx < 180 {
                    if (21..150).contains(&sx) && sy % 40 < 5 {
                        [121, 136, 160]
                    } else {
                        [27, 31, 40]
                    }
                } else if sy < 64 || sx < 196 || cx >= 244 || cy >= 196 {
                    [17, 20, 27]
                } else if cy < 32 {
                    [45, 53, 68]
                } else if (17..200).contains(&cx) && cy < 92 && cy % 13 < 3 {
                    [174, 183, 201]
                } else if cx > 16 && cy > 108 && cy > 180 - (cx / 24) * 7 {
                    [58, 130 + (cx / 24) as u8 * 8, 204]
                } else {
                    [29, 35, 46]
                }
            }
            _ => unreachable!("unknown generated pattern"),
        };
        *pixel = [rgb[0], rgb[1], rgb[2], 255];
    }
    rgba8(width, height, bytes)
}

fn files(path: &Path, out: &mut Vec<PathBuf>) {
    let metadata =
        std::fs::metadata(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    if metadata.is_dir() {
        for entry in
            std::fs::read_dir(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        {
            let child = entry.expect("could not read directory entry").path();
            if child.is_dir()
                || child
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                files(&child, out);
            }
        }
    } else {
        assert!(
            metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png")),
            "{} is not a PNG file or directory",
            path.display()
        );
        out.push(path.to_owned());
    }
}

struct Options {
    roots: Vec<PathBuf>,
    generated: bool,
    sizes: Vec<(u32, u32)>,
    modes: Vec<usize>,
    filter: Option<String>,
    verify_only: bool,
}

fn options() -> Option<Options> {
    let mut options = Options {
        roots: Vec::new(),
        generated: false,
        sizes: vec![(640, 360), (1920, 1080), (3840, 2160)],
        modes: (0..MODES.len()).collect(),
        filter: None,
        verify_only: false,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "encode [--verify-only] [--modes NAME,...] [--filter TEXT] \
                     [--generated] [--sizes WIDTHxHEIGHT,...] [PNG files/directories...]\n\
                     No input paths: generate flat, gradient, ui, and noise at 640x360, \
                     1920x1080, and 3840x2160. --generated adds these to supplied paths; \
                     --sizes replaces their sizes and implies --generated (e.g. 17x13,1920x1080).\n\
                     --filter matches a case-sensitive substring of the normalized input name.\n\
                     Modes: {}\n\
                     PNG_BENCH_MS: minimum milliseconds per batch (default 20, five batches).\n\
                     Timing/allocation columns are zero in --verify-only; allocation builds \
                     zero timing columns. Thread cycles are zero outside Windows.\n\
                     Alpha is set to 255 before verification and measurement. Input decoding, \
                     generation, and encoded FNV-1a hashing are outside measurement.",
                    MODES.join(",")
                );
                return None;
            }
            Some("--verify-only") => options.verify_only = true,
            Some("--generated") => options.generated = true,
            Some("--filter") => {
                options.filter = Some(
                    args.next()
                        .expect("--filter needs TEXT")
                        .into_string()
                        .expect("--filter must be UTF-8"),
                );
            }
            Some("--modes") => {
                let value = args
                    .next()
                    .expect("--modes needs NAME,...")
                    .into_string()
                    .expect("--modes must be UTF-8");
                options.modes.clear();
                for name in value.split(',') {
                    let mode = MODES
                        .iter()
                        .position(|&mode| mode == name)
                        .unwrap_or_else(|| {
                            panic!("unknown mode {name:?}; expected {}", MODES.join(","))
                        });
                    if !options.modes.contains(&mode) {
                        options.modes.push(mode);
                    }
                }
            }
            Some("--sizes") => {
                let value = args
                    .next()
                    .expect("--sizes needs WIDTHxHEIGHT,...")
                    .into_string()
                    .expect("--sizes must be UTF-8");
                options.sizes = value
                    .split(',')
                    .map(|size| {
                        let (width, height) = size
                            .split_once('x')
                            .unwrap_or_else(|| panic!("invalid generated size {size:?}"));
                        let width = width.parse::<u32>().expect("invalid generated width");
                        let height = height.parse::<u32>().expect("invalid generated height");
                        assert!(
                            (1..=0x7fff_ffff).contains(&width)
                                && (1..=0x7fff_ffff).contains(&height),
                            "generated dimensions must be in 1..=2147483647"
                        );
                        (width, height)
                    })
                    .collect();
                options.sizes.sort_unstable();
                options.sizes.dedup();
                options.generated = true;
            }
            Some("--") => {
                options.roots.extend(args.map(PathBuf::from));
                break;
            }
            Some(flag) if flag.starts_with('-') => panic!("unknown option {flag:?}; use --help"),
            _ => options.roots.push(PathBuf::from(arg)),
        }
    }
    options.generated |= options.roots.is_empty();
    Some(options)
}

#[cfg(feature = "allocations")]
fn calibrate_allocator() {
    use std::{
        alloc::{GlobalAlloc, Layout},
        sync::atomic::Ordering::Relaxed,
    };
    allocations::CALLS.store(0, Relaxed);
    allocations::BYTES.store(0, Relaxed);
    allocations::ENABLED.store(true, Relaxed);
    // Each pointer is checked, then reallocated/freed using its actual layout.
    unsafe {
        let first = Layout::from_size_align(31, 8).unwrap();
        let grown = Layout::from_size_align(96, 8).unwrap();
        let zeroed = Layout::from_size_align(17, 8).unwrap();
        let ptr = black_box(ALLOCATOR.alloc(first));
        assert!(!ptr.is_null(), "allocator calibration allocation failed");
        let ptr = black_box(ALLOCATOR.realloc(ptr, first, grown.size()));
        assert!(!ptr.is_null(), "allocator calibration reallocation failed");
        let other = black_box(ALLOCATOR.alloc_zeroed(zeroed));
        assert!(
            !other.is_null(),
            "allocator calibration zeroed allocation failed"
        );
        ALLOCATOR.dealloc(ptr, grown);
        ALLOCATOR.dealloc(other, zeroed);
    }
    allocations::ENABLED.store(false, Relaxed);
    assert_eq!(
        allocations::CALLS.load(Relaxed),
        3,
        "allocator calibration calls"
    );
    assert_eq!(
        allocations::BYTES.load(Relaxed),
        144,
        "allocator calibration bytes"
    );
}

#[cfg(not(feature = "allocations"))]
fn measure(mode: usize, input: &Png, sample_ms: u64) -> (u64, u64, usize, usize) {
    let mut wall = [0; 5];
    let mut cpu = [0; 5];
    let group = if u64::from(input.image.width) * u64::from(input.image.height) >= 1_000_000 {
        1
    } else {
        8
    };
    for batch in 0..5 {
        let start_cycles = cycles::read();
        let start = Instant::now();
        let mut iterations = 0;
        loop {
            for _ in 0..group {
                run(mode, input);
            }
            iterations += group;
            if start.elapsed() >= Duration::from_millis(sample_ms) {
                break;
            }
        }
        wall[batch] = start.elapsed().as_nanos() as u64 / iterations;
        cpu[batch] = (cycles::read() - start_cycles) / iterations;
    }
    wall.sort_unstable();
    cpu.sort_unstable();
    (wall[2], cpu[2], 0, 0)
}

#[cfg(feature = "allocations")]
fn measure(mode: usize, input: &Png, _sample_ms: u64) -> (u64, u64, usize, usize) {
    use std::sync::atomic::Ordering::Relaxed;
    allocations::CALLS.store(0, Relaxed);
    allocations::BYTES.store(0, Relaxed);
    allocations::ENABLED.store(true, Relaxed);
    run(mode, input);
    allocations::ENABLED.store(false, Relaxed);
    (
        0,
        0,
        allocations::CALLS.load(Relaxed),
        allocations::BYTES.load(Relaxed),
    )
}

fn benchmark(name: &str, input: &Png, options: &Options, index: usize, sample_ms: u64) {
    assert!(
        !name.contains(['\t', '\r', '\n']),
        "input name cannot contain TSV control characters"
    );
    eprintln!(
        "VERIFYING\t{name}\t{}x{}",
        input.image.width, input.image.height
    );
    // Verify every selected encoder before timing any encoder for this input.
    // These bytes, their decoding, and their fingerprints are never measured.
    let verified: Vec<_> = options
        .modes
        .iter()
        .map(|&mode| verify(name, mode, input))
        .collect();
    let width = input.image.width;
    let height = input.image.height;
    let pixels = u64::from(width) * u64::from(height);
    for offset in 0..options.modes.len() {
        let selected = (offset + index) % options.modes.len();
        let mode = options.modes[selected];
        let (encoded_bytes, hash) = verified[selected];
        let (ns, cycles, calls, requested) = if options.verify_only {
            (0, 0, 0, 0)
        } else {
            run(mode, input);
            measure(mode, input, sample_ms)
        };
        let ns_per_mpix = ns as f64 * 1_000_000.0 / pixels as f64;
        let mpix_s = if ns == 0 {
            0.0
        } else {
            pixels as f64 * 1000.0 / ns as f64
        };
        println!(
            "{name}\t{}\t{width}\t{height}\t{pixels}\t{encoded_bytes}\t{hash:016x}\t{ns}\t{cycles}\t{ns_per_mpix:.3}\t{mpix_s:.3}\t{calls}\t{requested}",
            MODES[mode]
        );
    }
}

fn main() {
    let Some(options) = options() else { return };
    let sample_ms = std::env::var("PNG_BENCH_MS").map_or(20, |value| {
        value
            .parse::<u64>()
            .expect("PNG_BENCH_MS must be a positive integer")
    });
    assert!(sample_ms > 0, "PNG_BENCH_MS must be positive");
    let mut paths = Vec::new();
    for root in &options.roots {
        files(root, &mut paths);
    }
    paths.sort();
    paths.dedup();
    if !options.verify_only {
        cycles::pin();
        #[cfg(feature = "allocations")]
        calibrate_allocator();
    }
    eprintln!(
        "Opaque RGBA8 encode core; five median batches of at least {sample_ms} ms; fresh owned PNG outputs including destruction. Verification, input preparation, and encoded FNV-1a fingerprints are outside measurement."
    );
    eprintln!(
        "Native defaults: Fast/Adaptive. image 0.25.10 default: Fast/Adaptive. png 0.18.1 controls: Fast/Adaptive and Balanced/Adaptive. Allocation builds and --verify-only emit zero timing columns."
    );
    println!(
        "file\tcodec\twidth\theight\tpixels\tencoded_bytes\tencoded_fnv1a64\tns\tthread_cycles\tns_per_mpix\tmpix_s\talloc_calls\trequested_bytes"
    );
    let mut count = 0;
    for path in paths {
        let name = path.to_string_lossy().replace('\\', "/");
        if options
            .filter
            .as_ref()
            .is_none_or(|filter| name.contains(filter.as_str()))
        {
            benchmark(&name, &load(&path), &options, count, sample_ms);
            count += 1;
        }
    }
    if options.generated {
        for &(width, height) in &options.sizes {
            for pattern in ["flat", "gradient", "ui", "noise"] {
                let name = format!("generated/{pattern}-{width}x{height}");
                if options
                    .filter
                    .as_ref()
                    .is_none_or(|filter| name.contains(filter.as_str()))
                {
                    benchmark(
                        &name,
                        &generated(pattern, width, height),
                        &options,
                        count,
                        sample_ms,
                    );
                    count += 1;
                }
            }
        }
    }
    assert!(
        count > 0,
        "no PNG or generated inputs matched the selection"
    );
    eprintln!(
        "Completed {count} inputs, {} modes per input; no decoder or encoder failures excluded.",
        options.modes.len()
    );
}
