//! Explicit decoder paths; fixture construction and exact PCM checks are untimed.
use deadlib_audio_decode_flac::{
    bitstream::BitWriter,
    checksum::{Md5, crc8, crc16},
    *,
};
use std::{
    hint::black_box,
    io::{Cursor, Write},
    time::{Duration, Instant},
};
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod alloc;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../tests/support/decoder_vectors.rs"]
mod vectors;

fn fixture(case: &str) -> (Vec<u8>, Vec<Vec<i32>>) {
    assert!(
        matches!(
            case,
            "fixed2"
                | "fixed3"
                | "fixed4"
                | "short-fixed4"
                | "fixed4-wide"
                | "lpc1"
                | "lpc2"
                | "lpc12"
                | "stereo8"
                | "stereo9"
                | "stereo10"
                | "stereo10-wide"
                | "verbatim24"
                | "verbatim32"
        ),
        "unknown decoder case: {case}"
    );
    let n = if case == "short-fixed4" { 32 } else { 4096 };
    let wide = case.ends_with("wide");
    let bits = if wide || case == "verbatim32" {
        32
    } else if case == "verbatim24" {
        24
    } else {
        16
    };
    let assignment = if case.starts_with("stereo8") {
        8
    } else if case.starts_with("stereo9") {
        9
    } else if case.starts_with("stereo10") {
        10
    } else if wide {
        8
    } else {
        0
    };
    let stereo = assignment >= 8;
    let planes = if case.starts_with("stereo") {
        vec![
            vec![if wide { i32::MAX } else { 50 }; n],
            vec![if wide { i32::MIN } else { -21 }; n],
        ]
    } else {
        let wave: Vec<_> = (0..n)
            .map(|i| ((i % 128) as i32 - 64) * 9 + ((i * 7 % 9) as i32 - 4))
            .collect();
        if wide {
            vec![
                wave.iter().map(|&v| (1 << 30) + v).collect(),
                wave.iter().map(|&v| -(1 << 30) - v).collect(),
            ]
        } else {
            vec![wave]
        }
    };
    let mut coded: Vec<Vec<i64>> = planes
        .iter()
        .map(|p| p.iter().copied().map(i64::from).collect())
        .collect();
    if stereo {
        for i in 0..n {
            let (l, r) = (i64::from(planes[0][i]), i64::from(planes[1][i]));
            (coded[0][i], coded[1][i]) = match assignment {
                8 => (l, l - r),
                9 => (l - r, r),
                10 => ((l + r) >> 1, l - r),
                _ => unreachable!(),
            };
        }
    }
    let mut body = BitWriter::new();
    for (ch, samples) in coded.iter().enumerate() {
        let side = (assignment == 9 && ch == 0) || (matches!(assignment, 8 | 10) && ch == 1);
        let width = bits + u32::from(side);
        if case.starts_with("stereo") {
            body.write(0, 8).unwrap();
            body.signed(samples[0], width).unwrap();
        } else if case.starts_with("verbatim") || (wide && ch == 0) {
            body.write(2, 8).unwrap();
            for &sample in samples {
                body.signed(sample, width).unwrap();
            }
        } else {
            let (coeff, shift, kind): (Vec<i64>, u32, u64) =
                if let Some(order) = case.strip_prefix("lpc") {
                    let order = order.parse::<usize>().unwrap();
                    let mut coeff = vec![0; order];
                    coeff[0] = if order == 1 { 1 } else { 3 };
                    if order > 1 {
                        coeff[1] = -1;
                    }
                    (coeff, u32::from(order != 1), 31 + order as u64)
                } else {
                    let order = if case.starts_with("fixed2") {
                        2
                    } else if case.starts_with("fixed3") {
                        3
                    } else {
                        4
                    };
                    (
                        match order {
                            2 => vec![2, -1],
                            3 => vec![3, -3, 1],
                            _ => vec![4, -6, 4, -1],
                        },
                        0,
                        8 + order,
                    )
                };
            let order = coeff.len();
            body.write(kind << 1, 8).unwrap();
            for &sample in &samples[..order] {
                body.signed(sample, width).unwrap();
            }
            if kind >= 32 {
                body.write(3, 4).unwrap();
                body.signed(i64::from(shift), 5).unwrap();
                for &c in &coeff {
                    body.signed(c, 4).unwrap();
                }
            }
            body.write(0, 6).unwrap(); // Rice method 0, one partition.
            body.write(4, 4).unwrap();
            for i in order..n {
                let prediction: i64 = coeff
                    .iter()
                    .enumerate()
                    .map(|(j, c)| c * samples[i - j - 1])
                    .sum();
                let value = samples[i] - (prediction >> shift);
                assert!((-i64::from(i32::MAX)..=i64::from(i32::MAX)).contains(&value));
                let folded = ((value << 1) ^ (value >> 63)) as u64;
                body.unary(folded >> 4).unwrap();
                body.write(folded & 15, 4).unwrap();
            }
        }
    }
    let mut header = vec![255, 248, 0x70, assignment << 4, 0];
    header.extend_from_slice(&((n - 1) as u16).to_be_bytes());
    header.push(crc8(&header));
    body.align_zero();
    header.extend_from_slice(body.as_bytes());
    header.extend_from_slice(&crc16(&header).to_be_bytes());
    let mut digest = Md5::new();
    for i in 0..n {
        for plane in &planes {
            digest.update(&plane[i].to_le_bytes()[..(bits / 8) as usize]);
        }
    }
    let info = StreamInfo {
        min_block_size: n as u16,
        max_block_size: n as u16,
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 48000,
        channels: planes.len() as u8,
        bits_per_sample: bits as u8,
        total_samples: n as u64,
        md5: digest.finalize(),
    };
    let mut data = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &info.encode().unwrap(),
    }
    .write(true, &mut data)
    .unwrap();
    data.extend(header);
    (data, planes)
}

fn verify(data: &[u8], planes: &[Vec<i32>]) {
    let mut reader = Reader::new(Cursor::new(data)).unwrap();
    let frame = reader.read_next().unwrap().unwrap();
    for (a, b) in frame.pcm.planes().zip(planes) {
        assert_eq!(a, b);
    }
    assert!(reader.read_next().unwrap().is_none());
    assert_eq!(reader.integrity(), Integrity::Verified);
}

fn file_case(path: &str, iterations: usize) {
    let data = std::fs::read(path).unwrap();
    let metadata = Metadata::parse(&data, Limits::default()).unwrap();
    let mut decoder = FrameDecoder::new(Some(metadata.stream_info)).unwrap();
    let mut frames = Vec::new();
    let mut offset = metadata.audio_offset;
    let mut samples = 0;
    let mut digest = Md5::new();
    let (_, cold) = alloc::measure(|| {
        black_box(decoder.decode(&data[offset..]).unwrap());
    });
    while offset < data.len() {
        let frame = decoder.decode(&data[offset..]).unwrap();
        frames.push(offset..offset + frame.bytes);
        offset += frame.bytes;
        samples += frame.pcm.samples() * usize::from(frame.pcm.channels());
        frame
            .pcm
            .update_md5(&mut digest, frame.header.bits_per_sample);
    }
    let signature = digest.finalize();
    if metadata.stream_info.md5 != [0; 16] {
        assert_eq!(signature, metadata.stream_info.md5);
    }
    let mut decode = || {
        for range in &frames {
            black_box(decoder.decode(black_box(&data[range.clone()])).unwrap());
        }
    };
    let (_, warm) = alloc::measure(&mut decode);
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(200) {
        decode();
    }
    let cycles = cpu::cycles();
    let start = Instant::now();
    for _ in 0..iterations {
        decode();
    }
    let ns = start.elapsed().as_nanos();
    let cycles = cpu::cycles() - cycles;
    println!(
        "{path}\t{iterations}\t{cycles}\t{ns}\t{samples}\t{}\t{}\t{}\t{}\t{}\t{}\t{:02x?}",
        data.len(),
        cold.allocs,
        cold.reallocs,
        cold.allocated_bytes,
        warm.allocs,
        warm.reallocs,
        signature
    );
}

fn main() {
    cpu::pin_thread();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args[0] == "headers" {
        let mut out = std::fs::File::create(&args[1]).unwrap();
        let info = EncodeOptions::default().stream_info().unwrap();
        for sync in [0xfff8u16, 0xfff9, 0xfffa, 0x7ff8] {
            for codes in [
                0x00u8, 0x10, 0x60, 0x70, 0x80, 0x8c, 0x8d, 0x8e, 0x8f, 0x89, 0xf9,
            ] {
                for format in 0..=255u8 {
                    let mut header = sync.to_be_bytes().to_vec();
                    header.extend_from_slice(&[codes, format, 0]);
                    match codes >> 4 {
                        6 => header.push(31),
                        7 => header.extend_from_slice(&31u16.to_be_bytes()),
                        _ => (),
                    }
                    match codes & 15 {
                        12 => header.push(48),
                        13 => header.extend_from_slice(&48000u16.to_be_bytes()),
                        14 => header.extend_from_slice(&4800u16.to_be_bytes()),
                        _ => (),
                    }
                    header.push(crc8(&header));
                    for supplied in [None, Some(&info)] {
                        for end in 0..=header.len() {
                            let result =
                                format!("{:?}", FrameHeader::parse(&header[..end], supplied));
                            out.write_all(&(result.len() as u32).to_le_bytes()).unwrap();
                            out.write_all(result.as_bytes()).unwrap();
                        }
                    }
                }
            }
        }
        return;
    }
    if args[0] == "matrix" {
        let mut out = std::fs::File::create(&args[1]).unwrap();
        for case in vectors::cases() {
            verify(&case.data, &case.planes);
            for end in 0..=case.data.len() {
                let result = (|| {
                    let mut decoder = Decoder::new(&case.data[..end])?;
                    let mut bytes = Vec::new();
                    while let Some(frame) = decoder.read_next()? {
                        for p in frame.pcm.planes() {
                            for &sample in p {
                                bytes.extend_from_slice(&sample.to_le_bytes());
                            }
                        }
                    }
                    Ok::<_, Error>(bytes)
                })();
                let record = match result {
                    Ok(bytes) => bytes,
                    Err(error) => format!("{error:?}").into_bytes(),
                };
                out.write_all(&(record.len() as u32).to_le_bytes()).unwrap();
                out.write_all(&record).unwrap();
            }
        }
        return;
    }
    let case = &args[0];
    let iterations: usize = args[1].parse().unwrap();
    assert!(iterations > 0);
    if std::path::Path::new(case).is_file() {
        file_case(case, iterations);
        return;
    }
    let (data, planes) = fixture(case);
    verify(&data, &planes);
    let metadata = Metadata::parse(&data, Limits::default()).unwrap();
    let raw = &data[metadata.audio_offset..];
    let mut decoder = FrameDecoder::new(Some(metadata.stream_info)).unwrap();
    let (_, cold) = alloc::measure(|| {
        black_box(decoder.decode(raw).unwrap());
    });
    let frame = decoder.decode(raw).unwrap();
    let mut digest = Md5::new();
    for plane in frame.pcm.planes() {
        for sample in plane {
            digest.update(&sample.to_le_bytes());
        }
    }
    let signature = digest.finalize();
    let (_, warm) = alloc::measure(|| {
        black_box(decoder.decode(raw).unwrap());
    });
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(200) {
        black_box(decoder.decode(black_box(raw)).unwrap());
    }
    let cycles = cpu::cycles();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(decoder.decode(black_box(raw)).unwrap());
    }
    let ns = start.elapsed().as_nanos();
    let cycles = cpu::cycles() - cycles;
    println!(
        "{case}\t{iterations}\t{cycles}\t{ns}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:02x?}",
        planes.len() * planes[0].len(),
        raw.len(),
        cold.allocs,
        cold.reallocs,
        cold.allocated_bytes,
        warm.allocs,
        warm.reallocs,
        signature
    );
}
