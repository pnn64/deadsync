//! Paired complete decodes with an immutable baseline in the same caller.
use selected::{ColorType, Image, Samples};
use std::hint::black_box;
#[path = "../../examples/support/cpu.rs"]
mod cpu;

fn image(width: u32, height: u32, color: ColorType, depth: u8) -> Image {
    let mut random = 0x4fe3_a279u32;
    let count = width as usize * height as usize * color.channels();
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        (random & ((1 << depth) - 1)) as u16
    };
    Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16((0..count).map(|_| next()).collect())
        } else {
            Samples::U8((0..count).map(|_| next() as u8).collect())
        },
    }
}

fn main() {
    let mut cases = Vec::new();
    for color in [
        ColorType::Grayscale,
        ColorType::GrayscaleAlpha,
        ColorType::Rgb,
        ColorType::Rgba,
    ] {
        for depth in [8, 16] {
            for filter in [baseline::Filter::None, baseline::Filter::Up] {
                cases.push((4097, 1, color, depth, false, filter));
            }
            for (interlaced, filter) in [
                (false, baseline::Filter::Sub),
                (true, baseline::Filter::None),
                (true, baseline::Filter::Paeth),
            ] {
                cases.push((513, 257, color, depth, interlaced, filter));
            }
        }
    }
    for depth in [1, 2, 4] {
        cases.push((
            513,
            257,
            ColorType::Grayscale,
            depth,
            true,
            baseline::Filter::None,
        ));
    }
    for depth in [8, 16] {
        for filter in [baseline::Filter::None, baseline::Filter::Sub] {
            cases.push((33, 9, ColorType::Rgba, depth, true, filter));
        }
    }
    for (width, height, color, depth, interlaced, filter) in cases {
        let expected = image(width, height, color, depth);
        let bytes = baseline::encode(
            &baseline::Png::new(expected.clone()),
            &baseline::EncodeOptions {
                interlaced,
                filter,
                compression: baseline::zlib::Compression::Stored,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(baseline::decode(&bytes).unwrap().image, expected);
        assert_eq!(selected::decode(&bytes).unwrap().image, expected);
        assert_eq!(
            baseline::decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            expected
        );
        assert_eq!(
            selected::decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            expected
        );
        let name = format!(
            "c{}-d{depth}-{width}x{height}-i{}-f{}",
            color as u8,
            u8::from(interlaced),
            filter as u8
        );
        let iterations = if height == 1 {
            64
        } else if width < 100 {
            1024
        } else {
            3
        };
        cpu::compare(&format!("{name}-buffered"), iterations, |current| {
            black_box(if current {
                selected::decode(black_box(&bytes)).unwrap().image
            } else {
                baseline::decode(black_box(&bytes)).unwrap().image
            });
        });
        cpu::compare(&format!("{name}-reader"), iterations, |current| {
            black_box(if current {
                selected::decode_reader(black_box(bytes.as_slice()), &Default::default())
                    .unwrap()
                    .image
            } else {
                baseline::decode_reader(black_box(bytes.as_slice()), &Default::default())
                    .unwrap()
                    .image
            });
        });
    }
}
