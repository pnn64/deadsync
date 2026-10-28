use deadlib_image_jpeg::{
    AbbreviatedEncoder, ArithmeticConditioning, CodingOptions, CoefficientImage, EntropyCoding,
    Error, Event, HeightDeclaration, HierarchicalData, HierarchicalImage, HierarchicalOptions,
    HuffmanClass, Limits, LosslessImage, LosslessOptions, MarkerReader, MetadataOptions,
    MetadataSegment, QuantizationTable, Tables, decode_coefficients, decode_hierarchical,
    encode_coefficients,
};
use std::path::{Path, PathBuf};

fn events(input: &[u8]) -> Vec<(u8, usize, usize)> {
    let mut reader = MarkerReader::new(input).unwrap();
    let mut result = vec![];
    while let Some(event) = reader.next() {
        let (marker, start) = match event.unwrap() {
            Event::Segment { marker, offset, .. } => (marker, offset),
            Event::Scan { offset, .. } => (0xda, offset),
            Event::End { offset } => (0xd9, offset),
        };
        result.push((marker, start, reader.consumed()));
    }
    result
}
fn without(input: &[u8], omit: impl Fn(u8) -> bool) -> Vec<u8> {
    let mut result = input[..2].to_vec();
    for (marker, start, end) in events(input) {
        if !omit(marker) {
            result.extend_from_slice(&input[start..end]);
        }
    }
    result
}
fn split_initial(input: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut tables = vec![255, 0xd8];
    let mut image = tables.clone();
    let mut scanned = false;
    for (marker, start, end) in events(input) {
        scanned |= marker == 0xda;
        if !scanned && matches!(marker, 0xdb | 0xc4) {
            tables.extend_from_slice(&input[start..end]);
        } else {
            image.extend_from_slice(&input[start..end]);
        }
    }
    tables.extend_from_slice(&[255, 0xd9]);
    (tables, image)
}
fn paths() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut result = vec![];
    for name in [
        "sequential",
        "progressive",
        "arithmetic",
        "arithmetic-progressive",
        "arithmetic-model",
        "lossless",
        "lossless-arithmetic",
        "hierarchical",
        "hierarchical-native",
        "pyramid-native",
        "dnl",
    ] {
        result.extend(
            std::fs::read_dir(root.join(name))
                .unwrap()
                .map(|p| p.unwrap().path())
                .filter(|p| p.extension().is_some_and(|v| v == "jpg")),
        );
    }
    result.sort();
    result
}
enum Image<'a> {
    Dct(CoefficientImage<'a>),
    Lossless(LosslessImage<'a>),
    Hierarchical(HierarchicalImage<'a>),
}

#[test]
fn mixed_native_color_spaces_keep_exact_samples_and_image_tags() {
    use deadlib_image_jpeg::{ColorSpace, EncodeInput, SampleData};
    let mut sender = AbbreviatedEncoder::new();
    let mut receiver = Tables::new();
    for precision in [2, 8, 12, 16] {
        for color in [
            ColorSpace::Grayscale,
            ColorSpace::Rgb,
            ColorSpace::YCbCr,
            ColorSpace::Cmyk,
            ColorSpace::Ycck,
        ] {
            let channels = match color {
                ColorSpace::Grayscale => 1,
                ColorSpace::Cmyk | ColorSpace::Ycck => 4,
                _ => 3,
            };
            let samples: Vec<u16> = (0..11 * 7 * channels)
                .map(|i| ((i * 193 + i * i * 53) & ((1usize << precision) - 1)) as u16)
                .collect();
            let input = EncodeInput {
                width: 11,
                height: 7,
                precision,
                color_space: color,
                samples: SampleData::U16(&samples),
            };
            for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
                for _ in 0..2 {
                    let encoded = sender
                        .encode_lossless_samples(
                            &input,
                            LosslessOptions {
                                entropy_coding,
                                restart_rows: 1,
                                height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                                ..LosslessOptions::default()
                            },
                            MetadataOptions::default(),
                        )
                        .unwrap();
                    assert!(
                        receiver
                            .load(&encoded.tables, Limits::default())
                            .unwrap()
                            .metadata
                            .is_empty()
                    );
                    let decoded = receiver
                        .decode_lossless(&encoded.image, Limits::default())
                        .unwrap();
                    assert_eq!(decoded.color_space(), color);
                    for (i, plane) in decoded.components.iter().enumerate() {
                        assert_eq!(
                            plane.samples,
                            samples
                                .iter()
                                .skip(i)
                                .step_by(channels)
                                .copied()
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}
fn family(input: &[u8]) -> u8 {
    let mut hierarchy = false;
    for event in MarkerReader::new(input).unwrap() {
        if let Event::Segment { marker, .. } = event.unwrap() {
            hierarchy |= marker == 0xde;
            if let Ok(process) = deadlib_image_jpeg::Process::try_from(marker) {
                return if hierarchy {
                    2
                } else {
                    u8::from(process.is_lossless())
                };
            }
        }
    }
    panic!("missing frame")
}
fn read<'a>(input: &'a [u8], tables: &mut Tables) -> Image<'a> {
    match family(input) {
        0 => Image::Dct(
            tables
                .decode_coefficients(input, Limits::default())
                .unwrap(),
        ),
        1 => Image::Lossless(tables.decode_lossless(input, Limits::default()).unwrap()),
        _ => Image::Hierarchical(
            tables
                .decode_hierarchical(input, Limits::default())
                .unwrap(),
        ),
    }
}
fn same_dct(
    a: &[deadlib_image_jpeg::CoefficientComponent],
    b: &[deadlib_image_jpeg::CoefficientComponent],
) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(
            (
                a.width,
                a.height,
                a.spec.id,
                a.spec.horizontal_sampling,
                a.spec.vertical_sampling
            ),
            (
                b.width,
                b.height,
                b.spec.id,
                b.spec.horizontal_sampling,
                b.spec.vertical_sampling
            )
        );
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
fn same_planes(a: &[deadlib_image_jpeg::ComponentPlane], b: &[deadlib_image_jpeg::ComponentPlane]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!((a.width, a.height, a.spec), (b.width, b.height, b.spec));
        assert_eq!(a.samples, b.samples);
    }
}
fn same(a: &Image<'_>, b: &Image<'_>) {
    match (a, b) {
        (Image::Dct(a), Image::Dct(b)) => same_dct(&a.components, &b.components),
        (Image::Lossless(a), Image::Lossless(b)) => same_planes(&a.components, &b.components),
        (Image::Hierarchical(a), Image::Hierarchical(b)) => {
            assert_eq!(a.header, b.header);
            assert_eq!(a.frames.len(), b.frames.len());
            for (a, b) in a.frames.iter().zip(&b.frames) {
                assert_eq!(a.expansion, b.expansion);
                match (&a.data, &b.data) {
                    (HierarchicalData::Dct(a), HierarchicalData::Dct(b)) => same_dct(a, b),
                    (
                        HierarchicalData::Lossless { components: a, .. },
                        HierarchicalData::Lossless { components: b, .. },
                    ) => same_planes(a, b),
                    _ => panic!("changed coding family"),
                }
            }
            same_planes(
                &a.reconstruct_reference().unwrap(),
                &b.reconstruct_reference().unwrap(),
            );
        }
        _ => panic!("changed image family"),
    }
}
fn source() -> Vec<u8> {
    let path = paths()
        .into_iter()
        .find(|p| {
            p.parent().unwrap().ends_with("sequential")
                && p.file_name().unwrap().to_string_lossy().contains("12-gray")
        })
        .unwrap();
    std::fs::read(path).unwrap()
}

fn word(out: &mut Vec<u8>, value: usize) {
    out.extend_from_slice(&(value as u32).to_le_bytes());
}
fn sample_bytes(
    frame: &deadlib_image_jpeg::FrameHeader,
    planes: &[deadlib_image_jpeg::ComponentPlane],
) -> Vec<u8> {
    let mut out = b"JLP1".to_vec();
    for value in [
        frame.precision as usize,
        frame.width as usize,
        frame.height as usize,
        planes.len(),
    ] {
        word(&mut out, value);
    }
    for plane in planes {
        for value in [
            plane.spec.id as usize,
            plane.spec.horizontal_sampling as usize,
            plane.spec.vertical_sampling as usize,
            plane.width,
            plane.height,
        ] {
            word(&mut out, value);
        }
        for value in &plane.samples {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}
fn coefficient_bytes(
    frame: &deadlib_image_jpeg::FrameHeader,
    components: &[deadlib_image_jpeg::CoefficientComponent],
) -> Vec<u8> {
    let mut out = b"JCO1".to_vec();
    for value in [
        frame.precision as usize,
        frame.width as usize,
        frame.height as usize,
        components.len(),
    ] {
        word(&mut out, value);
    }
    for c in components {
        for value in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
            c.width.div_ceil(8),
            c.height.div_ceil(8),
        ] {
            word(&mut out, value);
        }
        for value in c.quantization {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for y in 0..c.height.div_ceil(8) {
            for x in 0..c.width.div_ceil(8) {
                for value in c.blocks[y * c.blocks_wide + x] {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
    out
}

#[test]
fn pinned_persistent_sessions_exact_data_and_malformed_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/abbreviated");
    let mut sessions: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".stream-0.jpg"))
        .collect();
    sessions.sort();
    assert!(sessions.len() >= 10);
    let mut processes = std::collections::BTreeSet::new();
    let mut malformed_coverage = std::collections::BTreeSet::new();
    let (mut images, mut prefixes, mut mutations) = (0, 0, 0);
    for session in sessions {
        let name = session
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .strip_suffix(".stream-0.jpg")
            .unwrap();
        let mut cache = Tables::new();
        let (mut stream, mut image_index) = (0, 0);
        loop {
            let path = root.join(format!("{name}.stream-{stream}.jpg"));
            if !path.exists() {
                break;
            }
            let bytes = std::fs::read(path).unwrap();
            let marker_list: Vec<_> = events(&bytes)
                .into_iter()
                .filter_map(|(marker, _, _)| {
                    deadlib_image_jpeg::Process::try_from(marker)
                        .ok()
                        .map(|_| marker)
                })
                .collect();
            if marker_list.is_empty() {
                cache.load(&bytes, Limits::default()).unwrap();
            } else {
                processes.extend(marker_list.iter().copied());
                let before = cache.clone();
                let model = read(&bytes, &mut cache);
                let expect = |tail: &str, actual: Vec<u8>| {
                    assert_eq!(
                        actual,
                        std::fs::read(root.join(format!("{name}.image-{image_index}.{tail}")))
                            .unwrap(),
                        "{name} image {image_index} {tail}"
                    )
                };
                let precision = match &model {
                    Image::Dct(image) => {
                        expect("coef", coefficient_bytes(&image.frame, &image.components));
                        image.frame.precision
                    }
                    Image::Lossless(image) => {
                        expect("samples", sample_bytes(&image.frame, &image.components));
                        image.frame.precision
                    }
                    Image::Hierarchical(image) => {
                        for (i, frame) in image.frames.iter().enumerate() {
                            match &frame.data {
                                HierarchicalData::Dct(c) => expect(
                                    &format!("frame-{i}.coef"),
                                    coefficient_bytes(&frame.frame, c),
                                ),
                                HierarchicalData::Lossless { components, .. } => expect(
                                    &format!("frame-{i}.samples"),
                                    sample_bytes(&frame.frame, components),
                                ),
                            }
                        }
                        let frame = deadlib_image_jpeg::FrameHeader {
                            process: deadlib_image_jpeg::Process::Lossless,
                            precision: image.header.precision,
                            width: image.header.width,
                            height: image.header.height,
                            components: image.header.components.clone(),
                        };
                        expect(
                            "samples",
                            sample_bytes(&frame, &image.reconstruct_reference().unwrap()),
                        );
                        image.header.precision
                    }
                };
                if malformed_coverage.insert((marker_list, precision)) {
                    let family = family(&bytes);
                    let parse = |input: &[u8], limits| {
                        let mut cache = before.clone();
                        match family {
                            0 => cache.decode_coefficients(input, limits).map(|_| ()),
                            1 => cache.decode_lossless(input, limits).map(|_| ()),
                            _ => cache.decode_hierarchical(input, limits).map(|_| ()),
                        }
                    };
                    for end in 0..bytes.len() {
                        assert!(
                            parse(&bytes[..end], Limits::default()).is_err(),
                            "{name} prefix {end}"
                        );
                        prefixes += 1;
                    }
                    let limits = Limits {
                        max_pixels: 65536,
                        max_memory_bytes: 2 * 1024 * 1024,
                        max_metadata_bytes: 65536,
                        max_scans: 256,
                    };
                    let mut state = 0x12345678u32;
                    for _ in 0..100 {
                        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                        let mut changed = bytes.clone();
                        let index = state as usize % changed.len();
                        changed[index] ^= (state >> 24) as u8 | 1;
                        assert!(std::panic::catch_unwind(|| parse(&changed, limits)).is_ok());
                        mutations += 1;
                    }
                }
                image_index += 1;
                images += 1;
            }
            stream += 1;
        }
    }
    assert_eq!(
        processes.into_iter().collect::<Vec<_>>(),
        [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf
        ]
    );
    eprintln!(
        "Checked {images} pinned images, {prefixes} truncated prefixes and {mutations} mutations"
    );
}

#[test]
fn all_table_destinations_widths_symbols_metadata_and_limits() {
    let mut bank = Tables::new();
    for id in 0..4 {
        bank.set_quantization(
            id,
            QuantizationTable {
                values: std::array::from_fn(|k| {
                    if id < 2 {
                        (k + id as usize + 1) as u16
                    } else {
                        (k * 911 + 1) as u16
                    }
                }),
                wide: id >= 2,
            },
        )
        .unwrap();
        for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
            let mut counts = [0; 16];
            counts[8] = 255;
            counts[9] = 1;
            let symbols: Vec<_> = (0..=255u8).map(|v| v.wrapping_add(id * 31)).collect();
            bank.set_huffman(class, id, counts, &symbols).unwrap();
        }
    }
    let metadata = [
        MetadataSegment {
            marker: 0xee,
            data: b"Adobe\0\x64\0\0\0\0\0",
            offset: 0,
            preceding_scans: 0,
        },
        MetadataSegment {
            marker: 0xfe,
            data: b"tables",
            offset: 0,
            preceding_scans: 0,
        },
    ];
    let encoded = bank
        .encode_with_metadata(&metadata, Limits::default(), 10000)
        .unwrap();
    let mut trailing = encoded.clone();
    trailing.extend_from_slice(&[3, 4, 5]);
    let mut decoded = Tables::new();
    let info = decoded.load(&trailing, Limits::default()).unwrap();
    assert_eq!(info.consumed, encoded.len());
    assert_eq!(
        info.metadata.iter().map(|v| v.data).collect::<Vec<_>>(),
        metadata.iter().map(|v| v.data).collect::<Vec<_>>()
    );
    for id in 0..4 {
        assert_eq!(decoded.quantization(id), bank.quantization(id));
        for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
            let (a, b) = (
                decoded.huffman(class, id).unwrap(),
                bank.huffman(class, id).unwrap(),
            );
            assert_eq!((a.counts, a.symbols), (b.counts, b.symbols));
        }
    }
    assert_eq!(
        decoded
            .encode_with_metadata(&info.metadata, Limits::default(), encoded.len())
            .unwrap(),
        encoded
    );
    assert!(
        bank.encode_with_metadata(&metadata, Limits::default(), encoded.len() - 1)
            .is_err()
    );
    assert!(
        decoded
            .load(
                &encoded,
                Limits {
                    max_metadata_bytes: 1,
                    ..Limits::default()
                }
            )
            .is_err()
    );
    assert!(
        bank.set_quantization(
            4,
            QuantizationTable {
                values: [1; 64],
                wide: false
            }
        )
        .is_err()
    );
    assert!(
        bank.set_quantization(
            0,
            QuantizationTable {
                values: [256; 64],
                wide: false
            }
        )
        .is_err()
    );
    assert!(
        bank.set_quantization(
            0,
            QuantizationTable {
                values: [0; 64],
                wide: true
            }
        )
        .is_err()
    );
    assert!(bank.set_huffman(HuffmanClass::Ac, 4, [0; 16], &[]).is_err());
    assert!(
        bank.set_huffman(HuffmanClass::Dc, 0, [0; 16], &[1])
            .is_err()
    );
    for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
        bank.set_huffman(class, 3, [0; 16], &[]).unwrap();
    }
    let packet = bank.encode(65536).unwrap();
    let mut restored = Tables::new();
    restored.load(&packet, Limits::default()).unwrap();
    for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
        assert!(restored.huffman(class, 3).unwrap().symbols.is_empty());
    }
    // Empty unused destinations do not invalidate another component's tables.
    let bytes = source();
    let restored_image = restored
        .decode_coefficients(&bytes, Limits::default())
        .unwrap();
    same_dct(
        &restored_image.components,
        &decode_coefficients(&bytes, Limits::default())
            .unwrap()
            .components,
    );
    let (packet, image) = split_initial(&bytes);
    restored.load(&packet, Limits::default()).unwrap();
    restored
        .set_huffman(HuffmanClass::Dc, 0, [0; 16], &[])
        .unwrap();
    assert!(
        restored
            .decode_coefficients(&image, Limits::default())
            .is_err()
    );
    assert!(bank.quantization(4).is_none() && bank.huffman(HuffmanClass::Dc, 4).is_none());
    let empty = Tables::new().encode(4).unwrap();
    assert_eq!(empty, [255, 0xd8, 255, 0xd9]);
    let previous = *decoded.quantization(3).unwrap();
    decoded.load(&empty, Limits::default()).unwrap();
    assert_eq!(decoded.quantization(3), Some(&previous));
    decoded.clear();
    assert!(decoded.is_empty());
}

#[test]
fn external_table_streams_decode_all_existing_processes() {
    let mut cache = Tables::new();
    for path in paths() {
        let bytes = std::fs::read(&path).unwrap();
        let original = read(&bytes, &mut Tables::new());
        let (packet, image) = split_initial(&bytes);
        cache.load(&packet, Limits::default()).unwrap();
        let decoded = read(&image, &mut cache);
        same(&decoded, &original);
        if family(&bytes) == 0 {
            assert!(
                decode_coefficients(&image, Limits::default()).is_err(),
                "{}",
                path.display()
            );
        }
        cache.clear(); // Decoded quantizers/models never borrow cache storage.
        same(&decoded, &original);
    }
}

#[test]
fn direct_abbreviated_encoder_reuses_tables_and_preserves_frame_values() {
    let mut sender = AbbreviatedEncoder::new();
    let mut receiver = Tables::new();
    for path in paths() {
        let bytes = std::fs::read(&path).unwrap();
        let original = read(&bytes, &mut Tables::new());
        for repeat in 0..2 {
            let encoded = match &original {
                Image::Dct(image) => sender.encode_coefficients(
                    image,
                    CodingOptions {
                        entropy_coding: if image.frame.process.is_arithmetic() {
                            EntropyCoding::Arithmetic
                        } else {
                            EntropyCoding::Huffman
                        },
                        progressive: image.frame.process.is_progressive(),
                        successive_approximation: 3,
                        restart_interval: 3,
                        height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                        ..CodingOptions::default()
                    },
                    MetadataOptions::default(),
                ),
                Image::Lossless(image) => sender.encode_lossless(
                    image,
                    LosslessOptions {
                        entropy_coding: if image.frame.process.is_arithmetic() {
                            EntropyCoding::Arithmetic
                        } else {
                            EntropyCoding::Huffman
                        },
                        restart_rows: 1,
                        height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                        ..LosslessOptions::default()
                    },
                    MetadataOptions::default(),
                ),
                Image::Hierarchical(image) => sender.encode_hierarchical(
                    image,
                    HierarchicalOptions::default(),
                    MetadataOptions::default(),
                ),
            }
            .unwrap();
            if repeat == 1 && !matches!(original, Image::Hierarchical(_)) {
                assert_eq!(encoded.tables.len(), 4);
            }
            receiver.load(&encoded.tables, Limits::default()).unwrap();
            let decoded = read(&encoded.image, &mut receiver);
            same(&decoded, &original);
        }
    }
}

#[test]
fn soi_resets_arithmetic_conditioning_restart_intervals_and_metadata() {
    let bytes = source();
    let model = decode_coefficients(&bytes, Limits::default()).unwrap();
    let first = encode_coefficients(
        &model,
        CodingOptions {
            entropy_coding: EntropyCoding::Arithmetic,
            restart_interval: 3,
            arithmetic_conditioning: ArithmeticConditioning {
                dc_lower: 3,
                dc_upper: 8,
                ac_split: 31,
            },
            ..CodingOptions::default()
        },
    )
    .unwrap();
    let defaults = encode_coefficients(
        &model,
        CodingOptions {
            entropy_coding: EntropyCoding::Arithmetic,
            ..CodingOptions::default()
        },
    )
    .unwrap();
    let abbreviated = without(&defaults, |m| matches!(m, 0xdb | 0xcc));
    assert!(!events(&abbreviated).iter().any(|e| e.0 == 0xdd));
    let mut cache = Tables::new();
    cache
        .decode_coefficients(&first, Limits::default())
        .unwrap();
    let result = cache
        .decode_coefficients(&abbreviated, Limits::default())
        .unwrap();
    same_dct(&result.components, &model.components);
    for marker in [0xcc, 0xdd, 0xdc, 0xde, 0xc0] {
        let invalid = [255, 0xd8, 255, marker, 0, 4, 0, 1, 255, 0xd9];
        assert!(cache.load(&invalid, Limits::default()).is_err());
    }
    let metadata = [MetadataSegment {
        marker: 0xfe,
        data: b"table-stream-only",
        offset: 0,
        preceding_scans: 0,
    }];
    let packet = cache
        .encode_with_metadata(&metadata, Limits::default(), 10000)
        .unwrap();
    let info = cache.load(&packet, Limits::default()).unwrap();
    assert_eq!(info.metadata[0].data, metadata[0].data);
    let result = cache
        .decode_coefficients(&abbreviated, Limits::default())
        .unwrap();
    assert!(!result.metadata.iter().any(|m| m.data == metadata[0].data));
}

#[test]
fn destination_three_and_external_redefinition_keep_prior_models_independent() {
    let bytes = source();
    let model = decode_coefficients(&bytes, Limits::default()).unwrap();
    let mut gray = model.clone();
    assert_eq!(gray.components.len(), 1);
    gray.metadata.clear();
    let mut full = encode_coefficients(&gray, CodingOptions::default()).unwrap();
    for (marker, start, _) in events(&full) {
        match marker {
            0xdb | 0xc4 => full[start + 4] = (full[start + 4] & 0xf0) | 3,
            0xc1 => full[start + 12] = 3,
            0xda => full[start + 6] = 0x33,
            _ => {}
        }
    }
    let (packet, image) = split_initial(&full);
    let mut bank = Tables::new();
    bank.load(&packet, Limits::default()).unwrap();
    let first = bank.decode_coefficients(&image, Limits::default()).unwrap();
    let original = first.components[0].quantization;
    assert_eq!(first.frame.components[0].quantization_table, 3);
    let mut update = Tables::new();
    update
        .set_quantization(
            3,
            QuantizationTable {
                values: [2; 64],
                wide: true,
            },
        )
        .unwrap();
    bank.load(&update.encode(1000).unwrap(), Limits::default())
        .unwrap();
    let second = bank.decode_coefficients(&image, Limits::default()).unwrap();
    assert_eq!(second.components[0].quantization, [2; 64]);
    assert_eq!(first.components[0].quantization, original);
    assert_eq!(first.components[0].blocks, second.components[0].blocks);
}

#[test]
fn total_and_table_budgets_fail_without_poisoning_sender_state() {
    let bytes = source();
    let model = decode_coefficients(&bytes, Limits::default()).unwrap();
    let plain = encode_coefficients(&model, CodingOptions::default()).unwrap();
    let first = AbbreviatedEncoder::new()
        .encode_coefficients(&model, CodingOptions::default(), MetadataOptions::default())
        .unwrap();
    assert_eq!(first.tables.len() + first.image.len(), plain.len() + 4);
    assert_eq!(first.image, without(&plain, |m| matches!(m, 0xdb | 0xc4)));
    let total = first.tables.len() + first.image.len();
    let mut sender = AbbreviatedEncoder::new();
    assert!(
        sender
            .encode_coefficients(
                &model,
                CodingOptions {
                    max_output_bytes: total - 1,
                    ..CodingOptions::default()
                },
                MetadataOptions::default()
            )
            .is_err()
    );
    assert!(sender.tables().is_empty());
    sender.max_table_bytes = first.tables.len() - 1;
    assert!(
        sender
            .encode_coefficients(&model, CodingOptions::default(), MetadataOptions::default())
            .is_err()
    );
    assert!(sender.tables().is_empty());
    sender.max_table_bytes = first.tables.len();
    let retry = sender
        .encode_coefficients(
            &model,
            CodingOptions {
                max_output_bytes: total,
                ..CodingOptions::default()
            },
            MetadataOptions::default(),
        )
        .unwrap();
    assert_eq!((retry.tables, retry.image), (first.tables, first.image));
    let hierarchy = paths()
        .into_iter()
        .find(|p| {
            p.parent().unwrap().ends_with("pyramid-native")
                && p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("ycck-dct-huf")
        })
        .unwrap();
    let input = std::fs::read(hierarchy).unwrap();
    let model = decode_hierarchical(&input, Limits::default()).unwrap();
    let output = AbbreviatedEncoder::new()
        .encode_hierarchical(
            &model,
            HierarchicalOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let total = output.tables.len() + output.image.len();
    let mut sender = AbbreviatedEncoder::new();
    assert!(matches!(
        sender.encode_hierarchical(
            &model,
            HierarchicalOptions {
                max_output_bytes: total - 1,
                ..HierarchicalOptions::default()
            },
            MetadataOptions::default()
        ),
        Err(Error::LimitExceeded(_))
    ));
    assert!(sender.tables().is_empty());
    let exact = sender
        .encode_hierarchical(
            &model,
            HierarchicalOptions {
                max_output_bytes: total,
                ..HierarchicalOptions::default()
            },
            MetadataOptions::default(),
        )
        .unwrap();
    assert_eq!(exact.tables.len() + exact.image.len(), total);
}

#[test]
fn malformed_tables_only_streams_and_valid_prefix_state_remain_bounded() {
    let mut bank = Tables::new();
    bank.set_quantization(
        0,
        QuantizationTable {
            values: [7; 64],
            wide: false,
        },
    )
    .unwrap();
    let encoded = bank.encode(1000).unwrap();
    for end in 0..encoded.len() {
        assert!(
            Tables::new()
                .load(&encoded[..end], Limits::default())
                .is_err()
        );
    }
    let mut invalid = encoded[..encoded.len() - 2].to_vec();
    invalid.extend_from_slice(&[255, 0xc4, 0, 3, 0, 255, 0xd9]);
    let mut updated = Tables::new();
    assert!(updated.load(&invalid, Limits::default()).is_err());
    assert_eq!(updated.quantization(0).unwrap().values, [7; 64]);
    let mut state = 0x12345678u32;
    for _ in 0..1000 {
        let mut changed = encoded.clone();
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let at = state as usize % changed.len();
        changed[at] ^= (state >> 24) as u8 | 1;
        let mut cache = Tables::new();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cache.load(
                &changed,
                Limits {
                    max_metadata_bytes: 1024,
                    max_memory_bytes: 4096,
                    ..Limits::default()
                }
            )))
            .is_ok()
        );
    }
}

#[test]
fn native_shared_and_pyramid_sources_preserve_metadata_and_sample_precision() {
    use deadlib_image::{ColorType, Image, Samples};
    use deadlib_image_jpeg::{
        ColorSpace, ComponentPlane, ComponentSpec, EncodeInput, EncodeOptions, Expansion,
        HierarchyHeader, IccWrite, PyramidLevel, PyramidOptions, PyramidRasterOptions, SampleData,
        Subsampling,
    };
    let profile = vec![71; 70000];
    let metadata = MetadataOptions {
        icc: IccWrite::Replace(&profile),
        ..MetadataOptions::default()
    };
    let mut sender = AbbreviatedEncoder::new();
    let mut receiver = Tables::new();
    for p in [8, 16] {
        let values: Vec<u16> = (0u32..7 * 5 * 3)
            .map(|v| ((v * 7319 + v * v * 117) & ((1u32 << p) - 1)) as u16)
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
        let input = EncodeInput {
            width: 7,
            height: 5,
            precision: p,
            color_space: ColorSpace::Rgb,
            samples: SampleData::U16(&values),
        };
        let options = EncodeOptions {
            precision: if p == 8 { 8 } else { 12 },
            rgb_to_ycbcr: false,
            subsampling: Subsampling::S444,
            ..EncodeOptions::default()
        };
        let expected = deadlib_image_jpeg::quantize(&input, options).unwrap();
        for output in [
            sender.encode_image(&image, options, metadata).unwrap(),
            sender.encode_samples(&input, options, metadata).unwrap(),
        ] {
            receiver.load(&output.tables, Limits::default()).unwrap();
            let decoded = receiver
                .decode_coefficients(&output.image, Limits::default())
                .unwrap();
            same_dct(&decoded.components, &expected.components);
            assert_eq!(
                decoded
                    .icc_profile()
                    .unwrap()
                    .unwrap()
                    .to_vec(profile.len())
                    .unwrap(),
                profile
            );
        }
        for output in [
            sender
                .encode_lossless_image(&image, LosslessOptions::default(), metadata)
                .unwrap(),
            sender
                .encode_lossless_samples(&input, LosslessOptions::default(), metadata)
                .unwrap(),
        ] {
            receiver.load(&output.tables, Limits::default()).unwrap();
            let decoded = receiver
                .decode_lossless(&output.image, Limits::default())
                .unwrap();
            for (i, c) in decoded.components.iter().enumerate() {
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
            assert_eq!(
                decoded
                    .icc_profile()
                    .unwrap()
                    .unwrap()
                    .to_vec(profile.len())
                    .unwrap(),
                profile
            );
        }
        let coding = LosslessOptions {
            entropy_coding: EntropyCoding::Arithmetic,
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
        let options = PyramidOptions {
            omit_dhp_height: true,
            ..PyramidOptions::new(&levels)
        };
        let specs: Vec<_> = b"RGB"
            .iter()
            .map(|&id| ComponentSpec {
                id,
                horizontal_sampling: 1,
                vertical_sampling: 1,
                quantization_table: 0,
            })
            .collect();
        let components: Vec<_> = specs
            .iter()
            .enumerate()
            .map(|(i, &spec)| ComponentPlane {
                spec,
                width: 7,
                height: 5,
                samples: values.iter().skip(i).step_by(3).copied().collect(),
            })
            .collect();
        let header = HierarchyHeader {
            width: 7,
            height: 5,
            precision: p,
            components: specs,
        };
        for output in [
            sender
                .encode_pyramid_samples(&input, PyramidRasterOptions::default(), options, metadata)
                .unwrap(),
            sender
                .encode_pyramid_image(&image, PyramidRasterOptions::default(), options, metadata)
                .unwrap(),
            sender
                .encode_pyramid_planes(header, &components, vec![], options, metadata)
                .unwrap(),
        ] {
            receiver.load(&output.tables, Limits::default()).unwrap();
            let decoded = receiver
                .decode_hierarchical(&output.image, Limits::default())
                .unwrap();
            assert_eq!((decoded.header.height, decoded.scans), (5, 2));
            same_planes(&decoded.reconstruct_reference().unwrap(), &components);
            assert_eq!(
                decoded
                    .icc_profile()
                    .unwrap()
                    .unwrap()
                    .to_vec(profile.len())
                    .unwrap(),
                profile
            );
        }
    }
}
