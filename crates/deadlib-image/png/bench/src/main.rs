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

// Consume the native sample representation directly, as an RGBA8 asset consumer
// would. In particular, RGBA8 reuses its allocation and 16-bit samples never pass
// through an intermediate RGBA16 image.
fn native_rgba8(bytes: &[u8]) -> image::RgbaImage {
    use deadlib_image_png::{ColorType, Samples, Transparency};
    let png = deadlib_image_png::decode(bytes).unwrap();
    let image = png.image;
    let (width, height, color, depth) =
        (image.width, image.height, image.color_type, image.bit_depth);
    let mask = ((1u32 << depth) - 1) as u16;
    let transparent_gray = match &png.transparency {
        Some(Transparency::Grayscale(value)) => Some(*value & mask),
        _ => None,
    };
    let transparent_rgb = match &png.transparency {
        Some(Transparency::Rgb(value)) => Some(value.map(|v| v & mask)),
        _ => None,
    };
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height)).unwrap();
    let pixels = match image.samples {
        Samples::U8(samples) if color == ColorType::Rgba => samples,
        Samples::U8(samples) => {
            let mut output: Vec<[u8; 4]> = Vec::with_capacity(pixel_count);
            match color {
                ColorType::Indexed => {
                    let mut palette = [[0, 0, 0, 255]; 256];
                    for (out, &[r, g, b]) in palette.iter_mut().zip(&png.palette) {
                        *out = [r, g, b, 255];
                    }
                    if let Some(Transparency::Palette(alpha)) = &png.transparency {
                        for (out, &alpha) in palette.iter_mut().take(png.palette.len()).zip(alpha) {
                            out[3] = alpha;
                        }
                    }
                    output.extend(samples.into_iter().map(|index| palette[usize::from(index)]));
                }
                ColorType::Grayscale => {
                    let factor = (255 / mask) as u8;
                    output.extend(samples.into_iter().map(|value| {
                        let gray = value * factor;
                        let alpha = if transparent_gray == Some(u16::from(value)) {
                            0
                        } else {
                            255
                        };
                        [gray, gray, gray, alpha]
                    }));
                }
                ColorType::Rgb => {
                    output.extend(samples.as_chunks::<3>().0.iter().map(|rgb| {
                        let alpha = if transparent_rgb == Some(rgb.map(u16::from)) {
                            0
                        } else {
                            255
                        };
                        [rgb[0], rgb[1], rgb[2], alpha]
                    }));
                }
                ColorType::GrayscaleAlpha => {
                    output.extend(
                        samples
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|&[gray, alpha]| [gray, gray, gray, alpha]),
                    );
                }
                ColorType::Rgba => unreachable!(),
            }
            output.into_flattened()
        }
        Samples::U16(samples) => {
            let mut output: Vec<[u8; 4]> = Vec::with_capacity(pixel_count);
            match color {
                ColorType::Grayscale => {
                    output.extend(samples.into_iter().map(|value| {
                        let gray = (value >> 8) as u8;
                        let alpha = if transparent_gray == Some(value) {
                            0
                        } else {
                            255
                        };
                        [gray, gray, gray, alpha]
                    }));
                }
                ColorType::Rgb => {
                    output.extend(samples.as_chunks::<3>().0.iter().map(|rgb| {
                        let alpha = if transparent_rgb == Some(*rgb) {
                            0
                        } else {
                            255
                        };
                        [
                            (rgb[0] >> 8) as u8,
                            (rgb[1] >> 8) as u8,
                            (rgb[2] >> 8) as u8,
                            alpha,
                        ]
                    }));
                }
                ColorType::GrayscaleAlpha => {
                    output.extend(samples.as_chunks::<2>().0.iter().map(|&[gray, alpha]| {
                        let gray = (gray >> 8) as u8;
                        [gray, gray, gray, (alpha >> 8) as u8]
                    }));
                }
                ColorType::Rgba => {
                    output.extend(
                        samples
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|pixel| pixel.map(|value| (value >> 8) as u8)),
                    );
                }
                ColorType::Indexed => unreachable!("PNG has no 16-bit palette indices"),
            }
            output.into_flattened()
        }
    };
    image::RgbaImage::from_raw(width, height, pixels).unwrap()
}

fn unpack_palette_rgba<const N: usize>(
    row: &[u8],
    pixels: &mut [[u8; 4]],
    palette: &[[u8; 4]; 256],
) {
    let index = |byte: u8, i| (byte >> (8 - (i + 1) * (8 / N))) & (u8::MAX >> (8 - 8 / N));
    let (groups, tail) = pixels.as_chunks_mut::<N>();
    for (pixels, &byte) in groups.iter_mut().zip(row) {
        *pixels = std::array::from_fn(|i| palette[index(byte, i) as usize]);
    }
    if !tail.is_empty() {
        let byte = row[groups.len()];
        for (i, pixel) in tail.iter_mut().enumerate() {
            *pixel = palette[index(byte, i) as usize];
        }
    }
}

// Match deadlib-assets::decode_png_from_reader with color_key=false. Memory input
// removes filesystem I/O equally from all adapters. The loader deliberately
// stops at next_frame rather than finishing the PNG stream.
fn png_assets_rgba8(bytes: &[u8]) -> image::RgbaImage {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    let max_alloc = image::Limits::default().max_alloc.unwrap_or(u64::MAX);
    decoder.set_limits(png::Limits {
        bytes: max_alloc.min(usize::MAX as u64) as usize,
    });
    let header = decoder.read_header_info().unwrap();
    assert!(u64::from(header.width) * u64::from(header.height) <= max_alloc / 4);
    let indexed = header.color_type == png::ColorType::Indexed;
    decoder.set_transformations(
        png::Transformations::STRIP_16
            | if indexed {
                png::Transformations::IDENTITY
            } else {
                png::Transformations::EXPAND
            },
    );
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    if info.color_type == png::ColorType::Rgba {
        bytes.truncate(info.buffer_size());
        return image::RgbaImage::from_raw(info.width, info.height, bytes).unwrap();
    }
    let mut output = image::RgbaImage::new(info.width, info.height);
    let pixels = output.as_mut().as_chunks_mut::<4>().0;
    if indexed {
        let palette = reader.info().palette.as_deref();
        let alpha = reader.info().trns.as_deref();
        let mut colors = [[0, 0, 0, 255]; 256];
        for (i, rgb) in palette
            .unwrap_or_default()
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
        {
            colors[i] = [
                rgb[0],
                rgb[1],
                rgb[2],
                alpha.and_then(|a| a.get(i)).copied().unwrap_or(255),
            ];
        }
        if info.bit_depth == png::BitDepth::Eight {
            for (pixel, &index) in pixels.iter_mut().zip(&bytes[..info.buffer_size()]) {
                *pixel = colors[usize::from(index)];
            }
        } else {
            for (pixels, row) in pixels
                .chunks_exact_mut(info.width as usize)
                .zip(bytes.chunks_exact(info.line_size))
            {
                match info.bit_depth {
                    png::BitDepth::One => unpack_palette_rgba::<8>(row, pixels, &colors),
                    png::BitDepth::Two => unpack_palette_rgba::<4>(row, pixels, &colors),
                    png::BitDepth::Four => unpack_palette_rgba::<2>(row, pixels, &colors),
                    _ => unreachable!(),
                }
            }
        }
    } else {
        let bytes = &bytes[..info.buffer_size()];
        match info.color_type {
            png::ColorType::Rgb => {
                for (pixel, rgb) in pixels.iter_mut().zip(bytes.as_chunks::<3>().0) {
                    *pixel = [rgb[0], rgb[1], rgb[2], 255];
                }
            }
            png::ColorType::Grayscale => {
                for (pixel, &gray) in pixels.iter_mut().zip(bytes) {
                    *pixel = [gray, gray, gray, 255];
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for (pixel, &[gray, alpha]) in pixels.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *pixel = [gray, gray, gray, alpha];
                }
            }
            png::ColorType::Rgba | png::ColorType::Indexed => unreachable!(),
        }
    }
    output
}

fn image_rgba8(bytes: &[u8]) -> image::RgbaImage {
    use image::DynamicImage;
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).unwrap();
    let (width, height) = (decoded.width(), decoded.height());
    if decoded.color().bits_per_pixel() / u16::from(decoded.color().channel_count()) != 16 {
        return decoded.into_rgba8();
    }
    // image's normal u16->u8 conversion rounds. The asset loader instead keeps
    // the high byte, after png has matched any full-precision tRNS value.
    let mut pixels: Vec<[u8; 4]> =
        Vec::with_capacity(usize::try_from(u64::from(width) * u64::from(height)).unwrap());
    match decoded {
        DynamicImage::ImageLuma16(image) => {
            pixels.extend(image.into_raw().into_iter().map(|value| {
                let gray = (value >> 8) as u8;
                [gray, gray, gray, 255]
            }));
        }
        DynamicImage::ImageLumaA16(image) => {
            pixels.extend(
                image
                    .as_raw()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&[gray, alpha]| {
                        let gray = (gray >> 8) as u8;
                        [gray, gray, gray, (alpha >> 8) as u8]
                    }),
            );
        }
        DynamicImage::ImageRgb16(image) => {
            pixels.extend(image.as_raw().as_chunks::<3>().0.iter().map(|rgb| {
                [
                    (rgb[0] >> 8) as u8,
                    (rgb[1] >> 8) as u8,
                    (rgb[2] >> 8) as u8,
                    255,
                ]
            }));
        }
        DynamicImage::ImageRgba16(image) => {
            pixels.extend(
                image
                    .as_raw()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|pixel| pixel.map(|value| (value >> 8) as u8)),
            );
        }
        _ => unreachable!("PNG decoding returns integer samples"),
    }
    image::RgbaImage::from_raw(width, height, pixels.into_flattened()).unwrap()
}

fn verify_rgba8(bytes: &[u8]) {
    let expected = png_assets_rgba8(bytes);
    assert_eq!(
        native_rgba8(bytes),
        expected,
        "native RGBA8 differs from the asset loader"
    );
    assert_eq!(
        image_rgba8(bytes),
        expected,
        "image RGBA8 differs from the asset loader"
    );
}

fn decode(codec: usize, bytes: &[u8], rgba8: bool) {
    if rgba8 {
        black_box(match codec {
            0 => native_rgba8(black_box(bytes)),
            1 => png_assets_rgba8(black_box(bytes)),
            2 => image_rgba8(black_box(bytes)),
            _ => unreachable!(),
        });
        return;
    }
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
    let mut verify_only = false;
    let mut rgba8 = false;
    for arg in std::env::args_os().skip(1) {
        if arg == "--verify-only" {
            verify_only = true;
        } else if arg == "--rgba8" {
            rgba8 = true;
        } else {
            files(Path::new(&arg), &mut paths);
        }
    }
    paths.sort();
    assert!(!paths.is_empty(), "supply PNG files or corpus directories");
    if verify_only {
        for path in &paths {
            eprintln!("verifying {}", path.display());
            let bytes =
                std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            verify(&bytes);
            if rgba8 {
                verify_rgba8(&bytes);
            }
            let buffered = deadlib_image_png::decode(&bytes).unwrap();
            let streamed =
                deadlib_image_png::decode_reader(bytes.as_slice(), &Default::default()).unwrap();
            assert!(
                buffered == streamed,
                "{}: buffered and reader PNG decoding differ",
                path.display()
            );
        }
        println!(
            "Verified {} PNG files against png samples, image RGBA16, and native reader decoding.",
            paths.len()
        );
        if rgba8 {
            println!("All three RGBA8 adapters match the asset loader's pixel output.");
        }
        return;
    }
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
    let codecs: &[&str] = if rgba8 {
        &["native-rgba8", "png-assets-rgba8", "image-rgba8"]
    } else {
        &["native", "png", "image", "png-packed"]
    };
    println!("file\tcodec\tns\tthread_cycles\talloc_calls\trequested_bytes\tpixels");
    for (file_index, path) in paths.into_iter().enumerate() {
        let bytes = std::fs::read(&path).unwrap();
        verify(&bytes);
        if rgba8 {
            verify_rgba8(&bytes);
        }
        let pixels = u64::from(u32::from_be_bytes(bytes[16..20].try_into().unwrap()))
            * u64::from(u32::from_be_bytes(bytes[20..24].try_into().unwrap()));
        for offset in 0..codecs.len() {
            let codec = (offset + file_index) % codecs.len();
            decode(codec, &bytes, rgba8);
            #[cfg(feature = "allocations")]
            {
                use std::sync::atomic::Ordering::Relaxed;
                allocations::CALLS.store(0, Relaxed);
                allocations::BYTES.store(0, Relaxed);
                allocations::ENABLED.store(true, Relaxed);
                decode(codec, &bytes, rgba8);
                allocations::ENABLED.store(false, Relaxed);
                println!(
                    "{}\t{}\t0\t0\t{}\t{}\t{pixels}",
                    path.display(),
                    codecs[codec],
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
                            decode(codec, &bytes, rgba8);
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
                    "{}\t{}\t{ns}\t{cycles}\t0\t0\t{pixels}",
                    path.display(),
                    codecs[codec]
                );
            }
        }
    }
}
