#[path = "support/raw_cases.rs"]
mod raw_cases;
mod support;
#[path = "support/degenerate_books.rs"]
mod vectors;
use deadlib_audio_decode_vorbis::{DecodeOptions, ParseOptions};
use raw_cases::{Case, compare, reference, report};
use vectors::{Config, Use};

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
                            channels: [2, 3, 5, 8, 17][n % 5],
                            coupling: n % 2 != 0,
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
