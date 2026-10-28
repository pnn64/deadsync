use deadlib_image::{ColorType, Image, Samples};
use deadlib_image_jpeg::{
    CmykPolarity, ColorSpace, ComponentPlane, ComponentSpec, EncodeInput, Error, Event,
    FrameHeader, IccWrite, Limits, LosslessImage, LosslessOptions, LosslessScan, MarkerReader,
    MetadataOptions, MetadataSegment, Process, SampleData, decode_lossless, encode_lossless,
    encode_lossless_samples, encode_lossless_samples_with_metadata,
};

fn source(p: u8, count: usize, w: usize, h: usize) -> Vec<u16> {
    let mut state = 0x82345678u32;
    (0..count * w * h)
        .map(|i| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (if i % 7 == 0 {
                if i % 2 == 0 { 0 } else { 65535 }
            } else {
                state >> 8
            }) as u16
                & ((1u32 << p) - 1) as u16
        })
        .collect()
}

#[test]
fn all_precisions_predictors_point_transforms_and_entropy_policies() {
    for precision in 2..=16 {
        for (color_space, count) in [
            (ColorSpace::Grayscale, 1),
            (ColorSpace::Rgb, 3),
            (ColorSpace::YCbCr, 3),
            (ColorSpace::Cmyk, 4),
            (ColorSpace::Ycck, 4),
        ] {
            let values = source(precision, count, 9, 11);
            let input = EncodeInput {
                width: 9,
                height: 11,
                precision,
                color_space,
                samples: SampleData::U16(&values),
            };
            for predictor in 1..=7 {
                for point in [0, precision - 1] {
                    for optimize in [false, true] {
                        let bytes = encode_lossless_samples(
                            &input,
                            LosslessOptions {
                                predictor,
                                point_transform: point,
                                restart_rows: 2,
                                optimize_huffman: optimize,
                                separate_components: predictor % 2 == 0,
                                ..LosslessOptions::default()
                            },
                        )
                        .unwrap();
                        let image = decode_lossless(&bytes, Limits::default()).unwrap();
                        assert_eq!(image.color_space(), color_space);
                        assert_eq!(image.frame.precision, precision);
                        for (i, c) in image.components.iter().enumerate() {
                            assert_eq!(
                                c.samples,
                                values
                                    .iter()
                                    .skip(i)
                                    .step_by(count)
                                    .map(|v| (v >> point) << point)
                                    .collect::<Vec<_>>()
                            );
                        }
                        assert!(
                            !MarkerReader::new(&bytes)
                                .unwrap()
                                .any(|e| matches!(e, Ok(Event::Segment { marker: 0xdb, .. })))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn shared_images_preserve_native_values_and_cmyk_polarity_is_explicit() {
    for (color_type, count) in [(ColorType::Grayscale, 1), (ColorType::Rgb, 3)] {
        for bit_depth in [2, 4, 8, 16] {
            if color_type == ColorType::Rgb && bit_depth < 8 {
                continue;
            }
            let values = source(bit_depth, count, 5, 3);
            let image = Image {
                width: 5,
                height: 3,
                color_type,
                bit_depth,
                samples: if bit_depth <= 8 {
                    Samples::U8(values.iter().map(|&v| v as u8).collect())
                } else {
                    Samples::U16(values.clone())
                },
            };
            let bytes = encode_lossless(&image, LosslessOptions::default()).unwrap();
            let decoded = decode_lossless(&bytes, Limits::default()).unwrap();
            for (i, c) in decoded.components.iter().enumerate() {
                assert_eq!(
                    c.samples,
                    values
                        .iter()
                        .skip(i)
                        .step_by(count)
                        .copied()
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    for polarity in [CmykPolarity::Conventional, CmykPolarity::AdobeInverted] {
        let values = [0, 123, 4567, 65535];
        let bytes = encode_lossless_samples(
            &EncodeInput {
                width: 1,
                height: 1,
                precision: 16,
                color_space: ColorSpace::Cmyk,
                samples: SampleData::U16(&values),
            },
            LosslessOptions {
                cmyk_polarity: polarity,
                ..LosslessOptions::default()
            },
        )
        .unwrap();
        let raster = decode_lossless(&bytes, Limits::default())
            .unwrap()
            .into_planar()
            .unwrap()
            .to_cmyk(deadlib_image_jpeg::Upsampling::Nearest)
            .unwrap();
        assert_eq!(raster.samples, values);
        assert_eq!(raster.polarity, polarity);
    }
}

fn planar(
    count: usize,
    precision: u8,
    factors: &[(u8, u8)],
    w: u16,
    h: u16,
) -> LosslessImage<'static> {
    let max_h = factors.iter().map(|f| f.0).max().unwrap() as usize;
    let max_v = factors.iter().map(|f| f.1).max().unwrap() as usize;
    let specs: Vec<_> = (0..count)
        .map(|i| ComponentSpec {
            id: i as u8,
            horizontal_sampling: factors[i].0,
            vertical_sampling: factors[i].1,
            quantization_table: 0,
        })
        .collect();
    let components = specs
        .iter()
        .enumerate()
        .map(|(i, &spec)| {
            let width = (w as usize * spec.horizontal_sampling as usize).div_ceil(max_h);
            let height = (h as usize * spec.vertical_sampling as usize).div_ceil(max_v);
            let mut samples = source(precision, 1, width, height);
            for v in &mut samples {
                *v ^= i as u16 & ((1u32 << precision) - 1) as u16;
            }
            ComponentPlane {
                spec,
                width,
                height,
                samples,
            }
        })
        .collect();
    LosslessImage::from_planes(
        FrameHeader {
            process: Process::Lossless,
            precision,
            width: w,
            height: h,
            components: specs,
        },
        components,
        vec![],
        Limits::default(),
    )
    .unwrap()
}

#[test]
fn subsampling_odd_padding_and_full_component_counts() {
    for factors in [
        &[(2, 1), (1, 1)][..],
        &[(1, 2), (1, 1)],
        &[(3, 2), (2, 1), (1, 1)],
        &[(4, 4), (1, 1), (3, 4), (2, 3)],
    ] {
        for precision in [2, 8, 12, 16] {
            let image = planar(factors.len(), precision, factors, 7, 9);
            for predictor in 1..=7 {
                for restart_rows in [0, 1, 2] {
                    for separate_components in [false, true] {
                        let bytes = image
                            .encode(LosslessOptions {
                                predictor,
                                restart_rows,
                                separate_components,
                                ..LosslessOptions::default()
                            })
                            .unwrap();
                        let decoded = decode_lossless(&bytes, Limits::default()).unwrap();
                        for (a, b) in image.components.iter().zip(&decoded.components) {
                            assert_eq!(a.samples, b.samples);
                            assert_eq!(a.spec, b.spec);
                        }
                    }
                }
            }
        }
    }
    let factors = vec![(4, 4); 255];
    let image = planar(255, 16, &factors, 3, 2);
    let bytes = image.encode(LosslessOptions::default()).unwrap();
    let decoded = decode_lossless(&bytes, Limits::default()).unwrap();
    assert_eq!(decoded.scans, 255);
    for (a, b) in image.components.iter().zip(&decoded.components) {
        assert_eq!(a.samples, b.samples);
    }
}

#[test]
fn custom_scans_preserve_metadata_and_change_restart_intervals() {
    let mut image = planar(4, 16, &[(3, 1), (2, 1), (1, 1), (1, 1)], 11, 19);
    image.metadata = vec![
        MetadataSegment {
            marker: 0xfe,
            data: b"prefix",
            offset: 0,
            preceding_scans: 0,
        },
        MetadataSegment {
            marker: 0xe7,
            data: b"middle",
            offset: 0,
            preceding_scans: 1,
        },
        MetadataSegment {
            marker: 0xfe,
            data: b"suffix",
            offset: 0,
            preceding_scans: 99,
        },
    ];
    let scans = [
        LosslessScan::new(&[3], 7, 15, 1).unwrap(),
        LosslessScan::new(&[1], 6, 3, 0).unwrap(),
        LosslessScan::new(&[0, 2], 4, 0, 2).unwrap(),
    ];
    let extra = [MetadataSegment {
        marker: 0xe8,
        data: b"added",
        offset: 0,
        preceding_scans: 0,
    }];
    let icc = include_bytes!("fixtures/metadata/srgb.icc");
    let bytes = image
        .encode_with_metadata(
            LosslessOptions {
                scan_script: &scans,
                ..LosslessOptions::default()
            },
            MetadataOptions {
                additional_segments: &extra,
                icc: IccWrite::Replace(icc),
            },
        )
        .unwrap();
    let decoded = decode_lossless(&bytes, Limits::default()).unwrap();
    assert_eq!(decoded.scans, 3);
    for (i, c) in decoded.components.iter().enumerate() {
        let pt = match i {
            1 => 3,
            3 => 15,
            _ => 0,
        };
        assert_eq!(
            c.samples,
            image.components[i]
                .samples
                .iter()
                .map(|v| (v >> pt) << pt)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        decoded
            .metadata
            .iter()
            .find(|m| m.data == b"middle")
            .unwrap()
            .preceding_scans,
        1
    );
    assert_eq!(decoded.metadata.last().unwrap().data, b"suffix");
    assert_eq!(
        decoded
            .icc_profile()
            .unwrap()
            .unwrap()
            .to_vec(icc.len())
            .unwrap(),
        icc
    );
    let dri: Vec<_> = MarkerReader::new(&bytes)
        .unwrap()
        .filter_map(|e| match e.unwrap() {
            Event::Segment {
                marker: 0xdd, data, ..
            } => Some(u16::from_be_bytes(data.try_into().unwrap())),
            _ => None,
        })
        .collect();
    assert_eq!(dri, [4, 0, 8]);
    let removed = decoded
        .encode_with_metadata(
            LosslessOptions::default(),
            MetadataOptions {
                icc: IccWrite::Remove,
                ..MetadataOptions::default()
            },
        )
        .unwrap();
    assert!(
        decode_lossless(&removed, Limits::default())
            .unwrap()
            .icc_profile()
            .unwrap()
            .is_none()
    );
}

#[test]
fn fixed_entropy_matches_independent_annex_h_literals() {
    for (width, height, predictor, values, expected) in [
        (1, 1, 1, &[0][..], &[0x87][..]),
        (
            2,
            2,
            4,
            &[65535, 0, 0, 65535][..],
            &[0x7f, 0xff, 0x00, 0xf0, 0xc3, 0x13][..],
        ),
    ] {
        let bytes = encode_lossless_samples(
            &EncodeInput {
                width,
                height,
                precision: 16,
                color_space: ColorSpace::Grayscale,
                samples: SampleData::U16(values),
            },
            LosslessOptions {
                predictor,
                optimize_huffman: false,
                ..LosslessOptions::default()
            },
        )
        .unwrap();
        let entropy = MarkerReader::new(&bytes)
            .unwrap()
            .find_map(|e| match e.unwrap() {
                Event::Scan { entropy, .. } => Some(entropy),
                _ => None,
            })
            .unwrap();
        assert_eq!(entropy, expected);
    }
    // Exact manually derived Annex H fixtures also used by the decoder gate;
    // compare entropy bytes to avoid relying only on matching native traversal.
    for (w, h, factors, restart_rows, expected) in [
        (
            3,
            2,
            &[(2, 1), (1, 1)][..],
            0,
            &[0x00, 0x60, 0x0c, 0x01, 0x0b, 0x00, 0x90, 0x00, 0x1f][..],
        ),
        (
            2,
            3,
            &[(1, 2), (1, 1)][..],
            0,
            &[0x00, 0xa0, 0x06, 0x00, 0x85, 0x00, 0x48, 0x00, 0x0f][..],
        ),
        (
            2,
            3,
            &[(1, 2), (1, 1)][..],
            1,
            &[
                0x00, 0xa0, 0x06, 0x00, 0xbf, 0xff, 0xd0, 0x1c, 0x00, 0x90, 0xc0, 0x17,
            ][..],
        ),
    ] {
        let mut image = planar(2, 8, factors, w, h);
        image.components[0].samples = vec![128, 129, 130, 131, 132, 133];
        image.components[1].samples = vec![128, 127, 126, 125];
        let bytes = image
            .encode(LosslessOptions {
                predictor: 4,
                restart_rows,
                optimize_huffman: false,
                ..LosslessOptions::default()
            })
            .unwrap();
        let entropy = MarkerReader::new(&bytes)
            .unwrap()
            .find_map(|e| match e.unwrap() {
                Event::Scan { entropy, .. } => Some(entropy),
                _ => None,
            })
            .unwrap();
        assert_eq!(entropy, expected);
    }
}

#[test]
fn invalid_samples_scripts_and_budgets_return_errors() {
    let image = planar(2, 8, &[(1, 1), (1, 1)], 2, 2);
    for script in [
        vec![LosslessScan::new(&[0], 1, 0, 0).unwrap()],
        vec![LosslessScan::new(&[1, 0], 1, 0, 0).unwrap()],
        vec![LosslessScan::new(&[0, 0], 1, 0, 0).unwrap()],
        vec![LosslessScan::new(&[0, 2], 1, 0, 0).unwrap()],
        vec![LosslessScan::new(&[0, 1], 0, 0, 0).unwrap()],
        vec![LosslessScan::new(&[0, 1], 1, 8, 0).unwrap()],
    ] {
        assert!(
            image
                .encode(LosslessOptions {
                    scan_script: &script,
                    ..LosslessOptions::default()
                })
                .is_err()
        );
    }
    for options in [
        LosslessOptions {
            predictor: 8,
            ..LosslessOptions::default()
        },
        LosslessOptions {
            point_transform: 8,
            ..LosslessOptions::default()
        },
        LosslessOptions {
            restart_rows: 65535,
            ..LosslessOptions::default()
        },
        LosslessOptions {
            max_output_bytes: 1,
            ..LosslessOptions::default()
        },
        LosslessOptions {
            limits: Limits {
                max_pixels: 0,
                ..Limits::default()
            },
            ..LosslessOptions::default()
        },
        LosslessOptions {
            limits: Limits {
                max_memory_bytes: 1,
                ..Limits::default()
            },
            ..LosslessOptions::default()
        },
        LosslessOptions {
            limits: Limits {
                max_scans: 0,
                ..Limits::default()
            },
            ..LosslessOptions::default()
        },
    ] {
        assert!(image.encode(options).is_err());
    }
    for (precision, samples) in [(1, vec![0]), (17, vec![0]), (2, vec![4]), (8, vec![])] {
        assert!(
            encode_lossless_samples(
                &EncodeInput {
                    width: 1,
                    height: 1,
                    precision,
                    color_space: ColorSpace::Grayscale,
                    samples: SampleData::U16(&samples)
                },
                LosslessOptions::default()
            )
            .is_err()
        );
    }
    let values = [0];
    let input = EncodeInput {
        width: 1,
        height: 1,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&values),
    };
    assert!(matches!(
        encode_lossless_samples_with_metadata(
            &input,
            LosslessOptions {
                limits: Limits {
                    max_metadata_bytes: 1,
                    ..Limits::default()
                },
                ..LosslessOptions::default()
            },
            MetadataOptions {
                icc: IccWrite::Replace(&[0; 100]),
                ..MetadataOptions::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    let mut malformed = image.clone();
    malformed.components[0].samples.clear();
    assert!(malformed.encode(LosslessOptions::default()).is_err());
}
