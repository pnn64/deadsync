mod support;
#[path = "support/synthetic.rs"]
mod synthetic;
use deadlib_audio_decode_vorbis::*;
use synthetic::Config;

struct Case {
    name: String,
    packets: Vec<Vec<u8>>,
}
fn recovery(config: Config, name: String, packet: Vec<u8>) -> Case {
    let last = config.modes - 1;
    Case {
        name,
        packets: vec![
            config.audio(0, 0, 1, None),
            config.audio(last, 3, 2, None),
            packet,
            config.audio(0, 0, 3, None),
            config.audio(last, 0, 4, None),
        ],
    }
}
fn request(headers: &[Vec<u8>], cases: &[Case]) -> Vec<u8> {
    let mut wire = Vec::new();
    for header in headers {
        wire.extend((header.len() as u64).to_le_bytes());
        wire.extend(header);
    }
    wire.extend((cases.len() as u64).to_le_bytes());
    for case in cases {
        wire.extend((case.packets.len() as u64).to_le_bytes());
        for packet in &case.packets {
            wire.extend((packet.len() as u64).to_le_bytes());
            wire.extend(packet);
        }
    }
    wire
}
fn reference(label: &str, headers: &[Vec<u8>], cases: &[Case]) -> Vec<u8> {
    let wire = request(headers, cases);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_CONFIGURATION_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join(format!("{label}.bin")),
            &wire,
        )
        .unwrap();
    }
    support::file_oracle("pcm-cases", &wire)
}
#[derive(Default)]
struct Counts {
    cases: usize,
    packets: usize,
    discarded: usize,
    pcm: support::Difference,
}
fn compare(headers: &[Vec<u8>], cases: &[Case], reference: &[u8]) -> Counts {
    let mut native = support::decoder(headers);
    let mut wire = reference;
    assert_eq!(support::u32_in(&mut wire), 2);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()),
        cases.len() as u64
    );
    let mut total = Counts {
        cases: cases.len(),
        ..Counts::default()
    };
    for case in cases {
        native.reset();
        let mut position = 0;
        for (i, packet) in case.packets.iter().enumerate() {
            total.packets += 1;
            let kind = support::take(&mut wire, 1)[0];
            let block = native
                .decode(packet)
                .unwrap_or_else(|e| panic!("{}/step-{i}: {e}", case.name));
            if kind == b'S' {
                assert!(block.is_none(), "{}/step-{i}", case.name);
                assert_eq!(native.position(), position);
                total.discarded += 1;
                continue;
            }
            assert_eq!(kind, b'P');
            let block = block.unwrap_or_else(|| panic!("{}/step-{i}: native discarded", case.name));
            assert_eq!(
                support::u32_in(&mut wire) as usize,
                block.block_size,
                "{}/step-{i}",
                case.name
            );
            assert_eq!(
                support::u32_in(&mut wire) as usize,
                block.frames,
                "{}/step-{i}",
                case.name
            );
            assert_eq!(block.start_frame, position, "{}/step-{i}", case.name);
            position += block.frames as u64;
            let mut difference = support::Difference::default();
            for plane in block.planes() {
                for &value in plane {
                    difference.add(value, f32::from_bits(support::u32_in(&mut wire)));
                }
            }
            if difference.peak_error > 3e-6 * difference.peak_reference.max(1.0)
                || difference.rms() > 5e-7 * difference.peak_reference.max(1.0)
            {
                difference.check(&format!("{}/step-{i}", case.name));
            }
            total.pcm.merge(&difference);
        }
    }
    assert!(wire.is_empty());
    total
}
fn report(label: &str, counts: Counts) {
    eprintln!(
        "{label}: {} cases, {} packets, {} discarded",
        counts.cases, counts.packets, counts.discarded
    );
    counts.pcm.check(label);
}
#[test]
fn unused_modes_preserve_ogg_streaming_counts_and_seek_positions() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    for modes in [3, 5, 33, 63] {
        let config = Config {
            modes,
            ..Config::default()
        };
        let headers = config.headers();
        let invalid = config.audio(modes, 0, 0, None);
        // The low-level parser retains its useful diagnostic.
        assert_eq!(
            support::decoder(&headers)
                .setup()
                .read_audio_header(&mut bitpack::BitReader::new(&invalid)),
            Err(Error::Invalid("audio mode number"))
        );
        let packets = [
            invalid.clone(),
            config.audio(0, 0, 1, None),
            config.audio(1, 3, 2, None),
            invalid,
            config.audio(0, 0, 3, None),
            vec![1],
            config.audio(1, 0, 4, None),
            vec![],
            config.audio(0, 0, 5, None),
        ];
        let sizes: [Option<u64>; 9] = [
            None,
            Some(64),
            Some(256),
            None,
            Some(64),
            None,
            Some(256),
            None,
            Some(64),
        ];
        let mut expected = [Vec::new(), Vec::new()];
        let mut raw = support::decoder(&headers);
        let mut writer = PacketWriter::new(Vec::new(), 71);
        for header in &headers {
            writer.write_packet(header, 0, false).unwrap();
        }
        writer.flush_page().unwrap();
        let mut position = 0;
        let mut previous = None;
        for (i, (packet, size)) in packets.iter().zip(sizes).enumerate() {
            if let Some(size) = size {
                if let Some(previous) = previous {
                    position += (previous + size) / 4;
                }
                previous = Some(size);
                let block = raw.decode(packet).unwrap().unwrap();
                for (out, plane) in expected.iter_mut().zip(block.planes()) {
                    out.extend_from_slice(plane);
                }
            }
            writer
                .write_packet(packet, position, i + 1 == packets.len())
                .unwrap();
            writer.flush_page().unwrap();
        }
        assert_eq!(position, 320);
        let bytes = writer.into_inner();
        for chunk in [1, 7, 257, bytes.len()] {
            let mut stream = OggStreamDecoder::new(OggOptions::default());
            let mut actual = [Vec::new(), Vec::new()];
            let mut ends = 0;
            for chunk in bytes.chunks(chunk) {
                let mut input = chunk;
                while !input.is_empty() {
                    let consumed = stream
                        .push(input, |event| match event {
                            OggEvent::Pcm(block) => {
                                assert_eq!(block.start_frame as usize, actual[0].len());
                                assert_eq!(block.start_granule, block.start_frame);
                                for (out, plane) in actual.iter_mut().zip(block.pcm.planes()) {
                                    out.extend_from_slice(plane);
                                }
                            }
                            OggEvent::StreamEnd(end) => {
                                assert_eq!(end.frames, 320);
                                ends += 1;
                            }
                            _ => {}
                        })
                        .unwrap();
                    assert!(consumed > 0);
                    input = &input[consumed..];
                }
            }
            stream.finish().unwrap();
            assert_eq!(ends, 1);
            assert_eq!(actual, expected);
        }
        for packet_preroll in [false, true] {
            let mut reader = OggSeekReader::with_options(
                Cursor::new(&bytes),
                SeekOptions {
                    packet_preroll,
                    ..SeekOptions::default()
                },
            )
            .unwrap();
            let stream = &reader.index().streams()[0];
            assert_eq!(stream.end.frames, 320);
            let id = stream.info.id;
            // Repeated and backwards seeks include every packet boundary.
            for target in [0, 1, 79, 80, 81, 159, 160, 239, 240, 319, 320, 0, 81] {
                reader.seek(id, target).unwrap();
                assert_eq!(reader.position(), target);
                let mut actual = [Vec::new(), Vec::new()];
                while reader
                    .read_next(|event| {
                        if let OggEvent::Pcm(block) = event {
                            assert_eq!(block.start_frame, target + actual[0].len() as u64);
                            for (out, plane) in actual.iter_mut().zip(block.pcm.planes()) {
                                out.extend_from_slice(plane);
                            }
                        }
                    })
                    .unwrap()
                {}
                for (actual, expected) in actual.iter().zip(&expected) {
                    assert_eq!(actual, &expected[target as usize..]);
                }
            }
        }
    }
}
fn golden_cases() -> Vec<(Config, Vec<Case>)> {
    let base = Config::default();
    let mut sets = vec![(
        base,
        vec![recovery(
            base,
            "unused-mode-3".into(),
            base.audio(3, 0, 0, None),
        )],
    )];
    for kind in 0..=2 {
        let config = Config {
            kind,
            stages: 3,
            ..base
        };
        let corrupt = config.audio(1, 3, 5, Some((1, 0, 7)));
        sets.push((
            config,
            vec![recovery(
                config,
                format!("residue-{kind}/surplus-word"),
                corrupt,
            )],
        ));
        let config = Config {
            empty_first_submap: true,
            ..config
        };
        sets.push((
            config,
            vec![recovery(
                config,
                format!("residue-{kind}/empty-submap"),
                config.audio(1, 0, 5, None),
            )],
        ));
    }
    let config = Config {
        kind: 0,
        classifications: 1,
        stages: 255,
        empty_first_submap: true,
        ..base
    };
    sets.push((
        config,
        vec![recovery(
            config,
            "eight-stage-cancellation".into(),
            config.audio(1, 3, 5, Some((0, 0, 1))),
        )],
    ));
    sets
}
#[test]
fn synthetic_setup_regressions_match_retained_libvorbis_pcm() {
    let mut golden = include_bytes!("fixtures/synthetic-setup-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVSSET01");
    let sets = golden_cases();
    assert_eq!(support::u32_in(&mut golden) as usize, sets.len());
    for (config, cases) in sets {
        let length = u64::from_le_bytes(support::take(&mut golden, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut golden, length);
        report(
            &format!("golden/{config:?}"),
            compare(&config.headers(), &cases, reference),
        );
    }
    assert!(golden.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_synthetic_setup_golden() {
    let sets = golden_cases();
    let mut golden = b"DVSSET01".to_vec();
    golden.extend((sets.len() as u32).to_le_bytes());
    for (i, (config, cases)) in sets.into_iter().enumerate() {
        let reference = reference(&format!("golden-{i}"), &config.headers(), &cases);
        golden.extend((reference.len() as u64).to_le_bytes());
        golden.extend(reference);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/synthetic-setup-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn all_mode_counts_and_unused_slots_match_libvorbis_with_recovery() {
    for first_long in [false, true] {
        for modes in 1..=64 {
            let config = Config {
                modes,
                first_long,
                ..Config::default()
            };
            let mut cases = Vec::new();
            let slots = 1u8 << bitpack::ilog(modes as u32 - 1);
            for mode in 0..slots {
                for windows in 0..if mode < modes && config.long(mode) {
                    4
                } else {
                    1
                } {
                    let packet = config.audio(mode, windows, mode as u32 + windows, None);
                    cases.push(recovery(
                        config,
                        format!("modes-{modes}/mode-{mode}/windows-{windows}"),
                        packet.clone(),
                    ));
                    if mode < modes && config.long(mode) {
                        // A seven-bit mode/type prefix leaves window flags in the
                        // following octet for mode tables with 33..=64 entries.
                        let prefix = (1 + bitpack::ilog(modes as u32 - 1) + 2).div_ceil(8) as usize;
                        for len in 0..prefix {
                            cases.push(recovery(
                                config,
                                format!("modes-{modes}/mode-{mode}/windows-{windows}/prefix-{len}"),
                                packet[..len].to_vec(),
                            ));
                        }
                    }
                }
            }
            cases.push(recovery(config, format!("modes-{modes}/empty"), vec![]));
            cases.push(recovery(
                config,
                format!("modes-{modes}/non-audio"),
                vec![1, 255],
            ));
            let headers = config.headers();
            let label = format!("modes-{modes}-first-long-{first_long}");
            let reference = reference(&label, &headers, &cases);
            report(&label, compare(&headers, &cases, &reference));
        }
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn residue_classwords_and_stageless_submaps_match_libvorbis_with_recovery() {
    for kind in 0..=2 {
        for dimensions in 1..=3 {
            for classifications in 1..=2 {
                for stages in [0, 1, 2, 128, 255] {
                    for empty_first_submap in [false, true] {
                        let config = Config {
                            kind,
                            dimensions,
                            classifications,
                            stages,
                            empty_first_submap,
                            ..Config::default()
                        };
                        let mut cases = Vec::new();
                        let vectors = if kind == 2 || empty_first_submap {
                            1
                        } else {
                            2
                        };
                        for word in 0..4usize.div_ceil(dimensions as usize) {
                            for vector in 0..vectors {
                                for entry in 0..8 {
                                    let name = format!(
                                        "{config:?}/word-{word}/vector-{vector}/entry-{entry}"
                                    );
                                    cases.push(recovery(
                                        config,
                                        name,
                                        config.audio(1, 3, 5, Some((word, vector, entry))),
                                    ));
                                }
                            }
                        }
                        let headers = config.headers();
                        let label = format!(
                            "residue-{kind}-dim-{dimensions}-classes-{classifications}-stages-{stages}-submap-{empty_first_submap}"
                        );
                        let reference = reference(&label, &headers, &cases);
                        report(&label, compare(&headers, &cases, &reference));
                    }
                }
            }
        }
    }
}
