#[path = "support/float_vectors.rs"]
#[allow(dead_code)]
mod floats;
#[path = "support/floor0_vectors.rs"]
#[allow(dead_code)]
mod floor_zero;
#[path = "support/raw_cases.rs"]
#[allow(dead_code)]
mod raw_cases;
#[path = "support/residue_vectors.rs"]
mod residue;
mod support;
use codec::*;
use deadlib_audio_decode_vorbis as codec;
use floats::replace_floats;
#[derive(Clone, Copy, Debug)]
enum Shape {
    Residue(residue::Geometry),
    Floor(floor_zero::Config, u8),
}
#[derive(Clone, Copy, Debug)]
struct Config {
    shape: Shape,
    minimum: u32,
    delta: u32,
}
impl Config {
    fn headers(self) -> Vec<Vec<u8>> {
        let mut h = match self.shape {
            Shape::Residue(c) => c.headers(),
            Shape::Floor(c, _) => c.headers(),
        };
        let books = match self.shape {
            Shape::Residue(_) => vec![1, 2, 3, 4],
            Shape::Floor(_, 1) => vec![1],
            Shape::Floor(_, 2) => vec![2],
            Shape::Floor(_, 3) => vec![1, 2],
            _ => unreachable!(),
        };
        replace_floats(&mut h, &books, self.minimum, self.delta);
        h
    }
    fn audio(self, long: bool, windows: u32, seed: usize) -> Vec<u8> {
        match self.shape {
            Shape::Residue(c) => c.audio(long, windows, seed),
            Shape::Floor(c, _) => c.audio(long, windows, c.amplitude(seed), 0, seed),
        }
    }
    fn sequence(self) -> Vec<Vec<u8>> {
        [
            (false, 0, 1),
            (true, 3, 2),
            (true, 0, 3),
            (false, 0, 5),
            (true, 0, 4),
        ]
        .into_iter()
        .map(|(long, w, seed)| self.audio(long, w, seed))
        .collect()
    }
    fn channels(self) -> usize {
        match self.shape {
            Shape::Residue(c) => c.channels as usize,
            Shape::Floor(c, _) => c.channels as usize,
        }
    }
}
fn pairs() -> Vec<(u32, u32)> {
    vec![
        ((724 << 21) | 1, 0),
        ((725 << 21) | 1, 0),
        ((851 << 21) | 1, 0),
        ((852 << 21) | 1, 0),
        ((1023 << 21) | 1, 0),
        (0, (1023 << 21) | 0x1fffff),
        ((724 << 21) | 1, (1023 << 21) | 1),
        (0x80000000 | (1023 << 21) | 1, (852 << 21) | 1),
        (
            (1023 << 21) | 0x1fffff,
            0x80000000 | (1023 << 21) | 0x1fffff,
        ),
        ((725 << 21) | 1, 0x80000001),
    ]
}
fn configurations() -> Vec<Config> {
    let mut out = Vec::new();
    for channels in [1, 2, 3, 8, 17] {
        for kind in 0..=2 {
            for submaps in [1, 2] {
                let shape = Shape::Residue(residue::Geometry {
                    channels,
                    submaps,
                    kind,
                    class_dimensions: 2,
                    begin: 1,
                    clipped_end: true,
                    coupling: channels > 1 && submaps > 1,
                    alignment: 3,
                });
                for (minimum, delta) in pairs() {
                    out.push(Config {
                        shape,
                        minimum,
                        delta,
                    });
                }
            }
        }
    }
    for (channels, order, dimensions, coupling) in [
        (1, 1, 1, false),
        (2, 7, 3, false),
        (3, 31, 3, true),
        (8, 31, 8, true),
    ] {
        for target in 1..=3 {
            let shape = Shape::Floor(
                floor_zero::Config {
                    channels,
                    order,
                    dimensions,
                    coupling,
                    ..floor_zero::Config::default()
                },
                target,
            );
            for (minimum, delta) in pairs() {
                out.push(Config {
                    shape,
                    minimum,
                    delta,
                });
            }
        }
    }
    out
}
fn parsing(behavior: CodebookBehavior) -> ParseOptions {
    ParseOptions {
        codebook_behavior: behavior,
        ..ParseOptions::default()
    }
}
fn decoding() -> DecodeOptions {
    DecodeOptions {
        allow_nonfinite: true,
        floor0_behavior: Floor0Behavior::Libvorbis,
        residue2_layout: Residue2Layout::Libvorbis,
        ..DecodeOptions::default()
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn custom_float_packet_prefixes_and_recovery_match_reference() {
    let configs = configurations();
    for (i, c) in configs.iter().copied().enumerate() {
        let headers = c.headers();
        let packets = c.sequence();
        let mut cases = vec![raw_cases::Case {
            name: format!("{c:?}/complete"),
            packets: packets.clone(),
        }];
        for (long, seed) in [(false, 5), (true, 3)] {
            let candidate = c.audio(long, 0, seed);
            let lengths: std::collections::BTreeSet<_> = if c.channels() <= 3 {
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
            for length in lengths {
                let mut packets = packets.clone();
                packets[2] = candidate[..length].to_vec();
                cases.push(raw_cases::Case {
                    name: format!("{c:?}/long-{long}/seed-{seed}/prefix-{length}"),
                    packets,
                });
            }
        }
        for (batch, cases) in cases.chunks(32).enumerate() {
            let label = format!("float-pcm-{i}-batch-{batch}");
            let reference = raw_cases::reference(&label, &headers, cases);
            raw_cases::report(
                &label,
                raw_cases::compare_with_parse_options(
                    &headers,
                    cases,
                    &reference,
                    decoding(),
                    parsing(CodebookBehavior::Libvorbis),
                ),
            );
        }
    }
    eprintln!("custom float packet configurations: {}", configs.len());
}

fn regressions() -> Vec<Config> {
    let configs = configurations();
    [84, 178, 185, 334, 375, 416]
        .into_iter()
        .map(|i| configs[i])
        .collect()
}
#[test]
fn custom_float_pcm_matches_retained_reference() {
    let mut golden = include_bytes!("fixtures/codebook-float-pcm-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVQPCM01");
    let configs = regressions();
    assert_eq!(support::u32_in(&mut golden) as usize, configs.len());
    for (i, c) in configs.into_iter().enumerate() {
        let n = u64::from_le_bytes(support::take(&mut golden, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut golden, n);
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        raw_cases::report(
            &format!("float-golden-{i}"),
            raw_cases::compare_with_parse_options(
                &c.headers(),
                &cases,
                reference,
                decoding(),
                parsing(CodebookBehavior::Libvorbis),
            ),
        );
    }
    assert!(golden.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_codebook_float_pcm_golden() {
    let configs = regressions();
    let mut golden = b"DVQPCM01".to_vec();
    golden.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.into_iter().enumerate() {
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        let response = raw_cases::reference(&format!("float-golden-{i}"), &c.headers(), &cases);
        golden.extend((response.len() as u64).to_le_bytes());
        golden.extend(response);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codebook-float-pcm-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}

fn pcm_bits(value: &f32) -> u32 {
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}
#[test]
fn codebook_policy_survives_ogg_reset_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    let mut seeks = 0;
    for (i, c) in regressions().into_iter().enumerate() {
        for behavior in [CodebookBehavior::Vorbis, CodebookBehavior::Libvorbis] {
            let headers = c.headers();
            let id = Identification::parse(&headers[0]).unwrap();
            let setup = Setup::parse_with_options(&headers[2], &id, parsing(behavior)).unwrap();
            let mut raw = PacketDecoder::with_options(setup, decoding()).unwrap();
            let packets = c.sequence();
            let mut expected = vec![Vec::new(); c.channels()];
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
            if i == 0 {
                let nan = expected
                    .iter()
                    .flatten()
                    .any(|&w| f32::from_bits(w).is_nan());
                assert_eq!(nan, behavior == CodebookBehavior::Vorbis);
            }
            let bytes = writer.into_inner();
            if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(
                    std::path::Path::new(&root).join(format!("float-stream-{i}.ogg")),
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
                codebook_behavior: behavior,
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
            let mut out = vec![Vec::new(); c.channels()];
            while pull.read_next(|e| collect(&mut out, e)).unwrap() {}
            assert_eq!(out, expected, "pull {c:?}");
            let mut stream = OggStreamDecoder::new(options);
            for size in [1, 17, bytes.len()] {
                stream.reset();
                let mut out = vec![Vec::new(); c.channels()];
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
                    let mut out = vec![Vec::new(); c.channels()];
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
