#[path = "support/raw_cases.rs"]
mod raw_cases;
mod support;
#[path = "support/degenerate_books.rs"]
mod vectors;
use deadlib_audio_decode_vorbis::{DecodeOptions, ParseOptions};
use raw_cases::{Case, compare, reference, report};
use vectors::{Config, Use};

#[test]
fn empty_book_policy_preserves_reader_state_and_zero_dimension_scalar_codes() {
    use deadlib_audio_decode_vorbis::{Error, Limits, bitpack::BitReader, codebook::Codebook};
    for dimensions in [0, 1, 4, u16::MAX] {
        for entries in [0, 1, 4] {
            for lookup in 0..=2 {
                let config = Config {
                    dimensions,
                    entries,
                    lookup,
                    ..Config::default()
                };
                let bytes = config.packed_target();
                assert!(Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).is_err());
                let book =
                    Codebook::read_with_options(&mut BitReader::new(&bytes), parsing(config))
                        .unwrap();
                assert!(book.is_empty());
                assert!(book.huffman().is_empty());
                assert_eq!(book.huffman().node_count(), 0);
                let mut bits = BitReader::new(&[21]);
                bits.read(1).unwrap();
                assert_eq!(book.decode_scalar(&mut bits), Err(Error::EndOfPacket));
                assert_eq!(bits.position(), 1);
                assert!(!bits.is_exhausted());
                assert_eq!(bits.read(4), Ok(10));
            }
        }
    }
    for lookup in [0, 2] {
        let config = Config {
            dimensions: 0,
            lookup,
            empty: false,
            ..Config::default()
        };
        let bytes = config.packed_target();
        let book = Codebook::read(&mut BitReader::new(&bytes), &Limits::default()).unwrap();
        assert!(!book.is_empty());
        assert_eq!(book.dimensions(), 0);
        // Four complete two-bit words in entry order: 00,01,10,11 (MSB paths).
        let mut bits = BitReader::new(&[0xd8]);
        for entry in 0..4 {
            assert_eq!(book.decode_scalar(&mut bits), Ok(entry));
        }
        assert_eq!(bits.position(), 8);
    }
}

#[test]
fn degenerate_books_retain_context_and_resource_validation() {
    use deadlib_audio_decode_vorbis::{
        Error, Identification, Limits, Setup, bitpack::BitReader, codebook::Codebook,
    };
    for config in [
        Config {
            usage: Use::FloorZero,
            dimensions: 0,
            lookup: 2,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueClass,
            dimensions: 0,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueClass,
            entries: 0,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueVector,
            dimensions: 0,
            lookup: 2,
            empty: false,
            ..Config::default()
        },
    ] {
        let headers = config.headers();
        let id = Identification::parse(&headers[0]).unwrap();
        assert!(matches!(
            Setup::parse_with_options(
                &headers[2],
                &id,
                ParseOptions {
                    allow_empty_codebooks: true,
                    ..ParseOptions::default()
                }
            ),
            Err(Error::Invalid(_))
        ));
    }
    let config = Config {
        dimensions: 0,
        lookup: 1,
        empty: false,
        ..Config::default()
    };
    let bytes = config.packed_target();
    assert!(matches!(
        Codebook::read_with_options(
            &mut BitReader::new(&bytes),
            ParseOptions {
                allow_empty_codebooks: true,
                ..ParseOptions::default()
            }
        ),
        Err(Error::Invalid(_))
    ));
    let config = Config::default();
    let bytes = config.packed_target();
    assert!(matches!(
        Codebook::read_with_options(
            &mut BitReader::new(&bytes),
            ParseOptions {
                limits: Limits {
                    max_codebook_entries: 0,
                    ..Limits::default()
                },
                allow_empty_codebooks: true
            }
        ),
        Err(Error::LimitExceeded(_))
    ));
}

fn regressions() -> Vec<Config> {
    vec![
        Config {
            usage: Use::FloorMaster,
            dimensions: 0,
            entries: 1,
            empty: false,
            coupling: true,
            ..Config::default()
        },
        Config {
            usage: Use::FloorSubclass,
            dimensions: 0,
            lookup: 2,
            empty: false,
            channels: 3,
            coupling: true,
            ..Config::default()
        },
        Config {
            dimensions: 0,
            entries: 0,
            lookup: 1,
            ordered_zero: true,
            ..Config::default()
        },
        Config {
            usage: Use::FloorMaster,
            dimensions: 0,
            entries: 1,
            coupling: true,
            ..Config::default()
        },
        Config {
            usage: Use::FloorSubclass,
            dimensions: 2,
            lookup: 2,
            channels: 3,
            coupling: true,
            ..Config::default()
        },
        Config {
            usage: Use::FloorZero,
            entries: 0,
            lookup: 1,
            kind: 0,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueClass,
            dimensions: 4,
            entries: 1,
            channels: 3,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueVector,
            dimensions: 0,
            entries: 0,
            lookup: 2,
            channels: 5,
            kind: 2,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueVector,
            dimensions: 3,
            lookup: 1,
            kind: 0,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueVector,
            dimensions: u16::MAX,
            lookup: 2,
            ..Config::default()
        },
        Config {
            usage: Use::ResidueClass,
            dimensions: u16::MAX,
            entries: 1,
            ..Config::default()
        },
    ]
}
fn regression_cases(config: Config) -> Vec<Case> {
    let mut cases = vec![Case {
        name: format!("{config:?}/complete"),
        packets: sequence(config),
    }];
    if config.usage == Use::FloorZero {
        let mut packets = sequence(config);
        packets[2] = config.audio(true, 0, 2)[..2].to_vec();
        cases.push(Case {
            name: "later-floor-eof".into(),
            packets,
        });
    }
    cases
}
#[test]
fn degenerate_book_consumers_match_retained_libvorbis_pcm() {
    let mut golden = include_bytes!("fixtures/degenerate-books-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVCBOOK1");
    let configs = regressions();
    assert_eq!(support::u32_in(&mut golden) as usize, configs.len());
    for (i, config) in configs.into_iter().enumerate() {
        let length = u64::from_le_bytes(support::take(&mut golden, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut golden, length);
        report(
            &format!("book-golden-{i}"),
            raw_cases::compare_with_parse_options(
                &config.headers(),
                &regression_cases(config),
                reference,
                decoding(config),
                parsing(config),
            ),
        );
    }
    assert!(golden.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_degenerate_book_golden() {
    let configs = regressions();
    let mut golden = b"DVCBOOK1".to_vec();
    golden.extend((configs.len() as u32).to_le_bytes());
    for (i, config) in configs.into_iter().enumerate() {
        let reference = reference(
            &format!("book-golden-{i}"),
            &config.headers(),
            &regression_cases(config),
        );
        golden.extend((reference.len() as u64).to_le_bytes());
        golden.extend(reference);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/degenerate-books-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}

#[test]
#[ignore = "requires external libvorbis oracle and DEADLIB_VORBIS_FLOOR0"]
fn reference_nonfinite_cases_preserve_counts_and_ieee_classes() {
    let bytes = std::fs::read(
        std::env::var_os("DEADLIB_VORBIS_FLOOR0").expect("set DEADLIB_VORBIS_FLOOR0"),
    )
    .unwrap();
    let mut reader = ogg::PacketReader::new(std::io::Cursor::new(bytes));
    let mut packets = Vec::new();
    while packets.len() < 3 + 35 {
        packets.push(reader.read_packet().unwrap().unwrap().data);
    }
    let audio = &packets[3..];
    let mut cases = Vec::new();
    for (number, bit) in [
        (27, 16),
        (29, 16),
        (29, 63),
        (29, 69),
        (29, 70),
        (29, 72),
        (29, 74),
        (29, 75),
        (29, 76),
        (29, 89),
        (31, 13),
    ] {
        let mut data = audio[number].clone();
        data[bit / 8] ^= 1 << (bit & 7);
        cases.push(Case {
            name: format!("nonfinite/packet-{number}/bit-{bit}"),
            packets: vec![
                audio[number - 2].clone(),
                audio[number - 1].clone(),
                data,
                audio[number + 1].clone(),
                audio[number + 2].clone(),
            ],
        });
    }
    let reference = reference("nonfinite-reference", &packets[..3], &cases);
    report(
        "nonfinite-reference",
        raw_cases::compare_with_options(
            &packets[..3],
            &cases,
            &reference,
            DecodeOptions {
                allow_nonfinite: true,
                ..DecodeOptions::default()
            },
        ),
    );
}

fn sequence(config: Config) -> Vec<Vec<u8>> {
    vec![
        config.audio(false, 0, 1),
        config.audio(true, 3, 2),
        config.audio(true, 0, 5),
        config.audio(false, 0, 3),
        config.audio(true, 0, 4),
    ]
}

fn parsing(config: Config) -> ParseOptions {
    ParseOptions {
        allow_empty_codebooks: config.needs_empty_policy(),
        ..ParseOptions::default()
    }
}
fn decoding(config: Config) -> DecodeOptions {
    DecodeOptions {
        allow_nonfinite: config.usage == Use::FloorZero && config.needs_empty_policy(),
        ..DecodeOptions::default()
    }
}
fn native(config: Config) -> deadlib_audio_decode_vorbis::PacketDecoder {
    use deadlib_audio_decode_vorbis::{Identification, PacketDecoder, Setup};
    let headers = config.headers();
    let id = Identification::parse(&headers[0]).unwrap();
    let setup = Setup::parse_with_options(&headers[2], &id, parsing(config)).unwrap();
    PacketDecoder::with_options(setup, decoding(config)).unwrap()
}

fn pcm_key(value: f32) -> u32 {
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

#[test]
fn codebook_policies_survive_streaming_reset_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use deadlib_audio_decode_vorbis::*;
    use std::io::Cursor;
    let mut seeks = 0;
    for config in regressions() {
        let packets = sequence(config);
        let mut writer = PacketWriter::new(Vec::new(), 71);
        for header in config.headers() {
            writer.write_packet(&header, 0, false).unwrap();
        }
        writer.flush_page().unwrap();
        let mut position = 0;
        let mut raw = native(config);
        let mut expected = vec![Vec::new(); config.channels as usize];
        for (i, (packet, frames)) in packets.iter().zip([0, 80, 128, 80, 80]).enumerate() {
            let block = raw.decode(packet).unwrap().unwrap();
            assert_eq!(block.start_frame, position);
            assert_eq!(block.frames, frames as usize);
            for (out, plane) in expected.iter_mut().zip(block.planes()) {
                out.extend(plane.iter().copied().map(pcm_key));
            }
            position += frames;
            writer
                .write_packet(packet, position, i + 1 == packets.len())
                .unwrap();
            writer.flush_page().unwrap();
        }
        assert_eq!(position, 368);
        let bytes = writer.into_inner();
        if config.needs_empty_policy() {
            let mut strict = OggReader::new(Cursor::new(&bytes));
            while let Ok(true) = strict.read_next(|_| {}) {}
            assert!(matches!(strict.read_next(|_| {}), Err(Error::Failed)));
            assert!(VorbisIndex::read(&mut Cursor::new(&bytes), SeekOptions::default()).is_err());
        }
        // Reset must discard the preceding nonfinite overlap and retain options.
        raw.reset();
        let mut fresh = native(config);
        for packet in [config.audio(false, 0, 0), config.audio(true, 0, 4)] {
            let a = raw.decode(&packet).unwrap().unwrap();
            let b = fresh.decode(&packet).unwrap().unwrap();
            assert_eq!(a.start_frame, b.start_frame);
            for (a, b) in a.planes().zip(b.planes()) {
                assert_eq!(
                    a.iter().copied().map(pcm_key).collect::<Vec<_>>(),
                    b.iter().copied().map(pcm_key).collect::<Vec<_>>()
                );
            }
        }
        let options = OggOptions {
            allow_empty_codebooks: config.needs_empty_policy(),
            allow_nonfinite: decoding(config).allow_nonfinite,
            residue2_layout: Residue2Layout::Libvorbis,
            ..OggOptions::default()
        };
        let collect = |actual: &mut Vec<Vec<u32>>, event: OggEvent<'_>| {
            if let OggEvent::Pcm(block) = event {
                assert_eq!(block.start_frame as usize, actual[0].len());
                assert_eq!(block.start_granule, block.start_frame);
                for (out, plane) in actual.iter_mut().zip(block.pcm.planes()) {
                    out.extend(plane.iter().copied().map(pcm_key));
                }
            }
        };
        let mut pull = OggReader::with_options(Cursor::new(&bytes), options);
        let mut actual = vec![Vec::new(); config.channels as usize];
        while pull.read_next(|event| collect(&mut actual, event)).unwrap() {}
        assert_eq!(actual, expected, "pull {config:?}");
        let mut stream = OggStreamDecoder::new(options);
        for chunk_size in [1, 17, bytes.len()] {
            stream.reset();
            let mut actual = vec![Vec::new(); config.channels as usize];
            let mut ends = 0;
            for chunk in bytes.chunks(chunk_size) {
                let mut input = chunk;
                while !input.is_empty() {
                    let used = stream
                        .push(input, |event| {
                            if let OggEvent::StreamEnd(end) = &event {
                                assert_eq!(end.frames, 368);
                                ends += 1;
                            }
                            collect(&mut actual, event);
                        })
                        .unwrap();
                    assert!(used > 0);
                    input = &input[used..];
                }
            }
            stream.finish().unwrap();
            assert_eq!(ends, 1);
            assert_eq!(actual, expected, "push {config:?}, chunk {chunk_size}");
        }
        for packet_preroll in [false, true] {
            let options = SeekOptions {
                allow_empty_codebooks: options.allow_empty_codebooks,
                allow_nonfinite: options.allow_nonfinite,
                residue2_layout: options.residue2_layout,
                packet_preroll,
                ..SeekOptions::default()
            };
            let index = VorbisIndex::read(&mut Cursor::new(&bytes), options).unwrap();
            let mut reader = OggSeekReader::with_index(Cursor::new(&bytes), index).unwrap();
            let id = reader.selected().info.id;
            for target in [0, 1, 79, 80, 81, 207, 208, 287, 288, 367, 368, 0, 81] {
                reader.seek(id, target).unwrap();
                assert_eq!(reader.position(), target);
                let mut actual = vec![Vec::new(); config.channels as usize];
                let mut step = 0;
                loop {
                    let emit = |event: OggEvent<'_>| {
                        if let OggEvent::Pcm(block) = event {
                            assert_eq!(block.start_frame, target + actual[0].len() as u64);
                            for (out, plane) in actual.iter_mut().zip(block.pcm.planes()) {
                                out.extend(plane.iter().copied().map(pcm_key));
                            }
                        }
                    };
                    let more = if step % 2 == 0 {
                        reader.read_packet(emit)
                    } else {
                        reader.read_next(emit)
                    }
                    .unwrap();
                    if !more {
                        break;
                    }
                    step += 1;
                }
                for (actual, expected) in actual.iter().zip(&expected) {
                    assert_eq!(
                        actual,
                        &expected[target as usize..],
                        "seek {target}, {config:?}, packet pre-roll {packet_preroll}"
                    );
                }
                seeks += 1;
            }
        }
    }
    assert_eq!(seeks, 286);
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn zero_dimension_scalar_books_match_libvorbis() {
    for usage in [Use::Unused, Use::FloorMaster, Use::FloorSubclass] {
        for entries in [1, 4, 16] {
            for lookup in [0, 2] {
                for coupling in [false, true] {
                    let config = Config {
                        usage,
                        dimensions: 0,
                        entries,
                        lookup,
                        empty: false,
                        coupling,
                        ..Config::default()
                    };
                    let headers = config.headers();
                    let packets = sequence(config);
                    let mut a = native(config);
                    let mut b = native(Config {
                        dimensions: 1,
                        ..config
                    });
                    for packet in &packets {
                        let a = a.decode(packet).unwrap().unwrap();
                        let b = b.decode(packet).unwrap().unwrap();
                        assert_eq!(a.start_frame, b.start_frame);
                        assert_eq!(a.frames, b.frames);
                        assert_eq!(
                            a.planes().collect::<Vec<_>>(),
                            b.planes().collect::<Vec<_>>()
                        );
                    }
                    let cases = [Case {
                        name: format!("{config:?}"),
                        packets,
                    }];
                    let label = format!(
                        "scalar-{usage:?}-entries-{entries}-lookup-{lookup}-coupling-{coupling}"
                    );
                    let reference = reference(&label, &headers, &cases);
                    report(&label, compare(&headers, &cases, &reference));
                }
            }
        }
    }
}
fn empty_configs() -> Vec<Config> {
    let mut out = Vec::new();
    for usage in [
        Use::Unused,
        Use::FloorMaster,
        Use::FloorSubclass,
        Use::FloorZero,
        Use::ResidueClass,
        Use::ResidueVector,
    ] {
        for dimensions in [0, 1, 2, 3, 4] {
            if dimensions == 0 && matches!(usage, Use::FloorZero | Use::ResidueClass) {
                continue;
            }
            for entries in [0, 1, 4] {
                if entries == 0 && usage == Use::ResidueClass {
                    continue;
                }
                for lookup in 0..=2 {
                    if lookup == 0 && matches!(usage, Use::FloorZero | Use::ResidueVector) {
                        continue;
                    }
                    for ordered_zero in [false, true] {
                        if ordered_zero && entries != 0 {
                            continue;
                        }
                        let n = out.len();
                        out.push(Config {
                            usage,
                            dimensions,
                            entries,
                            lookup,
                            empty: true,
                            ordered_zero,
                            kind: (n % 3) as u8,
                            channels: [1, 2, 3, 5, 8, 17][n % 6],
                            coupling: n % 2 != 0 && n % 6 != 0,
                        });
                    }
                }
            }
        }
    }
    out
}
fn check_prefixes(config: Config, label: &str) {
    use deadlib_audio_decode_vorbis::{Identification, Limits, Setup};
    let headers = config.headers();
    let id = Identification::parse(&headers[0]).unwrap();
    assert!(Setup::parse(&headers[2], &id, &Limits::default()).is_err());
    for (number, (long, windows, seed)) in [
        (false, 0, 0),
        (false, 0, 1),
        (true, 0, 2),
        (true, 3, 3),
        (true, 1, 5),
    ]
    .into_iter()
    .enumerate()
    {
        let packet = config.audio(long, windows, seed);
        let lengths: Vec<_> = if config.channels <= 8 {
            (0..=packet.len()).collect()
        } else {
            let mut selected =
                std::collections::BTreeSet::from([0, 1, packet.len(), packet.len() - 1]);
            for i in 0..=16 {
                selected.insert(packet.len() * i / 16);
            }
            selected.into_iter().collect()
        };
        for (batch, lengths) in lengths.chunks(32).enumerate() {
            let cases: Vec<_> = lengths
                .iter()
                .map(|&length| Case {
                    name: format!("{label}/candidate-{number}/prefix-{length}"),
                    packets: vec![
                        config.audio(false, 0, 1),
                        config.audio(true, 3, 2),
                        packet[..length].to_vec(),
                        config.audio(false, 0, 3),
                        config.audio(true, 0, 4),
                    ],
                })
                .collect();
            let label = format!("{label}-candidate-{number}-batch-{batch}");
            let reference = reference(&label, &headers, &cases);
            report(
                &label,
                raw_cases::compare_with_parse_options(
                    &headers,
                    &cases,
                    &reference,
                    decoding(config),
                    parsing(config),
                ),
            );
        }
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn empty_floor_zero_preserves_reference_ieee_propagation_after_later_floor_eof() {
    let config = Config {
        usage: Use::FloorZero,
        entries: 0,
        lookup: 1,
        kind: 0,
        ..Config::default()
    };
    check_prefixes(config, "floor-zero-later-floor-eof");
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn empty_codebook_consumers_match_libvorbis() {
    for (index, config) in empty_configs().into_iter().enumerate() {
        let label = format!(
            "empty-{index}-role-{:?}-dim-{}-entries-{}-lookup-{}-ordered-{}-channels-{}-kind-{}-coupling-{}",
            config.usage,
            config.dimensions,
            config.entries,
            config.lookup,
            config.ordered_zero,
            config.channels,
            config.kind,
            config.coupling
        );
        check_prefixes(config, &label);
    }
}
