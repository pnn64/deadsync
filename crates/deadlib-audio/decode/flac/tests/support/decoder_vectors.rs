//! Explicit wire families, independent of FrameEncoder's mode selection.
use deadlib_audio_decode_flac::{
    bitstream::BitWriter,
    checksum::{Md5, crc8, crc16},
    *,
};
pub struct Case {
    pub name: String,
    pub data: Vec<u8>,
    pub planes: Vec<Vec<i32>>,
}
/// Normalize only frame numbering and enclosing metadata. Subframe bytes stay
/// unchanged, so accepted fuzz mutations can be independently decoded as files.
pub fn fuzz_case(name: String, input: &[u8]) -> Option<Case> {
    if input.len() < 34 || input.len() > 131072 {
        return None;
    }
    let mut decoder = FrameDecoder::new(StreamInfo::parse(&input[..34]).ok()).unwrap();
    let raw = &input[34..];
    let frame = decoder.decode(raw).ok()?;
    let h = frame.header;
    let planes: Vec<Vec<i32>> = frame.pcm.planes().map(|p| p.to_vec()).collect();
    let n = h.block_size as u16;
    let mut info = StreamInfo {
        min_block_size: n.max(16),
        max_block_size: n.max(16),
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: h.sample_rate,
        channels: h.channels,
        bits_per_sample: h.bits_per_sample,
        total_samples: n as u64,
        md5: [0; 16],
    };
    let mut digest = Md5::new();
    frame.pcm.update_md5(&mut digest, h.bits_per_sample);
    info.md5 = digest.finalize();
    let old_number = if raw[4] < 128 {
        1
    } else {
        raw[4].leading_ones() as usize
    };
    let mut normalized = raw[..4].to_vec();
    normalized.push(0);
    normalized.extend_from_slice(&raw[4 + old_number..h.header_bytes - 1]);
    normalized.push(crc8(&normalized));
    normalized.extend_from_slice(&raw[h.header_bytes..frame.bytes - 2]);
    normalized.extend_from_slice(&crc16(&normalized).to_be_bytes());
    let mut data = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &info.encode().unwrap(),
    }
    .write(true, &mut data)
    .unwrap();
    data.extend(normalized);
    Some(Case { name, data, planes })
}
fn case(
    name: String,
    bits: u8,
    assignment: u8,
    planes: Vec<Vec<i32>>,
    mut body: BitWriter,
) -> Case {
    let n = planes[0].len();
    let mut info = StreamInfo {
        min_block_size: (n as u16).max(16),
        max_block_size: (n as u16).max(16),
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 48000,
        channels: planes.len() as u8,
        bits_per_sample: bits,
        total_samples: n as u64,
        md5: [0; 16],
    };
    let mut digest = Md5::new();
    for i in 0..n {
        for p in &planes {
            digest.update(&p[i].to_le_bytes()[..bits.div_ceil(8) as usize]);
        }
    }
    info.md5 = digest.finalize();
    let mut data = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &info.encode().unwrap(),
    }
    .write(true, &mut data)
    .unwrap();
    let mut head = vec![255, 248, 0x70, assignment << 4, 0];
    head.extend_from_slice(&((n - 1) as u16).to_be_bytes());
    head.push(crc8(&head));
    body.align_zero();
    head.extend_from_slice(body.as_bytes());
    head.extend_from_slice(&crc16(&head).to_be_bytes());
    data.extend(head);
    Case { name, data, planes }
}
fn verbatim(w: &mut BitWriter, samples: &[i64], width: u32, wasted: u32) {
    w.write(2 | u64::from(wasted != 0), 8).unwrap();
    if wasted != 0 {
        w.unary((wasted - 1) as u64).unwrap();
    }
    for &v in samples {
        w.signed(v >> wasted, width - wasted).unwrap();
    }
}
fn residual(
    w: &mut BitWriter,
    values: &[i64],
    order: usize,
    method: u32,
    partition: u32,
    escape: bool,
) {
    w.write(method as u64, 2).unwrap();
    w.write(partition as u64, 4).unwrap();
    let size = values.len() >> partition;
    for p in 0..1usize << partition {
        w.write(if escape { (1 << (4 + method)) - 1 } else { 2 }, 4 + method)
            .unwrap();
        if escape {
            w.write(8, 5).unwrap();
        }
        for &value in &values[if p == 0 { order } else { p * size }..(p + 1) * size] {
            if escape {
                w.signed(value, 8).unwrap();
            } else {
                let folded = ((value << 1) ^ (value >> 63)) as u64;
                w.unary(folded >> 2).unwrap();
                w.write(folded & 3, 2).unwrap();
            }
        }
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for bits in 4..=32u8 {
        for assignment in 0..=10u8 {
            for wasted in [0, 1, bits - 1] {
                let channels = if assignment < 8 { assignment + 1 } else { 2 };
                let base = [
                    -(1i64 << (bits - 1)),
                    0,
                    (1i64 << (bits - 1)) - (1i64 << wasted),
                    0,
                ];
                let planes: Vec<Vec<i32>> = (0..channels)
                    .map(|ch| {
                        (0..33)
                            .map(|i| base[(i + ch as usize) % 4] as i32)
                            .collect()
                    })
                    .collect();
                let mut coded: Vec<Vec<i64>> = planes
                    .iter()
                    .map(|p| p.iter().map(|&v| v as i64).collect())
                    .collect();
                if assignment >= 8 {
                    for i in 0..33 {
                        let (l, r) = (planes[0][i] as i64, planes[1][i] as i64);
                        (coded[0][i], coded[1][i]) = match assignment {
                            8 => (l, l - r),
                            9 => (l - r, r),
                            10 => ((l + r) >> 1, l - r),
                            _ => unreachable!(),
                        };
                    }
                }
                let mut w = BitWriter::new();
                for (ch, p) in coded.iter().enumerate() {
                    let side =
                        (assignment == 9 && ch == 0) || (matches!(assignment, 8 | 10) && ch == 1);
                    let wasted = if assignment == 10 && ch == 0 {
                        wasted.saturating_sub(1)
                    } else {
                        wasted
                    };
                    verbatim(&mut w, p, bits as u32 + u32::from(side), wasted as u32);
                }
                cases.push(case(
                    format!("verbatim-b{bits}-a{assignment}-w{wasted}"),
                    bits,
                    assignment,
                    planes,
                    w,
                ));
            }
        }
    }
    for order in 0..=4usize {
        for method in 0..=1 {
            for partition in 0..=2 {
                for escape in [false, true] {
                    let pcm: Vec<i32> = (0..64).map(|i| i - 16).collect();
                    let mut residuals: Vec<_> = pcm.iter().map(|&v| v as i64).collect();
                    let coefficients: [&[i64]; 5] =
                        [&[], &[1], &[2, -1], &[3, -3, 1], &[4, -6, 4, -1]];
                    for i in order..64 {
                        residuals[i] -= coefficients[order]
                            .iter()
                            .enumerate()
                            .map(|(j, c)| c * pcm[i - j - 1] as i64)
                            .sum::<i64>();
                    }
                    let mut w = BitWriter::new();
                    w.write(((8 + order) * 2) as u64, 8).unwrap();
                    for &v in &pcm[..order] {
                        w.signed(v as i64, 16).unwrap();
                    }
                    residual(&mut w, &residuals, order, method, partition, escape);
                    cases.push(case(
                        format!("fixed-o{order}-m{method}-p{partition}-e{escape}"),
                        16,
                        0,
                        vec![pcm],
                        w,
                    ));
                }
            }
        }
    }
    for order in 1..=32usize {
        for method in 0..=1 {
            for precision in [1, 15] {
                for shift in [0, 15] {
                    let mut coeff = vec![0i64; order];
                    if precision == 1 {
                        coeff[0] = -1;
                    } else if shift == 0 {
                        coeff[0] = 1;
                        if order >= 2 {
                            coeff[0] = 16383;
                            coeff[1] = -16382;
                        }
                    } else if order >= 3 {
                        coeff[..3].copy_from_slice(&[16383, 16383, 2]);
                    }
                    let mut pcm = vec![3i64; 64];
                    let residuals = vec![0; 64];
                    for i in order..64 {
                        pcm[i] = coeff
                            .iter()
                            .enumerate()
                            .map(|(j, c)| c * pcm[i - j - 1])
                            .sum::<i64>()
                            >> shift;
                    }
                    let mut w = BitWriter::new();
                    w.write(((31 + order) * 2) as u64, 8).unwrap();
                    for &v in &pcm[..order] {
                        w.signed(v, 16).unwrap();
                    }
                    w.write(precision - 1, 4).unwrap();
                    w.signed(shift as i64, 5).unwrap();
                    for &c in &coeff {
                        w.signed(c, precision as u32).unwrap();
                    }
                    residual(&mut w, &residuals, order, method, 0, false);
                    cases.push(case(
                        format!("lpc-o{order}-m{method}-c{precision}-s{shift}"),
                        16,
                        0,
                        vec![pcm.iter().map(|&v| v as i32).collect()],
                        w,
                    ));
                }
            }
        }
    }
    for assignment in [8, 9, 10] {
        for shift in [0, 15] {
            let left = vec![i32::MAX; 64];
            let right = vec![i32::MIN; 64];
            let (first, second) = match assignment {
                8 => (i32::MAX as i64, u32::MAX as i64),
                9 => (u32::MAX as i64, i32::MIN as i64),
                10 => (-1, u32::MAX as i64),
                _ => unreachable!(),
            };
            let mut w = BitWriter::new();
            for (ch, v) in [first, second].into_iter().enumerate() {
                let side =
                    (assignment == 9 && ch == 0) || (matches!(assignment, 8 | 10) && ch == 1);
                let width = 32 + u32::from(side);
                w.write(126, 8).unwrap();
                for _ in 0..32 {
                    w.signed(v, width).unwrap();
                }
                w.write(14, 4).unwrap();
                w.signed(shift, 5).unwrap();
                for j in 0..32 {
                    w.signed(
                        if shift == 0 {
                            match j {
                                0 => 16383,
                                1 => -16382,
                                _ => 0,
                            }
                        } else {
                            match j {
                                0 | 1 => 16383,
                                2 => 2,
                                _ => 0,
                            }
                        },
                        15,
                    )
                    .unwrap();
                }
                residual(&mut w, &[0; 64], 32, 1, 0, false);
            }
            cases.push(case(
                format!("wide-lpc32-a{assignment}-s{shift}"),
                32,
                assignment,
                vec![left, right],
                w,
            ));
        }
    }
    cases
}
