#![no_main]
use deadlib_image_png::{
    ColorType, EncodeOptions, Filter, Png, Samples, decode, decode_reader, encode, encode_writer,
    zlib::Compression,
};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 12 || bytes.len() > 4096 {
        return;
    }
    let (color, channels) = [
        (ColorType::Grayscale, 1),
        (ColorType::Rgb, 3),
        (ColorType::GrayscaleAlpha, 2),
        (ColorType::Rgba, 4),
        (ColorType::Indexed, 1),
    ][usize::from(bytes[0]) % 5];
    let small_width = 1 + u32::from(bytes[1] % 17);
    let small_height = 1 + u32::from(bytes[2] % 17);
    let flags = bytes[3];
    let width = if flags & 32 != 0 {
        0
    } else if flags & 64 != 0 {
        0x7fff_ffff
    } else {
        small_width
    };
    let height = if flags & 128 != 0 {
        u32::MAX
    } else {
        small_height
    };
    let bits: Vec<u8> = (0..channels)
        .map(|c| {
            if flags & 1 == 0 {
                1 + bytes[4 + c] % 16
            } else {
                bytes[4 + c]
            }
        })
        .collect();
    let valid_bits = bits.iter().all(|&n| (1..=16).contains(&n));
    let expected_count = small_width as usize * small_height as usize * channels;
    let count = if flags & 4 != 0 {
        expected_count - 1
    } else if flags & 8 != 0 {
        expected_count + 1
    } else {
        expected_count
    };
    let original: Vec<u16> = (0..count)
        .map(|n| {
            let value = if flags & 2 == 0 {
                u16::from(bytes[(8 + n) % bytes.len()])
            } else {
                u16::from_be_bytes([
                    bytes[(8 + 2 * n) % bytes.len()],
                    bytes[(9 + 2 * n) % bytes.len()],
                ])
            };
            if valid_bits && flags & 16 == 0 {
                value & ((1u32 << bits[n % channels]) - 1) as u16
            } else {
                value
            }
        })
        .collect();
    let valid = valid_bits
        && color != ColorType::Indexed
        && width == small_width
        && height == small_height
        && count == expected_count
        && original
            .iter()
            .enumerate()
            .all(|(n, &v)| u32::from(v) < 1u32 << bits[n % channels]);
    let samples = if flags & 2 == 0 {
        Samples::U8(original.iter().map(|&v| v as u8).collect())
    } else {
        Samples::U16(original.clone())
    };
    let result = Png::from_reference_samples(width, height, color, &bits, samples);
    if !valid {
        assert!(result.is_err());
        return;
    }
    let mut png = result.unwrap();
    let largest = *bits.iter().max().unwrap();
    let depth = if color == ColorType::Grayscale {
        match largest {
            1 => 1,
            2 => 2,
            3 | 4 => 4,
            5..=8 => 8,
            _ => 16,
        }
    } else if largest <= 8 {
        8
    } else {
        16
    };
    assert_eq!(
        (
            png.image.width,
            png.image.height,
            png.image.color_type,
            png.image.bit_depth
        ),
        (width, height, color, depth)
    );
    png.image.validate().unwrap();
    let actual: Vec<u16> = match &png.image.samples {
        Samples::U8(v) => v.iter().map(|&v| u16::from(v)).collect(),
        Samples::U16(v) => v.clone(),
    };
    for (n, (&stored, &source)) in actual.iter().zip(&original).enumerate() {
        let significant = bits[n % channels];
        let denominator = (1u64 << significant) - 1;
        let numerator = u64::from(source) * ((1u64 << depth) - 1);
        assert_eq!(
            u64::from(stored),
            (numerator * 2 + denominator) / (denominator * 2)
        );
        assert_eq!(stored >> (depth - significant), source);
    }
    let shifted = bits.iter().any(|&n| n != depth);
    assert_eq!(png.ancillary.len(), usize::from(shifted));
    if shifted {
        assert_eq!(png.ancillary[0].kind, *b"sBIT");
        assert_eq!(png.ancillary[0].data, bits);
    }
    let options = EncodeOptions {
        interlaced: bytes[9] & 1 != 0,
        compression: if bytes[9] & 2 == 0 {
            Compression::Stored
        } else {
            Compression::Fast
        },
        filter: [
            Filter::None,
            Filter::Sub,
            Filter::Up,
            Filter::Average,
            Filter::Paeth,
            Filter::Adaptive,
        ][usize::from(bytes[8]) % 6],
        idat_chunk_size: 1 + usize::from(bytes[10] % 31),
    };
    png.interlaced = options.interlaced;
    let mut writer = Vec::new();
    encode_writer(&png, &options, &mut writer).unwrap();
    for encoded in [encode(&png, &options).unwrap(), writer] {
        assert_eq!(decode(&encoded).unwrap(), png);
        assert_eq!(
            decode_reader(encoded.as_slice(), &Default::default()).unwrap(),
            png
        );
        let mut reader = png::Decoder::new(Cursor::new(encoded.as_slice()))
            .read_info()
            .unwrap();
        assert_eq!(
            reader.info().sbit.as_deref(),
            if shifted { Some(bits.as_slice()) } else { None }
        );
        let mut output = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut output).unwrap();
        let per_row = width as usize * channels;
        for y in 0..height as usize {
            for n in 0..per_row {
                let value = match depth {
                    16 => u16::from_be_bytes([
                        output[y * info.line_size + 2 * n],
                        output[y * info.line_size + 2 * n + 1],
                    ]),
                    8 => u16::from(output[y * info.line_size + n]),
                    _ => u16::from(
                        (output[y * info.line_size + n * depth as usize / 8]
                            >> (8 - depth - (n * depth as usize % 8) as u8))
                            & ((1 << depth) - 1),
                    ),
                };
                assert_eq!(value, actual[y * per_row + n]);
            }
        }
        reader.finish().unwrap();
    }
});
