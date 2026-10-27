#[path = "support/raw_cases.rs"]
#[allow(dead_code)]
mod raw_cases;
mod support;
#[path = "support/residue_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_vorbis::*;
use vectors::Geometry;
fn parsing() -> ParseOptions {
    ParseOptions {
        residue_behavior: ResidueBehavior::Libvorbis,
        ..ParseOptions::default()
    }
}
fn geometry(kind: u8) -> Geometry {
    Geometry {
        channels: 3,
        submaps: 2,
        kind,
        class_dimensions: 2,
        begin: 1,
        clipped_end: true,
        coupling: true,
        alignment: 3,
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn reference_accepts_partial_partitions_and_inverted_ranges() {
    let mut mismatches = Vec::new();
    for kind in 0..=2 {
        for (i, first) in [(1, 60, 5), (7, 3, 12), (1, 60, 12)]
            .into_iter()
            .enumerate()
        {
            let c = geometry(kind);
            let headers = c.headers_with_residues(&[first, (1, 80, 12)]);
            let refs: Vec<_> = headers.iter().map(Vec::as_slice).collect();
            let reference = support::oracle("pcm", &refs);
            assert_eq!(reference, (c.channels as u32).to_le_bytes());
            if let Some(root) = std::env::var_os("DEADLIB_VORBIS_RESIDUE_TAIL_INPUTS") {
                std::fs::create_dir_all(&root).unwrap();
                let mut raw = Vec::new();
                for h in &headers {
                    raw.extend((h.len() as u64).to_le_bytes());
                    raw.extend(h);
                }
                std::fs::write(
                    std::path::Path::new(&root).join(format!("init-{kind}-{i}.bin")),
                    raw,
                )
                .unwrap();
            }
            let id = Identification::parse(&headers[0]).unwrap();
            let native = Setup::parse_with_options(&headers[2], &id, parsing());
            if let Err(error) = native {
                mismatches.push((kind, first, error));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "reference accepted but native rejected: {mismatches:?}"
    );
}

#[derive(Clone, Debug)]
struct Config {
    geometry: Geometry,
    ranges: Vec<(u32, u32, u32)>,
}
impl Config {
    fn headers(&self) -> Vec<Vec<u8>> {
        self.geometry.headers_with_residues(&self.ranges)
    }
    fn audio(&self, long: bool, windows: u32, seed: usize) -> Vec<u8> {
        self.geometry
            .audio_with_residues(&self.ranges, long, windows, seed)
    }
    fn channels(&self) -> usize {
        self.geometry.channels as usize
    }
    fn sequence(&self) -> Vec<Vec<u8>> {
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
}
fn configurations() -> Vec<Config> {
    let mut out = Vec::new();
    for channels in [1, 3, 8] {
        for submaps in [1, 2, 4] {
            for kind in 0..=2 {
                for (i, first) in [
                    (1, 127, 1),
                    (1, 127, 2),
                    (1, 127, 3),
                    (1, 127, 5),
                    (1, 127, 7),
                    (1, 127, 11),
                    (1, 127, 12),
                    (1, 127, 13),
                    (1, 127, 17),
                    (7, 3, 12),
                    (0xffffff, 0, 1),
                    (1, 0xffffff, 1 << 24),
                ]
                .into_iter()
                .enumerate()
                {
                    let g = Geometry {
                        channels,
                        submaps,
                        kind,
                        class_dimensions: (i % 3 + 1) as u8,
                        begin: 1,
                        clipped_end: true,
                        coupling: channels > 1,
                        alignment: 3,
                    };
                    let mut ranges = vec![(1, 0xffffff, 5); submaps as usize];
                    ranges[0] = first;
                    if submaps > 2 {
                        ranges[2] = (32, 32, 3);
                    }
                    out.push(Config {
                        geometry: g,
                        ranges,
                    });
                }
            }
        }
    }
    out
}
fn decoding() -> DecodeOptions {
    DecodeOptions {
        allow_nonfinite: true,
        residue2_layout: Residue2Layout::Libvorbis,
        ..DecodeOptions::default()
    }
}
#[test]
fn reference_residue_ranges_preserve_strict_defaults_and_metadata() {
    assert_eq!(
        ParseOptions::default().residue_behavior,
        ResidueBehavior::Vorbis
    );
    assert_eq!(
        OggOptions::default().residue_behavior,
        ResidueBehavior::Vorbis
    );
    assert_eq!(
        SeekOptions::default().residue_behavior,
        ResidueBehavior::Vorbis
    );
    for kind in 0..=2 {
        for (first, strict) in [
            ((1, 60, 5), false),
            ((7, 3, 12), false),
            ((1, 60, 12), true),
            ((1, 0xffffff, 1 << 24), false),
        ] {
            let h = geometry(kind).headers_with_residues(&[first, (1, 80, 12)]);
            let id = Identification::parse(&h[0]).unwrap();
            assert_eq!(Setup::parse(&h[2], &id, &Limits::default()).is_ok(), strict);
            let setup = Setup::parse_with_options(&h[2], &id, parsing()).unwrap();
            assert_eq!(setup.residue_behavior(), ResidueBehavior::Libvorbis);
            let r = &setup.residues()[0];
            assert_eq!((r.begin, r.end, r.partition_size), first);
        }
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn partial_residue_packet_prefixes_and_later_submaps_match_reference() {
    let configs = configurations();
    for (i, c) in configs.iter().enumerate() {
        let h = c.headers();
        let sequence = c.sequence();
        let mut cases = vec![raw_cases::Case {
            name: format!("{c:?}/complete"),
            packets: sequence.clone(),
        }];
        for long in [false, true] {
            let candidate = c.audio(long, 0, 3);
            let lengths: std::collections::BTreeSet<_> = if c.geometry.channels <= 3 {
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
                let mut packets = sequence.clone();
                packets[2] = candidate[..length].to_vec();
                cases.push(raw_cases::Case {
                    name: format!("{c:?}/long-{long}/prefix-{length}"),
                    packets,
                });
            }
        }
        for (batch, cases) in cases.chunks(32).enumerate() {
            let label = format!("residue-tail-pcm-{i}-batch-{batch}");
            let reference = raw_cases::reference(&label, &h, cases);
            raw_cases::report(
                &label,
                raw_cases::compare_with_parse_options(&h, cases, &reference, decoding(), parsing()),
            );
        }
    }
    eprintln!("residue tail configurations: {}", configs.len());
}
fn regressions() -> Vec<Config> {
    let configs = configurations();
    [0, 123, 160, 201, 250, 311]
        .into_iter()
        .map(|i| configs[i].clone())
        .collect()
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_residue_tail_pcm_golden() {
    let configs = regressions();
    let mut golden = b"DRTPCM01".to_vec();
    golden.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.iter().enumerate() {
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        let reference =
            raw_cases::reference(&format!("residue-tail-golden-{i}"), &c.headers(), &cases);
        golden.extend((reference.len() as u64).to_le_bytes());
        golden.extend(reference);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/residue-tail-pcm-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}
#[test]
fn partial_residue_pcm_matches_retained_reference() {
    let mut wire = include_bytes!("fixtures/residue-tail-pcm-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut wire, 8), b"DRTPCM01");
    let configs = regressions();
    assert_eq!(support::u32_in(&mut wire) as usize, configs.len());
    for (i, c) in configs.iter().enumerate() {
        let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
        let reference = support::take(&mut wire, n);
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        raw_cases::report(
            &format!("residue-tail-golden-{i}"),
            raw_cases::compare_with_parse_options(
                &c.headers(),
                &cases,
                reference,
                decoding(),
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
fn residue_policy_survives_ogg_reset_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    let mut seeks = 0;
    for (i, c) in regressions().into_iter().enumerate() {
        let behavior = ResidueBehavior::Libvorbis;
        let headers = c.headers();
        let id = Identification::parse(&headers[0]).unwrap();
        let setup = Setup::parse_with_options(&headers[2], &id, parsing()).unwrap();
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
        let bytes = writer.into_inner();
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_RESIDUE_TAIL_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("residue-tail-stream-{i}.ogg")),
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
            residue_behavior: behavior,
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
                    residue_behavior: options.residue_behavior,
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
    assert_eq!(seeks, 168);
}
