use deadlib_image_png::{
    ColorType, EncodeOptions, Filter, Image, Png, Samples, Transparency, decode, encode,
    zlib::Compression,
};
use std::io::Cursor;

fn fixture(w: u32, h: u32, color: ColorType, depth: u8) -> Png {
    let count = w as usize * h as usize * color.channels();
    let samples = if depth == 16 {
        Samples::U16(
            (0..count)
                .map(|n| (n.wrapping_mul(37139) ^ n.wrapping_mul(n + 31)) as u16)
                .collect(),
        )
    } else {
        Samples::U8(
            (0..count)
                .map(|n| ((n.wrapping_mul(37) ^ n.wrapping_mul(n + 31)) & ((1 << depth) - 1)) as u8)
                .collect(),
        )
    };
    let mut png = Png::new(Image {
        width: w,
        height: h,
        color_type: color,
        bit_depth: depth,
        samples,
    });
    if color == ColorType::Indexed {
        png.palette = (0..1usize << depth)
            .map(|n| [n as u8, (n * 37) as u8, 255 - n as u8])
            .collect();
        png.transparency = Some(Transparency::Palette(vec![0, 127]));
    }
    png
}

fn packed(image: &Image) -> Vec<u8> {
    match &image.samples {
        Samples::U16(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
        Samples::U8(v) if image.bit_depth == 8 => v.clone(),
        Samples::U8(v) => {
            let samples_per_row = image.width as usize * image.color_type.channels();
            let per_byte = 8 / image.bit_depth as usize;
            let mut result = Vec::new();
            for row in v.chunks(samples_per_row) {
                for group in row.chunks(per_byte) {
                    let mut byte = 0;
                    for (n, &value) in group.iter().enumerate() {
                        byte |= value << (8 - (n + 1) * image.bit_depth as usize);
                    }
                    result.push(byte);
                }
            }
            result
        }
    }
}

fn assert_reference(encoded: &[u8], image: &Image) {
    let mut reader = png::Decoder::new(Cursor::new(encoded)).read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    assert_eq!(info.width, image.width);
    assert_eq!(info.height, image.height);
    assert_eq!(info.color_type as u8, image.color_type as u8);
    assert_eq!(info.bit_depth as u8, image.bit_depth);
    let expected = packed(image);
    // Padding bits are not sample data. Some reference Adam7 scatter paths
    // retain unspecified bits, so compare only meaningful sub-byte samples.
    if image.bit_depth < 8 {
        let Samples::U8(samples) = &image.samples else {
            unreachable!()
        };
        let stride = (image.width as usize * image.bit_depth as usize).div_ceil(8);
        for y in 0..image.height as usize {
            for x in 0..image.width as usize {
                let depth = image.bit_depth as usize;
                let value = (data[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                    & ((1 << depth) - 1);
                assert_eq!(value, samples[y * image.width as usize + x]);
            }
        }
    } else {
        assert_eq!(&data[..info.buffer_size()], expected);
    }
    reader.finish().unwrap();
}

#[test]
fn complete_native_format_filter_and_adam7_matrix() {
    let mut cases = 0;
    for color in [
        ColorType::Grayscale,
        ColorType::Rgb,
        ColorType::Indexed,
        ColorType::GrayscaleAlpha,
        ColorType::Rgba,
    ] {
        for depth in [1, 2, 4, 8, 16] {
            if !color.allows_depth(depth) {
                continue;
            }
            for (w, h) in [
                (1, 1),
                (1, 9),
                (9, 1),
                (2, 3),
                (3, 2),
                (7, 7),
                (8, 8),
                (9, 9),
                (17, 13),
            ] {
                let original = fixture(w, h, color, depth);
                for filter in [
                    Filter::None,
                    Filter::Sub,
                    Filter::Up,
                    Filter::Average,
                    Filter::Paeth,
                    Filter::Adaptive,
                ] {
                    for interlaced in [false, true] {
                        for compression in [Compression::Stored, Compression::Fast] {
                            let options = EncodeOptions {
                                interlaced,
                                filter,
                                compression,
                                idat_chunk_size: 7,
                            };
                            let encoded = encode(&original, &options).unwrap();
                            let actual = decode(&encoded).unwrap();
                            assert_eq!(
                                actual.image, original.image,
                                "{color:?} {depth} {w}x{h} {filter:?} {interlaced}"
                            );
                            assert_eq!(actual.palette, original.palette);
                            assert_eq!(actual.transparency, original.transparency);
                            assert_reference(&encoded, &original.image);
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 3240);
}

#[test]
fn decode_reference_encodings() {
    for color in [
        ColorType::Grayscale,
        ColorType::Rgb,
        ColorType::Indexed,
        ColorType::GrayscaleAlpha,
        ColorType::Rgba,
    ] {
        for depth in [1, 2, 4, 8, 16] {
            if !color.allows_depth(depth) {
                continue;
            }
            let original = fixture(37, 19, color, depth);
            for filter in [
                png::Filter::NoFilter,
                png::Filter::Sub,
                png::Filter::Up,
                png::Filter::Avg,
                png::Filter::Paeth,
                png::Filter::Adaptive,
            ] {
                let mut bytes = Vec::new();
                let mut encoder = png::Encoder::new(&mut bytes, 37, 19);
                encoder.set_color(png::ColorType::from_u8(color as u8).unwrap());
                encoder.set_depth(png::BitDepth::from_u8(depth).unwrap());
                encoder.set_filter(filter);
                if !original.palette.is_empty() {
                    encoder.set_palette(
                        original
                            .palette
                            .iter()
                            .flatten()
                            .copied()
                            .collect::<Vec<_>>(),
                    );
                }
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&packed(&original.image))
                    .unwrap();
                assert_eq!(decode(&bytes).unwrap().image, original.image);
            }
        }
    }
}

#[test]
fn one_byte_idat_and_truncations() {
    let original = fixture(5, 4, ColorType::Rgba, 16);
    let bytes = encode(
        &original,
        &EncodeOptions {
            idat_chunk_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(decode(&bytes).unwrap().image, original.image);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "{end}");
    }
    let mut corrupt = bytes.clone();
    corrupt[29] ^= 1;
    assert!(decode(&corrupt).is_err());
}
