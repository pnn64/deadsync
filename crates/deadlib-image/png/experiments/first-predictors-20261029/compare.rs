use selected::{ColorType, Image, Png, Samples};
use std::hint::black_box;
#[path = "../../examples/support/cpu.rs"]
mod cpu;

fn image(width: u32, height: u32, color: ColorType, depth: u8) -> Image {
    let mut random = 0x4fe3_a279u32;
    let count = width as usize * height as usize * color.channels();
    let mask = (1u32 << depth) - 1;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        (random & mask) as u16
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
    let formats = [
        (ColorType::Grayscale, 8),
        (ColorType::Rgba, 8),
        (ColorType::Rgba, 16),
        (ColorType::Grayscale, 2),
    ];
    let mut cases = Vec::new();
    for (color, depth) in formats {
        for interlaced in [false, true] {
            for filter in [2, 4, 5] {
                cases.push((color, depth, 4097, 1, interlaced, filter));
            }
            cases.push((color, depth, 33, 9, interlaced, 5));
        }
    }
    for &(color, depth) in &formats[..3] {
        for filter in [0, 5] {
            cases.push((color, depth, 513, 257, false, filter));
        }
    }
    for filter in 0..6 {
        cases.push((ColorType::Rgba, 8, 1, 1, false, filter));
    }
    for interlaced in [false, true] {
        cases.push((ColorType::Rgba, 16, 4097, 1, interlaced, 3));
        for filter in [0, 1] {
            cases.push((ColorType::Grayscale, 8, 4097, 1, interlaced, filter));
        }
    }
    assert_eq!(cases.len(), 50);
    for (color, depth, width, height, interlaced, filter) in cases {
        let original = image(width, height, color, depth);
        let old = baseline::Png::new(original.clone());
        let new = Png::new(original);
        let old_options = baseline::EncodeOptions {
            filter: [
                baseline::Filter::None,
                baseline::Filter::Sub,
                baseline::Filter::Up,
                baseline::Filter::Average,
                baseline::Filter::Paeth,
                baseline::Filter::Adaptive,
            ][filter],
            interlaced,
            compression: baseline::zlib::Compression::Stored,
            ..Default::default()
        };
        let new_options = selected::EncodeOptions {
            filter: [
                selected::Filter::None,
                selected::Filter::Sub,
                selected::Filter::Up,
                selected::Filter::Average,
                selected::Filter::Paeth,
                selected::Filter::Adaptive,
            ][filter],
            interlaced,
            compression: selected::zlib::Compression::Stored,
            ..Default::default()
        };
        let old_bytes = baseline::encode(&old, &old_options).unwrap();
        let new_bytes = selected::encode(&new, &new_options).unwrap();
        assert_eq!(old_bytes, new_bytes);
        assert_eq!(baseline::decode(&old_bytes).unwrap().image, old.image);
        assert_eq!(selected::decode(&new_bytes).unwrap().image, new.image);
        let mut before = Vec::new();
        let mut after = Vec::new();
        let old_stats = baseline::encode_writer(&old, &old_options, &mut before).unwrap();
        let new_stats = selected::encode_writer(&new, &new_options, &mut after).unwrap();
        assert_eq!(before, after);
        assert_eq!(baseline::decode(&before).unwrap().image, old.image);
        assert_eq!(selected::decode(&after).unwrap().image, new.image);
        let case = format!(
            "first-c{}-d{depth}-{width}x{height}-i{}-f{filter}",
            color as u8,
            u8::from(interlaced)
        );
        println!(
            "{case}: writer-owned {} -> {}",
            old_stats.peak_image_buffer_bytes, new_stats.peak_image_buffer_bytes
        );
        let iterations = if width < 100 {
            512
        } else if height == 1 {
            128
        } else {
            6
        };
        cpu::compare(&format!("{case}-buffered"), iterations, |current| {
            if current {
                black_box(selected::encode(black_box(&new), black_box(&new_options)).unwrap());
            } else {
                black_box(baseline::encode(black_box(&old), black_box(&old_options)).unwrap());
            }
        });
        cpu::compare(&format!("{case}-writer"), iterations, |current| {
            let mut output = Vec::new();
            if current {
                black_box(
                    selected::encode_writer(black_box(&new), black_box(&new_options), &mut output)
                        .unwrap(),
                );
            } else {
                black_box(
                    baseline::encode_writer(black_box(&old), black_box(&old_options), &mut output)
                        .unwrap(),
                );
            }
            black_box(output);
        });
    }
}
