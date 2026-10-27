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
