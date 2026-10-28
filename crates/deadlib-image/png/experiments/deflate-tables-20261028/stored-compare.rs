//! Isolate final Stored history copies from the other retained changes.
use selected::{ColorType, Image, Samples};
use std::hint::black_box;
#[path = "../../examples/support/cpu.rs"]
mod cpu;

fn main() {
    let mut formats = vec![
        (ColorType::Grayscale, 1),
        (ColorType::Grayscale, 2),
        (ColorType::Grayscale, 4),
    ];
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
        ColorType::Indexed,
    ] {
        for depth in [8, 16] {
            if color.allows_depth(depth) {
                formats.push((color, depth));
            }
        }
    }
    for (color, depth) in formats {
        for (width, height) in [(33u32, 9u32), (64, 64), (128, 32)] {
            for interlaced in [false, true] {
                let count = width as usize * height as usize * color.channels();
                let mut random = 0x4fe3_a279u32;
                let mut next = || {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    (random & ((1u32 << depth) - 1)) as u16
                };
                let image = Image {
                    width,
                    height,
                    color_type: color,
                    bit_depth: depth,
                    samples: if depth == 16 {
                        Samples::U16((0..count).map(|_| next()).collect())
                    } else {
                        Samples::U8((0..count).map(|_| next() as u8).collect())
                    },
                };
                let mut png = baseline::Png::new(image);
                if color == ColorType::Indexed {
                    png.palette = (0..256)
                        .map(|n| [n as u8, (n * 37) as u8, 255 - n as u8])
                        .collect();
                }
                let bytes = baseline::encode(
                    &png,
                    &baseline::EncodeOptions {
                        interlaced,
                        filter: baseline::Filter::None,
                        compression: baseline::zlib::Compression::Stored,
                        ..Default::default()
                    },
                )
                .unwrap();
                let mut pos = 8;
                loop {
                    let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
                    if &bytes[pos + 4..pos + 8] == b"IDAT" {
                        assert_eq!(bytes[pos + 10] & 7, 1);
                        break;
                    }
                    pos += len + 12;
                }
                let old = baseline::decode(&bytes).unwrap();
                let new = selected::decode(&bytes).unwrap();
                assert_eq!(old.image, png.image);
                assert_eq!(new.image, png.image);
                let old = baseline::decode_reader(bytes.as_slice(), &Default::default()).unwrap();
                let new = selected::decode_reader(bytes.as_slice(), &Default::default()).unwrap();
                assert_eq!(old.image, png.image);
                assert_eq!(new.image, png.image);
                assert_eq!(old.palette, png.palette);
                assert_eq!(new.palette, png.palette);
                let case = format!(
                    "stored-c{}-d{depth}-{width}x{height}-i{}",
                    color as u8,
                    u8::from(interlaced)
                );
                cpu::compare(&format!("{case}-buffered"), 128, |current| {
                    if current {
                        black_box(selected::decode(black_box(&bytes)).unwrap());
                    } else {
                        black_box(baseline::decode(black_box(&bytes)).unwrap());
                    }
                });
                cpu::compare(&format!("{case}-reader"), 128, |current| {
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
