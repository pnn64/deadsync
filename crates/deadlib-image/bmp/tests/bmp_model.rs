use deadlib_image_bmp::{Bitmap, Error, Limits, Pixels};

#[test]
fn valid_models_convert_to_explicit_top_down_rgba_pixels() {
    for (image, expected) in [
        (
            Bitmap {
                width: 3,
                height: 1,
                pixels: Pixels::Rgb(vec![1, 2, 3, 0, 127, 255, 31, 63, 95]),
            },
            vec![1, 2, 3, 255, 0, 127, 255, 255, 31, 63, 95, 255],
        ),
        (
            Bitmap {
                width: 1,
                height: 2,
                pixels: Pixels::Rgba(vec![1, 2, 3, 0, 4, 5, 6, 127]),
            },
            vec![1, 2, 3, 0, 4, 5, 6, 127],
        ),
        (
            Bitmap {
                width: 2,
                height: 2,
                pixels: Pixels::Indexed {
                    bit_depth: 1,
                    indices: vec![1, 0, 0, 1],
                    palette: vec![[11, 22, 33], [44, 55, 66]],
                },
            },
            vec![
                44, 55, 66, 255, 11, 22, 33, 255, 11, 22, 33, 255, 44, 55, 66, 255,
            ],
        ),
    ] {
        image.validate().unwrap();
        assert_eq!(image.to_rgba8().unwrap(), expected);
        assert_eq!(
            image.to_rgba8_with_limits(Limits::default()).unwrap(),
            expected
        );
    }
}

#[test]
fn indexed_models_allow_short_palettes_for_each_supported_depth() {
    for bit_depth in [1, 4, 8] {
        let image = Bitmap {
            width: 3,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth,
                indices: vec![0, 1, 0],
                palette: vec![[0, 127, 255], [193, 17, 43]],
            },
        };
        image.validate().unwrap();
        assert_eq!(
            image.to_rgba8().unwrap(),
            [0, 127, 255, 255, 193, 17, 43, 255, 0, 127, 255, 255]
        );
    }
}

#[test]
fn invalid_public_models_return_errors_before_conversion() {
    let mut cases = vec![
        Bitmap {
            width: 0,
            height: 1,
            pixels: Pixels::Rgb(vec![]),
        },
        Bitmap {
            width: 1,
            height: 0,
            pixels: Pixels::Rgb(vec![]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgb(vec![1, 2]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgb(vec![1, 2, 3, 4]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgba(vec![1, 2, 3]),
        },
        Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Rgba(vec![1, 2, 3, 4, 5]),
        },
        Bitmap {
            width: u32::MAX,
            height: u32::MAX,
            pixels: Pixels::Rgba(vec![]),
        },
    ];
    for (bit_depth, indices, palette) in [
        (1, vec![], vec![[0; 3]]),
        (1, vec![0, 0], vec![[0; 3]]),
        (1, vec![0], vec![]),
        (1, vec![1], vec![[0; 3]]),
        (1, vec![0], vec![[0; 3]; 3]),
        (4, vec![16], vec![[0; 3]; 16]),
        (4, vec![0], vec![[0; 3]; 17]),
        (8, vec![0], vec![[0; 3]; 257]),
        (0, vec![0], vec![[0; 3]]),
        (2, vec![0], vec![[0; 3]]),
        (16, vec![0], vec![[0; 3]]),
    ] {
        cases.push(Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth,
                indices,
                palette,
            },
        });
    }
    for image in cases {
        assert!(image.validate().is_err());
        assert!(image.to_rgba8().is_err());
        assert!(image.to_rgba8_with_limits(Limits::default()).is_err());
    }
}

#[test]
fn rgba_conversion_has_independent_pixel_and_output_budgets() {
    let image = Bitmap {
        width: 2,
        height: 1,
        pixels: Pixels::Rgb(vec![1, 2, 3, 4, 5, 6]),
    };
    let limits = Limits {
        max_pixels: 2,
        max_decoded_bytes: 8,
        ..Limits::default()
    };
    assert_eq!(
        image.to_rgba8_with_limits(limits).unwrap(),
        [1, 2, 3, 255, 4, 5, 6, 255]
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
        assert!(matches!(
            image.to_rgba8_with_limits(limited),
            Err(Error::LimitExceeded)
        ));
    }

    // Conversion charges its RGBA output, independently of the already-owned
    // indexed palette and the decoder's original storage budget.
    let mut palette = vec![[0; 3]; 256];
    palette[255] = [13, 29, 47];
    let indexed = Bitmap {
        width: 1,
        height: 1,
        pixels: Pixels::Indexed {
            bit_depth: 8,
            indices: vec![255],
            palette,
        },
    };
    assert_eq!(
        indexed
            .to_rgba8_with_limits(Limits {
                max_pixels: 1,
                max_decoded_bytes: 4,
                ..limits
            })
            .unwrap(),
        [13, 29, 47, 255]
    );
    assert!(matches!(
        indexed.to_rgba8_with_limits(Limits {
            max_decoded_bytes: 3,
            ..limits
        }),
        Err(Error::LimitExceeded)
    ));
}
