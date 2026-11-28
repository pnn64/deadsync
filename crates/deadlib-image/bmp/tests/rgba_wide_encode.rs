use deadlib_image_bmp::{Bitmap, EncodeOptions, Error, Pixels, encode_with_options};

#[test]
fn wide_rgba_encoding_matches_scalar_bytes_and_limits() {
    for width in [
        5usize, 6, 7, 8, 15, 16, 17, 18, 19, 31, 32, 33, 511, 512, 513,
    ] {
        for height in [1usize, 3] {
            for zero_alpha in [false, true] {
                let mut samples = Vec::with_capacity(width * height * 4);
                for y in 0..height {
                    for x in 0..width {
                        samples.extend_from_slice(&[
                            (17 * x + 53 * y + 1) as u8,
                            (71 * x + 11 * y + 3) as u8,
                            (131 * x + 29 * y + 7) as u8,
                            if zero_alpha {
                                0
                            } else {
                                [0, 1, 127, 128, 254, 255][(x + 3 * y) % 6]
                            },
                        ]);
                    }
                }
                let bitmap = Bitmap {
                    width: width as u32,
                    height: height as u32,
                    pixels: Pixels::Rgba(samples),
                };
                let Pixels::Rgba(samples) = &bitmap.pixels else {
                    unreachable!();
                };
                let raster_bytes = width * height * 4;
                let file_size = 122 + raster_bytes;
                for top_down in [false, true] {
                    let stored_height = if top_down {
                        -(height as i32)
                    } else {
                        height as i32
                    };
                    let mut expected = vec![0u8; 122];
                    expected[..2].copy_from_slice(b"BM");
                    expected[26] = 1;
                    expected[28] = 32;
                    for (offset, value) in [
                        (2, file_size as u32),
                        (10, 122),
                        (14, 108),
                        (18, width as u32),
                        (22, stored_height as u32),
                        (30, 3),
                        (34, raster_bytes as u32),
                        (54, 0x00ff_0000),
                        (58, 0x0000_ff00),
                        (62, 0x0000_00ff),
                        (66, 0xff00_0000),
                        (70, 0x7352_4742),
                    ] {
                        expected[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                    }
                    for stored_y in 0..height {
                        let source_y = if top_down {
                            stored_y
                        } else {
                            height - 1 - stored_y
                        };
                        for x in 0..width {
                            let p = (source_y * width + x) * 4;
                            expected.extend_from_slice(&[
                                samples[p + 2],
                                samples[p + 1],
                                samples[p],
                                samples[p + 3],
                            ]);
                        }
                    }
                    for max_output_bytes in [file_size, file_size + 1] {
                        assert_eq!(
                            encode_with_options(
                                &bitmap,
                                EncodeOptions {
                                    top_down,
                                    max_output_bytes
                                }
                            )
                            .unwrap(),
                            expected,
                            "{width}x{height}, top_down={top_down}, zero_alpha={zero_alpha}"
                        );
                    }
                    assert_eq!(
                        encode_with_options(
                            &bitmap,
                            EncodeOptions {
                                top_down,
                                max_output_bytes: file_size - 1
                            }
                        ),
                        Err(Error::LimitExceeded),
                        "{width}x{height}, top_down={top_down}, zero_alpha={zero_alpha}"
                    );
                }
            }
        }
    }
}
