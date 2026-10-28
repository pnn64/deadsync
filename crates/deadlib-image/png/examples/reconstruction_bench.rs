//! CPU cycles and untimed allocation counts for decoding and RGBA expansion.
use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, EncodeOptions, Filter, Frame, FrameControl, Image,
    Limits, Png, Samples, Transparency, decode, decode_reader, encode,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

#[allow(dead_code)]
#[path = "../tests/support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "../tests/support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/cpu.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../tests/support/compositor.rs"]
mod oracle;

struct Counting;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static REALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every operation forwards the original pointer and layout to System.
// The counters never access the allocation or change the allocator contract.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size, Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn fixture(width: u32, height: u32, color: ColorType, depth: u8) -> Png {
    let count = width as usize * height as usize * color.channels();
    let mask = (1u32 << depth) - 1;
    let mut random = 0x4fe3_a279u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        (random & mask) as u16
    };
    Png::new(Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16((0..count).map(|_| next()).collect())
        } else {
            Samples::U8((0..count).map(|_| next() as u8).collect())
        },
    })
}

fn fingerprint(samples: &Samples) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |value: u16| {
        hash = (hash ^ u64::from(value)).wrapping_mul(0x100_0000_01b3);
    };
    match samples {
        Samples::U8(v) => v.iter().for_each(|&v| add(u16::from(v))),
        Samples::U16(v) => v.iter().for_each(|&v| add(v)),
    }
    hash
}

fn measure<T>(case: &str, operation: &str, bytes: usize, hash: u64, mut run: impl FnMut() -> T) {
    let iterations: usize = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .unwrap_or_else(|_| "12".into())
        .parse()
        .unwrap();
    assert!(iterations > 0);
    let iterations = iterations.max(if operation == "decode-reader" && bytes <= 65536 {
        64
    } else if bytes < 4096 {
        2048
    } else {
        1
    });
    for _ in 0..3 {
        black_box(run());
    }
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    black_box(run());
    TRACK.store(false, Relaxed);
    let counts = (
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed),
    );
    let mut samples = Vec::new();
    for _ in 0..7 {
        let start_cycles = cpu::cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(run());
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cycles = (cpu::cycles() - start_cycles) as f64 / iterations as f64;
        samples.push((cycles, ns));
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let (cycles, ns) = samples[3];
    let rate = bytes as f64 * 1e9 / ns / 1048576.0;
    println!(
        "{case}\t{operation}\t{iterations}\t{bytes}\t{cycles:.3}\t{ns:.3}\t{rate:.3}\t{}\t{}\t{}\t{hash:016x}",
        counts.0, counts.1, counts.2,
    );
}

fn main() {
    cpu::pin_thread();
    println!(
        "case\toperation\titerations\tsample_bytes\tcycles_per_image\tns_per_image\tMiB_per_second\tallocations\treallocations\trequested_bytes\tfingerprint"
    );
    let stage = std::env::var("DEADLIB_PNG_BENCH_STAGE");
    if stage.as_deref() == Ok("reader-buffers") {
        reader_buffers();
        return;
    }
    if stage.as_deref() == Ok("decoder-lanes") {
        decoder_lanes();
        return;
    }
    if matches!(
        stage.as_deref(),
        Ok("encode" | "rows" | "bitwords" | "rowbuffers")
    ) {
        encoding(stage.as_deref() != Ok("encode"));
        return;
    }
    for (name, color) in [
        ("gray", ColorType::Grayscale),
        ("ga", ColorType::GrayscaleAlpha),
        ("rgb", ColorType::Rgb),
        ("rgba", ColorType::Rgba),
    ] {
        for depth in [8, 16] {
            let mut png = fixture(513, 257, color, depth);
            let expected = png.to_rgba16(Limits::default()).unwrap();
            assert_eq!(
                expected.samples,
                Samples::U16(
                    oracle::expand(&png, &png.image)
                        .into_iter()
                        .flatten()
                        .collect()
                )
            );
            let hash = fingerprint(&expected.samples);
            measure(
                &format!("{name}{depth}"),
                "rgba16",
                expected.samples.len() * 2,
                hash,
                || black_box(&png).to_rgba16(Limits::default()).unwrap(),
            );
            png.animation = Some(Animation {
                plays: 0,
                default_is_frame: true,
                frames: vec![Frame {
                    control: FrameControl {
                        width: png.image.width,
                        height: png.image.height,
                        x_offset: 0,
                        y_offset: 0,
                        delay_num: 1,
                        delay_den: 10,
                        dispose: DisposeOp::None,
                        blend: BlendOp::Source,
                    },
                    image: None,
                }],
            });
            let mut compositor = png.compositor(Limits::default()).unwrap();
            assert_eq!(
                compositor.next_frame().unwrap().unwrap().samples,
                match &expected.samples {
                    Samples::U16(v) => v,
                    _ => unreachable!(),
                }
            );
            measure(
                &format!("{name}{depth}"),
                "apng-source",
                expected.samples.len() * 2,
                hash,
                || {
                    compositor.reset();
                    black_box(compositor.next_frame().unwrap().unwrap().samples);
                },
            );
        }
    }
    for palette_len in [2, 256] {
        for (width, height) in [(1, 1), (17, 1), (255, 1), (256, 1), (257, 1), (513, 257)] {
            let mut png = fixture(width, height, ColorType::Indexed, 8);
            png.palette = (0..palette_len)
                .map(|n| [n as u8, (n * 71 + 19) as u8, (n * 113 + 27) as u8])
                .collect();
            png.transparency = Some(Transparency::Palette(
                (0..palette_len / 2).map(|n| (n * 73) as u8).collect(),
            ));
            let expected = Samples::U16(
                oracle::expand(&png, &png.image)
                    .into_iter()
                    .flatten()
                    .collect(),
            );
            assert_eq!(png.to_rgba16(Limits::default()).unwrap().samples, expected);
            measure(
                &format!("indexed8-{palette_len}-{width}x{height}"),
                "rgba16",
                expected.len() * 2,
                fingerprint(&expected),
                || black_box(&png).to_rgba16(Limits::default()).unwrap(),
            );
        }
    }
    for depth in [8, 16] {
        for background in ["transparent", "translucent", "opaque", "mixed"] {
            let mut png = fixture(513, 257, ColorType::Rgba, depth);
            let mut foreground = fixture(513, 257, ColorType::Rgba, depth).image;
            let max = ((1u32 << depth) - 1) as u16;
            let alpha = |n| match background {
                "transparent" => 0,
                "translucent" => max / 2,
                "opaque" => max,
                _ => [0, max / 2, max][n % 3],
            };
            match &mut png.image.samples {
                Samples::U8(v) => v
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .enumerate()
                    .for_each(|(n, p)| p[3] = alpha(n) as u8),
                Samples::U16(v) => v
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .enumerate()
                    .for_each(|(n, p)| p[3] = alpha(n)),
            }
            match &mut foreground.samples {
                Samples::U8(v) => v
                    .iter_mut()
                    .enumerate()
                    .filter(|(n, _)| n % 4 != 3)
                    .for_each(|(_, v)| *v = !*v),
                Samples::U16(v) => v
                    .iter_mut()
                    .enumerate()
                    .filter(|(n, _)| n % 4 != 3)
                    .for_each(|(_, v)| *v = !*v),
            }
            let control = FrameControl {
                width: png.image.width,
                height: png.image.height,
                x_offset: 0,
                y_offset: 0,
                delay_num: 1,
                delay_den: 10,
                dispose: DisposeOp::None,
                blend: BlendOp::Source,
            };
            png.animation = Some(Animation {
                plays: 0,
                default_is_frame: true,
                frames: vec![
                    Frame {
                        control,
                        image: None,
                    },
                    Frame {
                        control: FrameControl {
                            blend: BlendOp::Over,
                            ..control
                        },
                        image: Some(foreground),
                    },
                ],
            });
            let expected = oracle::render(&png);
            let mut compositor = png.compositor(Limits::default()).unwrap();
            for frame in &expected {
                assert_eq!(compositor.next_frame().unwrap().unwrap().samples, frame);
            }
            measure(
                &format!("rgba{depth}-over-{background}"),
                "apng-over",
                expected[1].len() * 4,
                fingerprint(&Samples::U16(expected[1].clone())),
                || {
                    compositor.reset();
                    black_box(compositor.next_frame().unwrap().unwrap().samples);
                    black_box(compositor.next_frame().unwrap().unwrap().samples);
                },
            );
        }
    }
    for depth in [1, 2, 4, 8, 16] {
        let png = fixture(4097, 257, ColorType::Grayscale, depth);
        let bytes = encode(
            &png,
            &EncodeOptions {
                filter: Filter::None,
                ..Default::default()
            },
        )
        .unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.image, png.image);
        assert_eq!(
            decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
            decoded
        );
        measure(
            &format!("gray{depth}-large"),
            "decode-reader",
            png.image.samples.len() * if depth == 16 { 2 } else { 1 },
            fingerprint(&decoded.image.samples),
            || decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap(),
        );
        measure(
            &format!("gray{depth}-large"),
            "decode",
            png.image.samples.len() * if depth == 16 { 2 } else { 1 },
            fingerprint(&decoded.image.samples),
            || decode(black_box(&bytes)).unwrap(),
        );
    }
    for filter in [
        Filter::None,
        Filter::Sub,
        Filter::Up,
        Filter::Average,
        Filter::Paeth,
    ] {
        let png = fixture(33, 1, ColorType::Rgba, 8);
        let bytes = encode(
            &png,
            &EncodeOptions {
                filter,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode(&bytes).unwrap().image, png.image);
        measure(
            &format!("rgba8-short-{}", filter as u8),
            "decode",
            png.image.samples.len(),
            fingerprint(&png.image.samples),
            || decode(black_box(&bytes)).unwrap(),
        );
    }
}

fn encoding(rows: bool) {
    let mut cases = Vec::new();
    let rowbuffers = std::env::var("DEADLIB_PNG_BENCH_STAGE").as_deref() == Ok("rowbuffers");
    if rowbuffers {
        for (color, depth) in [
            (ColorType::Grayscale, 8),
            (ColorType::Rgba, 8),
            (ColorType::Rgba, 16),
        ] {
            for interlaced in [false, true] {
                cases.push((513, 257, color, depth, interlaced, Filter::Sub));
            }
        }
        cases.push((33, 9, ColorType::Rgba, 8, true, Filter::Sub));
        for depth in [8, 16] {
            for filter in [Filter::Up, Filter::Adaptive] {
                cases.push((513, 257, ColorType::Rgba, depth, false, filter));
            }
        }
    }
    let filters = if rows {
        &[Filter::None][..]
    } else {
        &[
            Filter::None,
            Filter::Sub,
            Filter::Up,
            Filter::Average,
            Filter::Paeth,
            Filter::Adaptive,
        ][..]
    };
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
    ] {
        for depth in [8, 16] {
            for &filter in filters {
                cases.push((513, 257, color, depth, false, filter));
            }
            cases.push((
                513,
                257,
                color,
                depth,
                true,
                if rows { Filter::None } else { Filter::Adaptive },
            ));
        }
    }
    for depth in [1, 2, 4] {
        for interlaced in [false, true] {
            for filter in [Filter::None, Filter::Adaptive] {
                if rows && filter != Filter::None {
                    continue;
                }
                cases.push((4097, 129, ColorType::Grayscale, depth, interlaced, filter));
            }
        }
    }
    for width in [1, 9, 33] {
        for filter in [Filter::None, Filter::Adaptive] {
            if rows && filter != Filter::None {
                continue;
            }
            cases.push((width, 9, ColorType::Rgba, 8, true, filter));
        }
    }
    for (width, height, color, depth, interlaced, filter) in cases {
        let png = fixture(width, height, color, depth);
        let opts = EncodeOptions {
            interlaced,
            filter,
            compression: deadlib_image_png::zlib::Compression::Stored,
            ..Default::default()
        };
        let expected = apng_recovery::filtered_with(&png.image, interlaced, filter);
        let hash = fingerprint(&Samples::U8(expected.clone()));
        let buffered = encode(&png, &opts).unwrap();
        let mut streamed = Vec::new();
        deadlib_image_png::encode_writer(&png, &opts, &mut streamed).unwrap();
        assert_eq!(apng_recovery::inflate_idat(&buffered), expected);
        assert_eq!(apng_recovery::inflate_idat(&streamed), expected);
        assert_eq!(decode(&buffered).unwrap().image, png.image);
        assert_eq!(decode(&streamed).unwrap().image, png.image);
        let case = format!(
            "c{}-d{depth}-{width}x{height}-i{}-f{}",
            color as u8,
            u8::from(interlaced),
            filter as u8
        );
        measure(&case, "encode", expected.len(), hash, || {
            encode(black_box(&png), black_box(&opts)).unwrap()
        });
        if rowbuffers || matches!(filter, Filter::None | Filter::Adaptive) {
            measure(&case, "encode-writer", expected.len(), hash, || {
                let mut output = Vec::new();
                deadlib_image_png::encode_writer(black_box(&png), black_box(&opts), &mut output)
                    .unwrap();
                output
            });
        }
    }
    if rows {
        first_rows();
    }
}

fn first_rows() {
    let mut cases = Vec::new();
    if matches!(
        std::env::var("DEADLIB_PNG_BENCH_STAGE").as_deref(),
        Ok("bitwords" | "rowbuffers")
    ) {
        for (color, depth) in [
            (ColorType::Grayscale, 8),
            (ColorType::Rgba, 8),
            (ColorType::Rgba, 16),
        ] {
            cases.push((4097, 257, color, depth, false, Filter::None));
        }
    }
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
    ] {
        for depth in [8, 16] {
            for filter in [
                Filter::None,
                Filter::Sub,
                Filter::Up,
                Filter::Average,
                Filter::Paeth,
            ] {
                cases.push((4097, 1, color, depth, false, filter));
            }
        }
    }
    for depth in [8, 16] {
        cases.push((513, 257, ColorType::Rgba, depth, false, Filter::Paeth));
        for filter in [Filter::Average, Filter::Paeth] {
            cases.push((513, 9, ColorType::Rgba, depth, true, filter));
        }
    }
    for depth in [1, 2, 4] {
        cases.push((4097, 1, ColorType::Grayscale, depth, false, Filter::Paeth));
    }
    for (width, height, color, depth, interlaced, filter) in cases {
        let png = fixture(width, height, color, depth);
        let options = EncodeOptions {
            interlaced,
            filter,
            compression: deadlib_image_png::zlib::Compression::Stored,
            ..Default::default()
        };
        let bytes = encode(&png, &options).unwrap();
        assert_eq!(
            apng_recovery::inflate_idat(&bytes),
            apng_recovery::filtered_with(&png.image, interlaced, filter)
        );
        assert_eq!(
            decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            png.image
        );
        let hash = fingerprint(&Samples::U8(bytes.clone()));
        let case = format!(
            "first-c{}-d{depth}-{width}x{height}-i{}-f{}",
            color as u8,
            u8::from(interlaced),
            filter as u8
        );
        measure(
            &case,
            "decode-reader",
            png.image.samples.len() * if depth == 16 { 2 } else { 1 },
            hash,
            || decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap(),
        );
    }
}

fn decoder_lanes() {
    let mut cases = Vec::new();
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
    ] {
        for depth in [8, 16] {
            for filter in [Filter::None, Filter::Up] {
                cases.push((4097, 1, color, depth, false, filter));
            }
            for (interlaced, filter) in [
                (false, Filter::Sub),
                (true, Filter::None),
                (true, Filter::Paeth),
            ] {
                cases.push((513, 257, color, depth, interlaced, filter));
            }
        }
    }
    for depth in [1, 2, 4] {
        cases.push((513, 257, ColorType::Grayscale, depth, true, Filter::None));
    }
    for depth in [8, 16] {
        for filter in [Filter::None, Filter::Sub] {
            cases.push((33, 9, ColorType::Rgba, depth, true, filter));
        }
    }
    for (width, height, color, depth, interlaced, filter) in cases {
        let png = fixture(width, height, color, depth);
        let bytes = encode(
            &png,
            &EncodeOptions {
                interlaced,
                filter,
                compression: deadlib_image_png::zlib::Compression::Stored,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            apng_recovery::inflate_idat(&bytes),
            apng_recovery::filtered_with(&png.image, interlaced, filter)
        );
        assert_eq!(decode(&bytes).unwrap().image, png.image);
        assert_eq!(
            decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            png.image
        );
        let case = format!(
            "lanes-c{}-d{depth}-{width}x{height}-i{}-f{}",
            color as u8,
            u8::from(interlaced),
            filter as u8
        );
        let hash = fingerprint(&Samples::U8(bytes.clone()));
        let count = png.image.samples.len() * if depth == 16 { 2 } else { 1 };
        measure(&case, "decode", count, hash, || {
            decode(black_box(&bytes)).unwrap()
        });
        measure(&case, "decode-reader", count, hash, || {
            decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap()
        });
    }
}

fn reader_buffers() {
    use deadlib_image_png::zlib::Compression;
    let mut cases = Vec::new();
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
        ColorType::Indexed,
    ] {
        for depth in [8, 16] {
            if color.allows_depth(depth) {
                for (width, height) in [(513, 257), (33, 9)] {
                    for interlaced in [false, true] {
                        cases.push((width, height, color, depth, interlaced, Filter::None));
                    }
                }
            }
        }
    }
    for depth in [1, 2, 4] {
        for interlaced in [false, true] {
            cases.push((33, 9, ColorType::Grayscale, depth, interlaced, Filter::None));
        }
    }
    for depth in [8, 16] {
        cases.push((513, 257, ColorType::Rgba, depth, true, Filter::Paeth));
        cases.push((4097, 257, ColorType::Rgba, depth, false, Filter::None));
        cases.push((1, 1, ColorType::Rgba, depth, false, Filter::None));
    }
    for (width, height, color, depth, interlaced, filter) in cases {
        for (mode, compression) in [("stored", Compression::Stored), ("fast", Compression::Fast)] {
            if width == 4097 && mode == "fast" {
                continue;
            }
            let mut png = fixture(width, height, color, depth);
            if color == ColorType::Indexed {
                png.palette = (0..256)
                    .map(|n| [n as u8, (n * 37) as u8, 255 - n as u8])
                    .collect();
            }
            if mode == "fast" {
                let channels = color.channels();
                let value = |n: usize| {
                    ((n / channels % width as usize + n % channels) & 31) as u16
                        & ((1u32 << depth) - 1) as u16
                };
                match &mut png.image.samples {
                    Samples::U8(v) => v
                        .iter_mut()
                        .enumerate()
                        .for_each(|(n, v)| *v = value(n) as u8),
                    Samples::U16(v) => v.iter_mut().enumerate().for_each(|(n, v)| *v = value(n)),
                }
            }
            let bytes = encode(
                &png,
                &EncodeOptions {
                    interlaced,
                    filter,
                    compression,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(
                apng_recovery::inflate_idat(&bytes),
                apng_recovery::filtered_with(&png.image, interlaced, filter)
            );
            for actual in [
                decode(&bytes).unwrap(),
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
            ] {
                assert_eq!(actual.image, png.image);
                assert_eq!(actual.palette, png.palette);
            }
            let case = format!(
                "reader-c{}-d{depth}-{width}x{height}-i{}-f{}-{mode}",
                color as u8,
                u8::from(interlaced),
                filter as u8
            );
            let hash = fingerprint(&Samples::U8(bytes.clone()));
            let count = png.image.samples.len() * if depth == 16 { 2 } else { 1 };
            measure(&case, "decode", count, hash, || {
                decode(black_box(&bytes)).unwrap()
            });
            measure(&case, "decode-reader", count, hash, || {
                decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap()
            });
        }
    }
}
