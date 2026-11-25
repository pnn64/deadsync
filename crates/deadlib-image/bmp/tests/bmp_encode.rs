use deadlib_image_bmp::{
    Bitmap, EncodeOptions, Error, Pixels, decode, encode, encode_with_options,
};

fn rgb_image() -> Bitmap {
    Bitmap {
        width: 3,
        height: 2,
        pixels: Pixels::Rgb(vec![
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 12, 34, 56, 0, 0, 0,
        ]),
    }
}

fn rgba_image() -> Bitmap {
    Bitmap {
        width: 2,
        height: 2,
        pixels: Pixels::Rgba(vec![1, 2, 3, 0, 4, 5, 6, 127, 7, 8, 9, 255, 10, 11, 12, 0]),
    }
}

fn indexed_image() -> Bitmap {
    Bitmap {
        width: 3,
        height: 2,
        pixels: Pixels::Indexed {
            bit_depth: 4,
            indices: vec![0, 1, 2, 2, 1, 0],
            palette: vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
        },
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn rgb_header_pixels_and_zero_padding_are_independent_of_the_decoder() {
    let bitmap = rgb_image();
    let bytes = encode(&bitmap).unwrap();
    assert_eq!(&bytes[..2], b"BM");
    assert_eq!(bytes.len(), 78);
    assert_eq!(u32_at(&bytes, 2), 78);
    assert_eq!(&bytes[6..10], &[0; 4]);
    assert_eq!(u32_at(&bytes, 10), 54);
    assert_eq!(u32_at(&bytes, 14), 40);
    assert_eq!(u32_at(&bytes, 18), 3);
    assert_eq!(u32_at(&bytes, 22), 2);
    assert_eq!(u16_at(&bytes, 26), 1);
    assert_eq!(u16_at(&bytes, 28), 24);
    assert_eq!(u32_at(&bytes, 30), 0);
    assert_eq!(u32_at(&bytes, 34), 24);
    assert_eq!(&bytes[38..54], &[0; 16]);
    assert_eq!(
        &bytes[54..],
        &[
            255, 255, 255, 56, 34, 12, 0, 0, 0, 0, 0, 0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0,
        ]
    );

    let top_down = encode_with_options(
        &bitmap,
        EncodeOptions {
            top_down: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(&top_down[22..26], &(-2i32).to_le_bytes());
    assert_eq!(&top_down[54..66], &bytes[66..78]);
    assert_eq!(&top_down[66..78], &bytes[54..66]);
    assert_eq!(decode(&bytes).unwrap(), bitmap);
    assert_eq!(decode(&top_down).unwrap(), bitmap);
}

#[test]
fn rgba_v4_masks_preserve_explicit_alpha() {
    let bitmap = rgba_image();
    let bytes = encode(&bitmap).unwrap();
    assert_eq!(bytes.len(), 138);
    assert_eq!(u32_at(&bytes, 2), 138);
    assert_eq!(u32_at(&bytes, 10), 122);
    assert_eq!(u32_at(&bytes, 14), 108);
    assert_eq!(u32_at(&bytes, 18), 2);
    assert_eq!(u32_at(&bytes, 22), 2);
    assert_eq!(u16_at(&bytes, 26), 1);
    assert_eq!(u16_at(&bytes, 28), 32);
    assert_eq!(u32_at(&bytes, 30), 3);
    assert_eq!(u32_at(&bytes, 34), 16);
    assert_eq!(
        &bytes[54..74],
        &[
            0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 0x42, 0x47, 0x52, 0x73,
        ]
    );
    assert_eq!(&bytes[74..122], &[0; 48]);
    assert_eq!(
        &bytes[122..],
        &[9, 8, 7, 255, 12, 11, 10, 0, 3, 2, 1, 0, 6, 5, 4, 127]
    );
    for top_down in [false, true] {
        let bytes = encode_with_options(
            &bitmap,
            EncodeOptions {
                top_down,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode(&bytes).unwrap(), bitmap);
    }

    let transparent = Bitmap {
        width: 2,
        height: 1,
        pixels: Pixels::Rgba(vec![90, 80, 70, 0, 30, 20, 10, 0]),
    };
    assert_eq!(decode(&encode(&transparent).unwrap()).unwrap(), transparent);
}

#[test]
fn indexed_input_expands_into_bgr24_rows() {
    let bitmap = indexed_image();
    let bytes = encode(&bitmap).unwrap();
    assert_eq!(u16_at(&bytes, 28), 24);
    assert_eq!(u32_at(&bytes, 10), 54);
    assert_eq!(u32_at(&bytes, 46), 0);
    assert_eq!(
        &bytes[54..],
        &[
            255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0,
        ]
    );
    let expected = Bitmap {
        width: 3,
        height: 2,
        pixels: Pixels::Rgb(vec![
            255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0,
        ]),
    };
    for top_down in [false, true] {
        let bytes = encode_with_options(
            &bitmap,
            EncodeOptions {
                top_down,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(decode(&bytes).unwrap(), expected);
    }
}

#[test]
fn indexed_and_rgb_encoders_match_across_the_palette_lookup_cutoff() {
    let colors = [[11, 23, 37], [47, 59, 71], [83, 97, 101]];
    for (width, height) in [
        (1, 1),
        (1, 127),
        (1, 128),
        (1, 1023),
        (1, 1024),
        (2, 5),
        (2, 511),
        (2, 512),
        (3, 7),
        (3, 42),
        (3, 43),
        (17, 8),
        (5, 204),
        (5, 205),
        (4, 255),
        (4, 256),
    ] {
        for palette_len in 1..=colors.len() {
            let indices: Vec<_> = (0..(width * height) as usize)
                .map(|i| (i % palette_len) as u8)
                .collect();
            let samples = indices
                .iter()
                .flat_map(|&index| colors[usize::from(index)])
                .collect();
            let indexed = Bitmap {
                width,
                height,
                pixels: Pixels::Indexed {
                    bit_depth: 8,
                    indices,
                    palette: colors[..palette_len].to_vec(),
                },
            };
            let rgb = Bitmap {
                width,
                height,
                pixels: Pixels::Rgb(samples),
            };
            for top_down in [false, true] {
                let options = EncodeOptions {
                    top_down,
                    ..EncodeOptions::default()
                };
                assert_eq!(
                    encode_with_options(&indexed, options).unwrap(),
                    encode_with_options(&rgb, options).unwrap()
                );
            }
        }
    }
}

#[test]
fn output_limit_includes_headers_and_row_padding() {
    for (bitmap, expected_size) in [
        (rgb_image(), 78),
        (rgba_image(), 138),
        (indexed_image(), 78),
    ] {
        assert!(matches!(
            encode_with_options(
                &bitmap,
                EncodeOptions {
                    max_output_bytes: expected_size - 1,
                    ..Default::default()
                }
            ),
            Err(Error::LimitExceeded)
        ));
        let bytes = encode_with_options(
            &bitmap,
            EncodeOptions {
                max_output_bytes: expected_size,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(bytes.len(), expected_size);
    }
}

#[test]
fn invalid_caller_models_return_errors() {
    let invalid = [
        Bitmap {
            width: 0,
            height: 1,
            pixels: Pixels::Rgb(Vec::new()),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgb(vec![1, 2]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgba(vec![1, 2, 3]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth: 1,
                indices: vec![1],
                palette: vec![[1, 2, 3]],
            },
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth: 2,
                indices: vec![0],
                palette: vec![[1, 2, 3]],
            },
        },
        Bitmap {
            width: u32::MAX,
            height: 1,
            pixels: Pixels::Rgb(Vec::new()),
        },
    ];
    for bitmap in invalid {
        assert!(encode(&bitmap).is_err());
    }
}
