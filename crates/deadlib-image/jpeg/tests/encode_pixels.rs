use deadlib_image::{ColorType, Image, Samples};
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, Error, Limits, SampleData, Subsampling,
    decode_coefficients, encode, encode_samples, quantize,
};

#[test]
fn constant_images_preserve_extremes_at_both_precisions() {
    for precision in [8, 12] {
        for value in [0, 1 << (precision - 1), (1 << precision) - 1] {
            for progressive in [false, true] {
                let samples = vec![value; 13 * 19];
                let input = EncodeInput {
                    width: 13,
                    height: 19,
                    precision,
                    color_space: ColorSpace::Grayscale,
                    samples: SampleData::U16(&samples),
                };
                let options = EncodeOptions {
                    quality: 100,
                    precision,
                    coding: CodingOptions {
                        progressive,
                        restart_interval: 1,
                        ..CodingOptions::default()
                    },
                    ..EncodeOptions::default()
                };
                let bytes = encode_samples(&input, options).unwrap();
                let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
                assert_eq!(decoded.color_space(), ColorSpace::Grayscale);
                let planes = decoded.reconstruct().unwrap();
                assert_eq!(planes.components[0].samples, samples);
            }
        }
    }
}

#[test]
fn native_color_tags_sampling_and_progressive_controls() {
    for (space, channels) in [
        (ColorSpace::Grayscale, 1),
        (ColorSpace::Rgb, 3),
        (ColorSpace::YCbCr, 3),
        (ColorSpace::Cmyk, 4),
        (ColorSpace::Ycck, 4),
    ] {
        let samples: Vec<_> = (0..31 * 25 * channels)
            .map(|i| ((i * 17 + i * i % 997) % 4096) as u16)
            .collect();
        let input = EncodeInput {
            width: 31,
            height: 25,
            precision: 12,
            color_space: space,
            samples: SampleData::U16(&samples),
        };
        for progressive in [false, true] {
            for subsampling in [
                Subsampling::S444,
                Subsampling::S420,
                Subsampling::Custom([(3, 1), (2, 1), (1, 1), (1, 1)]),
            ] {
                let options = EncodeOptions {
                    precision: 12,
                    quality: 90,
                    rgb_to_ycbcr: false,
                    subsampling,
                    coding: CodingOptions {
                        progressive,
                        ..CodingOptions::default()
                    },
                    ..EncodeOptions::default()
                };
                let bytes = encode_samples(&input, options).unwrap();
                let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
                assert_eq!(decoded.color_space(), space);
                assert_eq!(decoded.scans, if progressive { channels + 1 } else { 1 });
                assert_eq!(decoded.frame.precision, 12);
                let planes = decoded.reconstruct().unwrap();
                for c in planes.components {
                    assert_eq!(c.samples.len(), c.width * c.height);
                    assert!(c.samples.iter().all(|&s| s <= 4095));
                }
            }
        }
    }
}

#[test]
fn shared_precision_scaling_and_quality_affect_reconstruction() {
    let image = Image {
        width: 9,
        height: 7,
        color_type: ColorType::Grayscale,
        bit_depth: 16,
        samples: Samples::U16(vec![65535; 9 * 7]),
    };
    for precision in [8, 12] {
        let bytes = encode(
            &image,
            EncodeOptions {
                quality: 100,
                precision,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
        let planes = decoded.reconstruct().unwrap();
        assert!(
            planes.components[0]
                .samples
                .iter()
                .all(|&s| s == (1 << precision) - 1)
        );
    }
    let pixels: Vec<_> = (0..31 * 25)
        .map(|i| ((i * 17 + i * i % 997) % 256) as u8)
        .collect();
    let image = Image {
        width: 31,
        height: 25,
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8(pixels.clone()),
    };
    let result = |quality| {
        let bytes = encode(
            &image,
            EncodeOptions {
                quality,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        let decoded = decode_coefficients(&bytes, Limits::default())
            .unwrap()
            .reconstruct()
            .unwrap();
        let error: u64 = decoded.components[0]
            .samples
            .iter()
            .zip(&pixels)
            .map(|(&a, &b)| i64::from(a).abs_diff(i64::from(b)).pow(2))
            .sum();
        (bytes.len(), error)
    };
    let (low_size, low_error) = result(10);
    let (high_size, high_error) = result(95);
    assert!(high_size > low_size);
    assert!(high_error < low_error);
    let subbyte = Image {
        width: 4,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 2,
        samples: Samples::U8(vec![0, 1, 2, 3]),
    };
    assert!(encode(&subbyte, EncodeOptions::default()).is_ok());
}

#[test]
fn invalid_samples_options_and_resource_budgets_are_rejected() {
    let input = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Rgb,
        samples: SampleData::U8(&[128; 192]),
    };
    for options in [
        EncodeOptions {
            quality: 0,
            ..EncodeOptions::default()
        },
        EncodeOptions {
            quality: 101,
            ..EncodeOptions::default()
        },
        EncodeOptions {
            precision: 10,
            ..EncodeOptions::default()
        },
        EncodeOptions {
            subsampling: Subsampling::Custom([(0, 1); 4]),
            ..EncodeOptions::default()
        },
        EncodeOptions {
            subsampling: Subsampling::Custom([(4, 4); 4]),
            ..EncodeOptions::default()
        },
    ] {
        assert!(encode_samples(&input, options).is_err());
    }
    for limits in [
        Limits {
            max_pixels: 1,
            ..Limits::default()
        },
        Limits {
            max_memory_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_metadata_bytes: 0,
            ..Limits::default()
        },
        Limits {
            max_scans: 0,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            encode_samples(
                &input,
                EncodeOptions {
                    limits,
                    ..EncodeOptions::default()
                }
            ),
            Err(Error::LimitExceeded(_))
        ));
    }
    assert!(
        encode_samples(
            &EncodeInput {
                samples: SampleData::U8(&[0; 191]),
                ..input
            },
            EncodeOptions::default()
        )
        .is_err()
    );
    assert!(
        encode_samples(
            &EncodeInput {
                precision: 7,
                ..input
            },
            EncodeOptions::default()
        )
        .is_err()
    );
    assert!(encode_samples(&EncodeInput { width: 0, ..input }, EncodeOptions::default()).is_err());
    assert!(
        encode_samples(
            &EncodeInput {
                precision: 9,
                ..input
            },
            EncodeOptions::default()
        )
        .is_err()
    );
    assert!(
        encode_samples(
            &input,
            EncodeOptions {
                coding: CodingOptions {
                    max_output_bytes: 0,
                    ..CodingOptions::default()
                },
                ..EncodeOptions::default()
            }
        )
        .is_err()
    );
    let too_wide = Image {
        width: 65536,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8(vec![0; 65536]),
    };
    assert!(encode(&too_wide, EncodeOptions::default()).is_err());
    let alpha = Image {
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: Samples::U8(vec![0; 4]),
    };
    assert!(encode(&alpha, EncodeOptions::default()).is_err());
}

#[test]
fn full_quality_quantizers_and_extended_sequential_output() {
    let input = EncodeInput {
        width: 8,
        height: 8,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&[128; 64]),
    };
    let full = quantize(
        &input,
        EncodeOptions {
            quality: 100,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(full.components[0].quantization, [1; 64]);
    let coarse = quantize(
        &input,
        EncodeOptions {
            quality: 1,
            baseline_quantization: false,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    assert!(coarse.components[0].quantization.iter().any(|&q| q > 255));
    assert_eq!(
        coarse.frame.process,
        deadlib_image_jpeg::Process::ExtendedSequential
    );
    let bytes = encode_samples(
        &input,
        EncodeOptions {
            quality: 1,
            baseline_quantization: false,
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
    assert_eq!(
        decoded.frame.process,
        deadlib_image_jpeg::Process::ExtendedSequential
    );
}
