#[path = "support/raw_cases.rs"]
mod raw_cases;
mod support;
#[path = "support/residue_vectors.rs"]
mod vectors;
use deadlib_audio_decode_vorbis::*;
use raw_cases::{Case, compare, reference, report};
use vectors::Geometry;

#[test]
#[ignore = "requires external libvorbis oracle"]
fn unaligned_residue_two_matches_libvorbis() {
    for channels in [2, 3, 5, 7, 17] {
        for submaps in [1, 2] {
            for alignment in 1..=3 {
                let config = Geometry {
                    channels,
                    submaps,
                    kind: 2,
                    class_dimensions: channels % 3 + 1,
                    begin: 1,
                    clipped_end: alignment & 1 != 0,
                    coupling: submaps > 1,
                    alignment,
                };
                let label =
                    format!("unaligned-ch-{channels}-submaps-{submaps}-alignment-{alignment}");
                check(config, &label, Residue2Layout::Libvorbis);
            }
        }
    }
}

fn check(config: Geometry, label: &str, layout: Residue2Layout) {
    let headers = config.headers();
    for seed in 0..6 {
        for long in [false, true] {
            for windows in 0..if long { 4 } else { 1 } {
                let packet = config.audio(long, windows, seed);
                let lengths: Vec<_> = if config.channels <= 8 {
                    (0..=packet.len()).collect()
                } else {
                    // Bound wide-channel reference output. Retain packet ends,
                    // short prefixes and deterministic interior boundaries.
                    let mut selected = std::collections::BTreeSet::new();
                    for len in [0, 1, 2, 3, 7, 8, 15, 16, 31, 32, 63, 64] {
                        selected.insert(len.min(packet.len()));
                    }
                    for part in 0..=16 {
                        selected.insert(packet.len() * part / 16);
                    }
                    selected.insert(packet.len().saturating_sub(1));
                    selected.into_iter().collect()
                };
                for (batch, lengths) in lengths.chunks(32).enumerate() {
                    let cases: Vec<_> = lengths
                        .iter()
                        .map(|&len| Case {
                            name: format!(
                                "{config:?}/seed-{seed}/long-{long}/windows-{windows}/prefix-{len}"
                            ),
                            packets: vec![
                                config.audio(false, 0, 1),
                                config.audio(true, 3, 2),
                                packet[..len].to_vec(),
                                config.audio(false, 0, 3),
                                config.audio(true, 0, 4),
                            ],
                        })
                        .collect();
                    let label =
                        format!("{label}-seed-{seed}-long-{long}-windows-{windows}-batch-{batch}");
                    let reference = reference(&label, &headers, &cases);
                    report(
                        &label,
                        if layout == Residue2Layout::Interleaved {
                            compare(&headers, &cases, &reference)
                        } else {
                            raw_cases::compare_with_options(
                                &headers,
                                &cases,
                                &reference,
                                DecodeOptions {
                                    residue2_layout: layout,
                                    ..DecodeOptions::default()
                                },
                            )
                        },
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn mixed_residue_books_submaps_and_channel_masks_match_libvorbis() {
    for channels in [1, 2, 3, 5, 6, 8, 17, 255] {
        for (submaps, dimensions, begin, clipped_end, coupling) in [
            (1, 1, 0, false, false),
            (1, 3, 1, true, false),
            (2, 2, 3, false, false),
            (3, 3, 1, true, true),
            (16, 2, 40, true, true),
        ] {
            for kind in 0..=2 {
                let config = Geometry {
                    channels,
                    submaps,
                    kind,
                    class_dimensions: dimensions,
                    begin,
                    clipped_end,
                    coupling,
                    alignment: 0,
                };
                let label = format!(
                    "geometry-ch-{channels}-submaps-{submaps}-kind-{kind}-dim-{dimensions}-begin-{begin}-clip-{clipped_end}-coupling-{coupling}"
                );
                check(config, &label, Residue2Layout::Interleaved);
            }
        }
    }
}

fn regressions() -> Vec<Geometry> {
    [
        (2, 1, 2, 0, false, false, 1),
        (5, 1, 3, 0, true, false, 2),
        (7, 2, 2, 1, true, true, 3),
        (17, 1, 3, 1, true, true, 3),
    ]
    .into_iter()
    .map(
        |(channels, submaps, class_dimensions, begin, clipped_end, coupling, alignment)| Geometry {
            channels,
            submaps,
            kind: 2,
            class_dimensions,
            begin,
            clipped_end,
            coupling,
            alignment,
        },
    )
    .collect()
}
fn sequence(config: Geometry) -> Vec<Vec<u8>> {
    vec![
        config.audio(false, 0, 1),
        config.audio(true, 3, 2),
        config.audio(true, 0, 5),
        config.audio(false, 0, 3),
        config.audio(true, 0, 4),
    ]
}
#[test]
fn unaligned_residue_reference_layout_matches_retained_pcm() {
    let mut golden = include_bytes!("fixtures/residue-layout-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVRGEO01");
    let configs = regressions();
    assert_eq!(support::u32_in(&mut golden) as usize, configs.len());
    for (i, config) in configs.into_iter().enumerate() {
        let length = u64::from_le_bytes(support::take(&mut golden, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut golden, length);
        let cases = [Case {
            name: format!("golden-{i}"),
            packets: sequence(config),
        }];
        report(
            &format!("golden-{i}"),
            raw_cases::compare_with_options(
                &config.headers(),
                &cases,
                reference,
                DecodeOptions {
                    residue2_layout: Residue2Layout::Libvorbis,
                    ..DecodeOptions::default()
                },
            ),
        );
    }
    assert!(golden.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_residue_geometry_golden() {
    let configs = regressions();
    let mut golden = b"DVRGEO01".to_vec();
    golden.extend((configs.len() as u32).to_le_bytes());
    for (i, config) in configs.into_iter().enumerate() {
        let cases = [Case {
            name: format!("golden-{i}"),
            packets: sequence(config),
        }];
        let reference = reference(&format!("geometry-golden-{i}"), &config.headers(), &cases);
        golden.extend((reference.len() as u64).to_le_bytes());
        golden.extend(reference);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/residue-layout-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}
fn decoder(config: Geometry, layout: Residue2Layout) -> PacketDecoder {
    let headers = config.headers();
    let id = Identification::parse(&headers[0]).unwrap();
    let setup = Setup::parse(&headers[2], &id, &Limits::default()).unwrap();
    PacketDecoder::with_options(
        setup,
        DecodeOptions {
            residue2_layout: layout,
            ..DecodeOptions::default()
        },
    )
    .unwrap()
}
#[test]
fn residue_layout_is_preserved_by_ogg_reset_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    let mut difference = 0.0f32;
    for config in regressions() {
        let headers = config.headers();
        let packets = sequence(config);
        let mut writer = PacketWriter::new(Vec::new(), 71);
        for header in headers {
            writer.write_packet(&header, 0, false).unwrap();
        }
        writer.flush_page().unwrap();
        let mut position = 0;
        for (i, (packet, frames)) in packets.iter().zip([0, 80, 128, 80, 80]).enumerate() {
            position += frames;
            writer
                .write_packet(packet, position, i + 1 == packets.len())
                .unwrap();
            writer.flush_page().unwrap();
        }
        assert_eq!(position, 368);
        let bytes = writer.into_inner();
        let mut standards = Vec::new();
        for layout in [Residue2Layout::Interleaved, Residue2Layout::Libvorbis] {
            let mut raw = decoder(config, layout);
            let mut expected = vec![Vec::new(); config.channels as usize];
            for packet in &packets {
                let block = raw.decode(packet).unwrap().unwrap();
                for (out, plane) in expected.iter_mut().zip(block.planes()) {
                    out.extend_from_slice(plane);
                }
            }
            if layout == Residue2Layout::Interleaved {
                standards = expected.clone();
            } else {
                for (a, b) in standards.iter().flatten().zip(expected.iter().flatten()) {
                    difference = difference.max((a - b).abs());
                }
            }
            let mut stream = OggStreamDecoder::new(OggOptions {
                residue2_layout: layout,
                ..OggOptions::default()
            });
            for chunk_size in [1, 17, bytes.len()] {
                stream.reset();
                let mut actual = vec![Vec::new(); config.channels as usize];
                let mut ends = 0;
                for chunk in bytes.chunks(chunk_size) {
                    let mut input = chunk;
                    while !input.is_empty() {
                        let used = stream
                            .push(input, |event| match event {
                                OggEvent::Pcm(block) => {
                                    assert_eq!(block.start_frame as usize, actual[0].len());
                                    assert_eq!(block.start_granule, block.start_frame);
                                    for (out, plane) in actual.iter_mut().zip(block.pcm.planes()) {
                                        out.extend_from_slice(plane);
                                    }
                                }
                                OggEvent::StreamEnd(end) => {
                                    assert_eq!(end.frames, 368);
                                    ends += 1;
                                }
                                _ => {}
                            })
                            .unwrap();
                        assert!(used > 0);
                        input = &input[used..];
                    }
                }
                stream.finish().unwrap();
                assert_eq!(ends, 1);
                assert_eq!(actual, expected);
            }
            for packet_preroll in [false, true] {
                let options = SeekOptions {
                    residue2_layout: layout,
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
                                    out.extend_from_slice(plane);
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
                        assert_eq!(actual, &expected[target as usize..]);
                    }
                }
            }
        }
    }
    // The explicit option must not silently replace normative default output.
    assert!(difference > 0.5);
}
