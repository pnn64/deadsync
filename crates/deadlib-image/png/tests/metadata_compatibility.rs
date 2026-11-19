//! Independent wire fixtures for metadata recovery on user-supplied PNGs.
use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, DecodeEvent, DecodeOptions, DecodeStatus, Error, Limits,
    Metadata, MetadataPolicy, Png, RowSamples, Samples, StreamDecoder, decode, decode_reader,
    decode_with_options, encode, encode_writer,
};
use std::io::{self, Cursor, Read};

#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod wire;
use wire::{PIXEL, chunk, file, header, stored};

const PROFILE_A: &[u8] = include_bytes!("fixtures/icc/sRGB-v4.icc");
const PROFILE_B: &[u8] = include_bytes!("fixtures/icc/sRGB-v2-nano.icc");
const EXIF: &[u8] = b"MM\0\x2a\0\0\0\x08\0\0\0\0\0\0";

fn icc_body(profile: &[u8]) -> Vec<u8> {
    let mut data = b"Profile\0\0".to_vec();
    data.extend(stored(profile));
    data
}

fn icc(profile: &[u8]) -> Vec<u8> {
    chunk(*b"iCCP", &icc_body(profile))
}

fn profile(profile: &[u8]) -> Metadata {
    Metadata::IccProfile {
        name: b"Profile".to_vec(),
        profile: profile.to_vec(),
    }
}

fn bad_crc(mut wire: Vec<u8>) -> Vec<u8> {
    let last = wire.len() - 1;
    wire[last] ^= 1;
    wire
}

fn fixture(before: &[Vec<u8>], after: &[Vec<u8>]) -> Vec<u8> {
    let mut parts = vec![header(2, 8)];
    parts.extend_from_slice(before);
    parts.push(chunk(*b"IDAT", &stored(&[0, PIXEL[0], PIXEL[1], PIXEL[2]])));
    parts.extend_from_slice(after);
    parts.push(chunk(*b"IEND", &[]));
    file(&parts)
}

struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.0.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}

fn reference(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut out).unwrap();
    assert_eq!((info.width, info.height), (1, 1));
    assert_eq!(&out[..info.buffer_size()], PIXEL.as_slice());
    reader.finish().unwrap();
    (
        reader.info().icc_profile.as_deref().map(<[u8]>::to_vec),
        reader.info().exif_metadata.as_deref().map(<[u8]>::to_vec),
    )
}

#[derive(Debug, Default)]
struct Observed {
    metadata: Vec<(AncillaryChunk, Metadata)>,
    samples: Vec<u8>,
}

fn push(bytes: &[u8], options: DecodeOptions, fragment: usize) -> Result<Observed, Error> {
    let mut decoder = StreamDecoder::new(options);
    let (mut offset, mut rows, mut ends) = (0, 0, 0);
    let mut observed = Observed::default();
    for _ in 0..bytes.len() * 3 + 64 {
        let end = bytes.len().min(offset + fragment);
        let progress = match decoder.process(&bytes[offset..end]) {
            Ok(progress) => progress,
            Err(error) => {
                assert!(decoder.event().is_none());
                assert_eq!(decoder.finish().unwrap_err(), error);
                assert_eq!(decoder.process(&[]).unwrap_err(), error);
                return Err(error);
            }
        };
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        match decoder.event() {
            Some(DecodeEvent::Metadata { chunk, value }) => {
                observed.metadata.push((chunk.clone(), value.clone()));
            }
            Some(DecodeEvent::Row(row)) => {
                let RowSamples::U8(samples) = row.samples else {
                    panic!("8-bit fixture returned U16 samples");
                };
                observed.samples.extend_from_slice(samples);
                rows += 1;
            }
            Some(DecodeEvent::ImageEnd(_)) => ends += 1,
            _ => (),
        }
        match progress.status {
            DecodeStatus::Done => {
                decoder.finish()?;
                assert_eq!(offset, bytes.len());
                assert_eq!((rows, ends), (1, 1));
                return Ok(observed);
            }
            DecodeStatus::NeedInput if offset == bytes.len() => {
                decoder.finish()?;
                panic!("truncated PNG accepted");
            }
            DecodeStatus::NeedInput => assert_eq!(offset, end),
            DecodeStatus::Event => (),
        }
    }
    panic!("PNG stream stalled");
}

fn native_accepted(
    bytes: &[u8],
    options: DecodeOptions,
    expected: &[Metadata],
    samples: &[u8],
) -> Png {
    let actual = decode_with_options(bytes, &options).unwrap();
    assert_eq!((actual.image.width, actual.image.height), (1, 1));
    assert_eq!(actual.image.samples, Samples::U8(samples.to_vec()));
    assert_eq!(actual.metadata(options.limits).unwrap(), expected);
    let events: Vec<_> = actual
        .ancillary
        .iter()
        .cloned()
        .zip(expected.iter().cloned())
        .collect();
    assert_eq!(actual.ancillary.len(), expected.len());
    for fragment in [1, 7, bytes.len()] {
        assert_eq!(
            decode_reader(Fragments(bytes, fragment), &options).unwrap(),
            actual
        );
        let observed = push(bytes, options, fragment).unwrap();
        assert_eq!(observed.metadata, events);
        assert_eq!(observed.samples, samples);
    }
    actual
}

fn accepted(bytes: &[u8], expected: &[Metadata]) -> Png {
    reference(bytes);
    let actual = native_accepted(bytes, DecodeOptions::default(), expected, &PIXEL);
    assert_eq!(decode(bytes).unwrap(), actual);
    actual
}

fn rejected(bytes: &[u8], options: DecodeOptions, expected: Option<Error>) {
    let mut errors = vec![decode_with_options(bytes, &options).unwrap_err()];
    for fragment in [1, 7, bytes.len()] {
        errors.push(decode_reader(Fragments(bytes, fragment), &options).unwrap_err());
        errors.push(push(bytes, options, fragment).unwrap_err());
    }
    if let Some(expected) = expected {
        for error in errors {
            assert_eq!(error, expected);
        }
    }
}

fn strict_rejects(bytes: &[u8]) {
    rejected(
        bytes,
        DecodeOptions {
            metadata_policy: MetadataPolicy::Strict,
            ..Default::default()
        },
        None,
    );
}

#[test]
fn late_exif_preserves_its_payload_and_placement() {
    for exif in [EXIF, b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0"] {
        let bytes = fixture(&[], &[chunk(*b"eXIf", exif)]);
        let actual = accepted(&bytes, &[Metadata::Exif(exif.to_vec())]);
        assert_eq!(actual.ancillary[0].position, ChunkPosition::AfterData);
        assert_eq!(reference(&bytes).1.as_deref(), Some(exif));
        strict_rejects(&bytes);
        assert!(encode(&actual, &Default::default()).is_err());
        let mut output = Vec::new();
        assert!(encode_writer(&actual, &Default::default(), &mut output).is_err());
        assert!(output.is_empty());
    }
}

#[test]
fn bad_metadata_crc_is_discarded_without_claiming_a_singleton() {
    for extra in [
        icc(PROFILE_A),
        chunk(*b"gAMA", &45455u32.to_be_bytes()),
        chunk(*b"eXIf", EXIF),
        chunk(*b"vpAg", &[1, 2, 3]),
    ] {
        let bad = bad_crc(extra);
        let bad_chunk = std::slice::from_ref(&bad);
        for bytes in [fixture(bad_chunk, &[]), fixture(&[], bad_chunk)] {
            accepted(&bytes, &[]);
            strict_rejects(&bytes);
        }
    }
    let bytes = fixture(&[bad_crc(icc(PROFILE_A)), icc(PROFILE_B)], &[]);
    accepted(&bytes, &[profile(PROFILE_B)]);
    assert_eq!(reference(&bytes).0.as_deref(), Some(PROFILE_B));
    strict_rejects(&bytes);
}

#[test]
fn duplicate_profiles_keep_the_first_crc_valid_occurrence() {
    for second in [
        icc(PROFILE_A),
        icc(PROFILE_B),
        bad_crc(icc(PROFILE_B)),
        chunk(*b"iCCP", b"Profile\0\x01invalid compression method"),
    ] {
        let bytes = fixture(&[icc(PROFILE_A), second], &[]);
        accepted(&bytes, &[profile(PROFILE_A)]);
        assert_eq!(reference(&bytes).0.as_deref(), Some(PROFILE_A));
        strict_rejects(&bytes);
    }

    let bytes = fixture(
        &[
            chunk(*b"iCCP", b"Profile\0\x01invalid compression method"),
            icc(PROFILE_B),
        ],
        &[],
    );
    accepted(&bytes, &[]);
    assert_eq!(reference(&bytes).0, None);
    strict_rejects(&bytes);

    // Existing recovery retains unsupported ICC containers as raw metadata.
    // A later valid profile must not silently replace that first occurrence.
    let unsupported = b"unsupported ICC container";
    let bytes = fixture(&[icc(unsupported), icc(PROFILE_B)], &[]);
    accepted(
        &bytes,
        &[Metadata::Unknown {
            kind: *b"iCCP",
            data: icc_body(unsupported),
        }],
    );
    assert_eq!(reference(&bytes).0.as_deref(), Some(unsupported.as_slice()));
    strict_rejects(&bytes);
}

#[test]
fn malformed_or_misplaced_optional_metadata_does_not_block_pixels() {
    for bytes in [
        fixture(&[chunk(*b"gAMA", &[1, 2, 3])], &[]),
        fixture(&[chunk(*b"tEXt", b"missing separator")], &[]),
        fixture(&[], &[chunk(*b"gAMA", &45455u32.to_be_bytes())]),
    ] {
        accepted(&bytes, &[]);
        strict_rejects(&bytes);
    }

    let bytes = fixture(
        &[
            chunk(*b"gAMA", &[1, 2, 3]),
            chunk(*b"gAMA", &45455u32.to_be_bytes()),
        ],
        &[],
    );
    accepted(&bytes, &[Metadata::Gamma(45455)]);
    strict_rejects(&bytes);

    let chromaticities = [35400u16, 14600, 8500, 39850, 6550, 2300, 15635, 16450];
    let luminance = [40_000_000u32, 5];
    let data: Vec<_> = chromaticities
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .chain(luminance.into_iter().flat_map(u32::to_be_bytes))
        .collect();
    let bytes = fixture(&[chunk(*b"mDCV", &data)], &[]);
    accepted(
        &bytes,
        &[Metadata::MasteringDisplay {
            chromaticities,
            luminance,
        }],
    );
    strict_rejects(&bytes);
}

#[test]
fn recovery_preserves_critical_integrity_and_png_framing() {
    let idat = stored(&[0, PIXEL[0], PIXEL[1], PIXEL[2]]);
    let core = [
        header(2, 8),
        chunk(*b"PLTE", &[0, 0, 0]),
        chunk(*b"IDAT", &idat),
        chunk(*b"IEND", &[]),
    ];
    for index in 0..core.len() {
        let mut parts = core.clone();
        parts[index] = bad_crc(parts[index].clone());
        rejected(&file(&parts), Default::default(), Some(Error::Checksum));
    }
    // Transparency and animation controls are outside optional metadata recovery.
    for extra in [
        chunk(*b"tRNS", &[0, 17, 0, 34, 0, 51]),
        chunk(*b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]),
    ] {
        rejected(
            &fixture(&[bad_crc(extra)], &[]),
            Default::default(),
            Some(Error::Checksum),
        );
    }
    let mut bad_adler = idat.clone();
    *bad_adler.last_mut().unwrap() ^= 1;
    rejected(
        &file(&[
            header(2, 8),
            chunk(*b"IDAT", &bad_adler),
            chunk(*b"IEND", &[]),
        ]),
        Default::default(),
        Some(Error::Checksum),
    );
    rejected(
        &file(&[
            header(2, 8),
            chunk(*b"IDAT", &idat[..5]),
            bad_crc(chunk(*b"vpAg", &[1])),
            chunk(*b"IDAT", &idat[5..]),
            chunk(*b"IEND", &[]),
        ]),
        Default::default(),
        None,
    );
    let extra = bad_crc(icc(PROFILE_A));
    let bytes = fixture(std::slice::from_ref(&extra), &[]);
    rejected(&bytes[..33 + extra.len() - 1], Default::default(), None);
}

#[test]
fn metadata_recovery_keeps_resource_errors_fatal() {
    let bad = bad_crc(chunk(*b"vpAg", &[1, 2, 3]));
    let ignored = fixture(&[bad.clone(), bad], &[]);
    accepted(&ignored, &[]);
    for limits in [
        Limits {
            max_input_bytes: ignored.len() - 1,
            ..Default::default()
        },
        Limits {
            max_chunks: 4,
            ..Default::default()
        },
        Limits {
            max_metadata_bytes: 5,
            ..Default::default()
        },
    ] {
        rejected(
            &ignored,
            DecodeOptions {
                limits,
                ..Default::default()
            },
            Some(Error::LimitExceeded),
        );
    }
    let body = icc_body(PROFILE_A);
    let bytes = fixture(&[chunk(*b"iCCP", &body)], &[]);
    rejected(
        &bytes,
        DecodeOptions {
            limits: Limits {
                max_metadata_bytes: body.len() + PROFILE_A.len() - 1,
                ..Default::default()
            },
            ..Default::default()
        },
        Some(Error::LimitExceeded),
    );
}

#[test]
fn discarded_compressed_metadata_charges_every_produced_byte() {
    let mut parts = Vec::new();
    let mut minimum = 0;
    for (name, length, byte) in [("First", 37, b'a'), ("Second", 61, b'b')] {
        let mut compressed = stored(&vec![byte; length]);
        *compressed.last_mut().unwrap() ^= 1; // Bad Adler, valid outer chunk CRC.
        let mut data = name.as_bytes().to_vec();
        data.extend([0, 0]);
        data.extend(compressed);
        minimum += data.len() + length;
        parts.push(chunk(*b"zTXt", &data));
    }
    let bytes = fixture(&parts, &[]);
    reference(&bytes);
    let mut options = DecodeOptions {
        limits: Limits {
            max_metadata_bytes: minimum,
            ..Default::default()
        },
        ..Default::default()
    };
    native_accepted(&bytes, options, &[], &PIXEL);
    options.limits.max_metadata_bytes -= 1;
    rejected(&bytes, options, Some(Error::LimitExceeded));
}

#[test]
fn metadata_before_a_later_palette_obeys_the_original_position() {
    let bytes = file(&[
        header(2, 8),
        chunk(*b"bKGD", &[0, 17, 0, 34, 0, 51]),
        chunk(*b"PLTE", &[0, 0, 0]),
        chunk(*b"IDAT", &stored(&[0, PIXEL[0], PIXEL[1], PIXEL[2]])),
        chunk(*b"IEND", &[]),
    ]);
    let actual = accepted(&bytes, &[Metadata::BackgroundRgb([17, 34, 51])]);
    assert_eq!(actual.ancillary[0].position, ChunkPosition::BeforePalette);
    strict_rejects(&bytes);

    for extra in [chunk(*b"bKGD", &[0]), chunk(*b"hIST", &[0, 1])] {
        let bytes = file(&[
            header(3, 8),
            extra,
            chunk(*b"PLTE", &PIXEL),
            chunk(*b"IDAT", &stored(&[0, 0])),
            chunk(*b"IEND", &[]),
        ]);
        // Rust png can reject an indexed background before PLTE. Here the
        // invariant is consistent recovery across all three native APIs.
        let actual = native_accepted(&bytes, DecodeOptions::default(), &[], &[0]);
        assert_eq!(actual.palette, vec![PIXEL]);
        assert_eq!(decode(&bytes).unwrap(), actual);
        strict_rejects(&bytes);
    }
}
