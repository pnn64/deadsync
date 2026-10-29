use deadlib_image_jpeg::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
#[allow(dead_code)]
#[path = "support/components.rs"]
mod support;

#[derive(Default)]
struct Destination {
    bytes: Vec<u8>,
    short: usize,
    interrupt: usize,
    fail_at: Option<usize>,
    calls: usize,
    flushes: usize,
    zero: bool,
}
#[derive(Debug)]
struct DestinationError;
impl std::fmt::Display for DestinationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("destination sentinel")
    }
}
impl std::error::Error for DestinationError {}
impl Write for Destination {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.interrupt != 0 && self.calls.is_multiple_of(self.interrupt) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.fail_at.is_some_and(|n| self.bytes.len() >= n) {
            return Err(io::Error::other(DestinationError));
        }
        if self.zero {
            return Ok(0);
        }
        let mut n = bytes.len();
        if self.short != 0 {
            n = n.min(self.short);
        }
        if let Some(end) = self.fail_at {
            n = n.min(end - self.bytes.len());
        }
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

fn markers(bytes: &[u8]) -> Vec<Process> {
    MarkerReader::new(bytes)
        .unwrap()
        .filter_map(|event| match event.unwrap() {
            Event::Segment { marker, .. } => Process::try_from(marker).ok(),
            _ => None,
        })
        .collect()
}

fn representatives() -> Vec<(bool, Vec<u8>)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut sources = BTreeMap::new();
    for group in [
        "sequential",
        "progressive",
        "arithmetic",
        "arithmetic-progressive",
        "lossless",
        "lossless-arithmetic",
        "hierarchical",
    ] {
        let mut paths: Vec<_> = std::fs::read_dir(root.join(group))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
            .collect();
        paths.sort();
        for path in paths {
            let bytes = std::fs::read(&path).unwrap();
            let mut key = None;
            for event in MarkerReader::new(&bytes).unwrap() {
                if let Event::Segment { marker, data, .. } = event.unwrap()
                    && let Ok(process) = Process::try_from(marker)
                {
                    key = Some((process as u8, data[0]));
                }
            }
            if let Some(key) = key {
                sources
                    .entry(key)
                    .or_insert((group == "hierarchical", bytes));
            }
        }
    }
    sources.into_values().collect()
}

#[test]
fn all_thirteen_processes_match_vec_output_with_short_writes() {
    let mut seen = BTreeSet::new();
    let mut cases = 0;
    for (hierarchical, bytes) in representatives() {
        let process = *markers(&bytes).last().unwrap();
        let entropy_coding = if process.is_arithmetic() {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        };
        let coding = CodingOptions {
            entropy_coding,
            progressive: process.is_progressive(),
            successive_approximation: 3,
            restart_interval: 1,
            height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
            ..CodingOptions::default()
        };
        let lossless = LosslessOptions {
            entropy_coding,
            predictor: 7,
            restart_rows: 1,
            height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
            ..LosslessOptions::default()
        };
        let dct = (!hierarchical && !process.is_lossless())
            .then(|| decode_coefficients(&bytes, Limits::default()).unwrap());
        let lp = (!hierarchical && process.is_lossless())
            .then(|| decode_lossless(&bytes, Limits::default()).unwrap());
        let hierarchy =
            hierarchical.then(|| decode_hierarchical(&bytes, Limits::default()).unwrap());
        let expected = if let Some(image) = &dct {
            encode_coefficients(image, coding).unwrap()
        } else if let Some(image) = &lp {
            image.encode(lossless).unwrap()
        } else {
            hierarchy
                .as_ref()
                .unwrap()
                .encode(HierarchicalOptions::default())
                .unwrap()
        };
        for process in markers(&expected) {
            seen.insert(process as u8);
        }
        for buffer in [0, 1, 17, 8192] {
            let mut encoder = StreamEncoder::with_buffer_size(
                Destination {
                    short: 3,
                    interrupt: 7,
                    ..Destination::default()
                },
                buffer,
            );
            let count = if let Some(image) = &dct {
                encoder.encode_coefficients(image, coding, MetadataOptions::default())
            } else if let Some(image) = &lp {
                encoder.encode_lossless(image, lossless, MetadataOptions::default())
            } else {
                encoder.encode_hierarchical(
                    hierarchy.as_ref().unwrap(),
                    HierarchicalOptions::default(),
                    MetadataOptions::default(),
                )
            }
            .unwrap();
            assert_eq!(count, expected.len());
            assert_eq!(encoder.get_ref().bytes, expected);
            assert_eq!(encoder.get_ref().flushes, 0);
            assert!(encoder.buffer_capacity() <= buffer);
            cases += 1;
            // Fail in the header, entropy/frame body, and final drain for every
            // represented process/precision. Preserve the original I/O payload.
            for end in [0, expected.len() / 2, expected.len() - 1] {
                encoder.get_mut().bytes.clear();
                encoder.get_mut().fail_at = Some(end);
                let result = if let Some(image) = &dct {
                    encoder.encode_coefficients(image, coding, MetadataOptions::default())
                } else if let Some(image) = &lp {
                    encoder.encode_lossless(image, lossless, MetadataOptions::default())
                } else {
                    encoder.encode_hierarchical(
                        hierarchy.as_ref().unwrap(),
                        HierarchicalOptions::default(),
                        MetadataOptions::default(),
                    )
                };
                let Err(WriteError::Io(error)) = result else {
                    panic!("expected writer failure at {end}");
                };
                assert!(error.get_ref().unwrap().is::<DestinationError>());
                assert_eq!(encoder.get_ref().bytes, expected[..end]);
                assert_eq!(encoder.get_ref().flushes, 0);
            }
        }
    }
    assert_eq!(
        seen,
        [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf
        ]
        .into_iter()
        .collect()
    );
    eprintln!("Streaming: {cases} process/precision/buffer comparisons");
}

#[test]
fn large_component_quantizer_changes_and_metadata_cross_drains() {
    let payload = vec![0x39; 60_000];
    let extra = [MetadataSegment {
        marker: 0xe7,
        data: &payload,
        offset: 0,
        preceding_scans: 0,
    }];
    let profile = vec![0xa7; 70_000];
    let metadata = MetadataOptions {
        additional_segments: &extra,
        icc: IccWrite::Replace(&profile),
    };
    for precision in [8, 12] {
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for layout in [0, 1, 3] {
                let image = support::model(255, precision, layout, 3);
                let options = CodingOptions {
                    entropy_coding,
                    restart_interval: 3,
                    height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                    ..CodingOptions::default()
                };
                let expected =
                    encode_coefficients_with_metadata(&image, options, metadata).unwrap();
                let mut encoder = StreamEncoder::with_buffer_size(
                    Destination {
                        short: 7,
                        interrupt: 11,
                        ..Destination::default()
                    },
                    31,
                );
                assert_eq!(
                    encoder
                        .encode_coefficients(&image, options, metadata)
                        .unwrap(),
                    expected.len()
                );
                assert_eq!(encoder.get_ref().bytes, expected);
                let decoded =
                    decode_coefficients(&encoder.get_ref().bytes, Limits::default()).unwrap();
                support::same(&image, &decoded);
                assert_eq!(decoded.icc_profile().unwrap().unwrap().len(), profile.len());
            }
        }
    }
}

#[test]
fn every_write_failure_and_byte_budget_keeps_a_valid_prefix() {
    let image = support::model(1, 8, 0, 0);
    let options = CodingOptions::default();
    let expected = encode_coefficients(&image, options).unwrap();
    for buffer in [1, 17, 8192] {
        for end in 0..expected.len() {
            let mut encoder = StreamEncoder::with_buffer_size(
                Destination {
                    short: 5,
                    interrupt: 7,
                    fail_at: Some(end),
                    ..Destination::default()
                },
                buffer,
            );
            let error = encoder
                .encode_coefficients(&image, options, MetadataOptions::default())
                .unwrap_err();
            let WriteError::Io(error) = error else {
                panic!("expected original I/O error");
            };
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert!(
                error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<DestinationError>()
                    .is_some()
            );
            assert_eq!(encoder.get_ref().bytes, expected[..end]);
            assert_eq!(encoder.get_ref().flushes, 0);
            encoder.get_mut().bytes.clear();
            encoder.get_mut().fail_at = None;
            encoder
                .encode_coefficients(&image, options, MetadataOptions::default())
                .unwrap();
            assert_eq!(encoder.into_inner().bytes, expected);
        }
        for limit in 0..expected.len() {
            let mut encoder = StreamEncoder::with_buffer_size(Destination::default(), buffer);
            let error = encoder
                .encode_coefficients(
                    &image,
                    CodingOptions {
                        max_output_bytes: limit,
                        ..options
                    },
                    MetadataOptions::default(),
                )
                .unwrap_err();
            assert!(matches!(
                error,
                WriteError::Codec(Error::LimitExceeded("encoded bytes"))
            ));
            assert!(encoder.get_ref().bytes.len() <= limit);
            assert!(expected.starts_with(&encoder.get_ref().bytes));
            assert_eq!(encoder.get_ref().flushes, 0);
        }
    }
    let mut encoder = StreamEncoder::new(Destination {
        zero: true,
        ..Destination::default()
    });
    assert!(
        matches!(encoder.encode_coefficients(&image, options, MetadataOptions::default()), Err(WriteError::Io(e)) if e.kind() == io::ErrorKind::WriteZero)
    );
    let mut invalid = support::model(1, 8, 0, 0);
    invalid.components[0].blocks[0][1] = i16::MAX;
    let mut encoder = StreamEncoder::new(Destination::default());
    assert!(matches!(
        encoder.encode_coefficients(&invalid, options, MetadataOptions::default()),
        Err(WriteError::Codec(_))
    ));
    assert!(encoder.into_inner().bytes.is_empty());
}

struct DigestWriter {
    bytes: usize,
    hash: u64,
}
impl Write for DigestWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        for &byte in data {
            self.hash = (self.hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        self.bytes += data.len();
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("encoder must not flush the destination")
    }
}
fn dense(limits: Limits) -> CoefficientImage<'static> {
    let spec = ComponentSpec {
        id: 255,
        horizontal_sampling: 1,
        vertical_sampling: 1,
        quantization_table: 0,
    };
    let mut block = [1023; 64];
    block[0] = 0;
    CoefficientImage::from_components(
        FrameHeader {
            process: Process::Baseline,
            precision: 8,
            width: 256,
            height: 256,
            components: vec![spec],
        },
        vec![CoefficientComponent {
            spec,
            width: 256,
            height: 256,
            blocks_wide: 32,
            blocks_high: 32,
            quantization: [1; 64],
            successive_approximation: [0; 64],
            blocks: vec![block; 1024],
        }],
        vec![],
        limits,
    )
    .unwrap()
}

#[test]
fn encoded_size_is_independent_of_staging_memory() {
    let original = dense(Limits::default());
    let memory = original.components.capacity() * std::mem::size_of::<CoefficientComponent>()
        + original.frame.components.capacity() * std::mem::size_of::<ComponentSpec>()
        + original.components[0].blocks.capacity() * 128;
    let image = dense(Limits {
        max_memory_bytes: memory + 1024 + 64,
        ..Limits::default()
    });
    let options = CodingOptions {
        optimize_huffman: false,
        ..CodingOptions::default()
    };
    assert!(matches!(
        encode_coefficients(&image, options),
        Err(Error::LimitExceeded("encoded bytes"))
    ));
    let expected = encode_coefficients(&original, options).unwrap();
    let mut encoder = StreamEncoder::with_buffer_size(
        DigestWriter {
            bytes: 0,
            hash: 0xcbf29ce484222325,
        },
        8192,
    );
    encoder
        .encode_coefficients(&original, options, MetadataOptions::default())
        .unwrap();
    assert_eq!(encoder.buffer_capacity(), 8192);
    encoder.get_mut().bytes = 0;
    encoder.get_mut().hash = 0xcbf29ce484222325;
    assert_eq!(
        encoder
            .encode_coefficients(&image, options, MetadataOptions::default())
            .unwrap(),
        expected.len()
    );
    assert!(expected.len() > memory + 1024 + 64);
    assert_eq!(encoder.buffer_capacity(), 64);
    let mut digest = DigestWriter {
        bytes: 0,
        hash: 0xcbf29ce484222325,
    };
    digest.write_all(&expected).unwrap();
    assert_eq!(encoder.get_ref().hash, digest.hash);
    assert_eq!(encoder.get_ref().bytes, digest.bytes);
}

fn appended(encoder: &StreamEncoder<Vec<u8>>, start: &mut usize, expected: &[u8], count: usize) {
    assert_eq!(count, expected.len());
    assert_eq!(&encoder.get_ref()[*start..], expected);
    *start += count;
}

#[test]
fn raster_pyramid_and_table_entry_points_share_the_streaming_path() {
    let extra = [MetadataSegment {
        marker: 0xe7,
        data: b"opaque",
        offset: 0,
        preceding_scans: 0,
    }];
    let profile = vec![0x61; 70_000];
    let metadata = MetadataOptions {
        additional_segments: &extra,
        icc: IccWrite::Replace(&profile),
    };
    let mut encoder = StreamEncoder::with_buffer_size(Vec::new(), 7);
    let mut start = 0;
    for precision in [8, 12] {
        let values: Vec<u16> = (0..17 * 19 * 3)
            .map(|i| ((i * 193 + i * i * 17) & ((1 << precision) - 1)) as u16)
            .collect();
        let input = EncodeInput {
            width: 17,
            height: 19,
            precision,
            color_space: ColorSpace::Rgb,
            samples: SampleData::U16(&values),
        };
        let shared = deadlib_image::Image {
            width: 17,
            height: 19,
            color_type: deadlib_image::ColorType::Rgb,
            bit_depth: 16,
            samples: deadlib_image::Samples::U16(values.clone()),
        };
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            let coding = CodingOptions {
                entropy_coding,
                progressive: true,
                successive_approximation: 2,
                restart_interval: 1,
                ..CodingOptions::default()
            };
            let options = EncodeOptions {
                quality: 83,
                precision,
                subsampling: Subsampling::S444,
                coding,
                ..EncodeOptions::default()
            };
            let expected = encode_samples_with_metadata(&input, options, metadata).unwrap();
            let count = encoder.encode_samples(&input, options, metadata).unwrap();
            appended(&encoder, &mut start, &expected, count);
            let expected = encode_with_metadata(&shared, options, metadata).unwrap();
            let count = encoder.encode_image(&shared, options, metadata).unwrap();
            appended(&encoder, &mut start, &expected, count);
            let lossless = LosslessOptions {
                entropy_coding,
                predictor: 4,
                restart_rows: 2,
                ..LosslessOptions::default()
            };
            let expected =
                encode_lossless_samples_with_metadata(&input, lossless, metadata).unwrap();
            let count = encoder
                .encode_lossless_samples(&input, lossless, metadata)
                .unwrap();
            appended(&encoder, &mut start, &expected, count);
            let expected = encode_lossless_with_metadata(&shared, lossless, metadata).unwrap();
            let count = encoder
                .encode_lossless_image(&shared, lossless, metadata)
                .unwrap();
            appended(&encoder, &mut start, &expected, count);
            let none = Expansion {
                horizontal: false,
                vertical: false,
            };
            let both = Expansion {
                horizontal: true,
                vertical: true,
            };
            let levels = [
                PyramidLevel::dct(none, 75, coding),
                PyramidLevel::dct(both, 90, coding),
                PyramidLevel::lossless(none, lossless),
            ];
            let pyramid = PyramidOptions::new(&levels);
            let raster = PyramidRasterOptions {
                precision,
                ..PyramidRasterOptions::default()
            };
            let expected =
                encode_hierarchical_samples_with_metadata(&input, raster, pyramid, metadata)
                    .unwrap();
            let count = encoder
                .encode_pyramid_samples(&input, raster, pyramid, metadata)
                .unwrap();
            appended(&encoder, &mut start, &expected, count);
            let expected =
                encode_hierarchical_image_with_metadata(&shared, raster, pyramid, metadata)
                    .unwrap();
            let count = encoder
                .encode_pyramid_image(&shared, raster, pyramid, metadata)
                .unwrap();
            appended(&encoder, &mut start, &expected, count);
            let specs: Vec<_> = b"RGB"
                .iter()
                .map(|&id| ComponentSpec {
                    id,
                    horizontal_sampling: 1,
                    vertical_sampling: 1,
                    quantization_table: 0,
                })
                .collect();
            let header = HierarchyHeader {
                width: 17,
                height: 19,
                precision,
                components: specs.clone(),
            };
            let planes: Vec<_> = specs
                .iter()
                .enumerate()
                .map(|(i, &spec)| ComponentPlane {
                    spec,
                    width: 17,
                    height: 19,
                    samples: values.iter().skip(i).step_by(3).copied().collect(),
                })
                .collect();
            let expected = encode_hierarchical_planes_with_metadata(
                header.clone(),
                &planes,
                vec![],
                pyramid,
                metadata,
            )
            .unwrap();
            let count = encoder
                .encode_pyramid_planes(header, &planes, vec![], pyramid, metadata)
                .unwrap();
            appended(&encoder, &mut start, &expected, count);
        }
    }
    let mut tables = Tables::new();
    tables
        .load(include_bytes!("fixtures/t83/B1.JPG"), Limits::default())
        .unwrap();
    let expected = tables
        .encode_with_metadata(&extra, Limits::default(), 4096)
        .unwrap();
    let count = encoder
        .encode_tables(&tables, &extra, Limits::default(), 4096)
        .unwrap();
    appended(&encoder, &mut start, &expected, count);
    assert_eq!(encoder.into_inner().len(), start);
}

fn pair_equal(
    encoder: &mut AbbreviatedStreamEncoder<Destination, Destination>,
    expected: &AbbreviatedImage,
    count: AbbreviatedBytes,
) {
    assert_eq!(
        (count.tables, count.image),
        (expected.tables.len(), expected.image.len())
    );
    let (tables, image) = encoder.get_mut();
    assert_eq!(std::mem::take(&mut tables.bytes), expected.tables);
    assert_eq!(std::mem::take(&mut image.bytes), expected.image);
    assert_eq!((tables.flushes, image.flushes), (0, 0));
}

fn pair_encoder(buffer: usize) -> AbbreviatedStreamEncoder<Destination, Destination> {
    let destination = || Destination {
        short: 3,
        interrupt: 7,
        ..Destination::default()
    };
    AbbreviatedStreamEncoder::with_buffer_size(destination(), destination(), buffer)
}

#[test]
fn paired_all_processes_reuse_destinations_and_match_vector_sessions() {
    let mut seen = BTreeSet::new();
    let mut cases = 0;
    for buffer in [0, 1, 31, 8192] {
        let mut expected = AbbreviatedEncoder::new();
        let mut encoder = pair_encoder(buffer);
        let mut receiver = Tables::new();
        for (hierarchical, bytes) in representatives() {
            let process = *markers(&bytes).last().unwrap();
            let entropy_coding = if process.is_arithmetic() {
                EntropyCoding::Arithmetic
            } else {
                EntropyCoding::Huffman
            };
            let coding = CodingOptions {
                entropy_coding,
                progressive: process.is_progressive(),
                successive_approximation: 3,
                restart_interval: 1,
                height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                ..CodingOptions::default()
            };
            let lossless = LosslessOptions {
                entropy_coding,
                predictor: 7,
                restart_rows: 1,
                height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                ..LosslessOptions::default()
            };
            // Both senders and the receiver retain their cache across the whole
            // mixed-family session, including repeated images and precisions.
            for _ in 0..2 {
                let (pair, count) = if hierarchical {
                    let source = decode_hierarchical(&bytes, Limits::default()).unwrap();
                    let pair = expected
                        .encode_hierarchical(
                            &source,
                            HierarchicalOptions::default(),
                            MetadataOptions::default(),
                        )
                        .unwrap();
                    let count = encoder
                        .encode_hierarchical(
                            &source,
                            HierarchicalOptions::default(),
                            MetadataOptions::default(),
                        )
                        .unwrap();
                    receiver.load(&pair.tables, Limits::default()).unwrap();
                    let decoded = receiver
                        .decode_hierarchical(&pair.image, Limits::default())
                        .unwrap();
                    assert_eq!(decoded.frames.len(), source.frames.len());
                    for (a, b) in source.frames.iter().zip(&decoded.frames) {
                        match (&a.data, &b.data) {
                            (HierarchicalData::Dct(a), HierarchicalData::Dct(b)) => {
                                for (a, b) in a.iter().zip(b) {
                                    assert_eq!(a.blocks, b.blocks);
                                    assert_eq!(a.quantization, b.quantization);
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
                            _ => panic!("changed native data family"),
                        }
                    }
                    (pair, count)
                } else if process.is_lossless() {
                    let source = decode_lossless(&bytes, Limits::default()).unwrap();
                    let pair = expected
                        .encode_lossless(&source, lossless, MetadataOptions::default())
                        .unwrap();
                    let count = encoder
                        .encode_lossless(&source, lossless, MetadataOptions::default())
                        .unwrap();
                    receiver.load(&pair.tables, Limits::default()).unwrap();
                    let decoded = receiver
                        .decode_lossless(&pair.image, Limits::default())
                        .unwrap();
                    for (a, b) in source.components.iter().zip(&decoded.components) {
                        assert_eq!(a.samples, b.samples);
                    }
                    (pair, count)
                } else {
                    let source = decode_coefficients(&bytes, Limits::default()).unwrap();
                    let pair = expected
                        .encode_coefficients(&source, coding, MetadataOptions::default())
                        .unwrap();
                    let count = encoder
                        .encode_coefficients(&source, coding, MetadataOptions::default())
                        .unwrap();
                    receiver.load(&pair.tables, Limits::default()).unwrap();
                    support::same(
                        &source,
                        &receiver
                            .decode_coefficients(&pair.image, Limits::default())
                            .unwrap(),
                    );
                    (pair, count)
                };
                for process in markers(&pair.image) {
                    seen.insert(process as u8);
                }
                pair_equal(&mut encoder, &pair, count);
                assert!(encoder.buffer_capacity() <= buffer);
                cases += 1;
            }
        }
    }
    assert_eq!(
        seen,
        [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(cases, 520);
}

fn pair_prefixes(
    encoder: &AbbreviatedStreamEncoder<Destination, Destination>,
    expected: &AbbreviatedImage,
) {
    let (tables, image) = encoder.get_ref();
    assert!(expected.tables.starts_with(&tables.bytes));
    assert!(expected.image.starts_with(&image.bytes));
    assert_eq!((tables.flushes, image.flushes), (0, 0));
}

#[test]
fn paired_failure_offsets_total_and_table_ceilings_clear_cache_for_retry() {
    let source = support::model(1, 8, 0, 0);
    let options = CodingOptions::default();
    let metadata = MetadataOptions::default();
    let expected = AbbreviatedEncoder::new()
        .encode_coefficients(&source, options, metadata)
        .unwrap();
    for buffer in [0, 1, 17, 8192] {
        for table in [true, false] {
            let size = if table {
                expected.tables.len()
            } else {
                expected.image.len()
            };
            for end in 0..size {
                let mut encoder = pair_encoder(buffer);
                let (tables, image) = encoder.get_mut();
                if table {
                    tables.fail_at = Some(end);
                } else {
                    image.fail_at = Some(end);
                }
                let error = encoder
                    .encode_coefficients(&source, options, metadata)
                    .unwrap_err();
                let error = match error {
                    AbbreviatedWriteError::Tables(error) if table => error,
                    AbbreviatedWriteError::Image(error) if !table => error,
                    error => panic!("wrong destination: {error}"),
                };
                assert!(error.get_ref().unwrap().is::<DestinationError>());
                assert!(encoder.tables().is_empty());
                pair_prefixes(&encoder, &expected);
                assert_eq!(
                    if table {
                        encoder.get_ref().0.bytes.len()
                    } else {
                        encoder.get_ref().1.bytes.len()
                    },
                    end
                );
                let (tables, image) = encoder.get_mut();
                tables.bytes.clear();
                image.bytes.clear();
                tables.fail_at = None;
                image.fail_at = None;
                let count = encoder
                    .encode_coefficients(&source, options, metadata)
                    .unwrap();
                pair_equal(&mut encoder, &expected, count);
            }
        }
        let total = expected.tables.len() + expected.image.len();
        for limit in 0..=total {
            let mut encoder = pair_encoder(buffer);
            let result = encoder.encode_coefficients(
                &source,
                CodingOptions {
                    max_output_bytes: limit,
                    ..options
                },
                metadata,
            );
            if limit == total {
                pair_equal(&mut encoder, &expected, result.unwrap());
            } else {
                assert!(matches!(
                    result,
                    Err(AbbreviatedWriteError::Codec(Error::LimitExceeded(_)))
                ));
                assert!(encoder.tables().is_empty());
                pair_prefixes(&encoder, &expected);
                assert!(encoder.get_ref().0.bytes.len() + encoder.get_ref().1.bytes.len() <= limit);
            }
        }
        for limit in 0..expected.tables.len() {
            let mut encoder = pair_encoder(buffer);
            encoder.max_table_bytes = limit;
            assert!(matches!(
                encoder.encode_coefficients(&source, options, metadata),
                Err(AbbreviatedWriteError::Codec(Error::LimitExceeded(_)))
            ));
            assert!(encoder.get_ref().0.bytes.len() <= limit);
            assert!(encoder.tables().is_empty());
            pair_prefixes(&encoder, &expected);
        }
    }
    for table in [true, false] {
        let mut encoder = pair_encoder(8192);
        if table {
            encoder.get_mut().0.zero = true;
        } else {
            encoder.get_mut().1.zero = true;
        }
        let error = encoder
            .encode_coefficients(&source, options, metadata)
            .unwrap_err();
        let error = match error {
            AbbreviatedWriteError::Tables(e) if table => e,
            AbbreviatedWriteError::Image(e) if !table => e,
            e => panic!("wrong destination: {e}"),
        };
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert!(encoder.tables().is_empty());
    }
}

#[test]
fn paired_many_components_metadata_and_digest_only_tiny_staging() {
    let profile = vec![0x37; 70000];
    let payload = vec![0x39; 60000];
    let extra = [MetadataSegment {
        marker: 0xe7,
        data: &payload,
        offset: 0,
        preceding_scans: 0,
    }];
    let metadata = MetadataOptions {
        additional_segments: &extra,
        icc: IccWrite::Replace(&profile),
    };
    let mut expected = AbbreviatedEncoder::new();
    let mut encoder = pair_encoder(31);
    let mut receiver = Tables::new();
    for precision in [8, 12] {
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for layout in [0, 1, 3] {
                let source = support::model(255, precision, layout, 3);
                let options = CodingOptions {
                    entropy_coding,
                    restart_interval: 3,
                    height_declaration: HeightDeclaration::Dnl { initial_height: 0 },
                    ..CodingOptions::default()
                };
                for _ in 0..2 {
                    let pair = expected
                        .encode_coefficients(&source, options, metadata)
                        .unwrap();
                    let count = encoder
                        .encode_coefficients(&source, options, metadata)
                        .unwrap();
                    receiver.load(&pair.tables, Limits::default()).unwrap();
                    let decoded = receiver
                        .decode_coefficients(&pair.image, Limits::default())
                        .unwrap();
                    support::same(&source, &decoded);
                    assert_eq!(decoded.icc_profile().unwrap().unwrap().len(), profile.len());
                    pair_equal(&mut encoder, &pair, count);
                }
            }
        }
    }
    let source = dense(Limits::default());
    let memory = source.components.capacity() * std::mem::size_of::<CoefficientComponent>()
        + source.frame.components.capacity() * std::mem::size_of::<ComponentSpec>()
        + source.components[0].blocks.capacity() * 128;
    let limited = dense(Limits {
        max_memory_bytes: memory + 1024 + 64,
        ..Limits::default()
    });
    let options = CodingOptions {
        optimize_huffman: false,
        ..CodingOptions::default()
    };
    let expected = AbbreviatedEncoder::new()
        .encode_coefficients(&source, options, MetadataOptions::default())
        .unwrap();
    let digest = || DigestWriter {
        bytes: 0,
        hash: 0xcbf29ce484222325,
    };
    let mut encoder = AbbreviatedStreamEncoder::new(digest(), digest());
    let count = encoder
        .encode_coefficients(&limited, options, MetadataOptions::default())
        .unwrap();
    assert_eq!(encoder.buffer_capacity(), 64);
    assert!(count.image > memory + 1024 + 64);
    assert_eq!(
        (count.tables, count.image),
        (expected.tables.len(), expected.image.len())
    );
    for (actual, bytes) in [
        (encoder.get_ref().0, &expected.tables),
        (encoder.get_ref().1, &expected.image),
    ] {
        let mut expected = digest();
        expected.write_all(bytes).unwrap();
        assert_eq!((actual.bytes, actual.hash), (expected.bytes, expected.hash));
    }
}

#[test]
fn paired_raster_pyramid_conveniences_parts_and_preparation_failures() {
    let profile = vec![0x37; 70000];
    let metadata = MetadataOptions {
        icc: IccWrite::Replace(&profile),
        ..MetadataOptions::default()
    };
    let mut expected = AbbreviatedEncoder::new();
    let mut encoder = pair_encoder(7);
    macro_rules! compare {
        ($method:ident($($arg:expr),* $(,)?))=>{{
            let pair=expected.$method($($arg),*,metadata).unwrap();
            let count=encoder.$method($($arg),*,metadata).unwrap();
            pair_equal(&mut encoder,&pair,count);
        }};
    }
    for precision in [8, 12] {
        let values: Vec<u16> = (0..17 * 19 * 3)
            .map(|i| ((i * 193 + i * i * 17) & ((1 << precision) - 1)) as u16)
            .collect();
        let input = EncodeInput {
            width: 17,
            height: 19,
            precision,
            color_space: ColorSpace::Rgb,
            samples: SampleData::U16(&values),
        };
        let shared = deadlib_image::Image {
            width: 17,
            height: 19,
            color_type: deadlib_image::ColorType::Rgb,
            bit_depth: 16,
            samples: deadlib_image::Samples::U16(values.clone()),
        };
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            let coding = CodingOptions {
                entropy_coding,
                progressive: true,
                successive_approximation: 2,
                restart_interval: 1,
                ..CodingOptions::default()
            };
            let options = EncodeOptions {
                quality: 83,
                precision,
                subsampling: Subsampling::S444,
                coding,
                ..EncodeOptions::default()
            };
            compare!(encode_samples(&input, options));
            compare!(encode_image(&shared, options));
            let lossless = LosslessOptions {
                entropy_coding,
                predictor: 4,
                restart_rows: 2,
                ..LosslessOptions::default()
            };
            compare!(encode_lossless_samples(&input, lossless));
            compare!(encode_lossless_image(&shared, lossless));
            let none = Expansion {
                horizontal: false,
                vertical: false,
            };
            let both = Expansion {
                horizontal: true,
                vertical: true,
            };
            let levels = [
                PyramidLevel::dct(none, 75, coding),
                PyramidLevel::dct(both, 90, coding),
                PyramidLevel::lossless(none, lossless),
            ];
            let pyramid = PyramidOptions::new(&levels);
            let raster = PyramidRasterOptions {
                precision,
                ..PyramidRasterOptions::default()
            };
            compare!(encode_pyramid_samples(&input, raster, pyramid));
            compare!(encode_pyramid_image(&shared, raster, pyramid));
            let specs: Vec<_> = b"RGB"
                .iter()
                .map(|&id| ComponentSpec {
                    id,
                    horizontal_sampling: 1,
                    vertical_sampling: 1,
                    quantization_table: 0,
                })
                .collect();
            let header = HierarchyHeader {
                width: 17,
                height: 19,
                precision,
                components: specs.clone(),
            };
            let planes: Vec<_> = specs
                .iter()
                .enumerate()
                .map(|(i, &spec)| ComponentPlane {
                    spec,
                    width: 17,
                    height: 19,
                    samples: values.iter().skip(i).step_by(3).copied().collect(),
                })
                .collect();
            compare!(encode_pyramid_planes(
                header.clone(),
                &planes,
                vec![],
                pyramid
            ));
        }
    }
    let (cache, tables, image) = encoder.into_parts();
    assert!(!cache.is_empty());
    assert_eq!(image.buffer_capacity(), 7);
    expected = AbbreviatedEncoder::with_tables(cache.clone());
    encoder = AbbreviatedStreamEncoder::from_parts(cache, tables, image);
    let source = support::model(1, 8, 0, 0);
    compare!(encode_coefficients(&source, CodingOptions::default()));
    let before = (
        encoder.get_ref().0.bytes.len(),
        encoder.get_ref().1.bytes.len(),
    );
    let bad = EncodeInput {
        width: 0,
        height: 1,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&[]),
    };
    assert!(matches!(
        encoder.encode_samples(&bad, EncodeOptions::default(), metadata),
        Err(AbbreviatedWriteError::Codec(_))
    ));
    assert!(encoder.tables().is_empty());
    assert_eq!(
        before,
        (
            encoder.get_ref().0.bytes.len(),
            encoder.get_ref().1.bytes.len()
        )
    );
    expected.clear();
    compare!(encode_coefficients(&source, CodingOptions::default()));
    encoder.clear();
    assert!(encoder.tables().is_empty());
    let (tables, image) = encoder.into_inner();
    assert_eq!((tables.flushes, image.flushes), (0, 0));
}
