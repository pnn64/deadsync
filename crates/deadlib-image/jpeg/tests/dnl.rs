use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, Error, Event, HeightDeclaration, HierarchicalData,
    HierarchicalFrameOptions, HierarchicalOptions, Limits, LosslessOptions, MarkerReader,
    decode_coefficients, decode_hierarchical, decode_lossless, encode_coefficients,
};
use std::path::Path;

fn ranges(input: &[u8]) -> Vec<(u8, usize, usize)> {
    let mut reader = MarkerReader::new(input).unwrap();
    let mut output = vec![];
    while let Some(event) = reader.next() {
        let (marker, start) = match event.unwrap() {
            Event::Segment { marker, offset, .. } => (marker, offset),
            Event::Scan { offset, .. } => (0xda, offset),
            Event::End { offset } => (0xd9, offset),
        };
        output.push((marker, start, reader.consumed()));
    }
    output
}
fn restored(input: &[u8]) -> Vec<u8> {
    let mut bytes = input.to_vec();
    let mut frame = None;
    let mut scans = 0;
    let mut deletes = vec![];
    for (marker, start, end) in ranges(input) {
        if deadlib_image_jpeg::Process::try_from(marker).is_ok() {
            frame = Some(start);
            scans = 0;
        } else if marker == 0xda {
            scans += 1;
        } else if marker == 0xdc {
            assert_eq!(scans, 1);
            let frame = frame.unwrap();
            bytes[frame + 5..frame + 7].copy_from_slice(&input[start + 4..start + 6]);
            deletes.push((start, end));
        }
    }
    assert!(!deletes.is_empty());
    for (start, end) in deletes.into_iter().rev() {
        bytes.drain(start..end);
    }
    bytes
}
fn dct_options(image: &deadlib_image_jpeg::CoefficientImage<'_>) -> CodingOptions<'static> {
    CodingOptions {
        entropy_coding: if image.frame.process.is_arithmetic() {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        },
        progressive: image.frame.process.is_progressive(),
        successive_approximation: 3,
        restart_interval: 3,
        ..CodingOptions::default()
    }
}
fn same_coefficients(
    a: &deadlib_image_jpeg::CoefficientComponent,
    b: &deadlib_image_jpeg::CoefficientComponent,
) {
    assert_eq!((a.width, a.height), (b.width, b.height));
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
fn fixtures(directory: &str) -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|v| v.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jpg"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    paths
}

#[test]
fn dct_dnl_zero_redundant_and_redefined_headers_preserve_every_coefficient() {
    for directory in [
        "sequential",
        "progressive",
        "arithmetic",
        "arithmetic-progressive",
    ] {
        for path in fixtures(directory) {
            let source = std::fs::read(&path).unwrap();
            let model = decode_coefficients(&source, Limits::default()).unwrap();
            let options = dct_options(&model);
            let canonical = encode_coefficients(&model, options).unwrap();
            for initial_height in [0, model.frame.height, 65535] {
                let bytes = encode_coefficients(
                    &model,
                    CodingOptions {
                        height_declaration: HeightDeclaration::Dnl { initial_height },
                        ..options
                    },
                )
                .unwrap();
                assert_eq!(restored(&bytes), canonical);
                assert_eq!(bytes.len(), canonical.len() + 6);
                let decoded = decode_coefficients(&bytes, Limits::default()).unwrap();
                assert_eq!(
                    decoded.frame,
                    decode_coefficients(&canonical, Limits::default())
                        .unwrap()
                        .frame
                );
                for (a, b) in decoded.components.iter().zip(&model.components) {
                    same_coefficients(a, b);
                }
            }
        }
    }
}

#[test]
fn lossless_dnl_preserves_native_integers_for_both_entropy_families() {
    for directory in ["lossless", "lossless-arithmetic"] {
        for path in fixtures(directory) {
            let source = std::fs::read(&path).unwrap();
            let model = decode_lossless(&source, Limits::default()).unwrap();
            let options = LosslessOptions {
                entropy_coding: if model.frame.process.is_arithmetic() {
                    EntropyCoding::Arithmetic
                } else {
                    EntropyCoding::Huffman
                },
                restart_rows: 1,
                separate_components: true,
                ..LosslessOptions::default()
            };
            let canonical = model.encode(options).unwrap();
            for initial_height in [0, 1, 65535] {
                let bytes = model
                    .encode(LosslessOptions {
                        height_declaration: HeightDeclaration::Dnl { initial_height },
                        ..options
                    })
                    .unwrap();
                assert_eq!(restored(&bytes), canonical);
                let decoded = decode_lossless(&bytes, Limits::default()).unwrap();
                assert_eq!(decoded.frame, model.frame);
                for (a, b) in decoded.components.iter().zip(&model.components) {
                    assert_eq!(a.samples, b.samples);
                }
            }
        }
    }
}

#[test]
fn hierarchical_dnl_and_unknown_dhp_preserve_all_frame_values_and_final_planes() {
    for directory in ["hierarchical", "hierarchical-native", "pyramid-native"] {
        for path in fixtures(directory) {
            let source = std::fs::read(&path).unwrap();
            let model = decode_hierarchical(&source, Limits::default()).unwrap();
            let mut controls = vec![];
            for frame in &model.frames {
                let height_declaration = HeightDeclaration::Dnl { initial_height: 0 };
                let entropy_coding = if frame.frame.process.is_arithmetic() {
                    EntropyCoding::Arithmetic
                } else {
                    EntropyCoding::Huffman
                };
                controls.push(if frame.frame.process.is_lossless() {
                    HierarchicalFrameOptions::Lossless(LosslessOptions {
                        entropy_coding,
                        height_declaration,
                        restart_rows: 1,
                        ..LosslessOptions::default()
                    })
                } else {
                    HierarchicalFrameOptions::Dct(CodingOptions {
                        entropy_coding,
                        height_declaration,
                        progressive: frame.frame.process.is_progressive(),
                        successive_approximation: 3,
                        restart_interval: 3,
                        ..CodingOptions::default()
                    })
                });
            }
            for omit_dhp_height in [false, true] {
                let bytes = model
                    .encode(HierarchicalOptions {
                        frame_options: &controls,
                        omit_dhp_height,
                        ..HierarchicalOptions::default()
                    })
                    .unwrap();
                let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
                assert_eq!(decoded.header, model.header);
                assert_eq!(decoded.frames.len(), model.frames.len());
                for (a, b) in decoded.frames.iter().zip(&model.frames) {
                    match (&a.data, &b.data) {
                        (HierarchicalData::Dct(a), HierarchicalData::Dct(b)) => {
                            for (a, b) in a.iter().zip(b) {
                                same_coefficients(a, b);
                            }
                        }
                        (
                            HierarchicalData::Lossless { components: a, .. },
                            HierarchicalData::Lossless { components: b, .. },
                        ) => {
                            for (a, b) in a.iter().zip(b) {
                                assert_eq!(a.samples, b.samples);
                            }
                        }
                        _ => panic!("changed frame family"),
                    }
                }
                for (a, b) in decoded
                    .reconstruct_reference()
                    .unwrap()
                    .iter()
                    .zip(model.reconstruct_reference().unwrap())
                {
                    assert_eq!(a.samples, b.samples);
                }
            }
        }
    }
}

#[test]
fn dnl_adjacency_length_range_padding_and_resource_limits() {
    let source = std::fs::read(fixtures("sequential").remove(0)).unwrap();
    let model = decode_coefficients(&source, Limits::default()).unwrap();
    let options = CodingOptions {
        height_declaration: HeightDeclaration::Dnl {
            initial_height: 65535,
        },
        ..dct_options(&model)
    };
    let bytes = encode_coefficients(&model, options).unwrap();
    let (_, start, end) = ranges(&bytes).into_iter().find(|r| r.0 == 0xdc).unwrap();
    // NL replaces an excessively large provisional SOF height before allocation.
    assert!(
        decode_coefficients(
            &bytes,
            Limits {
                max_pixels: model.frame.width as usize * model.frame.height as usize,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    assert!(matches!(
        decode_coefficients(
            &bytes,
            Limits {
                max_pixels: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    assert!(matches!(
        decode_coefficients(
            &bytes,
            Limits {
                max_memory_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    let exact = encode_coefficients(
        &model,
        CodingOptions {
            max_output_bytes: bytes.len(),
            ..options
        },
    )
    .unwrap();
    assert_eq!(exact, bytes);
    assert!(
        encode_coefficients(
            &model,
            CodingOptions {
                max_output_bytes: bytes.len() - 1,
                ..options
            }
        )
        .is_err()
    );
    for length in [0, 1, 2, 3] {
        let mut invalid = bytes.clone();
        invalid[start + 2..start + 4].copy_from_slice(&(length as u16).to_be_bytes());
        assert!(decode_coefficients(&invalid, Limits::default()).is_err());
    }
    let mut zero = bytes.clone();
    zero[start + 4..end].fill(0);
    assert!(decode_coefficients(&zero, Limits::default()).is_err());
    let mut extension = bytes.clone();
    extension.splice(end..end, [255, 0xdc, 255, 0xd9]);
    extension[start + 2..start + 4].copy_from_slice(&8u16.to_be_bytes());
    assert_eq!(
        decode_coefficients(&extension, Limits::default())
            .unwrap()
            .frame
            .height,
        model.frame.height
    );
    let mut fill = bytes.clone();
    fill.splice(start..start, [255, 255]);
    assert!(decode_coefficients(&fill, Limits::default()).is_ok());
    for insert in [start, end] {
        let mut duplicate = bytes.clone();
        duplicate.splice(insert..insert, bytes[start..end].iter().copied());
        assert!(decode_coefficients(&duplicate, Limits::default()).is_err());
    }
    let mut delayed = bytes.clone();
    delayed.splice(start..start, [255, 0xfe, 0, 2]);
    assert!(decode_coefficients(&delayed, Limits::default()).is_err());
    let mut missing = bytes.clone();
    let sof = ranges(&bytes)
        .into_iter()
        .find(|r| deadlib_image_jpeg::Process::try_from(r.0).is_ok())
        .unwrap()
        .1;
    missing[sof + 5..sof + 7].fill(0);
    missing.drain(start..end);
    assert!(decode_coefficients(&missing, Limits::default()).is_err());
    for prefix in 0..bytes.len() {
        assert!(decode_coefficients(&bytes[..prefix], Limits::default()).is_err());
    }
}

#[test]
fn unknown_dhp_uses_full_resolution_components_instead_of_last_frame_height() {
    use deadlib_image_jpeg::{
        ComponentPlane, ComponentSpec, Expansion, FrameHeader, HierarchicalFrame,
        HierarchicalImage, HierarchyHeader, LosslessCoding, Process,
    };
    let specs = [
        ComponentSpec {
            id: 1,
            horizontal_sampling: 1,
            vertical_sampling: 2,
            quantization_table: 0,
        },
        ComponentSpec {
            id: 2,
            horizontal_sampling: 1,
            vertical_sampling: 1,
            quantization_table: 0,
        },
    ];
    let frames = specs
        .iter()
        .enumerate()
        .map(|(i, &spec)| {
            let height = if i == 0 { 5 } else { 3 };
            HierarchicalFrame {
                frame: FrameHeader {
                    process: Process::Lossless,
                    precision: 8,
                    width: 5,
                    height,
                    components: vec![spec],
                },
                data: HierarchicalData::Lossless {
                    components: vec![ComponentPlane {
                        spec,
                        width: 5,
                        height: height as usize,
                        samples: (0..5 * height).map(|v| v * 7).collect(),
                    }],
                    coding: vec![LosslessCoding {
                        predictor: 1,
                        point_transform: 0,
                    }],
                },
                expansion: Expansion::default(),
                offset: 0,
                first_scan: 0,
                scans: 1,
            }
        })
        .collect();
    let model = HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: 8,
            width: 5,
            height: 5,
            components: specs.to_vec(),
        },
        frames,
        vec![],
        Limits::default(),
    )
    .unwrap();
    let controls = [HierarchicalFrameOptions::Lossless(LosslessOptions {
        height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
        ..LosslessOptions::default()
    }); 2];
    let bytes = model
        .encode(HierarchicalOptions {
            frame_options: &controls,
            omit_dhp_height: true,
            ..HierarchicalOptions::default()
        })
        .unwrap();
    let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(decoded.header.height, 5);
    assert_eq!(decoded.frames.last().unwrap().frame.height, 3);
    let dhp = ranges(&bytes).into_iter().find(|r| r.0 == 0xde).unwrap().1;
    let mut inconsistent = bytes.clone();
    inconsistent[dhp + 14] = 0x12; // Both DHP components now have maximal V.
    assert!(decode_hierarchical(&inconsistent, Limits::default()).is_err());
    let sofs: Vec<_> = ranges(&bytes).into_iter().filter(|r| r.0 == 0xc3).collect();
    let mut missing = bytes.clone();
    missing.drain(sofs[0].1..sofs[1].1);
    assert!(decode_hierarchical(&missing, Limits::default()).is_err());

    // DHP may omit its height even when every SOF declares its height directly.
    let known_frames = model
        .encode(HierarchicalOptions {
            omit_dhp_height: true,
            ..HierarchicalOptions::default()
        })
        .unwrap();
    assert_eq!(
        decode_hierarchical(&known_frames, Limits::default())
            .unwrap()
            .header
            .height,
        5
    );

    // Exercise the source-pyramid wrapper, generated 255-component scans and
    // DNL after the first component group rather than after the whole frame.
    use deadlib_image_jpeg::{PyramidLevel, PyramidOptions, encode_hierarchical_planes};
    let components: Vec<_> = (0..255u16)
        .map(|i| ComponentPlane {
            spec: ComponentSpec {
                id: i as u8,
                horizontal_sampling: 1,
                vertical_sampling: 1,
                quantization_table: 0,
            },
            width: 2,
            height: 3,
            samples: (0..6).map(|j| i * 251 + j * 73).collect(),
        })
        .collect();
    let header = HierarchyHeader {
        precision: 16,
        width: 2,
        height: 3,
        components: components.iter().map(|c| c.spec).collect(),
    };
    let coding = LosslessOptions {
        height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
        ..LosslessOptions::default()
    };
    let levels = [
        PyramidLevel::lossless(Expansion::default(), coding),
        PyramidLevel::lossless(
            Expansion {
                horizontal: true,
                vertical: true,
            },
            coding,
        ),
    ];
    let bytes = encode_hierarchical_planes(
        header,
        &components,
        vec![],
        PyramidOptions {
            omit_dhp_height: true,
            limits: Limits {
                max_scans: 128,
                ..Limits::default()
            },
            ..PyramidOptions::new(&levels)
        },
    )
    .unwrap();
    let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!((decoded.header.height, decoded.scans), (3, 128));
    assert_eq!(ranges(&bytes).iter().filter(|r| r.0 == 0xdc).count(), 2);
    for (a, b) in decoded
        .reconstruct_reference()
        .unwrap()
        .iter()
        .zip(components)
    {
        assert_eq!(a.samples, b.samples);
    }
}

#[test]
fn huffman_dnl_requires_complete_mcu_rows_and_accepts_maximum_segment_length() {
    use deadlib_image_jpeg::{ColorSpace, EncodeInput, EncodeOptions, SampleData, encode_samples};
    let values: Vec<u16> = (0u32..8 * 17)
        .map(|v| ((v * 73 + v * v * 117) & 255) as u16)
        .collect();
    let bytes = encode_samples(
        &EncodeInput {
            width: 8,
            height: 17,
            precision: 8,
            color_space: ColorSpace::Grayscale,
            samples: SampleData::U16(&values),
        },
        EncodeOptions {
            coding: CodingOptions {
                height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                ..CodingOptions::default()
            },
            ..EncodeOptions::default()
        },
    )
    .unwrap();
    let (_, start, end) = ranges(&bytes).into_iter().find(|r| r.0 == 0xdc).unwrap();
    for height in [1u16, 8, 16, 25, 65535] {
        let mut invalid = bytes.clone();
        invalid[start + 4..end].copy_from_slice(&height.to_be_bytes());
        assert!(decode_coefficients(&invalid, Limits::default()).is_err());
    }
    for height in 17u16..=24 {
        let mut same_rows = bytes.clone();
        same_rows[start + 4..end].copy_from_slice(&height.to_be_bytes());
        assert_eq!(
            decode_coefficients(&same_rows, Limits::default())
                .unwrap()
                .frame
                .height,
            height
        );
    }
    let mut extension = bytes.clone();
    extension.splice(end..end, std::iter::repeat_n(0, 65531));
    extension[start + 2..start + 4].copy_from_slice(&65535u16.to_be_bytes());
    assert!(
        decode_coefficients(
            &extension,
            Limits {
                max_memory_bytes: 4096,
                max_metadata_bytes: 1024,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let scan = ranges(&bytes).into_iter().find(|r| r.0 == 0xda).unwrap().1;
    let mut empty = bytes[..scan].to_vec();
    empty.extend_from_slice(&[255, 0xd9]);
    assert!(decode_coefficients(&empty, Limits::default()).is_err());
}

fn word(data: &[u8], position: &mut usize) -> usize {
    let value = u32::from_le_bytes(data[*position..*position + 4].try_into().unwrap());
    *position += 4;
    value as usize
}
fn sample_expectation(
    data: &[u8],
    p: u8,
    w: u16,
    h: u16,
    components: &[deadlib_image_jpeg::ComponentPlane],
) {
    assert_eq!(&data[..4], b"JLP1");
    let mut pos = 4;
    assert_eq!(
        [
            word(data, &mut pos),
            word(data, &mut pos),
            word(data, &mut pos),
            word(data, &mut pos)
        ],
        [p as usize, w as usize, h as usize, components.len()]
    );
    for c in components {
        let fields = std::array::from_fn::<_, 5, _>(|_| word(data, &mut pos));
        assert_eq!(
            fields,
            [
                c.spec.id as usize,
                c.spec.horizontal_sampling as usize,
                c.spec.vertical_sampling as usize,
                c.width,
                c.height
            ]
        );
        for &value in &c.samples {
            assert_eq!(
                u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()),
                value
            );
            pos += 2;
        }
    }
    assert_eq!(pos, data.len());
}
fn coefficient_expectation(
    data: &[u8],
    frame: &deadlib_image_jpeg::FrameHeader,
    components: &[deadlib_image_jpeg::CoefficientComponent],
) {
    assert_eq!(&data[..4], b"JCO1");
    let mut pos = 4;
    assert_eq!(
        [
            word(data, &mut pos),
            word(data, &mut pos),
            word(data, &mut pos),
            word(data, &mut pos)
        ],
        [
            frame.precision as usize,
            frame.width as usize,
            frame.height as usize,
            components.len()
        ]
    );
    for c in components {
        let fields = std::array::from_fn::<_, 7, _>(|_| word(data, &mut pos));
        let (bw, bh) = (c.width.div_ceil(8), c.height.div_ceil(8));
        assert_eq!(
            fields,
            [
                c.spec.id as usize,
                c.spec.horizontal_sampling as usize,
                c.spec.vertical_sampling as usize,
                c.width,
                c.height,
                bw,
                bh
            ]
        );
        for q in c.quantization {
            assert_eq!(
                u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()),
                q
            );
            pos += 2;
        }
        for y in 0..bh {
            for x in 0..bw {
                for value in c.blocks[y * c.blocks_wide + x] {
                    assert_eq!(
                        i16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()),
                        value
                    );
                    pos += 2;
                }
            }
        }
    }
    assert_eq!(pos, data.len());
}

#[test]
fn pinned_dnl_sources_cover_all_processes_and_malformed_boundaries() {
    let paths = fixtures("dnl");
    assert!(paths.len() >= 12);
    let mut processes = std::collections::BTreeSet::new();
    let mut prefixes = 0;
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let mut hierarchical = false;
        let mut lossless = false;
        for event in MarkerReader::new(&bytes).unwrap() {
            if let Event::Segment { marker, data, .. } = event.unwrap() {
                hierarchical |= marker == 0xde;
                if let Ok(process) = deadlib_image_jpeg::Process::try_from(marker) {
                    processes.insert(marker);
                    lossless = process.is_lossless();
                    assert_eq!(
                        deadlib_image_jpeg::FrameHeader::parse(marker, data)
                            .unwrap()
                            .height,
                        0
                    );
                }
            }
        }
        let expected = std::fs::read(path.with_extension("samples")).unwrap();
        if hierarchical {
            let model = decode_hierarchical(&bytes, Limits::default()).unwrap();
            for (i, frame) in model.frames.iter().enumerate() {
                match &frame.data {
                    HierarchicalData::Dct(components) => coefficient_expectation(
                        &std::fs::read(path.with_extension(format!("frame-{i}.coef"))).unwrap(),
                        &frame.frame,
                        components,
                    ),
                    HierarchicalData::Lossless { components, .. } => sample_expectation(
                        &std::fs::read(path.with_extension(format!("frame-{i}.samples"))).unwrap(),
                        frame.frame.precision,
                        frame.frame.width,
                        frame.frame.height,
                        components,
                    ),
                }
            }
            sample_expectation(
                &expected,
                model.header.precision,
                model.header.width,
                model.header.height,
                &model.reconstruct_reference().unwrap(),
            );
        } else if lossless {
            let model = decode_lossless(&bytes, Limits::default()).unwrap();
            sample_expectation(
                &expected,
                model.frame.precision,
                model.frame.width,
                model.frame.height,
                &model.components,
            );
            assert_eq!(
                expected,
                std::fs::read(path.with_extension("frame-0.samples")).unwrap()
            );
        } else {
            let model = decode_coefficients(&bytes, Limits::default()).unwrap();
            coefficient_expectation(
                &std::fs::read(path.with_extension("frame-0.coef")).unwrap(),
                &model.frame,
                &model.components,
            );
            sample_expectation(
                &expected,
                model.frame.precision,
                model.frame.width,
                model.frame.height,
                &model.reconstruct().unwrap().components,
            );
        }
        let parse = |input: &[u8], limits| -> deadlib_image_jpeg::Result<()> {
            if hierarchical {
                decode_hierarchical(input, limits).map(|_| ())
            } else if lossless {
                decode_lossless(input, limits).map(|_| ())
            } else {
                decode_coefficients(input, limits).map(|_| ())
            }
        };
        for end in 0..bytes.len() {
            assert!(
                parse(&bytes[..end], Limits::default()).is_err(),
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
        let mut state = 0x98765432u32;
        for _ in 0..100 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut changed = bytes.clone();
            let index = state as usize % changed.len();
            changed[index] ^= (state >> 24) as u8 | 1;
            assert!(std::panic::catch_unwind(|| parse(&changed, limits)).is_ok());
        }
    }
    assert_eq!(
        processes.into_iter().collect::<Vec<_>>(),
        [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf
        ]
    );
    eprintln!(
        "Checked {prefixes} prefixes and {} bounded mutations",
        paths.len() * 100
    );
}
