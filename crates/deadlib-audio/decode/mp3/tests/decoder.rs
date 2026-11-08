use deadlib_audio_decode_mp3::{bits::BitWriter, checksum, *};

fn impulse(version: u32, channels: usize, crc: bool, back: u32, part_bits: Option<u32>) -> Vec<u8> {
    let word = 0xffe00000u32
        | version << 19
        | 1 << 17
        | u32::from(!crc) << 16
        | 9 << 12
        | if channels == 1 { 3 << 6 } else { 0 };
    let header = FrameHeader::parse(&word.to_be_bytes()).unwrap();
    let mut side = BitWriter::new();
    side.write(back, if version == 3 { 9 } else { 8 }).unwrap();
    side.write(
        0,
        match (version == 3, channels) {
            (true, 1) => 5,
            (true, _) => 3,
            (false, 1) => 1,
            _ => 2,
        },
    )
    .unwrap();
    if version == 3 {
        for _ in 0..channels {
            side.write(0, 4).unwrap();
        }
    }
    for g in 0..header.granules() {
        for _ in 0..channels {
            for (v, n) in [
                (if g == 0 { part_bits.unwrap_or(3) } else { 0 }, 12),
                (u32::from(g == 0), 9),
                (if g == 0 { 206 } else { 0 }, 8),
                (0, if version == 3 { 4 } else { 9 }),
                (0, 1),
            ] {
                side.write(v, n).unwrap();
            }
            for _ in 0..3 {
                side.write(u32::from(g == 0), 5).unwrap();
            }
            side.write(0, 4).unwrap();
            side.write(0, 3).unwrap();
            if version == 3 {
                side.write(0, 1).unwrap();
            }
            side.write(0, 1).unwrap();
            side.write(0, 1).unwrap();
        }
    }
    assert_eq!(side.as_bytes().len(), header.side_info_bytes());
    let mut main = BitWriter::new();
    for ch in 0..channels {
        main.write(1, 2).unwrap();
        main.write(ch as u32, 1).unwrap();
    }
    let mut data = vec![0; header.frame_bytes().unwrap()];
    data[..4].copy_from_slice(&word.to_be_bytes());
    let start = 4 + usize::from(crc) * 2;
    data[start..header.main_data_offset()].copy_from_slice(side.as_bytes());
    let end = header.main_data_offset() + main.as_bytes().len();
    data[header.main_data_offset()..end].copy_from_slice(main.as_bytes());
    if crc {
        let c = checksum::bytes(checksum::bytes(0xffff, &data[2..4]), side.as_bytes());
        data[4..6].copy_from_slice(&c.to_be_bytes());
    }
    data
}
#[test]
fn channel_transitions_match_independent_mono_histories() {
    for version in [3, 2, 0] {
        let mono = impulse(version, 1, false, 0, None);
        let stereo = impulse(version, 2, false, 0, None);
        let mut negative = mono.clone();
        let offset = FrameHeader::parse(&negative).unwrap().main_data_offset();
        negative[offset] ^= 0x20;
        let mut actual = FrameDecoder::new();
        let mut left = FrameDecoder::new();
        let mut right = FrameDecoder::new();
        for channels in [2, 1, 1, 1, 2, 2, 1, 2, 0, 1, 2, 1, 1, 2] {
            if channels == 0 {
                actual.reset();
                left.reset();
                right.reset();
                continue;
            }
            let data = if channels == 1 { &mono } else { &stereo };
            let frame = actual.decode(data, None).unwrap();
            let expected = left.decode(&mono, None).unwrap();
            assert_eq!(
                frame
                    .pcm
                    .plane(0)
                    .unwrap()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .pcm
                    .plane(0)
                    .unwrap()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
            if channels == 2 {
                let expected = right.decode(&negative, None).unwrap();
                assert_eq!(
                    frame
                        .pcm
                        .plane(1)
                        .unwrap()
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    expected
                        .pcm
                        .plane(0)
                        .unwrap()
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>()
                );
            } else {
                right.reset();
            }
        }
    }
}

#[test]
fn frame_scfsi_reuse_matches_explicit_factors_in_both_channels() {
    fn frame(channels: usize, mask: u32) -> Vec<u8> {
        let word = 0xfffb9000u32 | if channels == 1 { 3 << 6 } else { 0 };
        let header = FrameHeader::parse(&word.to_be_bytes()).unwrap();
        let mut side = BitWriter::new();
        side.write(0, 9).unwrap();
        side.write(0, if channels == 1 { 5 } else { 3 }).unwrap();
        for _ in 0..channels {
            for group in 0..4 {
                side.write(u32::from(mask & (1 << group) != 0), 1).unwrap();
            }
        }
        for granule in 0..2 {
            let count = if granule == 0 {
                21
            } else {
                [6, 5, 5, 5]
                    .into_iter()
                    .enumerate()
                    .filter(|(g, _)| mask & (1 << g) == 0)
                    .map(|(_, n)| n)
                    .sum()
            };
            for _ in 0..channels {
                for (value, width) in [(count + 3, 12), (1, 9), (206, 8), (5, 4), (0, 1)] {
                    side.write(value, width).unwrap();
                }
                for _ in 0..3 {
                    side.write(1, 5).unwrap();
                }
                for width in [4, 3, 1, 1, 1] {
                    side.write(0, width).unwrap();
                }
            }
        }
        assert_eq!(side.as_bytes().len(), header.side_info_bytes());
        let mut main = BitWriter::new();
        for granule in 0..2 {
            for channel in 0..channels {
                for (group, count) in [6, 5, 5, 5].into_iter().enumerate() {
                    if granule == 0 || mask & (1 << group) == 0 {
                        for _ in 0..count {
                            main.write(u32::from(channel == 0), 1).unwrap();
                        }
                    }
                }
                main.write(1, 2).unwrap();
                main.write(channel as u32, 1).unwrap();
            }
        }
        let mut data = vec![0; header.frame_bytes().unwrap()];
        data[..4].copy_from_slice(&word.to_be_bytes());
        data[4..header.main_data_offset()].copy_from_slice(side.as_bytes());
        data[header.main_data_offset()..header.main_data_offset() + main.as_bytes().len()]
            .copy_from_slice(main.as_bytes());
        data
    }
    for channels in [1, 2] {
        let explicit = frame(channels, 0);
        for mask in 0..16 {
            let reused = frame(channels, mask);
            let mut reference = FrameDecoder::new();
            let mut actual = FrameDecoder::new();
            for _ in 0..3 {
                let expected: Vec<_> = reference
                    .decode(&explicit, None)
                    .unwrap()
                    .pcm
                    .planes()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect();
                let got: Vec<_> = actual
                    .decode(&reused, None)
                    .unwrap()
                    .pcm
                    .planes()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect();
                assert_eq!(got, expected, "channels={channels} mask={mask}");
            }
        }
    }
}

#[test]
fn caller_payload_can_change_before_a_later_frame_reuses_its_tail() {
    for version in [3, 2, 0] {
        for channels in [1, 2] {
            let mut first = impulse(version, channels, false, 0, None);
            let header = FrameHeader::parse(&first).unwrap();
            let tail = first.len() - 1;
            first[tail] = first[header.main_data_offset()];
            let second = impulse(version, channels, false, 1, None);
            let mut reference = FrameDecoder::new();
            reference.decode(&first, None).unwrap();
            let expected: Vec<Vec<_>> = reference
                .decode(&second, None)
                .unwrap()
                .pcm
                .planes()
                .map(|plane| plane.iter().map(|v| v.to_bits()).collect())
                .collect();
            let mut decoder = FrameDecoder::new();
            decoder.decode(&first, None).unwrap();
            first.fill(0);
            let actual: Vec<Vec<_>> = decoder
                .decode(&second, None)
                .unwrap()
                .pcm
                .planes()
                .map(|plane| plane.iter().map(|v| v.to_bits()).collect())
                .collect();
            assert_eq!(actual, expected, "version={version} channels={channels}");
        }
    }
}

#[test]
fn raw_pcm_shape_prefix_retries_and_reset() {
    for version in [3, 2, 0] {
        for channels in [1, 2] {
            let data = impulse(version, channels, true, 0, None);
            let mut d = FrameDecoder::new();
            for n in 0..data.len() {
                assert!(matches!(d.decode(&data[..n], None), Err(Error::Truncated)));
            }
            let frame = d.decode(&data, None).unwrap();
            assert_eq!(frame.bytes, data.len());
            assert_eq!(frame.pcm.channels(), channels);
            assert_eq!(frame.pcm.samples(), if version == 3 { 1152 } else { 576 });
            assert!(frame.pcm.plane(channels).is_none());
            let first = frame.pcm.plane(0).unwrap().to_vec();
            assert!(first.iter().all(|v| v.is_finite()));
            assert!(first.iter().any(|v| v.abs() > 1e-5));
            if channels == 2 {
                assert!(
                    frame
                        .pcm
                        .plane(1)
                        .unwrap()
                        .iter()
                        .zip(&first)
                        .all(|(r, l)| (r + l).abs() < 1e-7)
                );
            }
            d.decode(&data, None).unwrap();
            d.reset();
            assert_eq!(d.decode(&data, None).unwrap().pcm.plane(0).unwrap(), first);
        }
    }
}
#[test]
fn crc_and_complete_main_data_errors_require_reset() {
    let good = impulse(3, 1, true, 0, None);
    let mut bad = good.clone();
    bad[4] ^= 1;
    let mut d = FrameDecoder::new();
    assert!(matches!(
        d.decode(&bad, None),
        Err(Error::CrcMismatch { .. })
    ));
    assert!(matches!(d.decode(&good, None), Err(Error::Invalid(_))));
    d.reset();
    assert!(d.decode(&good, None).is_ok());
    let mut loose = FrameDecoder::with_options(DecodeOptions {
        verify_crc: false,
        ..Default::default()
    });
    assert!(loose.decode(&bad, None).is_ok());
    let broken = impulse(3, 1, false, 0, Some(0));
    d.reset();
    assert!(matches!(d.decode(&broken, None), Err(Error::Invalid(_))));
    assert!(matches!(d.decode(&good, None), Err(Error::Invalid(_))));
    d.reset();
    assert!(d.decode(&good, None).is_ok());
}
#[test]
fn missing_reservoir_recovery_free_format_and_version_policy() {
    let mut d = FrameDecoder::new();
    let missing = impulse(3, 1, false, 1, None);
    assert!(matches!(
        d.decode(&missing, None),
        Err(Error::MissingReservoir {
            needed: 1,
            available: 0
        })
    ));
    let good = impulse(3, 1, false, 0, None);
    assert!(d.decode(&good, None).is_ok());
    d.reset();
    let mut free = good.clone();
    free[2] &= 15;
    assert!(matches!(d.decode(&free, None), Err(Error::Unsupported(_))));
    let size = FreeFormat::new(free.len(), Limits::default()).unwrap();
    assert!(d.decode(&free, Some(size)).is_ok());
    let mut strict = FrameDecoder::with_options(DecodeOptions {
        allow_mpeg25: false,
        ..Default::default()
    });
    assert!(matches!(
        strict.decode(&impulse(0, 1, false, 0, None), None),
        Err(Error::Unsupported(_))
    ));
    assert!(strict.decode(&good, None).is_ok());
}
