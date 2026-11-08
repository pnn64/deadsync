#[path = "support/raw_cases.rs"]
#[allow(dead_code)]
mod raw_cases;
mod support;
#[path = "support/floor0_vectors.rs"]
mod vectors;
use deadlib_audio_decode_vorbis as codec;
use deadlib_audio_decode_vorbis::*;
use vectors::Config;

#[test]
#[ignore = "requires external libvorbis oracle"]
fn wide_floor_zero_amplitudes_match_libvorbis() {
    for width in [31, 32, 33, 63] {
        let c = Config {
            width,
            ..Config::default()
        };
        let headers = c.headers();
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        let reference = raw_cases::reference(&format!("floor0-width-{width}"), &headers, &cases);
        raw_cases::report(
            &format!("floor0-width-{width}"),
            raw_cases::compare_with_options(
                &headers,
                &cases,
                &reference,
                DecodeOptions {
                    allow_nonfinite: true,
                    floor0_behavior: Floor0Behavior::Libvorbis,
                    ..DecodeOptions::default()
                },
            ),
        );
    }
}

fn options() -> DecodeOptions {
    DecodeOptions {
        allow_nonfinite: true,
        floor0_behavior: Floor0Behavior::Libvorbis,
        ..DecodeOptions::default()
    }
}
fn parse_options(c: Config) -> ParseOptions {
    ParseOptions {
        allow_empty_codebooks: c.empty,
        ..ParseOptions::default()
    }
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn floor_zero_parameter_pcm_and_prefix_recovery_match_libvorbis() {
    for (i, c) in vectors::pcm_configurations().into_iter().enumerate() {
        let headers = c.headers();
        let packets = c.sequence();
        let mut cases = vec![raw_cases::Case {
            name: format!("{c:?}/complete"),
            packets: packets.clone(),
        }];
        // All byte prefixes on the width/coefficient/selector cases. The larger
        // Bark/block grid uses complete packets; its integer maps are separately
        // exhaustive over every bin in the retained direct-backend grid.
        if !(64..1152).contains(&i) {
            for seed in [0, 1, 3, 5] {
                let candidate = c.audio(false, 0, c.amplitude(seed), 0, seed);
                for length in 0..=candidate.len() {
                    let mut sequence = packets.clone();
                    sequence[2] = candidate[..length].to_vec();
                    cases.push(raw_cases::Case {
                        name: format!("{c:?}/seed-{seed}/prefix-{length}"),
                        packets: sequence,
                    });
                }
            }
            for selector in [c.books as u32, (1 << bitpack::ilog(c.books as u32)) - 1] {
                let mut sequence = packets.clone();
                sequence[2] = c.audio(true, 0, c.maximum(), selector, 3);
                cases.push(raw_cases::Case {
                    name: format!("{c:?}/selector-{selector}"),
                    packets: sequence,
                });
            }
        }
        for (batch, cases) in cases.chunks(32).enumerate() {
            let label = format!("floor0-pcm-{i}-batch-{batch}");
            let reference = raw_cases::reference(&label, &headers, cases);
            raw_cases::report(
                &label,
                raw_cases::compare_with_parse_options(
                    &headers,
                    cases,
                    &reference,
                    options(),
                    parse_options(c),
                ),
            );
        }
    }
}

fn regressions() -> Vec<Config> {
    vec![
        Config {
            order: 31,
            ..Config::default()
        },
        Config {
            width: 32,
            ..Config::default()
        },
        Config {
            width: 33,
            ..Config::default()
        },
        Config {
            rate: 1,
            bark: 65535,
            long: 8192,
            ..Config::default()
        },
        Config {
            rate: 65535,
            bark: 65535,
            long: 2048,
            order: 7,
            dimensions: 3,
            books: 16,
            sequence: true,
            coupling: true,
            ..Config::default()
        },
        Config {
            order: 255,
            dimensions: 3,
            empty: true,
            ..Config::default()
        },
        Config {
            width: 31,
            offset: 255,
            order: 31,
            dimensions: 8,
            ..Config::default()
        },
    ]
}
#[test]
fn floor_zero_pcm_matches_retained_reference() {
    let mut golden = include_bytes!("fixtures/floor0-parameters-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut golden, 8), b"DVF0PCM1");
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
            &format!("floor0-golden-{i}"),
            raw_cases::compare_with_parse_options(
                &c.headers(),
                &cases,
                reference,
                options(),
                parse_options(c),
            ),
        );
    }
    assert!(golden.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_floor0_pcm_golden() {
    let configs = regressions();
    let mut golden = b"DVF0PCM1".to_vec();
    golden.extend((configs.len() as u32).to_le_bytes());
    for (i, c) in configs.into_iter().enumerate() {
        let cases = [raw_cases::Case {
            name: format!("{c:?}"),
            packets: c.sequence(),
        }];
        let reference = raw_cases::reference(&format!("floor0-golden-{i}"), &c.headers(), &cases);
        golden.extend((reference.len() as u64).to_le_bytes());
        golden.extend(reference);
    }
    std::fs::write(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/floor0-parameters-v1.libvorbis-pcm"),
        golden,
    )
    .unwrap();
}

pub fn floor_request(headers: &[Vec<u8>], cases: &[(bool, u8, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for h in headers {
        out.extend((h.len() as u64).to_le_bytes());
        out.extend(h);
    }
    out.extend((cases.len() as u64).to_le_bytes());
    for (long, prefix, data) in cases {
        out.extend((*long as u64).to_le_bytes());
        out.extend((*prefix as u64).to_le_bytes());
        out.extend((data.len() as u64).to_le_bytes());
        out.extend(data);
    }
    out
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn direct_floor_zero_probe_reports_amplitude_and_bark_geometry() {
    for width in [0, 6, 31, 32, 33, 63] {
        let c = Config {
            width,
            ..Config::default()
        };
        let headers = c.headers();
        let mut cases = Vec::new();
        for seed in 0..7 {
            for prefix in [0, 3, 7] {
                cases.push((
                    true,
                    prefix,
                    c.floor_packet(prefix, c.amplitude(seed), 0, seed),
                ));
            }
        }
        let input = floor_request(&headers, &cases);
        let out = support::file_oracle("floor0cases", &input);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOOR0_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("floor0-width-{width}.bin")),
                input,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("floor0-width-{width}.out")),
                out,
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn every_nonfinite_spectrum_position_produces_reference_nan_blocks() {
    let mut count = 0;
    let mut samples = 0;
    for n in [64usize, 128, 256, 512, 1024, 2048, 4096, 8192] {
        for word in [0x7f800000u32, 0xff800000, 0x7fc12345] {
            for first in (0..n / 2).step_by(64) {
                let end = (first + 64).min(n / 2);
                let mut wire = ((end - first) as u64).to_le_bytes().to_vec();
                for position in first..end {
                    wire.extend(((4 + n * 2) as u64).to_le_bytes());
                    wire.extend((n as u32).to_le_bytes());
                    for k in 0..n / 2 {
                        wire.extend(if k == position { word } else { 0 }.to_le_bytes());
                    }
                }
                let output = support::file_oracle("mdct-cases", &wire);
                assert_eq!(output.len(), (end - first) * n * 4);
                for bits in output.as_chunks::<4>().0 {
                    assert!(
                        f32::from_le_bytes(*bits).is_nan(),
                        "N={n}, first={first}, input={word:x}"
                    );
                }
                count += end - first;
                samples += output.len() / 4;
            }
        }
    }
    assert_eq!(count, 24480);
    eprintln!("reference IEEE MDCT: {count} isolated nonfinite inputs, {samples} NaN samples");
}
fn pcm_bits(value: &f32) -> u32 {
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        value.to_bits()
    }
}
#[test]
fn floor_zero_policy_survives_ogg_reset_and_both_seek_strategies() {
    use deadlib_audio_decode_ogg::PacketWriter;
    use std::io::Cursor;
    let mut seeks = 0;
    for (i, c) in regressions().into_iter().enumerate() {
        let headers = c.headers();
        let id = Identification::parse(&headers[0]).unwrap();
        let setup = Setup::parse_with_options(&headers[2], &id, parse_options(c)).unwrap();
        let mut raw = PacketDecoder::with_options(setup, options()).unwrap();
        let packets = c.sequence();
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
        assert_eq!(total as usize, 48 + 5 * c.long / 4);
        let bytes = writer.into_inner();
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOOR0_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("floor0-stream-{i}.ogg")),
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
            allow_nonfinite: true,
            allow_empty_codebooks: c.empty,
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
    assert_eq!(seeks, 196);
}
