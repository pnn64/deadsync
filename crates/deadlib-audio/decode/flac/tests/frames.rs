use deadlib_audio_decode_flac::{
    Error, FrameDecoder, FrameHeader, StreamInfo,
    bitstream::BitWriter,
    checksum::{crc8, crc16},
};

fn info(bits: u8, channels: u8) -> StreamInfo {
    StreamInfo {
        min_block_size: 16,
        max_block_size: 65535,
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 1048575,
        channels,
        bits_per_sample: bits,
        total_samples: 0,
        md5: [0; 16],
    }
}
fn header(n: usize, assignment: u8, variable: bool, number: u64) -> Vec<u8> {
    let mut result = vec![0xff, 0xf8 | u8::from(variable), 0x70, assignment << 4];
    if number < 128 {
        result.push(number as u8);
    } else {
        let lengths = [7, 11, 16, 21, 26, 31, 36];
        let bytes = lengths.iter().position(|&bits| number < 1 << bits).unwrap() + 1;
        result.push((0xffu8 << (8 - bytes)) | (number >> (6 * (bytes - 1))) as u8);
        for i in (0..bytes - 1).rev() {
            result.push(0x80 | ((number >> (6 * i)) as u8 & 63));
        }
    }
    result.extend_from_slice(&((n - 1) as u16).to_be_bytes());
    result.push(crc8(&result));
    result
}
fn finish(mut header: Vec<u8>, mut w: BitWriter) -> Vec<u8> {
    w.align_zero();
    header.extend_from_slice(w.as_bytes());
    header.extend_from_slice(&crc16(&header).to_be_bytes());
    header
}
fn verbatim(w: &mut BitWriter, pcm: &[i64], bits: u32, wasted: u32) {
    w.write(2 | u64::from(wasted != 0), 8).unwrap();
    if wasted != 0 {
        w.unary(u64::from(wasted - 1)).unwrap();
    }
    for &sample in pcm {
        w.signed(sample >> wasted, bits - wasted).unwrap();
    }
}

#[test]
fn all_depths_channels_stereo_assignments_and_wasted_bits() {
    for bits in 4u8..=32 {
        for assignment in 0..=10 {
            let channels = if assignment < 8 { assignment + 1 } else { 2 };
            for wasted in [0, 1, bits - 1] {
                let base = [
                    -(1i64 << (bits - 1)),
                    0,
                    (1i64 << (bits - 1)) - (1i64 << wasted),
                    if wasted < bits - 1 { 1i64 << wasted } else { 0 },
                ];
                let expected: Vec<Vec<i64>> = (0..channels)
                    .map(|c| (0..17).map(|i| base[(i + c as usize) % 4]).collect())
                    .collect();
                let mut coded = expected.clone();
                if assignment >= 8 {
                    for i in 0..17 {
                        let (l, r) = (expected[0][i], expected[1][i]);
                        (coded[0][i], coded[1][i]) = match assignment {
                            8 => (l, l - r),
                            9 => (l - r, r),
                            10 => ((l + r) >> 1, l - r),
                            _ => unreachable!(),
                        };
                    }
                }
                let mut w = BitWriter::new();
                for (ch, pcm) in coded.iter().enumerate() {
                    let side =
                        (assignment == 9 && ch == 0) || (matches!(assignment, 8 | 10) && ch == 1);
                    // Mid can lose one common trailing zero through the average.
                    let used = if assignment == 10 && ch == 0 {
                        wasted.saturating_sub(1)
                    } else {
                        wasted
                    };
                    verbatim(&mut w, pcm, u32::from(bits) + u32::from(side), used.into());
                }
                let data = finish(header(17, assignment, false, 0), w);
                let mut decoder = FrameDecoder::new(Some(info(bits, channels))).unwrap();
                let frame = decoder.decode(&data).unwrap();
                assert_eq!(frame.bytes, data.len());
                for (ch, expected) in expected.iter().enumerate() {
                    assert_eq!(
                        frame
                            .pcm
                            .plane(ch)
                            .iter()
                            .map(|&v| v as i64)
                            .collect::<Vec<_>>(),
                        *expected
                    );
                }
            }
        }
    }
}

#[test]
fn stereo_decorrelation_rejects_out_of_range_pcm() {
    for bits in 4..=32u8 {
        let low = -(1i64 << (bits - 1));
        let high = -low - 1;
        let side_low = -(1i64 << bits);
        let side_high = -side_low - 1;
        for (assignment, a, b) in [
            (8, low, side_high),
            (8, high, side_low),
            (9, side_high, high),
            (9, side_low, low),
            (10, high, side_high),
            (10, low, side_low),
        ] {
            let mut w = BitWriter::new();
            w.write(0, 8).unwrap();
            w.signed(a, bits as u32 + u32::from(assignment == 9))
                .unwrap();
            w.write(0, 8).unwrap();
            w.signed(b, bits as u32 + u32::from(assignment != 9))
                .unwrap();
            let frame = finish(header(16, assignment, false, 0), w);
            let mut decoder = FrameDecoder::new(Some(info(bits, 2))).unwrap();
            assert_eq!(
                decoder.decode(&frame).unwrap_err(),
                Error::Invalid("sample outside declared bit depth"),
                "bits {bits} assignment {assignment}"
            );
        }
    }
}

#[test]
fn fixed_predictors_reject_reconstructed_overflow() {
    for bits in 4..=32u8 {
        for order in 1..=4usize {
            let mut w = BitWriter::new();
            w.write(((8 + order) * 2) as u64, 8).unwrap();
            for _ in 0..order {
                w.signed((1i64 << (bits - 1)) - 1, bits.into()).unwrap();
            }
            w.write(0, 2).unwrap();
            w.write(0, 4).unwrap();
            w.write(0, 4).unwrap();
            w.unary(2).unwrap();
            for _ in order + 1..16 {
                w.unary(0).unwrap();
            }
            let data = finish(header(16, 0, false, 0), w);
            let mut decoder = FrameDecoder::new(Some(info(bits, 1))).unwrap();
            assert_eq!(
                decoder.decode(&data).unwrap_err(),
                Error::Invalid("sample outside declared bit depth"),
                "bits {bits} order {order}"
            );
        }
    }
}

#[test]
fn fixed_orders_and_all_lpc_orders_with_rice_and_escape() {
    for kind in (8..=12).chain(32..=63) {
        let order = if kind < 32 { kind - 8 } else { kind - 31 };
        for method in 0..=1 {
            for escape in [false, true] {
                let n = 64;
                let mut w = BitWriter::new();
                w.write((kind * 2) as u64, 8).unwrap();
                for _ in 0..order {
                    w.signed(3, 16).unwrap();
                }
                if kind >= 32 {
                    w.write(1, 4).unwrap(); // two-bit coefficients
                    w.write(0, 5).unwrap();
                    for i in 0..order {
                        w.signed(i64::from(i == 0), 2).unwrap();
                    }
                }
                w.write(method, 2).unwrap();
                w.write(0, 4).unwrap();
                let param_bits = method as u32 + 4;
                w.write(if escape { (1 << param_bits) - 1 } else { 0 }, param_bits)
                    .unwrap();
                if escape {
                    w.write(0, 5).unwrap();
                } else {
                    for _ in order..n {
                        w.unary(0).unwrap();
                    }
                }
                let data = finish(header(n, 0, false, 0), w);
                let mut decoder = FrameDecoder::new(Some(info(16, 1))).unwrap();
                let frame = decoder.decode(&data).unwrap();
                assert!(
                    frame
                        .pcm
                        .plane(0)
                        .iter()
                        .all(|&v| v == if order == 0 { 0 } else { 3 })
                );
            }
        }
    }
}

#[test]
fn signed_rice_partition_boundaries_and_extremes() {
    for method in 0..=1 {
        let k = if method == 0 { 14 } else { 30 };
        let samples = [-2147483647i64, 2147483647, 0, -1, 1, 64, -65, 10000];
        let mut w = BitWriter::new();
        w.write(16, 8).unwrap(); // fixed zero
        w.write(method, 2).unwrap();
        w.write(2, 4).unwrap(); // four partitions
        for part in samples.as_chunks::<2>().0 {
            w.write(k, method as u32 + 4).unwrap();
            for &x in part {
                let f = ((x << 1) ^ (x >> 63)) as u64;
                w.unary(f >> k).unwrap();
                w.write(f, k as u32).unwrap();
            }
        }
        let data = finish(header(samples.len(), 0, true, 0), w);
        let mut decoder = FrameDecoder::new(Some(info(32, 1))).unwrap();
        let frame = decoder.decode(&data).unwrap();
        assert_eq!(
            frame
                .pcm
                .plane(0)
                .iter()
                .map(|&x| x as i64)
                .collect::<Vec<_>>(),
            samples
        );
    }
}

#[test]
fn frame_numbers_checksums_and_all_truncations() {
    let inf = info(16, 1);
    for number in [
        0,
        127,
        128,
        2047,
        2048,
        65535,
        65536,
        (1 << 21) - 1,
        1 << 21,
        1 << 26,
        1 << 31,
        (1 << 36) - 1,
    ] {
        let head = header(16, 0, true, number);
        assert_eq!(
            FrameHeader::parse(&head, Some(&inf)).unwrap().number,
            number
        );
    }
    let mut w = BitWriter::new();
    w.write(0, 8).unwrap();
    w.signed(-32768, 16).unwrap();
    let data = finish(header(16, 0, false, 0), w);
    let mut decoder = FrameDecoder::new(Some(inf)).unwrap();
    for end in 0..data.len() {
        assert!(decoder.decode(&data[..end]).is_err());
    }
    for byte in 0..data.len() {
        let mut bad = data.clone();
        bad[byte] ^= 1;
        assert!(decoder.decode(&bad).is_err(), "corruption byte {byte}");
    }
    assert_eq!(decoder.decode(&data).unwrap().pcm.plane(0), [-32768; 16]);
    let mut bad = header(16, 0, false, 0);
    bad[5] = 255;
    bad[6] = 255;
    assert!(matches!(
        FrameHeader::parse(&bad, Some(&inf)),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn every_header_rate_block_and_depth_code() {
    let inf = info(4, 1);
    let rates = [
        1048575, 88200, 176400, 192000, 8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000,
        255000, 65535, 655350,
    ];
    for block in 1u8..16 {
        for rate in 0u8..15 {
            for depth in [0u8, 1, 2, 4, 5, 6, 7] {
                let mut h = vec![255, 249, block << 4 | rate, depth << 1, 0];
                let n = match block {
                    1 => 192,
                    2..=5 => 144usize << block,
                    6 => {
                        h.push(0);
                        1
                    }
                    7 => {
                        h.extend_from_slice(&[255, 254]);
                        65535
                    }
                    _ => 1usize << block,
                };
                match rate {
                    12 => h.push(255),
                    13 | 14 => h.extend_from_slice(&[255, 255]),
                    _ => (),
                }
                h.push(crc8(&h));
                let got = FrameHeader::parse(&h, Some(&inf)).unwrap();
                assert_eq!(got.block_size, n);
                assert_eq!(got.sample_rate, rates[rate as usize]);
                assert_eq!(
                    got.bits_per_sample,
                    [4, 8, 12, 0, 16, 20, 24, 32][depth as usize]
                );
            }
        }
    }
}
