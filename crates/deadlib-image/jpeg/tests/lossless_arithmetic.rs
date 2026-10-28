use deadlib_image_jpeg::{
    ArithmeticConditioning, ColorSpace, EncodeInput, EntropyCoding, Error, Event, Limits,
    LosslessImage, LosslessOptions, MarkerReader, Process, SampleData, decode_lossless,
    encode_lossless_samples,
};
use std::path::Path;

fn fixtures() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lossless-arithmetic");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|r| r.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 45);
    paths
}
fn serialized(image: &LosslessImage<'_>) -> Vec<u8> {
    let mut out = b"JLP1".to_vec();
    for v in [
        image.frame.precision as u32,
        image.frame.width as u32,
        image.frame.height as u32,
        image.components.len() as u32,
    ] {
        out.extend(v.to_le_bytes());
    }
    for c in &image.components {
        for v in [
            c.spec.id as u32,
            c.spec.horizontal_sampling as u32,
            c.spec.vertical_sampling as u32,
            c.width as u32,
            c.height as u32,
        ] {
            out.extend(v.to_le_bytes());
        }
        for v in &c.samples {
            out.extend(v.to_le_bytes());
        }
    }
    out
}
fn arithmetic() -> LosslessOptions<'static> {
    LosslessOptions {
        entropy_coding: EntropyCoding::Arithmetic,
        ..LosslessOptions::default()
    }
}
fn without_dac(bytes: &[u8]) -> Vec<u8> {
    let ranges: Vec<_> = MarkerReader::new(bytes)
        .unwrap()
        .filter_map(|e| match e.unwrap() {
            Event::Segment {
                marker: 0xcc,
                data,
                offset,
            } => Some(offset..offset + data.len() + 4),
            _ => None,
        })
        .collect();
    let mut out = bytes.to_vec();
    for range in ranges.into_iter().rev() {
        out.drain(range);
    }
    out
}

#[test]
fn pinned_independent_samples_subsampling_and_transcodes() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let expected = std::fs::read(path.with_extension("samples")).unwrap();
        let image = decode_lossless(&bytes, Limits::default()).unwrap();
        assert_eq!(image.frame.process, Process::ArithmeticLossless);
        assert_eq!(image.consumed, bytes.len());
        assert_eq!(serialized(&image), expected, "{}", path.display());
        for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
            for (lower, upper) in [(0, 0), (3, 7), (15, 15)] {
                let output = image
                    .encode(LosslessOptions {
                        entropy_coding,
                        predictor: 4,
                        restart_rows: 1,
                        arithmetic_conditioning: ArithmeticConditioning {
                            dc_lower: lower,
                            dc_upper: upper,
                            ac_split: 0,
                        },
                        ..LosslessOptions::default()
                    })
                    .unwrap();
                assert_eq!(
                    serialized(&decode_lossless(&output, Limits::default()).unwrap()),
                    expected
                );
            }
        }
    }
}

#[test]
fn default_dac_all_destinations_and_malformed_restart_conditioning() {
    let source = include_bytes!("fixtures/lossless-arithmetic/reference-p8-gray-r1.jpg");
    let image = decode_lossless(source, Limits::default()).unwrap();
    let encoded = image
        .encode(LosslessOptions {
            restart_rows: 1,
            ..arithmetic()
        })
        .unwrap();
    let expected = serialized(&image);
    for table in 0..=3 {
        let mut bytes = without_dac(&encoded);
        let offset = MarkerReader::new(&bytes)
            .unwrap()
            .find_map(|e| match e.unwrap() {
                Event::Scan { offset, .. } => Some(offset),
                _ => None,
            })
            .unwrap();
        bytes[offset + 6] = table << 4;
        assert_eq!(
            serialized(&decode_lossless(&bytes, Limits::default()).unwrap()),
            expected
        );
    }
    let mut bad = encoded.clone();
    let rst = bad.windows(2).position(|b| b == [255, 0xd0]).unwrap();
    bad[rst + 1] = 0xd7;
    assert!(decode_lossless(&bad, Limits::default()).is_err());
    let mut bad = encoded.clone();
    let dac = bad.windows(2).position(|b| b == [255, 0xcc]).unwrap();
    bad[dac + 5] = 0x01;
    assert!(decode_lossless(&bad, Limits::default()).is_err());
    assert!(
        image
            .encode(LosslessOptions {
                arithmetic_conditioning: ArithmeticConditioning {
                    dc_lower: 2,
                    dc_upper: 1,
                    ac_split: 5
                },
                ..arithmetic()
            })
            .is_err()
    );
    assert!(
        image
            .encode(LosslessOptions {
                max_output_bytes: 0,
                ..arithmetic()
            })
            .is_err()
    );
    let mut tail = encoded.clone();
    tail.extend(b"unconsumed");
    assert_eq!(
        decode_lossless(&tail, Limits::default()).unwrap().consumed,
        encoded.len()
    );
}

#[test]
fn prediction_class_history_is_bounded_before_allocation() {
    let values = vec![0; 65535];
    let input = EncodeInput {
        width: 65535,
        height: 1,
        precision: 8,
        color_space: ColorSpace::Grayscale,
        samples: SampleData::U8(&values),
    };
    let options = LosslessOptions {
        limits: Limits {
            max_memory_bytes: 65535,
            ..Limits::default()
        },
        ..arithmetic()
    };
    assert!(matches!(
        encode_lossless_samples(&input, options),
        Err(Error::LimitExceeded(_))
    ));
    let encoded = encode_lossless_samples(
        &input,
        LosslessOptions {
            limits: Limits {
                max_memory_bytes: 65535 * 2,
                ..Limits::default()
            },
            ..arithmetic()
        },
    )
    .unwrap();
    assert!(matches!(
        decode_lossless(
            &encoded,
            Limits {
                max_memory_bytes: 65535 * 3,
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
    let decoded = decode_lossless(
        &encoded,
        Limits {
            max_memory_bytes: 65535 * 4,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(decoded.components[0].samples, vec![0u16; 65535]);
}

#[test]
fn every_reference_prefix_and_bounded_arithmetic_mutations() {
    let limits = Limits {
        max_pixels: 4096,
        max_memory_bytes: 1024 * 1024,
        max_scans: 32,
        ..Limits::default()
    };
    let mut state = 0x71234567u32;
    for path in fixtures() {
        let bytes = std::fs::read(path).unwrap();
        for end in 0..bytes.len() {
            assert!(decode_lossless(&bytes[..end], limits).is_err());
        }
        for _ in 0..100 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut bad = bytes.clone();
            let index = state as usize % bad.len();
            bad[index] ^= ((state >> 16) as u8) | 1;
            if let Ok(image) = decode_lossless(&bad, limits) {
                image.into_planar().unwrap();
            }
        }
    }
}

#[test]
fn reference_limitations_keep_native_annex_h_semantics() {
    // Independent Turbo sample results establish Pt semantics for the same
    // Huffman stream that exposes JPEG XT's point-transform presentation bug.
    let bytes = include_bytes!("fixtures/jpegxt-limitations/pt-huffman.jpg");
    let expected = include_bytes!("fixtures/jpegxt-limitations/pt-huffman.samples");
    let image = decode_lossless(bytes, Limits::default()).unwrap();
    assert_eq!(serialized(&image), expected);
    let encoded = image
        .encode(LosslessOptions {
            point_transform: 1,
            predictor: 5,
            ..arithmetic()
        })
        .unwrap();
    assert_eq!(
        serialized(&decode_lossless(&encoded, Limits::default()).unwrap()),
        expected
    );
    // This reference-encoded 2x2 stream ends in one partial vertical MCU row.
    // The exact first plane is derived from its original input component.
    let bytes = include_bytes!("fixtures/jpegxt-limitations/partial-row.jpg");
    let expected = include_bytes!("fixtures/jpegxt-limitations/partial-row-first.samples");
    let image = decode_lossless(bytes, Limits::default()).unwrap();
    let samples: Vec<_> = image.components[0]
        .samples
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(samples, expected);
    let encoded = image
        .encode(LosslessOptions {
            predictor: 4,
            restart_rows: 1,
            ..arithmetic()
        })
        .unwrap();
    let decoded = decode_lossless(&encoded, Limits::default()).unwrap();
    assert_eq!(decoded.components[0].samples, image.components[0].samples);
}
