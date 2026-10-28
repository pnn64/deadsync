use deadlib_audio_decode_mp3::{
    bits::{BitReader, BitWriter},
    tags::*,
    *,
};

fn header(version: u32, bitrate: u32, rate: u32, mode: u32, crc: bool, pad: bool) -> [u8; 4] {
    (0xffe00000u32
        | version << 19
        | 1 << 17
        | u32::from(!crc) << 16
        | bitrate << 12
        | rate << 10
        | u32::from(pad) << 9
        | mode << 6)
        .to_be_bytes()
}
fn silent_frame(h: [u8; 4]) -> Vec<u8> {
    let hdr = FrameHeader::parse(&h).unwrap();
    let mut data = vec![0; hdr.frame_bytes().unwrap()];
    data[..4].copy_from_slice(&h);
    if hdr.has_crc() {
        let crc = checksum::bytes(
            checksum::bytes(0xffff, &h[2..]),
            &data[6..hdr.main_data_offset()],
        );
        data[4..6].copy_from_slice(&crc.to_be_bytes());
    }
    data
}

#[test]
fn bit_fields_and_atomic_boundaries() {
    let bytes = [0xd3, 0x69, 0xf0, 0x01, 0xfe, 0xa5, 0x19, 0x80];
    for start in 0..64 {
        for width in 0..=32.min(64 - start) {
            let mut reader = BitReader::bounded(&bytes, start, width).unwrap();
            let expected = (start..start + width).fold(0, |v, p| {
                (v << 1) | ((bytes[p / 8] >> (7 - p % 8)) & 1) as u32
            });
            assert_eq!(reader.read(width as u32).unwrap(), expected);
            assert_eq!(reader.remaining(), 0);
            assert_eq!(reader.read(1), Err(Error::Truncated));
            assert_eq!(reader.position(), start + width);
        }
    }
    let mut writer = BitWriter::new();
    for width in 0..=32 {
        writer
            .write(
                if width == 32 {
                    u32::MAX
                } else {
                    (1u32 << width) - 1
                },
                width,
            )
            .unwrap();
    }
    let mut reader = BitReader::new(writer.as_bytes());
    for width in 0..=32 {
        assert_eq!(
            reader.read(width).unwrap(),
            if width == 32 {
                u32::MAX
            } else {
                (1u32 << width) - 1
            }
        );
    }
    let before = writer.as_bytes().to_vec();
    assert!(writer.write(8, 3).is_err());
    assert!(writer.write(0, 33).is_err());
    assert_eq!(writer.as_bytes(), before);
    assert!(BitReader::bounded(&bytes, usize::MAX, 2).is_err());
}

#[test]
fn all_header_configurations_and_reserved_fields() {
    for (version, kind, scale, samples) in [
        (3, MpegVersion::Mpeg1, 1, 1152),
        (2, MpegVersion::Mpeg2, 2, 576),
        (0, MpegVersion::Mpeg25, 4, 576),
    ] {
        for rate in 0..3 {
            for bitrate in 0..15 {
                for mode in 0..4 {
                    for crc in [false, true] {
                        for pad in [false, true] {
                            let raw = header(version, bitrate, rate, mode, crc, pad);
                            let h = FrameHeader::parse(&raw).unwrap();
                            assert_eq!(h.to_bytes(), raw);
                            assert_eq!(h.version(), kind);
                            assert_eq!(
                                h.sample_rate(),
                                [44100, 48000, 32000][rate as usize] / scale
                            );
                            assert_eq!(h.samples(), samples);
                            assert_eq!(h.has_crc(), crc);
                            assert_eq!(h.channels(), if mode == 3 { 1 } else { 2 });
                            if let Some(bps) = h.bitrate() {
                                assert_eq!(
                                    h.frame_bytes(),
                                    Some(
                                        samples / 8 * bps as usize / h.sample_rate() as usize
                                            + usize::from(pad)
                                    )
                                );
                            } else {
                                assert_eq!(bitrate, 0);
                                assert!(h.frame_bytes().is_none());
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        FrameHeader::parse(&[0xff, 0xfb, 0x90, 0])
            .unwrap()
            .frame_bytes(),
        Some(417)
    );
    assert_eq!(
        FrameHeader::parse(&[0xff, 0xf3, 0x80, 0xc0])
            .unwrap()
            .frame_bytes(),
        Some(208)
    );
    let raw = header(3, 9, 0, 0, false, false);
    for n in 0..4 {
        assert!(matches!(
            FrameHeader::parse(&raw[..n]),
            Err(Error::Truncated)
        ));
    }
    for word in [0xffe90000u32, 0xfffbfc00, 0xfff90000, 0x7ffb9000] {
        assert!(FrameHeader::parse(&word.to_be_bytes()).is_err());
    }
    // Retain emphasis as a raw field, including the reserved value exercised
    // by the ISO header fixture. It does not alter frame length or PCM timing.
    assert_eq!(
        FrameHeader::parse(&0xfffb9202u32.to_be_bytes())
            .unwrap()
            .emphasis(),
        2
    );
}

#[test]
fn frame_truncation_crc_coverage_and_synchronization() {
    assert_eq!(checksum::bytes(0xffff, b"123456789"), 0xaee7);
    for v in [3, 2, 0] {
        for mode in [0, 3] {
            for crc in [false, true] {
                let mut data = silent_frame(header(v, 9, 0, mode, crc, false));
                let length = data.len();
                for n in 0..length {
                    assert!(matches!(
                        Frame::parse(&data[..n], None, true, Limits::default()),
                        Err(Error::Truncated)
                    ));
                }
                let frame = Frame::parse(&data, None, true, Limits::default()).unwrap();
                assert_eq!(frame.side_info.main_data_bits(frame.header), 0);
                assert_eq!(frame.side_info.main_data_begin, 0);
                if crc {
                    data[10] ^= 1;
                    assert!(matches!(
                        Frame::parse(&data, None, true, Limits::default()),
                        Err(Error::CrcMismatch { .. })
                    ));
                    data[10] ^= 1;
                    data[length - 1] ^= 0x80; // MPEG CRC deliberately does not cover main data.
                    assert!(Frame::parse(&data, None, true, Limits::default()).is_ok());
                }
                let mut junk = vec![0xff, 0, 0xff, 0x7b, 0x90, 0, 42];
                junk.extend_from_slice(&data);
                assert_eq!(
                    Frame::find(&junk, None, true, Limits::default()).unwrap().0,
                    7
                );
            }
        }
    }
}

#[test]
fn side_information_fields_switched_and_long() {
    for v in [3, 2, 0] {
        for mode in [0, 3] {
            for block in 0..=3 {
                let h = FrameHeader::parse(&header(v, 9, 0, mode, false, false)).unwrap();
                let mut w = BitWriter::new();
                w.write(19, if v == 3 { 9 } else { 8 }).unwrap();
                w.write(
                    0,
                    match (v == 3, mode == 3) {
                        (true, true) => 5,
                        (true, false) => 3,
                        (false, true) => 1,
                        _ => 2,
                    },
                )
                .unwrap();
                if v == 3 {
                    for _ in 0..h.channels() {
                        w.write(0b1010, 4).unwrap();
                    }
                }
                for _ in 0..h.granules() * h.channels() {
                    for (value, width) in [
                        (111, 12),
                        (123, 9),
                        (210, 8),
                        (7, if v == 3 { 4 } else { 9 }),
                        (u32::from(block != 0), 1),
                    ] {
                        w.write(value, width).unwrap();
                    }
                    if block != 0 {
                        w.write(block, 2).unwrap();
                        w.write(u32::from(block == 2), 1).unwrap();
                        w.write(7, 5).unwrap();
                        w.write(16, 5).unwrap();
                        for gain in [1, 2, 3] {
                            w.write(gain, 3).unwrap();
                        }
                    } else {
                        for table in [1, 2, 3] {
                            w.write(table, 5).unwrap();
                        }
                        w.write(6, 4).unwrap();
                        w.write(5, 3).unwrap();
                    }
                    if v == 3 {
                        w.write(1, 1).unwrap();
                    }
                    w.write(1, 1).unwrap();
                    w.write(1, 1).unwrap();
                }
                assert_eq!(w.len_bits(), h.side_info_bytes() * 8);
                let side = SideInfo::parse(h, w.as_bytes()).unwrap();
                assert_eq!(side.main_data_begin, 19);
                assert_eq!(side.main_data_bits(h), 111 * h.channels() * h.granules());
                for granule in &side.granules[..h.granules()] {
                    for g in &granule[..h.channels()] {
                        assert_eq!(
                            (g.big_values, g.global_gain, g.scalefac_compress),
                            (123, 210, 7)
                        );
                        assert_eq!(g.block_type, block as u8);
                        assert!(g.scalefac_scale && g.count1_table);
                        assert_eq!(g.preflag, v == 3);
                        assert_eq!(
                            g.table_select,
                            if block == 0 { [1, 2, 3] } else { [7, 16, 0] }
                        );
                    }
                }
                for n in 0..w.as_bytes().len() {
                    assert!(matches!(
                        SideInfo::parse(h, &w.as_bytes()[..n]),
                        Err(Error::Truncated)
                    ));
                }
            }
        }
    }
}

#[test]
fn reservoir_cross_frame_history_and_recovery() {
    let mut r = Reservoir::new();
    assert!(matches!(
        r.push(3, &[1, 2], 8),
        Err(Error::MissingReservoir {
            needed: 3,
            available: 0
        })
    ));
    assert_eq!(r.push(2, &[3, 4], 24).unwrap(), [1, 2, 3]);
    assert_eq!(r.push(3, &[5], 8).unwrap(), [2]); // view begins at pointer, not at newest history.
    assert!(matches!(r.push(1, &[], 24), Err(Error::Truncated)));
    let payload: Vec<u8> = (0..2000).map(|n| n as u8).collect();
    r.push(0, &payload, 0).unwrap();
    assert_eq!(r.retained_bytes(), 511);
    assert_eq!(
        r.push(511, &[], 511 * 8).unwrap(),
        &payload[payload.len() - 511..]
    );
    assert!(r.push(512, &[], 0).is_err());
    assert!(r.push(0, &[], usize::MAX).is_err());
    r.clear();
    assert_eq!(r.retained_bytes(), 0);
}

#[test]
fn free_format_inference_padding_false_sync_and_limits() {
    let mut data = Vec::new();
    for pad in [true, false, true, false] {
        let mut frame = vec![0; 400 + usize::from(pad)];
        frame[..4].copy_from_slice(&header(3, 0, 0, 0, false, pad));
        frame[100..104].copy_from_slice(&header(2, 0, 0, 0, false, false));
        data.extend_from_slice(&frame);
    }
    let free = FreeFormat::infer(&data, Limits::default()).unwrap();
    assert_eq!(free.unpadded_bytes(), 400);
    assert_eq!(
        Frame::parse(&data, Some(free), true, Limits::default())
            .unwrap()
            .data
            .len(),
        401
    );
    assert!(FreeFormat::infer(&data[..802], Limits::default()).is_err());
    assert!(Frame::parse(&data, None, true, Limits::default()).is_err());
    let limits = Limits {
        frame_bytes: 400,
        ..Default::default()
    };
    assert!(Frame::parse(&data, Some(free), true, limits).is_err());
}

fn id3(version: u8, flags: u8, payload: &[u8]) -> Vec<u8> {
    let n = payload.len();
    let mut tag = vec![
        b'I',
        b'D',
        b'3',
        version,
        0,
        flags,
        ((n >> 21) & 127) as u8,
        ((n >> 14) & 127) as u8,
        ((n >> 7) & 127) as u8,
        (n & 127) as u8,
    ];
    tag.extend_from_slice(payload);
    if flags & 0x10 != 0 {
        let mut footer = tag[..10].to_vec();
        footer[..3].copy_from_slice(b"3DI");
        tag.extend_from_slice(&footer);
    }
    tag
}
#[test]
fn id3_frames_unsynchronization_footers_and_bounds() {
    for version in [2, 3, 4] {
        let payload = if version == 2 {
            b"TT2\0\0\x03\0hi".as_slice()
        } else {
            b"TIT2\0\0\0\x03\0\0\0hi".as_slice()
        };
        let raw = id3(version, if version == 4 { 0x10 } else { 0 }, payload);
        for n in 0..raw.len() {
            assert!(matches!(
                Id3Tag::parse(&raw[..n], Limits::default()),
                Err(Error::Truncated)
            ));
        }
        let tag = Id3Tag::parse(&raw, Limits::default()).unwrap();
        let mut frames = tag.frames().unwrap();
        let frame = frames.next_frame().unwrap().unwrap();
        assert_eq!(frame.payload, b"\0hi");
        assert!(frames.next_frame().unwrap().is_none());
        let limit = Limits {
            metadata_bytes: raw.len() - 1,
            ..Default::default()
        };
        assert!(Id3Tag::parse(&raw, limit).is_err());
    }
    assert_eq!(
        deunsynchronize(&[0xff, 0, 0, 0xff, 0, 0xe1])
            .unwrap()
            .as_ref(),
        [0xff, 0, 0xff, 0xe1]
    );
    let tag = id3(3, 0x80, b"TIT2\0\0\0\x04\0\0\0\xff\0\xe1x");
    let mut frames = Id3Tag::parse(&tag, Limits::default())
        .unwrap()
        .frames()
        .unwrap();
    assert_eq!(
        frames.next_frame().unwrap().unwrap().payload,
        b"\0\xff\xe1x"
    );
    let mut bad = id3(4, 0, &[]);
    bad[6] = 128;
    assert!(Id3Tag::parse(&bad, Limits::default()).is_err());
}

#[test]
fn xing_gapless_and_vbri_optional_tables() {
    let h = FrameHeader::parse(&header(3, 9, 0, 0, true, false)).unwrap();
    let offset = 4 + h.side_info_bytes();
    let mut frame = vec![0; 417];
    frame[..4].copy_from_slice(&h.to_bytes());
    frame[offset..offset + 4].copy_from_slice(b"Info");
    frame[offset + 4..offset + 8].copy_from_slice(&3u32.to_be_bytes());
    frame[offset + 8..offset + 12].copy_from_slice(&42u32.to_be_bytes());
    frame[offset + 12..offset + 16].copy_from_slice(&10000u32.to_be_bytes());
    let encoder = offset + 16;
    frame[encoder..encoder + 9].copy_from_slice(b"LAME3.100");
    frame[encoder + 21..encoder + 24].copy_from_slice(&[0x24, 0x03, 0x45]);
    let Some(VbrHeader::Xing(xing)) = VbrHeader::parse(h, &frame).unwrap() else {
        panic!("Xing");
    };
    assert_eq!(xing.frames, Some(42));
    assert_eq!(xing.bytes, Some(10000));
    assert_eq!(
        xing.gapless,
        Some(Gapless {
            encoder_delay: 576,
            encoder_padding: 837
        })
    );
    assert!(xing.cbr_info);
    frame[36..].fill(0);
    frame[36..40].copy_from_slice(b"VBRI");
    let mut p = 40;
    for bytes in [
        1u16.to_be_bytes().as_slice(),
        &[0, 0],
        &[0, 100],
        &10000u32.to_be_bytes(),
        &42u32.to_be_bytes(),
        &[0, 3],
        &[0, 2],
        &[0, 2],
        &[0, 10],
        &[0, 100, 0, 200, 1, 44],
    ] {
        frame[p..p + bytes.len()].copy_from_slice(bytes);
        p += bytes.len();
    }
    let Some(VbrHeader::Vbri(vbri)) = VbrHeader::parse(h, &frame).unwrap() else {
        panic!("VBRI");
    };
    assert_eq!(vbri.byte_offset(25).unwrap(), 600);
    assert_eq!(vbri.byte_offset(40).unwrap(), 1200);
    assert!(VbrHeader::parse(h, &frame[..p - 1]).is_err());
}
