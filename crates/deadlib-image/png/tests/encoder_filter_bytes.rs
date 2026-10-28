//! Exact pass bytes and deterministic adaptive ties, independent of decoding.
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/apng_recovery.rs"]
mod oracle;
use deadlib_image_png::{
    EncodeOptions, Filter, Png, Samples, encode, encode_writer, zlib::Compression,
};

#[test]
fn all_formats_filters_and_pass_tails_match_normative_bytes() {
    for (color, depth) in oracle::formats() {
        for width in [1, 2, 3, 7, 8, 9, 17, 33] {
            for (height, pattern) in [(1, 0), (1, 2), (2, 2), (9, 0), (9, 1), (9, 2)] {
                let mut png = Png::new(oracle::image(width, height, color, depth, 71));
                if pattern < 2 {
                    let value = if pattern == 0 {
                        0
                    } else {
                        ((1u32 << depth) - 1) as u16
                    };
                    match &mut png.image.samples {
                        Samples::U8(v) => v.fill(value as u8),
                        Samples::U16(v) => v.fill(value),
                    }
                }
                if color == deadlib_image_png::ColorType::Indexed {
                    png.palette = vec![[0, 127, 255]; 1 << depth];
                }
                for interlaced in [false, true] {
                    for filter in [
                        Filter::None,
                        Filter::Sub,
                        Filter::Up,
                        Filter::Average,
                        Filter::Paeth,
                        Filter::Adaptive,
                    ] {
                        let options = EncodeOptions {
                            interlaced,
                            filter,
                            compression: Compression::Stored,
                            ..Default::default()
                        };
                        let expected = oracle::filtered_with(&png.image, interlaced, filter);
                        let buffered = encode(&png, &options).unwrap();
                        let mut streamed = Vec::new();
                        encode_writer(&png, &options, &mut streamed).unwrap();
                        for bytes in [&buffered, &streamed] {
                            assert_eq!(
                                oracle::inflate_idat(bytes),
                                expected,
                                "{color:?} {depth} {width}x{height} {pattern} {interlaced} {filter:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
