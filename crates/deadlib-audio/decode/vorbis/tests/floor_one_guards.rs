mod support;
use codec::*;
use deadlib_audio_decode_vorbis as codec;
#[path = "support/raw_cases.rs"]
#[allow(dead_code)]
mod raw_cases;
#[path = "support/floor1_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use vectors::Config;
fn parsing() -> ParseOptions {
    ParseOptions {
        codebook_behavior: CodebookBehavior::Libvorbis,
        residue_behavior: ResidueBehavior::Libvorbis,
        ..ParseOptions::default()
    }
}
fn decoding(behavior: Floor1Behavior) -> DecodeOptions {
    DecodeOptions {
        floor1_behavior: behavior,
        allow_nonfinite: true,
        residue2_layout: Residue2Layout::Libvorbis,
        ..DecodeOptions::default()
    }
}
fn sequence(c: Config) -> Vec<Vec<u8>> {
    [
        (false, 0, 1),
        (true, 3, 2),
        (true, 0, 3),
        (false, 0, 5),
        (true, 0, 4),
    ]
    .into_iter()
    .map(|(long, w, seed)| c.audio(long, w, seed))
    .collect()
}
fn configurations() -> Vec<Config> {
    let mut out = Vec::new();
    for channels in [1, 3, 8] {
        for multiplier in 1..=4 {
            for submaps in [1, 2, 4] {
                for posts in [0, 1, 7] {
                    for (i, high) in [64, 32768, 65535].into_iter().enumerate() {
                        out.push(Config {
                            channels,
                            multiplier,
                            submaps,
                            posts,
                            high,
                            subclasses: [0, 1, 3][i],
                            kind: i as u8,
                            coupling: channels > 1,
                            ..Config::default()
                        });
                    }
                }
            }
        }
    }
    for multiplier in 1..=4 {
        for high in [64, 32768, 65535] {
            out.push(Config {
                multiplier,
                high,
                posts: 63,
                subclasses: 3,
                channels: 8,
                submaps: 4,
                kind: 2,
                range_bits: 15,
                ..Config::default()
            });
        }
    }
    out
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn floor_one_packet_guards_prefixes_and_recovery_match_reference() {
    let configs = configurations();
    for (i, c) in configs.iter().copied().enumerate() {
        let headers = c.headers();
        let full = sequence(c);
        let mut cases = vec![raw_cases::Case {
            name: format!("{c:?}/complete"),
            packets: full.clone(),
        }];
        for long in [false, true] {
            let candidate = c.audio(long, 0, 3);
            let lengths: std::collections::BTreeSet<_> = if c.channels <= 3 {
                (0..=candidate.len()).collect()
            } else {
                [
                    0,
                    1,
                    2,
                    3,
                    7,
                    15,
                    candidate.len() / 2,
                    candidate.len().saturating_sub(1),
                    candidate.len(),
                ]
                .into_iter()
                .map(|n| n.min(candidate.len()))
                .collect()
            };
            for n in lengths {
                let mut packets = full.clone();
                packets[2] = candidate[..n].to_vec();
                cases.push(raw_cases::Case {
                    name: format!("{c:?}/long-{long}/prefix-{n}"),
                    packets,
                });
            }
        }
        for (batch, cases) in cases.chunks(32).enumerate() {
            let label = format!("floor1-pcm-{i}-batch-{batch}");
            let reference = raw_cases::reference(&label, &headers, cases);
            raw_cases::report(
                &label,
                raw_cases::compare_with_parse_options(
                    &headers,
                    cases,
                    &reference,
                    decoding(Floor1Behavior::Libvorbis),
                    parsing(),
                ),
            );
        }
    }
    eprintln!("floor1 packet configurations: {}", configs.len());
}
fn regressions() -> Vec<Config> {
    vec![
        Config::default(),
        Config {
            high: 32768,
            posts: 7,
            ..Config::default()
        },
        Config {
            high: 65535,
            posts: 63,
            multiplier: 3,
            subclasses: 3,
            range_bits: 15,
            channels: 8,
            submaps: 4,
            kind: 2,
            ..Config::default()
        },
        Config {
            posts: 0,
            multiplier: 3,
            channels: 1,
            submaps: 1,
            coupling: false,
            ..Config::default()
        },
        Config {
            high: 65535,
            posts: 7,
            multiplier: 2,
            absent: true,
            kind: 2,
            ..Config::default()
        },
        Config {
            high: 65536,
            posts: 1,
            multiplier: 1,
            channels: 8,
            submaps: 4,
            kind: 0,
            ..Config::default()
        },
    ]
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_floor1_pcm_golden() {
    let configs = regressions();
    let mut out = b"DF1PCM01".to_vec();
    out.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.into_iter().enumerate() {
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: sequence(c),
        }];
        let reference = raw_cases::reference(&format!("floor1-golden-{i}"), &c.headers(), &cases);
        out.extend((reference.len() as u64).to_le_bytes());
        out.extend(reference);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floor1-pcm-v1.libvorbis-pcm"),
        out,
    )
    .unwrap();
}
#[test]
fn floor_one_pcm_matches_retained_reference() {
    let mut wire = include_bytes!("fixtures/floor1-pcm-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut wire, 8), b"DF1PCM01");
    let configs = regressions();
    assert_eq!(support::u32_in(&mut wire) as usize, configs.len());
    for (i, c) in configs.into_iter().enumerate() {
        let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut wire, n);
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: sequence(c),
        }];
        raw_cases::report(
            &format!("floor1-golden-{i}"),
            raw_cases::compare_with_parse_options(
                &c.headers(),
                &cases,
                reference,
                decoding(Floor1Behavior::Libvorbis),
                parsing(),
            ),
        );
    }
    assert!(wire.is_empty());
}

fn pcm_bits(value: &f32) -> u32 {
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}
#[test]
fn floor_one_policy_survives_reset_streaming_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    let mut seeks = 0;
    for (i, c) in regressions().into_iter().enumerate() {
        for behavior in [Floor1Behavior::Vorbis, Floor1Behavior::Libvorbis] {
            let headers = c.headers();
            let id = Identification::parse(&headers[0]).unwrap();
            let setup = Setup::parse_with_options(&headers[2], &id, parsing()).unwrap();
            let mut raw = PacketDecoder::with_options(setup, decoding(behavior)).unwrap();
            let packets = sequence(c);
            let mut expected = vec![Vec::new(); c.channels as usize];
            let mut writer = PacketWriter::new(Vec::new(), 97);
            for h in &headers {
                writer.write_packet(h, 0, false).unwrap();
            }
            writer.flush_page().unwrap();
            let mut ends = vec![0u64];
            for (j, p) in packets.iter().enumerate() {
                let block = raw.decode(p).unwrap().unwrap();
                assert_eq!(block.start_frame, *ends.last().unwrap());
                let end = block.start_frame + block.frames as u64;
                for (out, plane) in expected.iter_mut().zip(block.planes()) {
                    out.extend(plane.iter().map(pcm_bits));
                }
                ends.push(end);
                writer.write_packet(p, end, j + 1 == packets.len()).unwrap();
                writer.flush_page().unwrap();
            }
            let total = *ends.last().unwrap();
            assert_eq!(total as usize, 368);
            let bytes = writer.into_inner();
            if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOOR1_INPUTS") {
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(
                    std::path::Path::new(&root).join(format!("floor1-stream-{i}.ogg")),
                    &bytes,
                )
                .unwrap();
            }
            raw.reset();
            for (j, p) in packets.iter().enumerate() {
                let block = raw.decode(p).unwrap().unwrap();
                assert_eq!(block.start_frame, ends[j]);
                for (plane, expected) in block.planes().zip(&expected) {
                    assert_eq!(
                        plane.iter().map(pcm_bits).collect::<Vec<_>>(),
                        expected[ends[j] as usize..ends[j + 1] as usize]
                    );
                }
            }
            let options = OggOptions {
                floor0_behavior: Floor0Behavior::Libvorbis,
                codebook_behavior: CodebookBehavior::Libvorbis,
                residue_behavior: ResidueBehavior::Libvorbis,
                floor1_behavior: behavior,
                residue2_layout: Residue2Layout::Libvorbis,
                allow_nonfinite: true,
                allow_empty_codebooks: false,
                ..OggOptions::default()
            };
            let collect = |out: &mut Vec<Vec<u32>>, event: OggEvent<'_>| match event {
                OggEvent::Pcm(block) => {
                    assert_eq!(block.start_frame as usize, out[0].len());
                    assert_eq!(block.start_granule, block.start_frame);
                    for (out, plane) in out.iter_mut().zip(block.pcm.planes()) {
                        out.extend(plane.iter().map(pcm_bits));
                    }
                }
                OggEvent::StreamEnd(end) => assert_eq!(end.frames, total),
                _ => {}
            };
            let mut pull = OggReader::with_options(Cursor::new(&bytes), options);
            let mut out = vec![Vec::new(); c.channels as usize];
            while pull.read_next(|e| collect(&mut out, e)).unwrap() {}
            assert_eq!(out, expected, "pull {c:?}");
            let mut stream = OggStreamDecoder::new(options);
            for size in [1, 17, bytes.len()] {
                stream.reset();
                let mut out = vec![Vec::new(); c.channels as usize];
                for chunk in bytes.chunks(size) {
                    let mut input = chunk;
                    while !input.is_empty() {
                        let used = stream.push(input, |e| collect(&mut out, e)).unwrap();
                        assert!(used > 0);
                        input = &input[used..];
                    }
                }
                stream.finish().unwrap();
                assert_eq!(out, expected, "push {c:?}, chunk {size}");
            }
            let targets: Vec<_> = [0, 1, total - 1, total, 0]
                .into_iter()
                .chain(
                    ends[2..ends.len() - 1]
                        .iter()
                        .flat_map(|&e| [e - 1, e, e + 1]),
                )
                .collect();
            for packet_preroll in [false, true] {
                let index = VorbisIndex::read(
                    &mut Cursor::new(&bytes),
                    SeekOptions {
                        floor0_behavior: options.floor0_behavior,
                        codebook_behavior: options.codebook_behavior,
                        residue_behavior: options.residue_behavior,
                        floor1_behavior: options.floor1_behavior,
                        residue2_layout: options.residue2_layout,
                        allow_nonfinite: options.allow_nonfinite,
                        allow_empty_codebooks: options.allow_empty_codebooks,
                        packet_preroll,
                        ..SeekOptions::default()
                    },
                )
                .unwrap();
                let mut reader = OggSeekReader::with_index(Cursor::new(&bytes), index).unwrap();
                let id = reader.selected().info.id;
                for &target in &targets {
                    reader.seek(id, target).unwrap();
                    assert_eq!(reader.position(), target);
                    let mut out = vec![Vec::new(); c.channels as usize];
                    while reader
                        .read_next(|e| {
                            if let OggEvent::Pcm(block) = e {
                                assert_eq!(block.start_frame, target + out[0].len() as u64);
                                for (out, plane) in out.iter_mut().zip(block.pcm.planes()) {
                                    out.extend(plane.iter().map(pcm_bits));
                                }
                            }
                        })
                        .unwrap()
                    {}
                    for (out, expected) in out.iter().zip(&expected) {
                        assert_eq!(
                            out,
                            &expected[target as usize..],
                            "seek {target}, {c:?}, packet pre-roll {packet_preroll}"
                        );
                    }
                    seeks += 1;
                }
            }
        }
    }
    assert_eq!(seeks, 336);
}
