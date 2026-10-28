use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, ComponentPlane, ComponentSpec, EncodeInput, EntropyCoding,
    Expansion, HierarchicalData, HierarchyHeader, Limits, LosslessOptions, PyramidLevel,
    PyramidOptions, PyramidRasterOptions, SampleData, ScanSpec, Subsampling,
    build_hierarchical_planes, build_hierarchical_samples, decode_hierarchical,
    encode_hierarchical_planes, encode_hierarchical_samples,
};
const BOTH: Expansion = Expansion {
    horizontal: true,
    vertical: true,
};
const NONE: Expansion = Expansion {
    horizontal: false,
    vertical: false,
};
fn planes(p: u8, w: u16, h: u16, factors: &[(u8, u8)]) -> (HierarchyHeader, Vec<ComponentPlane>) {
    let max_h = factors.iter().map(|v| v.0 as usize).max().unwrap();
    let max_v = factors.iter().map(|v| v.1 as usize).max().unwrap();
    let mut specs = vec![];
    let mut components = vec![];
    let maximum = (1u32 << p) - 1;
    for (i, &(sh, sv)) in factors.iter().enumerate() {
        let spec = ComponentSpec {
            id: i as u8,
            horizontal_sampling: sh,
            vertical_sampling: sv,
            quantization_table: 0,
        };
        specs.push(spec);
        let width = (w as usize * sh as usize).div_ceil(max_h);
        let height = (h as usize * sv as usize).div_ceil(max_v);
        let samples = (0..width * height)
            .map(|j| ((j * 7319 + j * j * 117 + i * 911) as u32 & maximum) as u16)
            .collect();
        components.push(ComponentPlane {
            spec,
            width,
            height,
            samples,
        });
    }
    (
        HierarchyHeader {
            precision: p,
            width: w,
            height: h,
            components: specs,
        },
        components,
    )
}
fn lp(entropy_coding: EntropyCoding, point: u8) -> LosslessOptions<'static> {
    LosslessOptions {
        entropy_coding,
        point_transform: point,
        predictor: 7,
        restart_rows: 1,
        ..LosslessOptions::default()
    }
}
#[test]
fn exact_all_precisions_sampling_axes_and_same_resolution_refinements() {
    for p in 2..=16 {
        for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for factors in [
                &[(1, 1)][..],
                &[(2, 2), (1, 1), (1, 1)][..],
                &[(4, 3), (3, 2), (1, 1)][..],
                &[(4, 4); 4][..],
            ] {
                let (header, source) = planes(p, 17, 13, factors);
                let levels = [
                    PyramidLevel::lossless(NONE, lp(entropy, p - 1)),
                    PyramidLevel::lossless(BOTH, lp(entropy, (p - 1).min(3))),
                    PyramidLevel::lossless(NONE, lp(entropy, 0)),
                    PyramidLevel::lossless(
                        Expansion {
                            horizontal: true,
                            vertical: false,
                        },
                        lp(entropy, 0),
                    ),
                    PyramidLevel::lossless(
                        Expansion {
                            horizontal: false,
                            vertical: true,
                        },
                        lp(entropy, 0),
                    ),
                ];
                let encoded = encode_hierarchical_planes(
                    header,
                    &source,
                    vec![],
                    PyramidOptions::new(&levels),
                )
                .unwrap();
                let image = decode_hierarchical(&encoded, Limits::default()).unwrap();
                assert_eq!(image.frames.len(), 5);
                for (a, b) in image.reconstruct_reference().unwrap().iter().zip(&source) {
                    assert_eq!(a.samples, b.samples);
                }
            }
        }
    }
}
#[test]
fn k5_literal_filter_and_fractional_component_rounding() {
    let input: Vec<u16> = (0..3)
        .flat_map(|y| (0..5).map(move |x| x * 10 + y * 40))
        .collect();
    let raster = EncodeInput {
        width: 5,
        height: 3,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U16(&input),
    };
    let levels = [
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
        PyramidLevel::lossless(BOTH, LosslessOptions::default()),
    ];
    let model = build_hierarchical_samples(
        &raster,
        PyramidRasterOptions::default(),
        PyramidOptions::new(&levels),
    )
    .unwrap();
    if let HierarchicalData::Lossless { components, .. } = &model.frames[0].data {
        assert_eq!(components[0].samples, [13, 30, 48, 73, 90, 108]);
    }
    assert_eq!(model.reconstruct_reference().unwrap()[0].samples, input);
    let (header, mut source) = planes(8, 5, 1, &[(4, 1), (3, 1)]);
    source[1].samples = vec![0, 30, 60, 90];
    let levels = [
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
        PyramidLevel::lossless(
            Expansion {
                horizontal: true,
                vertical: false,
            },
            LosslessOptions::default(),
        ),
    ];
    let model =
        build_hierarchical_planes(header, &source, vec![], PyramidOptions::new(&levels)).unwrap();
    if let HierarchicalData::Lossless { components, .. } = &model.frames[0].data {
        assert_eq!(components[1].samples, [8, 60, 90]);
    }
    assert_eq!(
        model.reconstruct_reference().unwrap()[1].samples,
        source[1].samples
    );
}
#[test]
fn repeated_singleton_resolutions_and_full_component_count() {
    let (header, source) = planes(16, 3, 1, &vec![(1, 1); 255]);
    for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let mut levels = vec![PyramidLevel::lossless(NONE, lp(entropy, 0))];
        levels.extend((0..7).map(|_| PyramidLevel::lossless(BOTH, lp(entropy, 0))));
        let bytes = encode_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions::new(&levels),
        )
        .unwrap();
        let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(model.header.components.len(), 255);
        for (a, b) in model.reconstruct_reference().unwrap().iter().zip(&source) {
            assert_eq!(a.samples, b.samples);
        }
    }
}
#[test]
fn dct_residuals_and_final_spatial_correction_use_matching_references() {
    for p in [8, 12] {
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for progressive in [false, true] {
                for quality in [30, 75, 100] {
                    let (header, source) = planes(p, 33, 25, &[(2, 2), (1, 1), (1, 1)]);
                    let coding = CodingOptions {
                        entropy_coding,
                        progressive,
                        successive_approximation: if progressive { 3 } else { 0 },
                        restart_interval: 3,
                        ..CodingOptions::default()
                    };
                    let levels = [
                        PyramidLevel::dct(NONE, quality, coding),
                        PyramidLevel::dct(BOTH, quality, coding),
                        PyramidLevel::dct(BOTH, quality, coding),
                        PyramidLevel::lossless(NONE, lp(entropy_coding, 0)),
                    ];
                    let bytes = encode_hierarchical_planes(
                        header,
                        &source,
                        vec![],
                        PyramidOptions::new(&levels),
                    )
                    .unwrap();
                    let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
                    for (a, b) in model.reconstruct_reference().unwrap().iter().zip(&source) {
                        assert_eq!(a.samples, b.samples);
                    }
                }
            }
        }
    }
}
#[test]
fn partial_progressive_base_is_applied_before_residual_generation() {
    let (header, source) = planes(8, 17, 11, &[(1, 1)]);
    let script = [ScanSpec::new(&[0], 0, 0, 0, 3).unwrap()];
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let partial = CodingOptions {
            entropy_coding,
            progressive: true,
            scan_script: &script,
            ..CodingOptions::default()
        };
        let full = CodingOptions {
            entropy_coding,
            progressive: true,
            successive_approximation: 3,
            ..CodingOptions::default()
        };
        let levels = [
            PyramidLevel::dct(NONE, 75, partial),
            PyramidLevel::dct(BOTH, 75, full),
            PyramidLevel::lossless(NONE, lp(entropy_coding, 0)),
        ];
        let model = build_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions::new(&levels),
        )
        .unwrap();
        if let HierarchicalData::Dct(c) = &model.frames[0].data {
            assert_eq!(c[0].successive_approximation[0], 3);
            assert_eq!(c[0].successive_approximation[1], -1);
            assert!(c[0].blocks.iter().all(|b| b[1..].iter().all(|&v| v == 0)));
        }
        let bytes = encode_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions::new(&levels),
        )
        .unwrap();
        assert_eq!(
            decode_hierarchical(&bytes, Limits::default())
                .unwrap()
                .reconstruct_reference()
                .unwrap()[0]
                .samples,
            source[0].samples
        );
    }
}
#[test]
fn native_raster_defaults_and_explicit_high_precision_color_conversion() {
    let levels = [
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
        PyramidLevel::lossless(BOTH, LosslessOptions::default()),
    ];
    for p in [2, 8, 12, 16] {
        for (space, n) in [
            (ColorSpace::Grayscale, 1),
            (ColorSpace::Rgb, 3),
            (ColorSpace::YCbCr, 3),
            (ColorSpace::Cmyk, 4),
            (ColorSpace::Ycck, 4),
        ] {
            let max = (1u32 << p) - 1;
            let values: Vec<u16> = (0..7 * 5 * n)
                .map(|j| ((j * 7319 + j * j * 117) as u32 & max) as u16)
                .collect();
            let input = EncodeInput {
                width: 7,
                height: 5,
                precision: p,
                color_space: space,
                samples: SampleData::U16(&values),
            };
            let bytes = encode_hierarchical_samples(
                &input,
                PyramidRasterOptions::default(),
                PyramidOptions::new(&levels),
            )
            .unwrap();
            let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
            assert_eq!(model.color_space(), space);
            for (i, c) in model.reconstruct_reference().unwrap().iter().enumerate() {
                assert_eq!(
                    c.samples,
                    values
                        .iter()
                        .skip(i)
                        .step_by(n)
                        .copied()
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    // Selecting CMYK polarity labels the supplied integers; the pyramid
    // source API never complements an unchanged CMYK component implicitly.
    let dct_levels = [
        PyramidLevel::dct(NONE, 75, CodingOptions::default()),
        PyramidLevel::dct(BOTH, 75, CodingOptions::default()),
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
    ];
    for polarity in [
        deadlib_image_jpeg::CmykPolarity::Conventional,
        deadlib_image_jpeg::CmykPolarity::AdobeInverted,
    ] {
        let values: Vec<u16> = (0u32..7 * 5 * 4)
            .map(|j| ((j * 73 + j * j * 117) & 4095) as u16)
            .collect();
        let input = EncodeInput {
            width: 7,
            height: 5,
            precision: 12,
            color_space: ColorSpace::Cmyk,
            samples: SampleData::U16(&values),
        };
        let bytes = encode_hierarchical_samples(
            &input,
            PyramidRasterOptions {
                cmyk_polarity: polarity,
                ..PyramidRasterOptions::default()
            },
            PyramidOptions::new(&dct_levels),
        )
        .unwrap();
        let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(model.reconstruct().unwrap().cmyk_polarity, polarity);
        for (i, c) in model.reconstruct_reference().unwrap().iter().enumerate() {
            assert_eq!(
                c.samples,
                values
                    .iter()
                    .skip(i)
                    .step_by(4)
                    .copied()
                    .collect::<Vec<_>>()
            );
        }
    }
    let values = [65535, 0, 0, 0, 65535, 0, 0, 0, 65535];
    let input = EncodeInput {
        width: 3,
        height: 1,
        precision: 16,
        color_space: ColorSpace::Rgb,
        samples: SampleData::U16(&values),
    };
    let raster = PyramidRasterOptions {
        rgb_to_ycbcr: true,
        subsampling: Subsampling::S444,
        ..PyramidRasterOptions::default()
    };
    let model = build_hierarchical_samples(&input, raster, PyramidOptions::new(&levels)).unwrap();
    let planes = model.reconstruct_reference().unwrap();
    for x in 0..3 {
        let r = i64::from(values[3 * x]);
        let g = i64::from(values[3 * x + 1]);
        let b = i64::from(values[3 * x + 2]);
        let expected = [
            (19595 * r + 38470 * g + 7471 * b + 32768) >> 16,
            ((-11059 * r - 21709 * g + 32768 * b + 32768) >> 16) + 32768,
            ((32768 * r - 27439 * g - 5329 * b + 32768) >> 16) + 32768,
        ];
        for i in 0..3 {
            assert_eq!(i64::from(planes[i].samples[x]), expected[i].clamp(0, 65535));
        }
    }
}
#[test]
fn invalid_plans_sources_and_resource_limits_are_bounded() {
    let (header, source) = planes(8, 17, 13, &[(1, 1)]);
    let levels = [
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
        PyramidLevel::lossless(BOTH, LosslessOptions::default()),
    ];
    for limits in [
        Limits {
            max_memory_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_pixels: 1,
            ..Limits::default()
        },
        Limits {
            max_scans: 1,
            ..Limits::default()
        },
    ] {
        assert!(
            build_hierarchical_planes(
                header.clone(),
                &source,
                vec![],
                PyramidOptions {
                    limits,
                    ..PyramidOptions::new(&levels)
                }
            )
            .is_err()
        );
    }
    assert!(
        build_hierarchical_planes(header.clone(), &source, vec![], PyramidOptions::new(&[]))
            .is_err()
    );
    let invalid = [PyramidLevel::lossless(BOTH, LosslessOptions::default())];
    assert!(
        build_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions::new(&invalid)
        )
        .is_err()
    );
    let invalid = [
        PyramidLevel::lossless(NONE, LosslessOptions::default()),
        PyramidLevel::dct(BOTH, 75, CodingOptions::default()),
    ];
    assert!(
        build_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions::new(&invalid)
        )
        .is_err()
    );
    let mut wrong = source.clone();
    wrong[0].samples.pop();
    assert!(
        build_hierarchical_planes(header.clone(), &wrong, vec![], PyramidOptions::new(&levels))
            .is_err()
    );
    let bytes = encode_hierarchical_planes(
        header.clone(),
        &source,
        vec![],
        PyramidOptions::new(&levels),
    )
    .unwrap();
    assert!(
        encode_hierarchical_planes(
            header.clone(),
            &source,
            vec![],
            PyramidOptions {
                max_output_bytes: bytes.len() - 1,
                ..PyramidOptions::new(&levels)
            }
        )
        .is_err()
    );
    assert_eq!(
        encode_hierarchical_planes(
            header,
            &source,
            vec![],
            PyramidOptions {
                max_output_bytes: bytes.len(),
                ..PyramidOptions::new(&levels)
            }
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn shared_images_and_plane_metadata_use_the_same_pyramid_writer() {
    use deadlib_image::{ColorType, Image, Samples};
    use deadlib_image_jpeg::{
        IccWrite, MetadataOptions, MetadataSegment, encode_hierarchical_image,
        encode_hierarchical_image_with_metadata, encode_hierarchical_planes_with_metadata,
    };
    let levels = [
        PyramidLevel::lossless(NONE, lp(EntropyCoding::Arithmetic, 1)),
        PyramidLevel::lossless(BOTH, lp(EntropyCoding::Arithmetic, 0)),
    ];
    let extra = [MetadataSegment {
        marker: 0xfe,
        data: b"source pyramid",
        offset: 0,
        preceding_scans: 0,
    }];
    let profile = vec![71; 70000];
    let metadata = MetadataOptions {
        additional_segments: &extra,
        icc: IccWrite::Replace(&profile),
    };
    for p in [8, 16] {
        let values: Vec<u16> = (0..35 * 3)
            .map(|i| (i * 601 + i * i * 17) as u16 & if p == 8 { 255 } else { 65535 })
            .collect();
        let image = Image {
            width: 7,
            height: 5,
            color_type: ColorType::Rgb,
            bit_depth: p,
            samples: if p == 8 {
                Samples::U8(values.iter().map(|&v| v as u8).collect())
            } else {
                Samples::U16(values.clone())
            },
        };
        let bytes = encode_hierarchical_image_with_metadata(
            &image,
            PyramidRasterOptions::default(),
            PyramidOptions::new(&levels),
            metadata,
        )
        .unwrap();
        let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(
            decoded
                .icc_profile()
                .unwrap()
                .unwrap()
                .to_vec(profile.len())
                .unwrap(),
            profile
        );
        assert!(
            decoded
                .metadata
                .iter()
                .any(|m| m.marker == 0xfe && m.data == extra[0].data)
        );
        for (i, c) in decoded.reconstruct_reference().unwrap().iter().enumerate() {
            assert_eq!(
                c.samples,
                values
                    .iter()
                    .skip(i)
                    .step_by(3)
                    .copied()
                    .collect::<Vec<_>>()
            );
        }
        let raw = encode_hierarchical_image(
            &image,
            PyramidRasterOptions::default(),
            PyramidOptions::new(&levels),
        )
        .unwrap();
        assert!(
            decode_hierarchical(&raw, Limits::default())
                .unwrap()
                .icc_profile()
                .unwrap()
                .is_none()
        );
    }
    let (header, source) = planes(12, 7, 5, &[(1, 1)]);
    let bytes = encode_hierarchical_planes_with_metadata(
        header,
        &source,
        extra.to_vec(),
        PyramidOptions::new(&levels),
        MetadataOptions {
            additional_segments: &[],
            ..metadata
        },
    )
    .unwrap();
    let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(
        decoded.reconstruct_reference().unwrap()[0].samples,
        source[0].samples
    );
    assert_eq!(
        decoded
            .icc_profile()
            .unwrap()
            .unwrap()
            .to_vec(profile.len())
            .unwrap(),
        profile
    );
    assert_eq!(
        decoded.metadata.iter().filter(|m| m.marker == 0xfe).count(),
        1
    );
}

#[test]
fn pinned_source_pyramids_match_original_samples_and_reject_every_prefix() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pyramid-native");
    let mut cases: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|s| s == "jpg"))
        .collect();
    cases.sort();
    assert_eq!(cases.len(), 12);
    let mut prefixes = 0;
    for path in cases {
        let bytes = std::fs::read(&path).unwrap();
        let raw = std::fs::read(path.with_extension("source.raw")).unwrap();
        let source: Vec<_> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes(*v))
            .collect();
        let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
        let count = model.header.components.len();
        assert_eq!((model.header.width, model.header.height), (17, 24));
        for (i, c) in model.reconstruct_reference().unwrap().iter().enumerate() {
            assert_eq!((c.width, c.height), (17, 24));
            assert_eq!(
                c.samples,
                source
                    .iter()
                    .skip(i)
                    .step_by(count)
                    .copied()
                    .collect::<Vec<_>>(),
                "{} component {i}",
                path.display()
            );
        }
        for end in 0..bytes.len() {
            assert!(
                decode_hierarchical(&bytes[..end], Limits::default()).is_err(),
                "{} prefix {end}",
                path.display()
            );
            prefixes += 1;
        }
        let limits = Limits {
            max_pixels: 65536,
            max_memory_bytes: 2 * 1024 * 1024,
            max_metadata_bytes: 65536,
            max_scans: 256,
        };
        let mut state = 0x78563412u32;
        for _ in 0..100 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut changed = bytes.clone();
            let at = state as usize % changed.len();
            changed[at] ^= (state >> 24) as u8 | 1;
            assert!(std::panic::catch_unwind(|| decode_hierarchical(&changed, limits)).is_ok());
        }
    }
    eprintln!("Checked {prefixes} truncated prefixes and 1200 bounded mutations");
}
