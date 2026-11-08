use deadlib_image_jpeg::{
    CodingOptions, CoefficientComponent, ComponentPlane, ComponentSpec, EntropyCoding, Error,
    Event, Expansion, FrameHeader, HierarchicalData, HierarchicalFrame, HierarchicalFrameOptions,
    HierarchicalImage, HierarchicalOptions, HierarchyHeader, IccWrite, Limits, LosslessCoding,
    LosslessOptions, LosslessScan, MarkerReader, MetadataOptions, MetadataSegment, Process,
    ScanSpec, decode_hierarchical,
};

fn spec(id: u8) -> ComponentSpec {
    ComponentSpec {
        id,
        horizontal_sampling: 1,
        vertical_sampling: 1,
        quantization_table: 0,
    }
}
fn header(process: Process, p: u8, w: u16, h: u16, specs: Vec<ComponentSpec>) -> FrameHeader {
    FrameHeader {
        process,
        precision: p,
        width: w,
        height: h,
        components: specs,
    }
}
fn predictive(frame: FrameHeader, values: Vec<Vec<u16>>) -> HierarchicalFrame {
    let differential = frame.process.is_differential();
    let components = frame
        .components
        .iter()
        .zip(values)
        .map(|(&spec, samples)| ComponentPlane {
            spec,
            width: frame.width as usize,
            height: frame.height as usize,
            samples,
        })
        .collect();
    let coding = vec![
        LosslessCoding {
            predictor: u8::from(!differential),
            point_transform: 0
        };
        frame.components.len()
    ];
    HierarchicalFrame {
        frame,
        expansion: Expansion::default(),
        data: HierarchicalData::Lossless { components, coding },
        offset: 0,
        first_scan: 0,
        scans: 1,
    }
}
fn lossless_pair(p: u8, base: Vec<u16>, delta: Vec<u16>) -> HierarchicalImage<'static> {
    let w = base.len() as u16;
    let frames = vec![
        predictive(
            header(Process::Lossless, p, w, 1, vec![spec(255)]),
            vec![base],
        ),
        predictive(
            header(Process::DifferentialLossless, p, w, 1, vec![spec(255)]),
            vec![delta],
        ),
    ];
    HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: p,
            width: w,
            height: 1,
            components: vec![spec(255)],
        },
        frames,
        vec![],
        Limits::default(),
    )
    .unwrap()
}
fn dct_pair(p: u8) -> HierarchicalImage<'static> {
    let mut frames = vec![];
    for (process, dc, ac) in [
        (Process::ExtendedSequential, 0, 0),
        (
            Process::DifferentialSequential,
            4,
            if p == 8 { 1024 } else { 16384 },
        ),
    ] {
        let frame = header(process, p, 16, 8, vec![spec(1)]);
        let mut blocks = vec![[0; 64]; 2];
        for block in &mut blocks {
            block[0] = dc;
            block[1] = ac;
            block[8] = 1;
            block[26] = ac;
        }
        let component = CoefficientComponent {
            spec: spec(1),
            width: 16,
            height: 8,
            blocks_wide: 2,
            blocks_high: 1,
            quantization: [1; 64],
            successive_approximation: [0; 64],
            blocks,
        };
        frames.push(HierarchicalFrame {
            frame,
            expansion: Expansion::default(),
            data: HierarchicalData::Dct(vec![component]),
            offset: 0,
            first_scan: 0,
            scans: 1,
        });
    }
    HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: p,
            width: 16,
            height: 8,
            components: vec![spec(1)],
        },
        frames,
        vec![],
        Limits::default(),
    )
    .unwrap()
}
fn equal_frames(source: &HierarchicalImage<'_>, output: &HierarchicalImage<'_>) {
    assert_eq!(source.header, output.header);
    assert_eq!(source.frames.len(), output.frames.len());
    for (a, b) in source.frames.iter().zip(&output.frames) {
        assert_eq!(
            (
                a.frame.precision,
                a.frame.width,
                a.frame.height,
                a.expansion
            ),
            (
                b.frame.precision,
                b.frame.width,
                b.frame.height,
                b.expansion
            )
        );
        match (&a.data, &b.data) {
            (HierarchicalData::Dct(a), HierarchicalData::Dct(b)) => {
                for (a, b) in a.iter().zip(b) {
                    assert_eq!(
                        (
                            a.spec.id,
                            a.spec.horizontal_sampling,
                            a.spec.vertical_sampling
                        ),
                        (
                            b.spec.id,
                            b.spec.horizontal_sampling,
                            b.spec.vertical_sampling
                        )
                    );
                    assert_eq!(
                        (a.width, a.height, a.quantization),
                        (b.width, b.height, b.quantization)
                    );
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
            (
                HierarchicalData::Lossless { components: a, .. },
                HierarchicalData::Lossless { components: b, .. },
            ) => {
                for (a, b) in a.iter().zip(b) {
                    assert_eq!((a.spec, a.width, a.height), (b.spec, b.width, b.height));
                    assert_eq!(a.samples, b.samples);
                }
            }
            _ => panic!("frame data family changed"),
        }
    }
    let a = source.reconstruct_reference().unwrap();
    let b = output.reconstruct_reference().unwrap();
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.samples, b.samples);
    }
}

fn words(data: &[u8]) -> Vec<u32> {
    data.as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .collect()
}
fn plane_dump(data: &[u8], frame: &FrameHeader, components: &[ComponentPlane]) {
    assert_eq!(&data[..4], b"JLP1");
    assert_eq!(
        words(&data[4..20]),
        [
            u32::from(frame.precision),
            u32::from(frame.width),
            u32::from(frame.height),
            components.len() as u32
        ]
    );
    let mut position = 20;
    for c in components {
        assert_eq!(
            words(&data[position..position + 20]),
            [
                u32::from(c.spec.id),
                u32::from(c.spec.horizontal_sampling),
                u32::from(c.spec.vertical_sampling),
                c.width as u32,
                c.height as u32
            ]
        );
        position += 20;
        for &v in &c.samples {
            assert_eq!(
                u16::from_le_bytes(data[position..position + 2].try_into().unwrap()),
                v
            );
            position += 2;
        }
    }
    assert_eq!(position, data.len());
}

#[test]
fn native_authored_pinned_frames_and_exact_lossless_targets() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hierarchical-native");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 62);
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        let source = decode_hierarchical(&bytes, Limits::default()).unwrap();
        for (i, f) in source.frames.iter().enumerate() {
            match &f.data {
                HierarchicalData::Lossless { components, .. } => plane_dump(
                    &std::fs::read(path.with_extension(format!("frame-{i}.samples"))).unwrap(),
                    &f.frame,
                    components,
                ),
                HierarchicalData::Dct(components) => {
                    let data =
                        std::fs::read(path.with_extension(format!("frame-{i}.coef"))).unwrap();
                    assert_eq!(&data[..4], b"JCO1");
                    assert_eq!(
                        words(&data[4..20]),
                        [
                            u32::from(f.frame.precision),
                            u32::from(f.frame.width),
                            u32::from(f.frame.height),
                            components.len() as u32
                        ]
                    );
                    let mut position = 20;
                    for c in components {
                        let (bw, bh) = (c.width.div_ceil(8), c.height.div_ceil(8));
                        assert_eq!(
                            words(&data[position..position + 28]),
                            [
                                u32::from(c.spec.id),
                                u32::from(c.spec.horizontal_sampling),
                                u32::from(c.spec.vertical_sampling),
                                c.width as u32,
                                c.height as u32,
                                bw as u32,
                                bh as u32
                            ]
                        );
                        position += 28;
                        for q in c.quantization {
                            assert_eq!(
                                u16::from_le_bytes(
                                    data[position..position + 2].try_into().unwrap()
                                ),
                                q
                            );
                            position += 2;
                        }
                        for y in 0..bh {
                            for x in 0..bw {
                                for v in c.blocks[y * c.blocks_wide + x] {
                                    assert_eq!(
                                        i16::from_le_bytes(
                                            data[position..position + 2].try_into().unwrap()
                                        ),
                                        v
                                    );
                                    position += 2;
                                }
                            }
                        }
                    }
                    assert_eq!(position, data.len());
                }
            }
        }
        let expected = path.with_extension("expected.samples");
        if expected.exists() {
            plane_dump(
                &std::fs::read(expected).unwrap(),
                &source.frames.last().unwrap().frame,
                &source.reconstruct_reference().unwrap(),
            );
        }
        for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            let controls = options(&source, entropy, true, false, 1, 13);
            let encoded = source
                .encode(HierarchicalOptions {
                    frame_options: &controls,
                    ..HierarchicalOptions::default()
                })
                .unwrap();
            equal_frames(
                &source,
                &decode_hierarchical(&encoded, Limits::default()).unwrap(),
            );
        }
    }
}
fn options<'a>(
    source: &HierarchicalImage<'_>,
    entropy_coding: EntropyCoding,
    progressive: bool,
    optimize_huffman: bool,
    restart: u16,
    initial: u8,
) -> Vec<HierarchicalFrameOptions<'a>> {
    source
        .frames
        .iter()
        .map(|f| {
            if f.frame.process.is_lossless() {
                HierarchicalFrameOptions::Lossless(LosslessOptions {
                    entropy_coding,
                    optimize_huffman,
                    predictor: 7,
                    restart_rows: restart,
                    ..LosslessOptions::default()
                })
            } else {
                HierarchicalFrameOptions::Dct(CodingOptions {
                    entropy_coding,
                    progressive,
                    optimize_huffman,
                    restart_interval: restart,
                    successive_approximation: initial,
                    ..CodingOptions::default()
                })
            }
        })
        .collect()
}

#[test]
fn every_pinned_frame_roundtrips_through_both_entropy_and_dct_processes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hierarchical");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 42);
    for path in paths {
        let bytes = std::fs::read(path).unwrap();
        let source = decode_hierarchical(&bytes, Limits::default()).unwrap();
        let encoded = source.encode(HierarchicalOptions::default()).unwrap();
        equal_frames(
            &source,
            &decode_hierarchical(&encoded, Limits::default()).unwrap(),
        );
        for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for progressive in [false, true] {
                for optimize in [false, true] {
                    for restart in [0, 1, 3] {
                        let controls = options(
                            &source,
                            entropy,
                            progressive,
                            optimize,
                            restart,
                            if progressive { restart as u8 } else { 0 },
                        );
                        let encoded = source
                            .encode(HierarchicalOptions {
                                frame_options: &controls,
                                ..HierarchicalOptions::default()
                            })
                            .unwrap();
                        let output = decode_hierarchical(&encoded, Limits::default()).unwrap();
                        equal_frames(&source, &output);
                    }
                }
            }
        }
    }
}

#[test]
fn direct_dc_extended_ac_and_full_fixed_progressive_tables() {
    for p in [8, 12] {
        let source = dct_pair(p);
        for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for progressive in [false, true] {
                let controls = options(
                    &source,
                    entropy,
                    progressive,
                    false,
                    1,
                    if progressive { 13 } else { 0 },
                );
                let bytes = source
                    .encode(HierarchicalOptions {
                        frame_options: &controls,
                        ..HierarchicalOptions::default()
                    })
                    .unwrap();
                let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
                equal_frames(&source, &decoded);
                if entropy == EntropyCoding::Huffman && progressive && p == 12 {
                    assert!(MarkerReader::new(&bytes).unwrap().any(|e|matches!(e.unwrap(),Event::Segment {marker:0xc4,data,..} if data[0]>>4==1 && data[1..17].iter().map(|&v|v as usize).sum::<usize>()==256)));
                }
            }
        }
    }
}

#[test]
fn signed_residual_pt_uses_integer_division_and_literal_huffman_bits() {
    let source = lossless_pair(8, vec![100, 100], vec![(-3i16) as u16, 3]);
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let controls = [
            HierarchicalFrameOptions::Lossless(LosslessOptions {
                entropy_coding,
                ..LosslessOptions::default()
            }),
            HierarchicalFrameOptions::Lossless(LosslessOptions {
                entropy_coding,
                optimize_huffman: false,
                point_transform: 1,
                ..LosslessOptions::default()
            }),
        ];
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        let decoded = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(
            decoded.reconstruct_reference().unwrap()[0].samples,
            [98, 102]
        );
        if entropy_coding == EntropyCoding::Huffman {
            let entropy = MarkerReader::new(&bytes)
                .unwrap()
                .filter_map(|e| match e.unwrap() {
                    Event::Scan {
                        header, entropy, ..
                    } if header[3] == 0 => Some(entropy),
                    _ => None,
                })
                .next()
                .unwrap();
            assert_eq!(entropy, [0x08, 0x3f]);
        }
    }
    let source = lossless_pair(16, vec![0, 65535], vec![65535, 1]);
    for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let controls = options(&source, entropy, false, false, 1, 0);
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        assert_eq!(
            decode_hierarchical(&bytes, Limits::default())
                .unwrap()
                .reconstruct_reference()
                .unwrap()[0]
                .samples,
            [65535, 0]
        );
    }
}

#[test]
fn inherited_restart_intervals_are_cleared_between_frames() {
    for source in [dct_pair(8), lossless_pair(8, vec![1; 17], vec![0; 17])] {
        let mut controls = options(&source, EntropyCoding::Arithmetic, false, true, 1, 0);
        match &mut controls[1] {
            HierarchicalFrameOptions::Dct(c) => c.restart_interval = 0,
            HierarchicalFrameOptions::Lossless(c) => c.restart_rows = 0,
        }
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        equal_frames(
            &source,
            &decode_hierarchical(&bytes, Limits::default()).unwrap(),
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
        assert_eq!(dri.len(), 2);
        assert_ne!(dri[0], 0);
        assert_eq!(dri[1], 0);
    }
}

#[test]
fn explicit_differential_scripts_partial_dct_and_metadata_clamping() {
    let specs = vec![spec(1), spec(2), spec(3)];
    let frames = vec![
        predictive(
            header(Process::Lossless, 8, 2, 1, specs.clone()),
            vec![vec![100; 2], vec![200; 2], vec![10; 2]],
        ),
        predictive(
            header(Process::DifferentialLossless, 8, 2, 1, specs.clone()),
            vec![
                vec![(-3i16) as u16, 3],
                vec![(-7i16) as u16, 7],
                vec![65535, 1],
            ],
        ),
    ];
    let source = HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: 8,
            width: 2,
            height: 1,
            components: specs,
        },
        frames,
        vec![],
        Limits::default(),
    )
    .unwrap();
    let base = [
        LosslessScan::new(&[3], 7, 0, 1).unwrap(),
        LosslessScan::new(&[1], 4, 0, 0).unwrap(),
        LosslessScan::new(&[2], 1, 0, 1).unwrap(),
    ];
    let diff = [
        LosslessScan::new(&[3], 0, 0, 1).unwrap(),
        LosslessScan::new(&[1, 2], 0, 1, 0).unwrap(),
    ];
    for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let controls = [
            HierarchicalFrameOptions::Lossless(LosslessOptions {
                entropy_coding,
                scan_script: &base,
                ..LosslessOptions::default()
            }),
            HierarchicalFrameOptions::Lossless(LosslessOptions {
                entropy_coding,
                scan_script: &diff,
                ..LosslessOptions::default()
            }),
        ];
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(image.scans, 5);
        let planes = image.reconstruct_reference().unwrap();
        assert_eq!(planes[0].samples, [98, 102]);
        assert_eq!(planes[1].samples, [194, 206]);
        assert_eq!(planes[2].samples, [9, 11]);
        if let HierarchicalData::Lossless { coding, .. } = &image.frames[1].data {
            assert_eq!(
                coding.iter().map(|c| c.point_transform).collect::<Vec<_>>(),
                [1, 1, 0]
            );
        }
        let source = dct_pair(12);
        let dc = [ScanSpec::new(&[1], 0, 0, 0, 1).unwrap()];
        let controls = [HierarchicalFrameOptions::Dct(CodingOptions {
            entropy_coding,
            progressive: true,
            scan_script: &dc,
            ..CodingOptions::default()
        }); 2];
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        let image = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(image.scans, 2);
        assert!(
            image.reconstruct_reference().unwrap()[0]
                .samples
                .iter()
                .all(|&v| v == 2049)
        );
        for f in &image.frames {
            if let HierarchicalData::Dct(c) = &f.data {
                for block in &c[0].blocks {
                    assert!(block[1..].iter().all(|&v| v == 0));
                }
                assert_eq!(c[0].successive_approximation[0], 1);
            }
        }
    }
    let mut source = dct_pair(8);
    source.frames[0].scans = 4;
    source.frames[1].first_scan = 4;
    source.scans = 5;
    source.metadata = vec![
        MetadataSegment {
            marker: 0xfe,
            data: b"inside",
            offset: 0,
            preceding_scans: 2,
        },
        MetadataSegment {
            marker: 0xfe,
            data: b"boundary",
            offset: 0,
            preceding_scans: 4,
        },
    ];
    let encoded = source.encode(HierarchicalOptions::default()).unwrap();
    let image = decode_hierarchical(&encoded, Limits::default()).unwrap();
    assert_eq!(image.scans, 2);
    assert_eq!(
        image
            .metadata
            .iter()
            .map(|s| s.preceding_scans)
            .collect::<Vec<_>>(),
        [1, 1]
    );
}

#[test]
fn full_component_range_can_be_authored_without_a_decoder() {
    let specs: Vec<_> = (0..255).map(spec).collect();
    let base: Vec<Vec<_>> = (0..255u16).map(|i| vec![i * 257]).collect();
    let delta = base.iter().map(|c| vec![0u16.wrapping_sub(c[0])]).collect();
    let frames = vec![
        predictive(header(Process::Lossless, 16, 1, 1, specs.clone()), base),
        predictive(
            header(Process::DifferentialLossless, 16, 1, 1, specs.clone()),
            delta,
        ),
    ];
    let source = HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: 16,
            width: 1,
            height: 1,
            components: specs,
        },
        frames,
        vec![],
        Limits::default(),
    )
    .unwrap();
    for entropy in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
        let controls = options(&source, entropy, false, true, 1, 0);
        let bytes = source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .unwrap();
        let output = decode_hierarchical(&bytes, Limits::default()).unwrap();
        assert_eq!(output.scans, 128);
        assert!(
            output
                .reconstruct_reference()
                .unwrap()
                .iter()
                .all(|c| c.samples == [0])
        );
        equal_frames(&source, &output);
    }
}

#[test]
fn metadata_frame_positions_icc_and_budget_errors() {
    let mut source = dct_pair(8);
    source.metadata = vec![
        MetadataSegment {
            marker: 0xfe,
            data: b"first",
            offset: 0,
            preceding_scans: 0,
        },
        MetadataSegment {
            marker: 0xe2,
            data: b"ICC_PROFILE\0\x01\x01old",
            offset: 0,
            preceding_scans: 0,
        },
        MetadataSegment {
            marker: 0xfe,
            data: b"middle",
            offset: 0,
            preceding_scans: 1,
        },
        MetadataSegment {
            marker: 0xfe,
            data: b"last",
            offset: 0,
            preceding_scans: 2,
        },
    ];
    let controls = options(&source, EntropyCoding::Huffman, true, true, 1, 3);
    let settings = HierarchicalOptions {
        frame_options: &controls,
        ..HierarchicalOptions::default()
    };
    let bytes = source.encode(settings).unwrap();
    let output = decode_hierarchical(&bytes, Limits::default()).unwrap();
    let comments: Vec<_> = output
        .metadata
        .iter()
        .filter(|s| s.marker == 0xfe)
        .map(|s| (s.data, s.preceding_scans))
        .collect();
    assert_eq!(
        comments,
        vec![
            (&b"first"[..], 0),
            (&b"middle"[..], output.frames[1].first_scan),
            (&b"last"[..], output.scans)
        ]
    );
    let bytes = source
        .encode_with_metadata(
            settings,
            MetadataOptions {
                icc: IccWrite::Replace(b"replacement"),
                ..MetadataOptions::default()
            },
        )
        .unwrap();
    let output = decode_hierarchical(&bytes, Limits::default()).unwrap();
    assert_eq!(
        output.icc_profile().unwrap().unwrap().to_vec(100).unwrap(),
        b"replacement"
    );
    let bytes = source
        .encode_with_metadata(
            settings,
            MetadataOptions {
                icc: IccWrite::Remove,
                ..MetadataOptions::default()
            },
        )
        .unwrap();
    assert!(
        decode_hierarchical(&bytes, Limits::default())
            .unwrap()
            .icc_profile()
            .unwrap()
            .is_none()
    );
    let bytes = source.encode(settings).unwrap();
    assert!(
        source
            .encode(HierarchicalOptions {
                max_output_bytes: bytes.len() - 1,
                ..settings
            })
            .is_err()
    );
    assert_eq!(
        source
            .encode(HierarchicalOptions {
                max_output_bytes: bytes.len(),
                ..settings
            })
            .unwrap(),
        bytes
    );
    for limits in [
        Limits {
            max_pixels: 1,
            ..Limits::default()
        },
        Limits {
            max_scans: 1,
            ..Limits::default()
        },
        Limits {
            max_memory_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_metadata_bytes: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            source.encode(HierarchicalOptions { limits, ..settings }),
            Err(Error::LimitExceeded(_))
        ));
    }
    let mut bad = controls.clone();
    if let HierarchicalFrameOptions::Dct(c) = &mut bad[1] {
        c.max_output_bytes = 0;
    }
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &bad,
                ..settings
            })
            .is_err()
    );
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &controls[..1],
                ..settings
            })
            .is_err()
    );
    let wrong = [HierarchicalFrameOptions::Lossless(LosslessOptions::default()); 2];
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &wrong,
                ..settings
            })
            .is_err()
    );
    let mut bad = options(&source, EntropyCoding::Huffman, false, true, 0, 0);
    if let HierarchicalFrameOptions::Dct(c) = &mut bad[1] {
        c.entropy_coding = EntropyCoding::Arithmetic;
    }
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &bad,
                ..settings
            })
            .is_ok()
    ); // baseline exception
    let source = dct_pair(12);
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &bad,
                ..HierarchicalOptions::default()
            })
            .is_err()
    );
    let source = lossless_pair(8, vec![1], vec![0]);
    let invalid = [LosslessScan::new(&[255], 1, 0, 0).unwrap()];
    let controls = [
        HierarchicalFrameOptions::Lossless(LosslessOptions::default()),
        HierarchicalFrameOptions::Lossless(LosslessOptions {
            scan_script: &invalid,
            ..LosslessOptions::default()
        }),
    ];
    assert!(
        source
            .encode(HierarchicalOptions {
                frame_options: &controls,
                ..HierarchicalOptions::default()
            })
            .is_err()
    );
}
