//! Independent byte fixtures for PNG Third Edition 5.3–5.6, 13.5 and 14.2.
//! No native encoder, compression or checksum code constructs these inputs.
use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, Error,
    ImageKind, Metadata, Png, Samples, StreamDecoder, decode, decode_reader, encode, encode_writer,
};
use std::io::{self, Cursor, Read};

#[path = "support/chunks.rs"]
mod fixtures;
use fixtures::{
    PIXEL, POSITIONS, STANDARD_NAMES, chunk, core_chunks, file, fixture, header, permutations,
    property_name, stored,
};

struct Fragmented<'a>(&'a [u8], usize);
impl Read for Fragmented<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.0.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}

fn parsers(bytes: &[u8]) -> [Result<Png, Error>; 3] {
    [
        decode(bytes),
        decode_reader(Fragmented(bytes, 1), &DecodeOptions::default()),
        decode_reader(Fragmented(bytes, 7), &DecodeOptions::default()),
    ]
}

fn expected_unknown(kind: [u8; 4], data: &[u8], position: ChunkPosition) -> AncillaryChunk {
    AncillaryChunk {
        kind,
        data: data.to_vec(),
        position,
    }
}

fn assert_rgb(png: &Png) {
    assert_eq!((png.image.width, png.image.height), (1, 1));
    assert_eq!(png.image.color_type, ColorType::Rgb);
    assert_eq!(png.image.bit_depth, 8);
    assert_eq!(png.image.samples, Samples::U8(PIXEL.to_vec()));
}

fn assert_unknown(bytes: &[u8], kind: [u8; 4], data: &[u8], position: ChunkPosition) {
    let expected = expected_unknown(kind, data, position);
    for result in parsers(bytes) {
        if kind[0] & 32 == 0 {
            assert_eq!(result, Err(Error::UnsupportedCriticalChunk(kind)));
        } else {
            let png = result.unwrap_or_else(|e| panic!("{kind:?} {position:?}: {e}"));
            assert_rgb(&png);
            assert_eq!(png.ancillary.as_slice(), std::slice::from_ref(&expected));
        }
    }
}

fn reference(bytes: &[u8]) -> Result<Vec<u8>, png::DecodingError> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut out)?;
    out.truncate(info.buffer_size());
    reader.finish()?;
    Ok(out)
}

#[test]
fn every_property_bit_and_name_byte() {
    let mut count = 0;
    for bits in 0..16 {
        let kind = property_name(*b"vPAg", bits);
        for position in POSITIONS {
            for data in [&[][..], &[0, 255, 13, 10, 128, 1][..]] {
                assert_unknown(&fixture(kind, data, position), kind, data, position);
                count += 1;
            }
        }
    }
    // Sweep every possible byte in each name position. Valid CRCs isolate the
    // ASCII restriction from the alphabetic bytes' property-bit interpretation.
    for index in 0..4 {
        for byte in 0..=255 {
            let mut kind = *b"vpAg";
            kind[index] = byte;
            for position in POSITIONS {
                let bytes = fixture(kind, &[255, 0], position);
                if byte.is_ascii_alphabetic() {
                    assert_unknown(&bytes, kind, &[255, 0], position);
                } else {
                    for result in parsers(&bytes) {
                        assert_eq!(result, Err(Error::Invalid("chunk type")), "{kind:?}");
                    }
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 3168);
}

#[test]
fn chunk_lengths_crc_alignment_and_encoder_name_validation() {
    let mut count = 0;
    for len in [
        0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 31, 32, 33, 63, 64, 65, 127, 128, 255, 256, 257, 1023,
        4095, 4096, 4097,
    ] {
        let payload: Vec<_> = (0..len).map(|i| (i * 71 + i / 13) as u8).collect();
        for position in POSITIONS {
            let bytes = fixture(*b"vpAg", &payload, position);
            assert_unknown(&bytes, *b"vpAg", &payload, position);
            count += 1;
        }
        let mut extra = chunk(*b"vpAg", &payload);
        // The chunk starts at an odd address (8-byte signature + 25-byte IHDR).
        // CRC covers the literal name and payload, including unknown chunks.
        for index in [4, extra.len() - 1] {
            extra[index] ^= 1;
            for result in parsers(&file(&[
                header(2, 8),
                extra.clone(),
                chunk(*b"IDAT", &stored(&[0, 17, 34, 51])),
                chunk(*b"IEND", &[]),
            ])) {
                assert_eq!(result, Err(Error::Checksum));
            }
            extra[index] ^= 1;
        }
        for declared in [0x7fff_ffffu32, 0x8000_0000, 0xffff_ffff] {
            extra[..4].copy_from_slice(&declared.to_be_bytes());
            for result in parsers(&file(&[header(2, 8), extra.clone()])) {
                assert!(result.is_err()); // No huge body/allocation is required.
                if declared > 0x7fff_ffff {
                    assert_eq!(result, Err(Error::Invalid("chunk length exceeds 31 bits")));
                }
            }
        }
    }
    assert_eq!(count, 78);

    let mut png = decode(&fixture(*b"vpAg", &[], ChunkPosition::BeforeData)).unwrap();
    for index in 0..4 {
        for byte in 0u8..=255 {
            if byte.is_ascii_alphabetic() {
                continue;
            }
            let mut kind = *b"vpAg";
            kind[index] = byte;
            png.ancillary[0].kind = kind;
            assert!(encode(&png, &Default::default()).is_err(), "{kind:?}");
            let mut output = Vec::new();
            assert!(
                encode_writer(&png, &Default::default(), &mut output).is_err(),
                "{kind:?}"
            );
            assert!(output.is_empty());
        }
    }
}

#[test]
fn standard_names_are_literal_not_case_folded() {
    let mut count = 0;
    for canonical in STANDARD_NAMES {
        for bits in 0..16 {
            let variant = property_name(canonical, bits);
            if variant == canonical {
                continue; // Defined payload semantics have their own suites.
            }
            assert!(!STANDARD_NAMES.contains(&variant));
            for position in POSITIONS {
                // Bytes deliberately unsuitable for the named standard chunk:
                // a case variant must receive unknown-chunk handling instead.
                let bytes = fixture(variant, &[255, 0, 128], position);
                assert_unknown(&bytes, variant, &[255, 0, 128], position);
                if variant[0] & 32 != 0 {
                    assert_eq!(reference(&bytes).unwrap(), PIXEL);
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 1125);
}

#[test]
fn reencoding_obeys_copy_bits_and_preserves_placement() {
    let mut count = 0;
    for bits in 0..16 {
        let kind = property_name(*b"vPAg", bits);
        for position in POSITIONS {
            // Build a native image from an independent valid file, then supply
            // each prospective name through the public raw ancillary API.
            let mut png = decode(&fixture(*b"vpAg", &[], position)).unwrap();
            png.ancillary = vec![expected_unknown(kind, &[0, 255, 0], position)];
            let buffered = encode(&png, &Default::default());
            let mut streamed = Vec::new();
            let writer = encode_writer(&png, &Default::default(), &mut streamed);
            if kind[0] & 32 == 0 || kind[2] & 32 != 0 {
                assert!(buffered.is_err(), "{kind:?}");
                assert!(writer.is_err(), "{kind:?}");
                assert!(streamed.is_empty());
            } else {
                writer.unwrap();
                for wire in [buffered.unwrap(), streamed] {
                    let actual = decode(&wire).unwrap();
                    assert_rgb(&actual);
                    assert_eq!(reference(&wire).unwrap(), PIXEL);
                    if kind[3] & 32 == 0 {
                        assert!(actual.ancillary.is_empty());
                    } else {
                        assert_eq!(actual.ancillary, png.ancillary);
                    }
                }
            }
            count += 1;
        }
    }
    // Unknown chunks may repeat. Preserve both order and binary/empty payloads
    // on each side of IDAT, even when the same safe name occurs in every slot.
    let mut png = decode(&fixture(*b"vpAg", &[], ChunkPosition::BeforeData)).unwrap();
    png.ancillary.clear();
    for position in POSITIONS {
        for payload in [&[][..], &[0, 255, 128][..], &[1][..]] {
            png.ancillary
                .push(expected_unknown(*b"vpAg", payload, position));
        }
    }
    // Known unsafe chunks use their defined rules instead of being dropped.
    png.ancillary.insert(
        0,
        expected_unknown(
            *b"gAMA",
            &45455u32.to_be_bytes(),
            ChunkPosition::BeforePalette,
        ),
    );
    png.ancillary.push(expected_unknown(
        *b"tIME",
        &[7, 234, 10, 27, 12, 34, 56],
        ChunkPosition::AfterData,
    ));
    let mut streamed = Vec::new();
    encode_writer(&png, &Default::default(), &mut streamed).unwrap();
    for bytes in [encode(&png, &Default::default()).unwrap(), streamed] {
        assert_eq!(decode(&bytes).unwrap(), png);
        assert_eq!(reference(&bytes).unwrap(), PIXEL);
    }
    assert_eq!(count, 48);
}

#[test]
fn all_core_chunk_permutations_and_unknown_insertions() {
    let chunks = core_chunks();
    let mut count = 0;
    let mut reference_recoveries = Vec::new();
    permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |order| {
        let bytes = file(&order.iter().map(|&i| chunks[i].clone()).collect::<Vec<_>>());
        let valid = order == [0, 1, 2, 3, 4, 5];
        for result in parsers(&bytes) {
            assert_eq!(result.is_ok(), valid, "order={order:?}: {result:?}");
            if let Ok(png) = result {
                assert_eq!(png.image.samples, Samples::U8(vec![0]));
            }
        }
        if reference(&bytes).is_ok() && !valid {
            reference_recoveries.push(order.to_vec());
        }
        count += 1;
    });
    assert_eq!(count, 720);
    println!(
        "Rust png accepts {} of 719 invalid core permutations: {reference_recoveries:?}",
        reference_recoveries.len()
    );

    let zlib = stored(&[0, 17, 34, 51]);
    let mut insertions = 0;
    for split in 0..=zlib.len() {
        for bits in 0..16 {
            let kind = property_name(*b"vPAg", bits);
            for slot in 0..3 {
                let mut chunks = vec![
                    header(2, 8),
                    chunk(*b"IDAT", &zlib[..split]),
                    chunk(*b"IDAT", &zlib[split..]),
                ];
                chunks.insert(slot + 1, chunk(kind, &[]));
                chunks.push(chunk(*b"IEND", &[]));
                let bytes = file(&chunks);
                let valid = kind[0] & 32 != 0 && slot != 1;
                for result in parsers(&bytes) {
                    assert_eq!(
                        result.is_ok(),
                        valid,
                        "split={split} kind={kind:?} slot={slot}: {result:?}"
                    );
                    if let Ok(png) = result {
                        assert_rgb(&png);
                    }
                }
                if valid {
                    assert_eq!(reference(&bytes).unwrap(), PIXEL);
                }
                insertions += 1;
            }
        }
    }
    assert_eq!(insertions, 768);

    // Unknown ancillary chunks cannot relax the mandatory first/last chunks.
    for bits in (1..16).step_by(2) {
        let unknown = chunk(property_name(*b"vPAg", bits), &[0, 255]);
        for slot in [0, 3] {
            let mut chunks = vec![header(2, 8), chunk(*b"IDAT", &zlib), chunk(*b"IEND", &[])];
            chunks.insert(slot, unknown.clone());
            for result in parsers(&file(&chunks)) {
                assert!(result.is_err());
            }
        }
    }
}

#[test]
fn unknown_stream_events_and_fused_critical_errors() {
    for bits in 0..16 {
        let kind = property_name(*b"vPAg", bits);
        for position in POSITIONS {
            let data = [0, 255, 128];
            let wire = fixture(kind, &data, position);
            for fragment in [1, 7, wire.len()] {
                let mut decoder = StreamDecoder::new(DecodeOptions::default());
                let (mut offset, mut metadata, mut rows, mut ends) = (0, 0, 0, 0);
                loop {
                    let end = wire.len().min(offset + fragment);
                    let progress = match decoder.process(&wire[offset..end]) {
                        Ok(progress) => progress,
                        Err(error) => {
                            assert_eq!(kind[0] & 32, 0);
                            assert_eq!(error, Error::UnsupportedCriticalChunk(kind));
                            assert_eq!(decoder.finish(), Err(error.clone()));
                            assert_eq!(decoder.process(&wire), Err(error));
                            assert!(decoder.event().is_none());
                            break;
                        }
                    };
                    offset += progress.consumed;
                    match progress.status {
                        DecodeStatus::Event => match decoder.event().unwrap() {
                            DecodeEvent::Metadata { chunk, value } => {
                                assert_eq!(*chunk, expected_unknown(kind, &data, position));
                                assert_eq!(
                                    *value,
                                    Metadata::Unknown {
                                        kind,
                                        data: data.to_vec()
                                    }
                                );
                                assert_eq!(ends, usize::from(position == ChunkPosition::AfterData));
                                metadata += 1;
                            }
                            DecodeEvent::Row(_) => rows += 1,
                            DecodeEvent::ImageEnd(image) => {
                                assert_eq!(image, ImageKind::Default);
                                ends += 1;
                            }
                            _ => (),
                        },
                        DecodeStatus::Done => {
                            assert_eq!(offset, wire.len());
                            assert_eq!((metadata, rows, ends), (1, 1, 1));
                            decoder.finish().unwrap();
                            break;
                        }
                        DecodeStatus::NeedInput => {
                            assert_eq!(offset, end);
                            assert!(offset < wire.len());
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_unknown_chunk_and_core_order_comparison() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch =
        std::env::temp_dir().join(format!("deadlib-png-chunks-{}.png", std::process::id()));
    let mut checks = 0;
    let mut reserved_rejections = 0;
    let mut reserved_cases = 0;
    let mut compare_unknown = |kind: [u8; 4], position| {
        let bytes = fixture(kind, &[0, 255, 128], position);
        fs::write(&scratch, &bytes).unwrap();
        let output = Command::new(&oracle).arg(&scratch).output().unwrap();
        if kind[0] & 32 == 0 {
            assert_eq!(output.status.code(), Some(1), "{kind:?} {position:?}");
        } else {
            if kind[2] & 32 != 0 {
                reserved_cases += 1;
            }
            if kind[2] & 32 != 0 && output.status.code() == Some(1) {
                reserved_rejections += 1;
            } else {
                assert!(
                    output.status.success(),
                    "{kind:?} {position:?}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(&output.stdout[16..], PIXEL);
            }
        }
        checks += 1;
    };
    for bits in 0..16 {
        for position in POSITIONS {
            compare_unknown(property_name(*b"vPAg", bits), position);
        }
    }
    for canonical in STANDARD_NAMES {
        for bits in 0..16 {
            let variant = property_name(canonical, bits);
            if variant == canonical {
                continue;
            }
            for position in POSITIONS {
                compare_unknown(variant, position);
            }
        }
    }
    let chunks = core_chunks();
    let mut recovered = Vec::new();
    permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |order| {
        fs::write(
            &scratch,
            file(&order.iter().map(|&i| chunks[i].clone()).collect::<Vec<_>>()),
        )
        .unwrap();
        let output = Command::new(&oracle).arg(&scratch).output().unwrap();
        if order == [0, 1, 2, 3, 4, 5] {
            assert!(output.status.success());
            assert_eq!(&output.stdout[16..], [0]);
        } else if output.status.success() {
            recovered.push(order.to_vec());
        }
        checks += 1;
    });
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng: {checks} comparisons, {reserved_rejections}/{reserved_cases} reserved-bit ancillary rejections, {}/719 malformed-order recoveries: {recovered:?}",
        recovered.len()
    );
    assert_eq!(checks, 1893);
}
