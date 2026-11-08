use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, Event, FrameHeader, Limits, LosslessImage, LosslessOptions,
    MarkerReader, Process, Tables, decode_coefficients, decode_lossless, encode_coefficients,
};
#[allow(dead_code)]
#[path = "support/components.rs"]
mod support;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/t83")
            .join(name),
    )
    .unwrap()
}

fn names() -> [&'static str; 21] {
    [
        "A1", "B1", "B2", "C1", "C2", "D1", "D2", "E1", "E2", "F1", "F2", "G1", "H1", "J1", "K1",
        "L1", "N1", "O1", "O2", "P1", "P2",
    ]
}

fn frame(bytes: &[u8]) -> Option<FrameHeader> {
    MarkerReader::new(bytes)
        .unwrap()
        .find_map(|event| match event.unwrap() {
            Event::Segment { marker, data, .. } if Process::try_from(marker).is_ok() => {
                Some(FrameHeader::parse(marker, data).unwrap())
            }
            _ => None,
        })
}

fn tables() -> Tables {
    let mut tables = Tables::new();
    tables.load(&file("B1.JPG"), Limits::default()).unwrap();
    tables
}

fn cache(name: &str) -> Tables {
    if name == "B2" {
        tables()
    } else {
        Tables::new()
    }
}

fn exact_lossless(image: &LosslessImage<'_>) {
    assert_eq!(image.components.len(), 4);
    assert!(matches!(image.frame.precision, 8 | 16));
    let expected_geometry = [(85, 65), (85, 129), (255, 65), (85, 257)];
    for ((plane, label), (width, height)) in image
        .components
        .iter()
        .zip('A'..='D')
        .zip(expected_geometry)
    {
        assert_eq!((plane.width, plane.height), (width, height));
        let source = file(&format!("{label}{}.SRC", image.frame.precision));
        let expected: Vec<_> = if image.frame.precision == 8 {
            source.into_iter().map(u16::from).collect()
        } else {
            source
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect()
        };
        assert_eq!(plane.samples, expected);
    }
}

#[test]
fn official_streams_have_exact_c_coefficients_and_lossless_references() {
    let a = file("A1.JPG");
    let e = file("E1.JPG");
    let canonical8 = decode_coefficients(&a, Limits::default()).unwrap();
    let canonical12 = decode_coefficients(&e, Limits::default()).unwrap();
    let (mut dct, mut lossless, mut packets) = (0, 0, 0);
    for name in names() {
        let wire = file(&format!("{name}.JPG"));
        let Some(header) = frame(&wire) else {
            assert_eq!(name, "B1");
            let mut cache = Tables::new();
            let stream = cache.load(&wire, Limits::default()).unwrap();
            assert_eq!(stream.consumed, wire.len());
            packets += 1;
            continue;
        };
        if header.process.is_lossless() {
            let image = decode_lossless(&wire, Limits::default()).unwrap();
            assert_eq!(image.consumed, wire.len());
            exact_lossless(&image);
            lossless += 1;
        } else {
            let image = cache(name)
                .decode_coefficients(&wire, Limits::default())
                .unwrap();
            assert_eq!(image.consumed, wire.len());
            let precision = image.frame.precision;
            let reference = file(&format!("observed-{precision}.coef"));
            if name == "B2" {
                assert_eq!(image.components.len(), 255);
                assert!(decode_coefficients(&wire, Limits::default()).is_err());
                // All 255 selectors retain their exact advertised IDs. The
                // four numeric component models repeat in the official B2 data.
                for (index, component) in image.components.iter().enumerate() {
                    assert_eq!(component.spec.id, 254 - index as u8);
                    let canonical = canonical8
                        .components
                        .iter()
                        .find(|c| {
                            (c.spec.horizontal_sampling, c.spec.vertical_sampling)
                                == (
                                    component.spec.horizontal_sampling,
                                    component.spec.vertical_sampling,
                                )
                        })
                        .unwrap();
                    assert_eq!(component.quantization, canonical.quantization);
                    assert_eq!(
                        (component.width, component.height),
                        (canonical.width, canonical.height)
                    );
                    for y in 0..component.height.div_ceil(8) {
                        let width = component.width.div_ceil(8);
                        assert_eq!(
                            component.blocks
                                [y * component.blocks_wide..y * component.blocks_wide + width],
                            canonical.blocks
                                [y * canonical.blocks_wide..y * canonical.blocks_wide + width]
                        );
                    }
                }
            } else {
                support::coefficient_dump(&reference, &image.frame, &image.components);
            }
            dct += 1;
        }
    }
    support::coefficient_dump(
        &file("observed-8.coef"),
        &canonical8.frame,
        &canonical8.components,
    );
    support::coefficient_dump(
        &file("observed-12.coef"),
        &canonical12.frame,
        &canonical12.components,
    );
    assert_eq!((dct, lossless, packets), (16, 4, 1));
}

#[test]
fn official_models_rewrite_through_native_entropy_families() {
    let mut cases = 0;
    for name in names().into_iter().filter(|name| *name != "B1") {
        let wire = file(&format!("{name}.JPG"));
        let header = frame(&wire).unwrap();
        if header.process.is_lossless() {
            let image = decode_lossless(&wire, Limits::default()).unwrap();
            for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
                for predictor in 1..=7 {
                    let output = image
                        .encode(LosslessOptions {
                            entropy_coding,
                            predictor,
                            restart_rows: 1,
                            separate_components: predictor % 2 == 0,
                            ..LosslessOptions::default()
                        })
                        .unwrap();
                    exact_lossless(&decode_lossless(&output, Limits::default()).unwrap());
                    cases += 1;
                }
            }
        } else {
            let image = cache(name)
                .decode_coefficients(&wire, Limits::default())
                .unwrap();
            for entropy_coding in [EntropyCoding::Huffman, EntropyCoding::Arithmetic] {
                for progressive in [false, true] {
                    if progressive && image.components.len() > 4 {
                        continue;
                    }
                    let output = encode_coefficients(
                        &image,
                        CodingOptions {
                            entropy_coding,
                            progressive,
                            successive_approximation: 3,
                            restart_interval: 7,
                            ..CodingOptions::default()
                        },
                    )
                    .unwrap();
                    let decoded = decode_coefficients(&output, Limits::default()).unwrap();
                    support::same(&image, &decoded);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 118);
}

#[test]
fn official_stream_boundaries_mutations_and_table_packet_limits() {
    let limits = Limits {
        max_pixels: 1_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_scans: 512,
        ..Limits::default()
    };
    let (mut prefixes, mut mutations) = (0, 0);
    for name in names() {
        let wire = file(&format!("{name}.JPG"));
        let header = frame(&wire);
        let check = |data: &[u8]| {
            let mut cache = cache(name);
            match header.as_ref().map(|f| f.process) {
                None => cache.load(data, limits).map(|_| ()),
                Some(process) if process.is_lossless() => {
                    cache.decode_lossless(data, limits).map(|_| ())
                }
                _ => cache.decode_coefficients(data, limits).map(|_| ()),
            }
        };
        let mut ends: Vec<_> = if name == "B1" {
            (0..wire.len()).collect()
        } else {
            let mut ends: Vec<_> = (0..64.min(wire.len())).collect();
            ends.extend((0..wire.len()).step_by(4096));
            ends.extend(wire.len().saturating_sub(8)..wire.len());
            for (i, event) in MarkerReader::new(&wire).unwrap().enumerate() {
                let offset = match event.unwrap() {
                    Event::Segment { offset, .. } | Event::Scan { offset, .. } => offset,
                    _ => continue,
                };
                if name == "B2" && i % 17 != 0 {
                    continue;
                }
                ends.extend(offset.saturating_sub(1)..(offset + 5).min(wire.len()));
            }
            ends
        };
        ends.sort_unstable();
        ends.dedup();
        for end in ends {
            assert!(check(&wire[..end]).is_err(), "{name} at {end}");
            prefixes += 1;
        }
        let mut state = 0x1943_83a1u32;
        for _ in 0..32 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut changed = wire.clone();
            let offset = state as usize % wire.len();
            changed[offset] ^= (state >> 24) as u8 | 1;
            let _ = check(&changed);
            mutations += 1;
        }
    }
    eprintln!("T83 malformed: {prefixes} prefixes/boundaries and {mutations} bounded mutations");
}
