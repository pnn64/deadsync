//! In-process API comparisons using an immutable baseline crate.
use selected::{ColorType, Image, Png, Samples};
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
        random as u16
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

fn old_options(kind: usize, interlaced: bool) -> baseline::EncodeOptions {
    use baseline::Filter::*;
    baseline::EncodeOptions {
        filter: [None, Sub, Up, Average, Paeth, Adaptive][kind],
        interlaced,
        compression: baseline::zlib::Compression::Stored,
        ..Default::default()
    }
}
fn new_options(kind: usize, interlaced: bool) -> selected::EncodeOptions {
    use selected::Filter::*;
    selected::EncodeOptions {
        filter: [None, Sub, Up, Average, Paeth, Adaptive][kind],
        interlaced,
        compression: selected::zlib::Compression::Stored,
        ..Default::default()
    }
}

fn main() {
    for (name, color, depth, width, height, interlaced, kind) in [
        ("gray8-none", ColorType::Grayscale, 8, 513, 257, false, 0),
        (
            "ga8-adam-none",
            ColorType::GrayscaleAlpha,
            8,
            513,
            257,
            true,
            0,
        ),
        (
            "ga16-adam-none",
            ColorType::GrayscaleAlpha,
            16,
            513,
            257,
            true,
            0,
        ),
        ("rgb8-none", ColorType::Rgb, 8, 513, 257, false, 0),
        ("rgb16-none", ColorType::Rgb, 16, 513, 257, false, 0),
        ("rgba8-none", ColorType::Rgba, 8, 513, 257, false, 0),
        ("rgba16-none", ColorType::Rgba, 16, 513, 257, false, 0),
        ("rgba8-small-none", ColorType::Rgba, 8, 33, 9, true, 0),
        ("rgba16-sub", ColorType::Rgba, 16, 513, 257, false, 1),
        ("rgba8-adaptive", ColorType::Rgba, 8, 513, 257, false, 5),
        ("rgba16-adaptive", ColorType::Rgba, 16, 513, 257, false, 5),
    ] {
        let original = image(width, height, color, depth);
        let old = baseline::Png::new(original.clone());
        let new = Png::new(original);
        let old_opts = old_options(kind, interlaced);
        let new_opts = new_options(kind, interlaced);
        assert_eq!(
            baseline::encode(&old, &old_opts).unwrap(),
            selected::encode(&new, &new_opts).unwrap()
        );
        let mut before = Vec::new();
        let mut after = Vec::new();
        baseline::encode_writer(&old, &old_opts, &mut before).unwrap();
        selected::encode_writer(&new, &new_opts, &mut after).unwrap();
        assert_eq!(before, after);
        let iterations = if width < 100 { 1024 } else { 3 };
        cpu::compare(&format!("{name}-buffered"), iterations, |current| {
            black_box(if current {
                selected::encode(black_box(&new), black_box(&new_opts)).unwrap()
            } else {
                baseline::encode(black_box(&old), black_box(&old_opts)).unwrap()
            });
        });
        cpu::compare(&format!("{name}-writer"), iterations, |current| {
            let mut output = Vec::new();
            if current {
                selected::encode_writer(black_box(&new), black_box(&new_opts), &mut output)
                    .unwrap();
            } else {
                baseline::encode_writer(black_box(&old), black_box(&old_opts), &mut output)
                    .unwrap();
            }
            black_box(output);
        });
    }
    for (name, color, depth, kind) in [
        ("gray8-up", ColorType::Grayscale, 8, 2),
        ("ga16-none", ColorType::GrayscaleAlpha, 16, 0),
        ("rgb8-none", ColorType::Rgb, 8, 0),
        ("rgba8-paeth", ColorType::Rgba, 8, 4),
    ] {
        let original = image(4097, 1, color, depth);
        let bytes = baseline::encode(
            &baseline::Png::new(original.clone()),
            &old_options(kind, false),
        )
        .unwrap();
        assert_eq!(
            baseline::decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            original
        );
        assert_eq!(
            selected::decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            original
        );
        cpu::compare(&format!("{name}-reader"), 64, |current| {
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
