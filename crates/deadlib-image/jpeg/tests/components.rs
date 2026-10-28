use deadlib_image_jpeg::{
    AbbreviatedEncoder, CodingOptions, EntropyCoding, Event, FrameHeader, HeightDeclaration,
    Limits, MarkerReader, MetadataOptions, Process, ScanSpec, Tables, decode_coefficients,
    encode_coefficients,
};
#[path = "support/components.rs"]
mod support;

fn controls(count: usize) -> CodingOptions<'static> {
    CodingOptions {
        entropy_coding: if count.is_multiple_of(2) {
            EntropyCoding::Huffman
        } else {
            EntropyCoding::Arithmetic
        },
        optimize_huffman: !count.is_multiple_of(3),
        restart_interval: [0, 1, 7][count % 3],
        ..CodingOptions::default()
    }
}
fn scan_rules(bytes: &[u8]) -> usize {
    let mut frame = None;
    let mut scans = 0;
    for event in MarkerReader::new(bytes).unwrap() {
        match event.unwrap() {
            Event::Segment { marker, data, .. } if Process::try_from(marker).is_ok() => {
                frame = Some(FrameHeader::parse(marker, data).unwrap())
            }
            Event::Scan { header, .. } => {
                let n = header[0] as usize;
                assert!((1..=4).contains(&n));
                let frame = frame.as_ref().unwrap();
                let sum: usize = header[1..1 + 2 * n]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| {
                        let c = frame.components.iter().find(|c| c.id == s[0]).unwrap();
                        c.horizontal_sampling as usize * c.vertical_sampling as usize
                    })
                    .sum();
                assert!(n == 1 || sum <= 10);
                scans += 1;
            }
            _ => {}
        }
    }
    scans
}

#[test]
fn every_sequential_frame_count_preserves_coefficients_and_quantizers() {
    for count in 1..=255 {
        let image = support::model(
            count,
            if count.is_multiple_of(2) { 8 } else { 12 },
            count % 4,
            count % 4,
        );
        let encoded = encode_coefficients(&image, controls(count)).unwrap();
        let decoded = decode_coefficients(&encoded, Limits::default()).unwrap();
        support::same(&image, &decoded);
        assert_eq!(scan_rules(&encoded), decoded.scans);
        assert!(decoded.scans <= count);
    }
}

#[test]
fn large_frames_custom_scans_dnl_and_abbreviated_redefinitions() {
    let mut sender = AbbreviatedEncoder::new();
    let mut receiver = Tables::new();
    for count in [4, 5, 10, 17, 254, 255] {
        for precision in [8, 12] {
            for layout in 0..4 {
                let image = support::model(
                    count,
                    precision,
                    layout,
                    if layout.is_multiple_of(2) { 2 } else { 3 },
                );
                let script: Vec<_> = image
                    .frame
                    .components
                    .iter()
                    .rev()
                    .map(|c| ScanSpec::new(&[c.id], 0, 63, 0, 0).unwrap())
                    .collect();
                for arithmetic in [false, true] {
                    let options = CodingOptions {
                        entropy_coding: if arithmetic {
                            EntropyCoding::Arithmetic
                        } else {
                            EntropyCoding::Huffman
                        },
                        scan_script: &script,
                        height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                        restart_interval: 3,
                        ..CodingOptions::default()
                    };
                    for _ in 0..2 {
                        let encoded = sender
                            .encode_coefficients(&image, options, MetadataOptions::default())
                            .unwrap();
                        receiver.load(&encoded.tables, Limits::default()).unwrap();
                        let decoded = receiver
                            .decode_coefficients(&encoded.image, Limits::default())
                            .unwrap();
                        support::same(&image, &decoded);
                        assert_eq!(decoded.scans, count);
                    }
                }
            }
        }
    }
}

#[test]
fn progressive_generated_dc_groups_obey_mcu_limits() {
    for count in 1..=4 {
        for layout in 0..4 {
            for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
                let image = support::model(count, 12, layout, 2);
                let encoded = encode_coefficients(
                    &image,
                    CodingOptions {
                        progressive: true,
                        successive_approximation: 3,
                        entropy_coding,
                        restart_interval: 1,
                        ..CodingOptions::default()
                    },
                )
                .unwrap();
                let decoded = decode_coefficients(&encoded, Limits::default()).unwrap();
                support::same(&image, &decoded);
                assert_eq!(scan_rules(&encoded), decoded.scans);
            }
        }
    }
    let image = support::model(5, 8, 0, 0);
    assert!(
        encode_coefficients(
            &image,
            CodingOptions {
                progressive: true,
                ..CodingOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn invalid_large_models_and_scan_limits_are_bounded() {
    let image = support::model(255, 8, 0, 2);
    assert!(
        deadlib_image_jpeg::CoefficientImage::from_components(
            image.frame.clone(),
            image.components.clone(),
            vec![],
            Limits {
                max_memory_bytes: 1000,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let mut invalid = image.clone();
    invalid.frame.components[254].id = invalid.frame.components[0].id;
    invalid.components[254].spec = invalid.frame.components[254];
    assert!(encode_coefficients(&invalid, CodingOptions::default()).is_err());
    let mut invalid = image.clone();
    invalid.frame.process = Process::Progressive;
    assert!(encode_coefficients(&invalid, CodingOptions::default()).is_err());
    let bounded = deadlib_image_jpeg::CoefficientImage::from_components(
        image.frame.clone(),
        image.components.clone(),
        vec![],
        Limits {
            max_scans: 63,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(encode_coefficients(&bounded, CodingOptions::default()).is_err());
    let mut duplicate: Vec<_> = image
        .frame
        .components
        .iter()
        .map(|c| ScanSpec::new(&[c.id], 0, 63, 0, 0).unwrap())
        .collect();
    duplicate[254] = duplicate[0];
    assert!(
        encode_coefficients(
            &image,
            CodingOptions {
                scan_script: &duplicate,
                ..CodingOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn many_component_dct_hierarchies_and_source_pyramids_keep_native_planes() {
    use deadlib_image_jpeg::{
        ComponentPlane, Expansion, HierarchicalData, HierarchicalFrame, HierarchicalFrameOptions,
        HierarchicalImage, HierarchicalOptions, HierarchyHeader, LosslessOptions, PyramidLevel,
        PyramidOptions, build_hierarchical_planes, decode_hierarchical,
    };
    let limits = Limits {
        max_scans: 2048,
        ..Limits::default()
    };
    for count in [5, 17, 255] {
        for layout in [0, 1, 3] {
            for precision in [8, 12] {
                let model = support::model(count, precision, layout, 3);
                let header = HierarchyHeader {
                    precision,
                    width: model.frame.width,
                    height: model.frame.height,
                    components: model.frame.components.clone(),
                };
                let mut residual = model.components.clone();
                for component in &mut residual {
                    for block in &mut component.blocks {
                        for v in block {
                            *v = -*v / 2;
                        }
                    }
                }
                let mut frame = model.frame.clone();
                frame.process = Process::DifferentialSequential;
                let image = HierarchicalImage::from_frames(
                    header.clone(),
                    vec![
                        HierarchicalFrame {
                            frame: model.frame.clone(),
                            data: HierarchicalData::Dct(model.components.clone()),
                            expansion: Expansion::default(),
                            offset: 0,
                            first_scan: 0,
                            scans: 1,
                        },
                        HierarchicalFrame {
                            frame,
                            data: HierarchicalData::Dct(residual),
                            expansion: Expansion::default(),
                            offset: 0,
                            first_scan: 0,
                            scans: 1,
                        },
                    ],
                    vec![],
                    limits,
                )
                .unwrap();
                for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
                    let coding = CodingOptions {
                        entropy_coding,
                        restart_interval: 3,
                        ..CodingOptions::default()
                    };
                    let options = [HierarchicalFrameOptions::Dct(coding); 2];
                    let encoded = image
                        .encode(HierarchicalOptions {
                            limits,
                            frame_options: &options,
                            ..HierarchicalOptions::default()
                        })
                        .unwrap();
                    let decoded = decode_hierarchical(&encoded, limits).unwrap();
                    assert_eq!(
                        image
                            .reconstruct_reference()
                            .unwrap()
                            .iter()
                            .map(|p| &p.samples)
                            .collect::<Vec<_>>(),
                        decoded
                            .reconstruct_reference()
                            .unwrap()
                            .iter()
                            .map(|p| &p.samples)
                            .collect::<Vec<_>>()
                    );
                    let planes: Vec<_> = model
                        .components
                        .iter()
                        .enumerate()
                        .map(|(i, c)| ComponentPlane {
                            spec: c.spec,
                            width: c.width,
                            height: c.height,
                            samples: (0..c.width * c.height)
                                .map(|j| ((i * 911 + j * 193) & ((1usize << precision) - 1)) as u16)
                                .collect(),
                        })
                        .collect();
                    let levels = [
                        PyramidLevel::dct(Expansion::default(), 60, coding),
                        PyramidLevel::dct(
                            Expansion {
                                horizontal: true,
                                vertical: true,
                            },
                            85,
                            coding,
                        ),
                        PyramidLevel::lossless(
                            Expansion::default(),
                            LosslessOptions {
                                entropy_coding,
                                ..LosslessOptions::default()
                            },
                        ),
                    ];
                    let options = PyramidOptions {
                        limits,
                        ..PyramidOptions::new(&levels)
                    };
                    let pyramid =
                        build_hierarchical_planes(header.clone(), &planes, vec![], options)
                            .unwrap();
                    assert_eq!(
                        pyramid
                            .reconstruct_reference()
                            .unwrap()
                            .iter()
                            .map(|p| &p.samples)
                            .collect::<Vec<_>>(),
                        planes.iter().map(|p| &p.samples).collect::<Vec<_>>()
                    );
                    let frame_options = [
                        HierarchicalFrameOptions::Dct(coding),
                        HierarchicalFrameOptions::Dct(coding),
                        HierarchicalFrameOptions::Lossless(LosslessOptions {
                            entropy_coding,
                            ..LosslessOptions::default()
                        }),
                    ];
                    let encoded = pyramid
                        .encode(HierarchicalOptions {
                            limits,
                            frame_options: &frame_options,
                            ..HierarchicalOptions::default()
                        })
                        .unwrap();
                    let decoded = decode_hierarchical(&encoded, limits).unwrap();
                    assert_eq!(
                        decoded
                            .reconstruct_reference()
                            .unwrap()
                            .iter()
                            .map(|p| &p.samples)
                            .collect::<Vec<_>>(),
                        planes.iter().map(|p| &p.samples).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn pinned_generic_component_streams_match_pre_entropy_data_and_boundaries() {
    use deadlib_image_jpeg::HierarchicalData;
    use std::{collections::BTreeSet, path::Path};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/components");
    let mut sessions: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".stream-0.jpg"))
        .collect();
    sessions.sort();
    assert!(sessions.len() >= 30);
    let limits = Limits {
        max_scans: 2048,
        ..Limits::default()
    };
    let mut small = BTreeSet::new();
    let mut large = BTreeSet::new();
    let mut markers = BTreeSet::new();
    let (mut prefixes, mut mutations, mut largest) = (0, 0, 0);
    for session in sessions {
        let name = session
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_suffix(".stream-0.jpg")
            .unwrap();
        let mut cache = Tables::new();
        let mut stream = 0;
        loop {
            let path = root.join(format!("{name}.stream-{stream}.jpg"));
            if !path.exists() {
                break;
            }
            let bytes = std::fs::read(path).unwrap();
            let mut hierarchy = false;
            let mut processes = vec![];
            let mut boundaries = vec![];
            for event in MarkerReader::new(&bytes).unwrap() {
                match event.unwrap() {
                    Event::Segment { marker, offset, .. } => {
                        hierarchy |= marker == 0xde;
                        boundaries.push(offset);
                        if Process::try_from(marker).is_ok() {
                            processes.push(marker);
                        }
                    }
                    Event::Scan { offset, .. } | Event::End { offset } => boundaries.push(offset),
                }
            }
            if processes.is_empty() {
                cache.load(&bytes, limits).unwrap();
                stream += 1;
                continue;
            }
            markers.extend(processes.iter().copied());
            let before = cache.clone();
            let expected =
                |suffix: &str| std::fs::read(root.join(format!("{name}.{suffix}"))).unwrap();
            let precision = if hierarchy {
                let image = cache.decode_hierarchical(&bytes, limits).unwrap();
                largest = largest.max(image.header.components.len());
                assert_eq!(image.consumed, bytes.len());
                for (i, frame) in image.frames.iter().enumerate() {
                    match &frame.data {
                        HierarchicalData::Dct(c) => support::coefficient_dump(
                            &expected(&format!("frame-{i}.coef")),
                            &frame.frame,
                            c,
                        ),
                        HierarchicalData::Lossless { components, .. } => support::sample_dump(
                            &expected(&format!("frame-{i}.samples")),
                            frame.frame.precision,
                            frame.frame.width,
                            frame.frame.height,
                            components,
                        ),
                    }
                }
                let planes = image.reconstruct_reference().unwrap();
                support::sample_dump(
                    &expected("samples"),
                    image.header.precision,
                    image.header.width,
                    image.header.height,
                    &planes,
                );
                if name.split('-').nth(9) == Some("2") {
                    for (i, c) in planes.iter().enumerate() {
                        let source: Vec<_> = (0..c.width * c.height)
                            .map(|j| {
                                ((i * 911 + j * 193) & ((1usize << image.header.precision) - 1))
                                    as u16
                            })
                            .collect();
                        assert_eq!(c.samples, source);
                    }
                }
                image.header.precision
            } else {
                let image = cache.decode_coefficients(&bytes, limits).unwrap();
                largest = largest.max(image.components.len());
                assert_eq!(image.consumed, bytes.len());
                support::coefficient_dump(
                    &expected("frame-0.coef"),
                    &image.frame,
                    &image.components,
                );
                image.frame.precision
            };
            let bounded = Limits {
                max_pixels: 65536,
                max_memory_bytes: 16 * 1024 * 1024,
                max_metadata_bytes: 65536,
                max_scans: 2048,
            };
            let parse = |input: &[u8]| {
                let mut state = before.clone();
                if hierarchy {
                    state.decode_hierarchical(input, bounded).map(|_| ())
                } else {
                    state.decode_coefficients(input, bounded).map(|_| ())
                }
            };
            let key = (processes, precision);
            let checked = if bytes.len() <= 4096 {
                if small.insert(key.clone()) {
                    for end in 0..bytes.len() {
                        assert!(parse(&bytes[..end]).is_err(), "{name} prefix {end}");
                        prefixes += 1;
                    }
                    true
                } else {
                    false
                }
            } else if large.insert(key) {
                boundaries.extend((0..bytes.len()).step_by((bytes.len() / 32).max(1)));
                boundaries.push(bytes.len() - 1);
                boundaries.sort_unstable();
                boundaries.dedup();
                for end in boundaries {
                    assert!(parse(&bytes[..end]).is_err(), "{name} boundary {end}");
                    prefixes += 1;
                }
                true
            } else {
                false
            };
            if checked {
                let mut state = 0xabcdef12u32;
                for _ in 0..100 {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    let mut changed = bytes.clone();
                    let i = state as usize % changed.len();
                    changed[i] ^= (state >> 24) as u8 | 1;
                    assert!(std::panic::catch_unwind(|| parse(&changed)).is_ok());
                    mutations += 1;
                }
            }
            stream += 1;
        }
    }
    assert_eq!(largest, 255);
    assert_eq!(
        markers.into_iter().collect::<Vec<_>>(),
        [0xc0, 0xc1, 0xc2, 0xc5, 0xc7, 0xc9, 0xca, 0xcd, 0xcf]
    );
    eprintln!(
        "Checked {prefixes} generic-component prefixes/boundaries and {mutations} bounded mutations"
    );
}
