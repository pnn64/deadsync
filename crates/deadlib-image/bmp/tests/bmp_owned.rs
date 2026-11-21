use deadlib_image_bmp::{Bitmap, Error, Limits, Pixels};

#[test]
fn consuming_rgba_reuses_the_original_allocation_and_preserves_zero_alpha() {
    let expected = [
        1, 2, 3, 0, 17, 31, 47, 1, 63, 79, 95, 127, 111, 127, 143, 255, 159, 175, 191, 0, 207, 223,
        239, 0,
    ];
    for explicit_limits in [false, true] {
        let mut samples = Vec::with_capacity(113);
        samples.extend(expected);
        let pointer = samples.as_ptr();
        let capacity = samples.capacity();
        let bitmap = Bitmap {
            width: 3,
            height: 2,
            pixels: Pixels::Rgba(samples),
        };
        let output = if explicit_limits {
            bitmap.into_rgba8_with_limits(Limits {
                // Conversion does not read encoded input. The output budget
                // bounds its length, not spare capacity in an existing buffer.
                max_input_bytes: 0,
                max_pixels: 6,
                max_decoded_bytes: 24,
            })
        } else {
            bitmap.into_rgba8()
        }
        .unwrap();
        assert_eq!(output, expected);
        assert_eq!(output.as_ptr(), pointer);
        assert_eq!(output.capacity(), capacity);
    }

    let transparent = Bitmap {
        width: 2,
        height: 1,
        pixels: Pixels::Rgba(vec![255, 0, 0, 0, 0, 0, 255, 0]),
    };
    assert_eq!(
        transparent.into_rgba8().unwrap(),
        [255, 0, 0, 0, 0, 0, 255, 0]
    );
}

#[test]
fn consuming_rgb_and_indexed_models_produce_exact_rgba_pixels() {
    for count in [1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 257] {
        let mut samples = Vec::new();
        let mut expected = Vec::new();
        for i in 0..count {
            let rgb = [i as u8, (i * 71 + 17) as u8, (i * 139 + 29) as u8];
            samples.extend(rgb);
            expected.extend([rgb[0], rgb[1], rgb[2], 255]);
        }
        let bitmap = Bitmap {
            width: count as u32,
            height: 1,
            pixels: Pixels::Rgb(samples),
        };
        assert_eq!(bitmap.to_rgba8().unwrap(), expected);
        assert_eq!(bitmap.into_rgba8().unwrap(), expected);
    }
    for bit_depth in [1, 4, 8] {
        let bitmap = Bitmap {
            width: 3,
            height: 2,
            pixels: Pixels::Indexed {
                bit_depth,
                indices: vec![1, 0, 1, 0, 0, 1],
                palette: vec![[13, 29, 47], [193, 211, 229]],
            },
        };
        let expected = [
            193, 211, 229, 255, 13, 29, 47, 255, 193, 211, 229, 255, 13, 29, 47, 255, 13, 29, 47,
            255, 193, 211, 229, 255,
        ];
        assert_eq!(bitmap.to_rgba8().unwrap(), expected);
        assert_eq!(bitmap.into_rgba8().unwrap(), expected);
    }
}

#[test]
fn consuming_conversion_obeys_pixel_and_output_limit_boundaries() {
    let mut palette = vec![[0; 3]; 256];
    palette[255] = [193, 211, 229];
    for bitmap in [
        Bitmap {
            width: 2,
            height: 1,
            pixels: Pixels::Rgb(vec![1, 2, 3, 4, 5, 6]),
        },
        Bitmap {
            width: 2,
            height: 1,
            pixels: Pixels::Rgba(vec![1, 2, 3, 0, 4, 5, 6, 255]),
        },
        Bitmap {
            width: 2,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth: 8,
                indices: vec![255, 0],
                palette,
            },
        },
    ] {
        let limits = Limits {
            max_input_bytes: 0,
            max_pixels: 2,
            max_decoded_bytes: 8,
        };
        let expected = bitmap.to_rgba8().unwrap();
        assert_eq!(bitmap.clone().into_rgba8().unwrap(), expected);
        assert_eq!(
            bitmap.clone().into_rgba8_with_limits(limits).unwrap(),
            expected
        );
        for limited in [
            Limits {
                max_pixels: 1,
                ..limits
            },
            Limits {
                max_decoded_bytes: 7,
                ..limits
            },
        ] {
            assert_eq!(
                bitmap.clone().into_rgba8_with_limits(limited),
                Err(Error::LimitExceeded)
            );
            assert_eq!(
                bitmap.clone().into_rgba8_with_limits(limited),
                bitmap.to_rgba8_with_limits(limited)
            );
        }
    }
}

#[test]
fn consuming_conversion_matches_borrowed_errors_and_checks_limits_first() {
    let mut cases = vec![
        (
            Bitmap {
                width: 0,
                height: 1,
                pixels: Pixels::Rgba(vec![]),
            },
            Error::Invalid("zero image dimension"),
        ),
        (
            Bitmap {
                width: 1,
                height: 0,
                pixels: Pixels::Rgb(vec![]),
            },
            Error::Invalid("zero image dimension"),
        ),
        (
            Bitmap {
                width: (64 << 20) + 1,
                height: 1,
                pixels: Pixels::Rgba(vec![]),
            },
            Error::LimitExceeded,
        ),
        (
            Bitmap {
                width: u32::MAX,
                height: u32::MAX,
                pixels: Pixels::Rgba(vec![]),
            },
            Error::LimitExceeded,
        ),
    ];
    for pixels in [
        Pixels::Rgb(vec![1, 2]),
        Pixels::Rgb(vec![1, 2, 3, 4]),
        Pixels::Rgba(vec![1, 2, 3]),
        Pixels::Rgba(vec![1, 2, 3, 4, 5]),
    ] {
        cases.push((
            Bitmap {
                width: 1,
                height: 1,
                pixels,
            },
            Error::Invalid("incorrect sample count"),
        ));
    }
    for (bit_depth, indices, palette, reason) in [
        (
            0,
            vec![0],
            vec![[0; 3]],
            "indexed bit depth must be 1, 4, or 8",
        ),
        (1, vec![0], vec![], "invalid palette size"),
        (1, vec![0], vec![[0; 3]; 3], "invalid palette size"),
        (1, vec![], vec![[0; 3]], "incorrect index count"),
        (1, vec![0, 0], vec![[0; 3]], "incorrect index count"),
        (
            1,
            vec![2],
            vec![[0; 3]; 2],
            "pixel index exceeds the palette",
        ),
        (
            4,
            vec![16],
            vec![[0; 3]; 16],
            "pixel index exceeds the palette",
        ),
        (8, vec![1], vec![[0; 3]], "pixel index exceeds the palette"),
    ] {
        cases.push((
            Bitmap {
                width: 1,
                height: 1,
                pixels: Pixels::Indexed {
                    bit_depth,
                    indices,
                    palette,
                },
            },
            Error::Invalid(reason),
        ));
    }
    for (bitmap, expected) in cases {
        assert_eq!(bitmap.to_rgba8(), Err(expected.clone()));
        assert_eq!(bitmap.clone().into_rgba8(), Err(expected));
        for limits in [
            Limits::default(),
            Limits {
                max_input_bytes: 0,
                max_pixels: 0,
                max_decoded_bytes: 0,
            },
            Limits {
                max_input_bytes: 0,
                max_pixels: 1,
                max_decoded_bytes: 3,
            },
        ] {
            assert_eq!(
                bitmap.clone().into_rgba8_with_limits(limits),
                bitmap.to_rgba8_with_limits(limits)
            );
        }
        if bitmap.width != 0 && bitmap.height != 0 {
            assert_eq!(
                bitmap.into_rgba8_with_limits(Limits {
                    max_pixels: 0,
                    max_decoded_bytes: 0,
                    ..Limits::default()
                }),
                Err(Error::LimitExceeded)
            );
        }
    }
}
