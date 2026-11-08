extern crate selected as deadlib_image_png;
#[allow(dead_code)]
#[path = "../../tests/support/chunks.rs"]
mod chunks;
use baseline::{EncodeOptions, Filter, Png, encode};
use selected::{ColorType, Image, Samples};
use std::hint::black_box;
#[path = "../../examples/support/cpu.rs"]
mod cpu;

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

fn reader_buffers() {
    use baseline::zlib::Compression;
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
            let old = baseline::decode(&bytes).unwrap();
            let new = selected::decode(&bytes).unwrap();
            assert_eq!(old.image, png.image);
            assert_eq!(new.image, png.image);
            assert_eq!(old.palette, png.palette);
            assert_eq!(new.palette, png.palette);
            let old = baseline::decode_reader(bytes.as_slice(), &Default::default()).unwrap();
            let new = selected::decode_reader(bytes.as_slice(), &Default::default()).unwrap();
            assert_eq!(old.image, png.image);
            assert_eq!(new.image, png.image);
            assert_eq!(old.palette, png.palette);
            assert_eq!(new.palette, png.palette);
            let case = format!(
                "reader-c{}-d{depth}-{width}x{height}-i{}-f{}-{mode}",
                color as u8,
                u8::from(interlaced),
                filter as u8
            );
            let iterations = if width < 100 { 512 } else { 3 };
            cpu::compare(&format!("{case}-buffered"), iterations, |current| {
                if current {
                    black_box(selected::decode(black_box(&bytes)).unwrap());
                } else {
                    black_box(baseline::decode(black_box(&bytes)).unwrap());
                }
            });
            cpu::compare(&format!("{case}-reader"), iterations, |current| {
                if current {
                    black_box(
                        selected::decode_reader(black_box(bytes.as_slice()), &Default::default())
                            .unwrap(),
                    );
                } else {
                    black_box(
                        baseline::decode_reader(black_box(bytes.as_slice()), &Default::default())
                            .unwrap(),
                    );
                }
            });
        }
    }
}

fn reference_deflate() {
    use std::io::Write;
    for (color, depth) in [
        (ColorType::Grayscale, 8),
        (ColorType::Rgba, 8),
        (ColorType::Rgba, 16),
    ] {
        for (width, height) in [(33u32, 9u32), (513, 257)] {
            let mut png = fixture(width, height, color, depth);
            match &mut png.image.samples {
                Samples::U8(v) => v
                    .iter_mut()
                    .enumerate()
                    .for_each(|(n, v)| *v = (n % 97) as u8),
                Samples::U16(v) => v
                    .iter_mut()
                    .enumerate()
                    .for_each(|(n, v)| *v = (n % 97) as u16),
            }
            let mut raw = Vec::new();
            let stride = width as usize * color.channels();
            for y in 0..height as usize {
                raw.push(0);
                match &png.image.samples {
                    Samples::U8(v) => raw.extend_from_slice(&v[y * stride..(y + 1) * stride]),
                    Samples::U16(v) => {
                        for value in &v[y * stride..(y + 1) * stride] {
                            raw.extend(value.to_be_bytes());
                        }
                    }
                }
            }
            for level in [1, 9] {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
                encoder.write_all(&raw).unwrap();
                let compressed = encoder.finish().unwrap();
                let block = (compressed[2] >> 1) & 3;
                if width == 513 {
                    assert_eq!(block, 2);
                }
                let mut header = width.to_be_bytes().to_vec();
                header.extend(height.to_be_bytes());
                header.extend([depth, color as u8, 0, 0, 0]);
                let bytes = chunks::file(&[
                    chunks::chunk(*b"IHDR", &header),
                    chunks::chunk(*b"IDAT", &compressed),
                    chunks::chunk(*b"IEND", &[]),
                ]);
                assert_eq!(baseline::decode(&bytes).unwrap().image, png.image);
                assert_eq!(selected::decode(&bytes).unwrap().image, png.image);
                assert_eq!(
                    baseline::decode_reader(bytes.as_slice(), &Default::default())
                        .unwrap()
                        .image,
                    png.image
                );
                assert_eq!(
                    selected::decode_reader(bytes.as_slice(), &Default::default())
                        .unwrap()
                        .image,
                    png.image
                );
                let case = format!(
                    "reference-c{}-d{depth}-{width}x{height}-l{level}-b{block}",
                    color as u8
                );
                let iterations = if width < 100 { 512 } else { 3 };
                cpu::compare(&format!("{case}-buffered"), iterations, |current| {
                    if current {
                        black_box(selected::decode(black_box(&bytes)).unwrap());
                    } else {
                        black_box(baseline::decode(black_box(&bytes)).unwrap());
                    }
                });
                cpu::compare(&format!("{case}-reader"), iterations, |current| {
                    if current {
                        black_box(
                            selected::decode_reader(
                                black_box(bytes.as_slice()),
                                &Default::default(),
                            )
                            .unwrap(),
                        );
                    } else {
                        black_box(
                            baseline::decode_reader(
                                black_box(bytes.as_slice()),
                                &Default::default(),
                            )
                            .unwrap(),
                        );
                    }
                });
            }
        }
    }
}

fn main() {
    println!(
        "type-sizes inflater {} -> {}; stream-decoder {} -> {}",
        std::mem::size_of::<baseline::zlib::Inflater>(),
        std::mem::size_of::<selected::zlib::Inflater>(),
        std::mem::size_of::<baseline::StreamDecoder>(),
        std::mem::size_of::<selected::StreamDecoder>()
    );
    println!(
        "inflater-owned-zero {} -> {}",
        baseline::zlib::Inflater::new(0)
            .unwrap()
            .working_memory_bytes(),
        selected::zlib::Inflater::new(0)
            .unwrap()
            .working_memory_bytes()
    );
    reader_buffers();
    reference_deflate();
}
