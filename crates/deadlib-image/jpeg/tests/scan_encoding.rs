use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, Error, Event, Limits, MarkerReader, ScanSpec,
    decode_coefficients, encode_coefficients,
};

fn same_coefficients(
    a: &deadlib_image_jpeg::CoefficientImage<'_>,
    b: &deadlib_image_jpeg::CoefficientImage<'_>,
) {
    assert_eq!(a.frame.precision, b.frame.precision);
    for (a, b) in a.components.iter().zip(&b.components) {
        assert_eq!(a.spec, b.spec);
        assert_eq!(a.quantization, b.quantization);
        for y in 0..a.height.div_ceil(8) {
            for x in 0..a.width.div_ceil(8) {
                assert_eq!(
                    a.blocks[y * a.blocks_wide + x],
                    b.blocks[y * b.blocks_wide + x]
                );
            }
        }
    }
}

#[test]
fn generated_successive_approximation_retains_coefficients() {
    for bytes in [
        &include_bytes!("fixtures/sequential/8-420.jpg")[..],
        &include_bytes!("fixtures/sequential/12-444.jpg")[..],
        &include_bytes!("fixtures/sequential/8-cmyk.jpg")[..],
    ] {
        let image = decode_coefficients(bytes, Limits::default()).unwrap();
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for successive_approximation in [1, 3, 13] {
                for restart_interval in [0, 1, 7] {
                    for optimize_huffman in [false, true] {
                        let encoded = encode_coefficients(
                            &image,
                            CodingOptions {
                                progressive: true,
                                successive_approximation,
                                entropy_coding,
                                restart_interval,
                                optimize_huffman,
                                ..CodingOptions::default()
                            },
                        )
                        .unwrap();
                        let decoded = decode_coefficients(&encoded, Limits::default()).unwrap();
                        same_coefficients(&image, &decoded);
                        assert_eq!(
                            decoded.scans,
                            (successive_approximation as usize + 1) * (image.components.len() + 1)
                        );
                        assert!(
                            decoded
                                .components
                                .iter()
                                .all(|c| c.successive_approximation == [0; 64])
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn split_bands_reversed_component_order_and_partial_precision() {
    let bytes = include_bytes!("fixtures/sequential/12-420.jpg");
    let image = decode_coefficients(bytes, Limits::default()).unwrap();
    let ids: Vec<_> = image.components.iter().map(|c| c.spec.id).collect();
    let mut script = Vec::new();
    for al in (0..=3).rev() {
        let ah = if al == 3 { 0 } else { al + 1 };
        for &id in ids.iter().rev() {
            script.push(ScanSpec::new(&[id], 0, 0, ah, al).unwrap());
        }
        for &id in ids.iter().rev() {
            for (ss, se) in [(6, 63), (1, 5)] {
                script.push(ScanSpec::new(&[id], ss, se, ah, al).unwrap());
            }
        }
    }
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        for restart_interval in [0, 1, 7] {
            let options = CodingOptions {
                progressive: true,
                entropy_coding,
                restart_interval,
                scan_script: &script,
                ..CodingOptions::default()
            };
            let encoded = encode_coefficients(&image, options).unwrap();
            let decoded = decode_coefficients(&encoded, Limits::default()).unwrap();
            same_coefficients(&image, &decoded);
            let partial = encode_coefficients(
                &image,
                CodingOptions {
                    scan_script: &script[..ids.len()],
                    ..options
                },
            )
            .unwrap();
            let partial = decode_coefficients(&partial, Limits::default()).unwrap();
            for (c, original) in partial.components.iter().zip(&image.components) {
                assert_eq!(c.successive_approximation[0], 3);
                assert_eq!(&c.successive_approximation[1..], &[-1; 63]);
                for y in 0..c.height.div_ceil(8) {
                    for x in 0..c.width.div_ceil(8) {
                        assert_eq!(
                            c.blocks[y * c.blocks_wide + x][0],
                            (original.blocks[y * original.blocks_wide + x][0] >> 3) << 3
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn full_sixteen_bit_coefficients_can_use_huffman_progressive_point_transforms() {
    for name in ["p8-gray-17x17-w0-d1", "p12-ycck-17x17-w1-d1"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/arithmetic-model/{name}.jpg"));
        let bytes = std::fs::read(path).unwrap();
        let image = decode_coefficients(&bytes, Limits::default()).unwrap();
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for restart_interval in [0, 1] {
                let encoded = encode_coefficients(
                    &image,
                    CodingOptions {
                        progressive: true,
                        successive_approximation: 13,
                        entropy_coding,
                        restart_interval,
                        ..CodingOptions::default()
                    },
                )
                .unwrap();
                same_coefficients(
                    &image,
                    &decode_coefficients(&encoded, Limits::default()).unwrap(),
                );
            }
        }
    }
}

#[test]
fn invalid_scan_scripts_and_scan_budgets_are_errors() {
    let bytes = include_bytes!("fixtures/sequential/8-420.jpg");
    let image = decode_coefficients(bytes, Limits::default()).unwrap();
    let ids: Vec<_> = image.components.iter().map(|c| c.spec.id).collect();
    let dc = ScanSpec::new(&ids, 0, 0, 0, 1).unwrap();
    let valid = [dc, ScanSpec::new(&[ids[0]], 1, 63, 0, 0).unwrap()];
    for script in [
        vec![ScanSpec {
            component_count: 0,
            ..dc
        }],
        vec![ScanSpec {
            component_count: 5,
            ..dc
        }],
        vec![ScanSpec::new(&[255], 0, 0, 0, 0).unwrap()],
        vec![ScanSpec::new(&[ids[0], ids[0]], 0, 0, 0, 0).unwrap()],
        vec![ScanSpec::new(&[ids[2], ids[1], ids[0]], 0, 0, 0, 0).unwrap()],
        vec![ScanSpec {
            successive_low: 14,
            ..dc
        }],
        vec![ScanSpec {
            successive_high: 255,
            ..dc
        }],
        vec![ScanSpec {
            spectral_end: 63,
            ..dc
        }],
        vec![dc, dc],
        vec![valid[1]],
        vec![dc, ScanSpec::new(&[ids[0]], 1, 63, 1, 0).unwrap()],
        vec![ScanSpec::new(&[ids[0]], 0, 0, 0, 0).unwrap()],
    ] {
        assert!(
            encode_coefficients(
                &image,
                CodingOptions {
                    progressive: true,
                    scan_script: &script,
                    ..CodingOptions::default()
                }
            )
            .is_err()
        );
    }
    let limited = decode_coefficients(
        bytes,
        Limits {
            max_scans: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        encode_coefficients(
            &limited,
            CodingOptions {
                progressive: true,
                successive_approximation: 1,
                ..CodingOptions::default()
            }
        ),
        Err(Error::LimitExceeded("encoder scans"))
    ));
    assert!(
        encode_coefficients(
            &image,
            CodingOptions {
                progressive: true,
                successive_approximation: 14,
                ..CodingOptions::default()
            }
        )
        .is_err()
    );
    assert!(ScanSpec::new(&[], 0, 0, 0, 0).is_err());
    assert!(ScanSpec::new(&[0; 5], 0, 0, 0, 0).is_err());
}

#[test]
fn sos_components_must_follow_frame_order_for_both_entropy_coders() {
    let image = decode_coefficients(
        include_bytes!("fixtures/sequential/8-420.jpg"),
        Limits::default(),
    )
    .unwrap();
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        for progressive in [false, true] {
            let mut bytes = encode_coefficients(
                &image,
                CodingOptions {
                    progressive,
                    entropy_coding,
                    ..CodingOptions::default()
                },
            )
            .unwrap();
            let offset = MarkerReader::new(&bytes)
                .unwrap()
                .find_map(|e| match e.unwrap() {
                    Event::Scan { offset, .. } => Some(offset),
                    _ => None,
                })
                .unwrap();
            bytes.swap(offset + 5, offset + 7);
            assert!(matches!(
                decode_coefficients(&bytes, Limits::default()),
                Err(Error::Invalid("SOS component order"))
            ));
        }
    }
}

#[test]
fn separate_scans_allow_large_sampling_mcus_and_pixel_api_scripts() {
    use deadlib_image_jpeg::{
        ColorSpace, EncodeInput, EncodeOptions, SampleData, Subsampling, encode_samples,
    };
    let samples = vec![97u8; 19 * 17 * 4];
    let input = EncodeInput {
        width: 19,
        height: 17,
        precision: 8,
        color_space: ColorSpace::Ycck,
        samples: SampleData::U8(&samples),
    };
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        for progressive in [false, true] {
            let mut script = Vec::new();
            for id in (1..=4).rev() {
                script
                    .push(ScanSpec::new(&[id], 0, if progressive { 0 } else { 63 }, 0, 0).unwrap());
                if progressive {
                    script.push(ScanSpec::new(&[id], 1, 63, 0, 0).unwrap());
                }
            }
            let encoded = encode_samples(
                &input,
                EncodeOptions {
                    subsampling: Subsampling::S410,
                    coding: CodingOptions {
                        progressive,
                        entropy_coding,
                        scan_script: &script,
                        restart_interval: 1,
                        ..CodingOptions::default()
                    },
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
            let image = decode_coefficients(&encoded, Limits::default()).unwrap();
            assert_eq!(image.scans, if progressive { 8 } else { 4 });
            image.reconstruct().unwrap();
            let scans = MarkerReader::new(&encoded)
                .unwrap()
                .filter(|e| matches!(e, Ok(Event::Scan { .. })))
                .count();
            assert_eq!(scans, image.scans);
        }
    }
}
