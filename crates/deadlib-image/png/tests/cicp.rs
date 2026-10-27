use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, Image,
    Limits, Metadata, Png, Samples, StreamDecoder, decode, decode_reader, encode, encode_writer,
    zlib::Compression,
};

// Independently transcribed H.273 (07/2024) tables 2 and 3. 2 is unspecified.
const PRIMARIES: &[u8] = &[1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 22];
const TRANSFERS: &[u8] = &[1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18];
fn base() -> Png {
    Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Rgb,
        bit_depth: 8,
        samples: Samples::U8(vec![1, 2, 3]),
    })
}
fn chunk(values: [u8; 4]) -> AncillaryChunk {
    AncillaryChunk {
        kind: *b"cICP",
        data: values.to_vec(),
        position: ChunkPosition::BeforePalette,
    }
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !c
}
fn fixture(values: [u8; 4]) -> Vec<u8> {
    let mut png = base();
    png.ancillary.push(
        Metadata::Gamma(45455)
            .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
            .unwrap(),
    );
    let bytes = encode(&png, &Default::default()).unwrap();
    let mut out = bytes[..33].to_vec();
    out.extend_from_slice(&4u32.to_be_bytes());
    out.extend_from_slice(b"cICP");
    out.extend_from_slice(&values);
    let sum = crc(&out[37..]);
    out.extend_from_slice(&sum.to_be_bytes());
    out.extend_from_slice(&bytes[33..]);
    out
}

#[test]
fn all_primaries_transfer_pairs_and_range_flags() {
    let mut png = base();
    png.ancillary.push(chunk([1, 1, 0, 0]));
    let mut known = 0;
    for primaries in 0..=255 {
        for transfer in 0..=255 {
            for range in [0, 1] {
                let values = [primaries, transfer, 0, range];
                png.ancillary[0] = chunk(values);
                let supported = PRIMARIES.contains(&primaries) && TRANSFERS.contains(&transfer);
                let metadata = png.metadata(Limits::default()).unwrap();
                assert_eq!(
                    metadata,
                    vec![if supported {
                        Metadata::Cicp(values)
                    } else {
                        Metadata::Unknown {
                            kind: *b"cICP",
                            data: values.to_vec(),
                        }
                    }]
                );
                let encoded = encode(&png, &Default::default());
                assert_eq!(encoded.is_ok(), supported, "{values:?}");
                assert_eq!(
                    Metadata::Cicp(values)
                        .to_chunk(ChunkPosition::BeforePalette, Compression::Stored)
                        .is_ok(),
                    supported
                );
                if let Ok(bytes) = encoded {
                    let actual = decode(&bytes).unwrap();
                    assert_eq!(actual, png);
                    assert_eq!(
                        decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                        png
                    );
                    let mut out = Vec::new();
                    encode_writer(&png, &Default::default(), &mut out).unwrap();
                    assert_eq!(decode(&out).unwrap(), png);
                    known += 1;
                }
            }
        }
    }
    assert_eq!(known, 408);
}

#[test]
fn reserved_codes_preserve_wire_data_and_fall_back_in_precedence() {
    for byte in 0..=255 {
        for (values, supported) in [
            ([byte, 16, 0, 1], PRIMARIES.contains(&byte)),
            ([9, byte, 0, 0], TRANSFERS.contains(&byte)),
        ] {
            let bytes = fixture(values);
            let actual = decode(&bytes).unwrap();
            assert_eq!(actual.image, base().image);
            assert_eq!(actual.ancillary[0], chunk(values));
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                actual
            );
            assert_eq!(
                actual.color_description().unwrap().kind,
                if supported { *b"cICP" } else { *b"gAMA" }
            );
            let mut stream = StreamDecoder::new(DecodeOptions::default());
            let mut offset = 0;
            let mut seen = false;
            loop {
                let end = bytes.len().min(offset + 1);
                let progress = stream.process(&bytes[offset..end]).unwrap();
                offset += progress.consumed;
                if let Some(DecodeEvent::Metadata { chunk, value }) = stream.event()
                    && &chunk.kind == b"cICP"
                {
                    assert_eq!(matches!(value, Metadata::Cicp(_)), supported);
                    seen = true;
                }
                if progress.status == DecodeStatus::Done {
                    break;
                }
                if progress.status == DecodeStatus::NeedInput {
                    assert!(offset < bytes.len());
                }
            }
            assert!(seen);
            if !supported {
                let mut output = Vec::new();
                assert!(encode_writer(&actual, &Default::default(), &mut output).is_err());
                assert!(output.is_empty());
            }
        }
    }
}

#[test]
fn all_png_matrix_and_full_range_flag_values() {
    for byte in 0..=255 {
        for (values, supported) in [([9, 16, byte, 1], byte == 0), ([9, 16, 0, byte], byte <= 1)] {
            let bytes = fixture(values);
            assert_eq!(decode(&bytes).is_ok(), supported, "{values:?}");
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).is_ok(),
                supported
            );
            let mut png = base();
            png.ancillary = vec![chunk(values)];
            assert_eq!(encode(&png, &Default::default()).is_ok(), supported);
            assert_eq!(
                Metadata::Cicp(values)
                    .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                    .is_ok(),
                supported
            );
        }
    }
}
