//! Resident-input probes. Fixture creation and exact output checks are untimed.
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;

use deadlib_audio_decode_mp3::{
    bands::Bands,
    bits::BitReader,
    scalefactors::ScaleFactors,
    synthesis::{Hybrid, Synthesis},
    *,
};
use std::{
    hint::black_box,
    io::{self, Write},
    path::Path,
};

fn probe(label: &str, iterations: usize, mut work: impl FnMut()) {
    cpu::measure(label, iterations, &mut work);
    let (_, counts) = heap::measure(work);
    println!("{label}: allocation {counts:?}");
}

fn signal<const N: usize>(seed: u64) -> [f64; N] {
    let mut state = seed;
    std::array::from_fn(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as i64 as f64) * (1.0 / i64::MAX as f64)
    })
}

fn packets(path: &Path) -> Vec<(Vec<u8>, Option<FreeFormat>)> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = PacketReader::new(bytes.as_slice());
    let mut frames = Vec::new();
    while let Some(packet) = reader.next_packet().unwrap() {
        frames.push((packet.data.to_vec(), packet.free_format));
    }
    assert!(!frames.is_empty());
    frames
}

fn joint(rate: u32, mode: u8) -> FrameHeader {
    let (version, index) = match rate {
        44100 => (3, 0),
        48000 => (3, 1),
        32000 => (3, 2),
        22050 => (2, 0),
        24000 => (2, 1),
        16000 => (2, 2),
        11025 => (0, 0),
        12000 => (0, 1),
        8000 => (0, 2),
        _ => panic!("benchmark sample rate"),
    };
    let word = 0xffe00000u32
        | version << 19
        | 1 << 17
        | 1 << 16
        | 9 << 12
        | index << 10
        | 1 << 6
        | u32::from(mode) << 4;
    FrameHeader::parse(&word.to_be_bytes()).unwrap()
}

fn verify(paths: &[String]) {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for size in 0..=16 {
        let data: Vec<_> = (0..size).map(|n| (n * 37 + 13) as u8).collect();
        for start in 0..=size * 8 {
            for available in [0, 1, 7, 8, 9, 31, 32, 39] {
                let available = available.min(size * 8 - start);
                for width in 0..=33 {
                    let mut bits = BitReader::bounded(&data, start, available).unwrap();
                    writeln!(
                        out,
                        "{:?} {} {}",
                        bits.read(width),
                        bits.position(),
                        bits.remaining()
                    )
                    .unwrap();
                    let mut bits = BitReader::bounded(&data, start, available).unwrap();
                    writeln!(
                        out,
                        "{:?} {} {}",
                        bits.flag(),
                        bits.position(),
                        bits.remaining()
                    )
                    .unwrap();
                }
            }
        }
    }
    let data: Vec<_> = (0..4096).map(|n| (n * 37 + 13) as u8).collect();
    for rate in [44100, 48000, 32000, 22050, 24000, 16000, 11025, 12000, 8000] {
        let bands = Bands::for_rate(rate).unwrap();
        for kind in 0..=4 {
            for mixed in [false, true] {
                for (region0_count, region1_count) in
                    [(0, 0), (7, 7), (15, 7), (21, 0), (22, 255), (255, 255)]
                {
                    for big_values in [0, 1, 17, 288, 289] {
                        for table_select in [[0, 0, 0], [1, 13, 23], [4, 14, 32]] {
                            let info = GranuleChannel {
                                block_type: kind,
                                mixed_block: mixed,
                                region0_count,
                                region1_count,
                                big_values,
                                table_select,
                                ..Default::default()
                            };
                            for (start, available) in [(3, 31), (7, data.len() * 8 - 7)] {
                                let mut bits = BitReader::bounded(&data, start, available).unwrap();
                                let mut samples = [0; 576];
                                let result =
                                    spectral::quantized(&mut bits, &info, bands, &mut samples);
                                writeln!(
                                    out,
                                    "{result:?} {} {}",
                                    bits.position(),
                                    bits.remaining()
                                )
                                .unwrap();
                                if result.is_ok() {
                                    for sample in samples {
                                        out.write_all(&sample.to_le_bytes()).unwrap();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Prefix lookahead must preserve exact short-tail errors and cursor state.
    for pattern in 0..256u32 {
        let data: [u8; 8] = std::array::from_fn(|n| (pattern * 37 + n as u32 * 113) as u8);
        for start in 0..8 {
            for available in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 23, 31, 47, 56] {
                for book in (0..=32).chain([255]) {
                    let mut bits = BitReader::bounded(&data, start, available).unwrap();
                    writeln!(
                        out,
                        "{:?} {} {}",
                        huffman::pair(&mut bits, book),
                        bits.position(),
                        bits.remaining()
                    )
                    .unwrap();
                }
                for table_b in [false, true] {
                    let mut bits = BitReader::bounded(&data, start, available).unwrap();
                    writeln!(
                        out,
                        "{:?} {} {}",
                        huffman::quad(&mut bits, table_b),
                        bits.position(),
                        bits.remaining()
                    )
                    .unwrap();
                }
            }
        }
    }
    for pattern in [0, 1] {
        let data: [u8; 32] = std::array::from_fn(|n| {
            if pattern == 0 {
                0xff
            } else {
                (n * 113 + 0x6c) as u8
            }
        });
        for compress in 0..16 {
            for (kind, mixed) in [(0, false), (2, false), (2, true)] {
                for granule in 0..2 {
                    for mask in [0, 4, 15] {
                        for prior in 0..3 {
                            let mut previous = ScaleFactors {
                                short_block: prior == 2,
                                intensity_scale: true,
                                ..Default::default()
                            };
                            previous.long = std::array::from_fn(|n| (n % 8) as u8);
                            previous.long_width = std::array::from_fn(|n| (n % 5) as u8);
                            for available in [0, 1, 7, 8, 31, 63, 95, 111, 128] {
                                for limit in [0, 4095] {
                                    let mut side = SideInfo::default();
                                    side.scfsi[0] = std::array::from_fn(|n| mask & (1 << n) != 0);
                                    side.granules[granule][0] = GranuleChannel {
                                        block_type: kind,
                                        mixed_block: mixed,
                                        scalefac_compress: compress,
                                        part2_3_bits: limit,
                                        preflag: true,
                                        ..Default::default()
                                    };
                                    let mut bits = BitReader::bounded(&data, 5, available).unwrap();
                                    let result = scalefactors::read(
                                        &mut bits,
                                        joint(44100, 0),
                                        &side,
                                        granule,
                                        0,
                                        if prior == 0 { None } else { Some(&previous) },
                                    );
                                    writeln!(
                                        out,
                                        "{result:?} {} {}",
                                        bits.position(),
                                        bits.remaining()
                                    )
                                    .unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for crc in 0..=u16::MAX {
        for byte in 0..=u8::MAX {
            out.write_all(&checksum::bytes(crc, &[byte]).to_le_bytes())
                .unwrap();
        }
    }
    for seed in [1, 0x73429, 0x982374ab] {
        let mut synthesis = Synthesis::new();
        for slot in 0..64 {
            let mut input = signal::<32>(seed + slot);
            if (31..=47).contains(&slot) {
                input.fill(0.0);
            } else if (48..=55).contains(&slot) {
                input.fill(-0.0);
            } else if slot >= 56 {
                input.fill(0.0);
                input[(slot - 56) as usize] = f64::from_bits(1);
            }
            let mut pcm = [0.0; 32];
            synthesis.process(&input, &mut pcm);
            for value in pcm {
                out.write_all(&value.to_le_bytes()).unwrap();
            }
        }
        // Dense, sparse, zero and signed-zero spectra with overlap history.
        for (kind, mixed) in [(0, 0), (1, 0), (3, 0), (2, 0), (2, 2), (2, 4)] {
            let mut hybrid = Hybrid::new();
            for band in 0..580 {
                let mut input = if band == 576 {
                    signal(seed)
                } else {
                    [0.0; 576]
                };
                if band < 576 {
                    input[band] = -0.5;
                }
                if band == 578 {
                    input.fill(-0.0);
                }
                let mut output = [0.0; 576];
                hybrid
                    .process(&mut input, kind, mixed, &mut output)
                    .unwrap();
                for value in input.into_iter().chain(output) {
                    out.write_all(&value.to_le_bytes()).unwrap();
                }
            }
        }
    }
    for path in paths {
        let mut decoder = FrameDecoder::new();
        for (data, free) in packets(Path::new(path)) {
            let frame = decoder.decode(&data, free).unwrap();
            for value in frame.pcm.planes().flatten() {
                out.write_all(&value.to_le_bytes()).unwrap();
            }
        }
    }
    // All public gain values, rates and long/short/mixed reorderings. Include
    // both common coefficients and the full signed-i16 magnitude boundary.
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        let bands = Bands::for_rate(rate).unwrap();
        for (kind, mixed) in [(0, false), (1, false), (3, false), (2, false), (2, true)] {
            for gain in 0..=255u8 {
                let input = std::array::from_fn(|n| {
                    [
                        0,
                        1,
                        -1,
                        2,
                        -2,
                        15,
                        -15,
                        16,
                        -16,
                        8206,
                        -8206,
                        i16::MAX,
                        i16::MIN,
                    ][n % 13]
                });
                let info = GranuleChannel {
                    global_gain: gain,
                    block_type: kind,
                    mixed_block: mixed,
                    scalefac_scale: gain & 2 != 0,
                    subblock_gain: [gain, gain.wrapping_mul(7), gain.wrapping_mul(19)],
                    ..Default::default()
                };
                let factors = ScaleFactors {
                    long: std::array::from_fn(|n| gain.wrapping_add(n as u8)),
                    short: std::array::from_fn(|n| {
                        std::array::from_fn(|w| gain.wrapping_add((n * 3 + w) as u8))
                    }),
                    preflag: gain & 1 != 0,
                    ..Default::default()
                };
                let mut output = [0.0; 576];
                spectral::requantize(&input, &info, &factors, bands, &mut output).unwrap();
                for value in output {
                    out.write_all(&value.to_le_bytes()).unwrap();
                }
            }
        }
    }
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        let bands = Bands::for_rate(rate).unwrap();
        for mode in 0..=3 {
            for (kind, mixed) in [(0, false), (1, false), (3, false), (2, false), (2, true)] {
                for pan in 0..16 {
                    let info = [GranuleChannel {
                        block_type: kind,
                        mixed_block: mixed,
                        ..Default::default()
                    }; 2];
                    let factors = std::array::from_fn(|_| ScaleFactors {
                        long: [pan; 22],
                        short: [[pan; 3]; 13],
                        long_width: [pan % 8; 22],
                        short_width: [[pan % 8; 3]; 13],
                        intensity_scale: pan & 1 != 0,
                        ..Default::default()
                    });
                    for seed in 1..=3 {
                        let mut input = [signal::<576>(seed), signal::<576>(seed + 19)];
                        input[1][(seed as usize - 1) * 192..].fill(0.0);
                        if seed == 3 {
                            for (n, value) in [
                                0.0,
                                -0.0,
                                f64::MIN_POSITIVE,
                                f64::from_bits(1),
                                f64::MAX,
                                f64::INFINITY,
                                f64::NEG_INFINITY,
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                input[0][n] = value;
                                input[1][n] = -value;
                            }
                        }
                        spectral::stereo(joint(rate, mode), &info, &factors, bands, &mut input)
                            .unwrap();
                        for value in input.into_iter().flatten() {
                            out.write_all(&value.to_le_bytes()).unwrap();
                        }
                    }
                }
            }
        }
    }
}

fn cold_intensity() {
    cpu::pin_thread();
    let header = joint(44100, 1);
    let bands = Bands::for_rate(44100).unwrap();
    let info = [GranuleChannel::default(); 2];
    let factors = std::array::from_fn(|_| ScaleFactors {
        long: [3; 22],
        long_width: [3; 22],
        ..Default::default()
    });
    let mut input = [signal::<576>(19), [0.0; 576]];
    heap::measure(|| ());
    black_box(cpu::cycles());
    let start = cpu::cycles();
    let clock = std::time::Instant::now();
    let (_, counts) = heap::measure(|| {
        spectral::stereo(
            black_box(header),
            black_box(&info),
            black_box(&factors),
            bands,
            black_box(&mut input),
        )
        .unwrap();
    });
    let ns = clock.elapsed().as_nanos();
    let cycles = cpu::cycles() - start;
    println!("cold/intensity: cycles/op {cycles}; ns/op {ns}");
    println!("cold/intensity: allocation {counts:?}");
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "reservoir-profile") {
        println!("file\tframes\tzero_back\tzero_back_nonempty\tassembled_bytes\tzero_back_bytes");
        for path in &args[1..] {
            let frames = packets(Path::new(path));
            let (mut zero, mut nonempty, mut total_bytes, mut direct_bytes) = (0, 0, 0, 0);
            for (data, free) in &frames {
                let frame = Frame::parse(data, *free, true, Limits::default()).unwrap();
                let bytes = frame.side_info.main_data_bits(frame.header).div_ceil(8);
                total_bytes += bytes;
                if frame.side_info.main_data_begin == 0 {
                    zero += 1;
                    nonempty += usize::from(bytes != 0);
                    direct_bytes += bytes;
                }
            }
            println!(
                "{}\t{}\t{zero}\t{nonempty}\t{total_bytes}\t{direct_bytes}",
                Path::new(path).file_name().unwrap().to_string_lossy(),
                frames.len()
            );
        }
        return;
    }
    if args.first().is_some_and(|s| s == "cold-intensity") {
        cold_intensity();
        return;
    }
    if args.first().is_some_and(|s| s == "cold") {
        cpu::pin_thread();
        let input = [0; 576];
        let info = GranuleChannel {
            global_gain: 210,
            ..Default::default()
        };
        let factors = ScaleFactors::default();
        let bands = Bands::for_rate(44100).unwrap();
        let mut output = [0.0; 576];
        heap::measure(|| ());
        black_box(cpu::cycles());
        let start = cpu::cycles();
        let clock = std::time::Instant::now();
        let (_, counts) = heap::measure(|| {
            spectral::requantize(
                black_box(&input),
                black_box(&info),
                black_box(&factors),
                bands,
                black_box(&mut output),
            )
            .unwrap();
        });
        let ns = clock.elapsed().as_nanos();
        let cycles = cpu::cycles() - start;
        println!("cold/zero: cycles/op {cycles}; ns/op {ns}");
        println!("cold/zero: allocation {counts:?}");
        return;
    }
    if args.first().is_some_and(|s| s == "verify") {
        verify(&args[1..]);
        return;
    }
    let input = signal::<32>(0x73429);
    let mut pcm = [0.0; 32];
    let mut synthesis = Synthesis::new();
    probe("synthesis/slot", 10_000, || {
        synthesis.process(black_box(&input), black_box(&mut pcm));
    });
    let zero = [0.0; 32];
    probe("synthesis/zero", 10_000, || {
        synthesis.process(black_box(&zero), black_box(&mut pcm));
    });
    let mut history = Synthesis::new();
    for _ in 0..16 {
        history.process(&input, &mut pcm);
    }
    probe("synthesis/zero-history", 10_000, || {
        let mut state = history.clone();
        state.process(black_box(&zero), black_box(&mut pcm));
    });
    for (kind, mixed) in [(0, 0), (1, 0), (3, 0), (2, 0), (2, 2), (2, 4)] {
        let source = signal::<576>(0x73429);
        let mut hybrid = Hybrid::new();
        let mut output = [0.0; 576];
        probe(&format!("hybrid/{kind}/{mixed}"), 1000, || {
            let mut input = source;
            hybrid
                .process(black_box(&mut input), kind, mixed, black_box(&mut output))
                .unwrap();
        });
    }
    let bytes: Vec<_> = (0..4096).map(|n| (n * 37 + 13) as u8).collect();
    for size in [2, 17, 32, 1024] {
        probe(
            &format!("crc/{size}"),
            if size == 1024 { 1000 } else { 10_000 },
            || {
                black_box(checksum::bytes(
                    black_box(0xffff),
                    black_box(&bytes[..size]),
                ));
            },
        );
    }
    for width in [0, 1, 2, 3, 4, 5, 7, 8, 9, 12, 13, 16, 24, 32] {
        probe(&format!("bits/{width}"), 1000, || {
            let mut bits = BitReader::new(black_box(&bytes));
            for _ in 0..512 {
                black_box(bits.read(black_box(width)).unwrap());
            }
        });
    }
    probe("bits/flag", 1000, || {
        let mut bits = BitReader::new(black_box(&bytes));
        for _ in 0..512 {
            black_box(bits.flag().unwrap());
        }
    });
    for book in [1, 13, 23, 31] {
        probe(&format!("huffman/{book}"), 1000, || {
            let mut bits = BitReader::new(black_box(&bytes));
            for _ in 0..256 {
                black_box(huffman::pair(&mut bits, book).unwrap());
            }
        });
    }
    for table_b in [false, true] {
        probe(&format!("huffman/count1/{table_b}"), 1000, || {
            let mut bits = BitReader::new(black_box(&bytes));
            for _ in 0..256 {
                black_box(huffman::quad(&mut bits, table_b).unwrap());
            }
        });
    }
    // Numeric fixture codewords match tests/data/huffman.csv. Setup is untimed.
    for (book, code, width, x, y, lin) in [
        (1, 1, 1, 0, 0, 0),
        (1, 0, 3, 1, 1, 0),
        (7, 1, 1, 0, 0, 0),
        (7, 0, 10, 5, 5, 0),
        (13, 1, 1, 0, 0, 0),
        (13, 1, 16, 15, 15, 0),
        (23, 1, 1, 0, 0, 13),
        (23, 3, 8, 15, 15, 13),
        (31, 15, 4, 0, 0, 13),
        (31, 3, 4, 15, 15, 13),
    ] {
        let mut writer = bits::BitWriter::new();
        let mut expected = [0; 2];
        for _ in 0..256 {
            writer.write(code, width).unwrap();
            for (n, value) in [x, y].into_iter().enumerate() {
                let extra = if value == 15 { (1 << lin) - 1 } else { 0 };
                if value == 15 && lin != 0 {
                    writer.write(extra, lin).unwrap();
                }
                if value != 0 {
                    writer.write(1, 1).unwrap();
                }
                expected[n] = -((value + extra) as i16);
            }
        }
        let data = writer.as_bytes();
        let length = writer.len_bits();
        let mut check = BitReader::bounded(data, 0, length).unwrap();
        for _ in 0..256 {
            assert_eq!(huffman::pair(&mut check, book).unwrap(), expected);
        }
        assert_eq!(check.remaining(), 0);
        probe(&format!("huffman/pattern/{book}/{x}/{y}"), 1000, || {
            let mut bits = BitReader::bounded(black_box(data), 0, length).unwrap();
            for _ in 0..256 {
                black_box(huffman::pair(&mut bits, book).unwrap());
            }
        });
    }
    for table_b in [false, true] {
        for (value, code, width) in [(0u8, 1, 1), (1, 5, 4), (5, 5, 6), (15, 1, 6)] {
            let mut writer = bits::BitWriter::new();
            let mut expected = [0; 4];
            for _ in 0..256 {
                if table_b {
                    writer.write(u32::from(15 ^ value), 4).unwrap();
                } else {
                    writer.write(code, width).unwrap();
                }
                for (n, sample) in expected.iter_mut().enumerate() {
                    if value & (1 << (3 - n)) != 0 {
                        writer.write((n & 1) as u32, 1).unwrap();
                        *sample = if n & 1 == 0 { 1 } else { -1 };
                    }
                }
            }
            let data = writer.as_bytes();
            let length = writer.len_bits();
            let mut check = BitReader::bounded(data, 0, length).unwrap();
            for _ in 0..256 {
                assert_eq!(huffman::quad(&mut check, table_b).unwrap(), expected);
            }
            assert_eq!(check.remaining(), 0);
            probe(
                &format!("huffman/count1-pattern/{table_b}/{value}"),
                1000,
                || {
                    let mut bits = BitReader::bounded(black_box(data), 0, length).unwrap();
                    for _ in 0..256 {
                        black_box(huffman::quad(&mut bits, table_b).unwrap());
                    }
                },
            );
        }
    }
    for (kind, mixed) in [(0, false), (2, false), (2, true)] {
        for intensity in [false, true] {
            for compress in [0, 399, 511] {
                let header = joint(22050, u8::from(intensity));
                let channel = usize::from(intensity);
                let mut side = SideInfo::default();
                side.granules[0][channel] = GranuleChannel {
                    block_type: kind,
                    mixed_block: mixed,
                    scalefac_compress: compress,
                    part2_3_bits: 4095,
                    ..Default::default()
                };
                probe(
                    &format!("scalefactors/lsf/{kind}/{mixed}/{intensity}/{compress}"),
                    10_000,
                    || {
                        let mut cursor = BitReader::new(black_box(&bytes));
                        black_box(
                            scalefactors::read(
                                &mut cursor,
                                header,
                                black_box(&side),
                                0,
                                channel,
                                None,
                            )
                            .unwrap(),
                        );
                    },
                );
            }
        }
    }
    let mpeg1_header = joint(44100, 0);
    for (kind, mixed) in [(0, false), (2, false), (2, true)] {
        for compress in [0, 4, 15] {
            let mut side = SideInfo::default();
            side.granules[0][0] = GranuleChannel {
                block_type: kind,
                mixed_block: mixed,
                scalefac_compress: compress,
                part2_3_bits: 4095,
                ..Default::default()
            };
            probe(
                &format!("scalefactors/mpeg1/{kind}/{mixed}/0/{compress}/0"),
                10_000,
                || {
                    let mut cursor = BitReader::new(black_box(&bytes));
                    black_box(
                        scalefactors::read(&mut cursor, mpeg1_header, black_box(&side), 0, 0, None)
                            .unwrap(),
                    );
                },
            );
        }
    }
    let previous = ScaleFactors {
        long: std::array::from_fn(|n| (n % 8) as u8),
        long_width: std::array::from_fn(|n| (n % 5) as u8),
        ..Default::default()
    };
    for compress in [0, 4, 15] {
        for mask in [0, 5, 15] {
            let mut side = SideInfo::default();
            side.scfsi[0] = std::array::from_fn(|n| mask & (1 << n) != 0);
            side.granules[1][0] = GranuleChannel {
                scalefac_compress: compress,
                part2_3_bits: 4095,
                ..Default::default()
            };
            probe(
                &format!("scalefactors/mpeg1/0/false/1/{compress}/{mask}"),
                10_000,
                || {
                    let mut cursor = BitReader::new(black_box(&bytes));
                    black_box(
                        scalefactors::read(
                            &mut cursor,
                            mpeg1_header,
                            black_box(&side),
                            1,
                            0,
                            Some(black_box(&previous)),
                        )
                        .unwrap(),
                    );
                },
            );
        }
    }
    for (region0_count, region1_count) in [(0, 0), (7, 7), (15, 7)] {
        let info = GranuleChannel {
            big_values: 288,
            region0_count,
            region1_count,
            table_select: [1, 13, 23],
            ..Default::default()
        };
        let bands = Bands::for_rate(44100).unwrap();
        let mut output = [0; 576];
        probe(
            &format!("quantized/{region0_count}/{region1_count}"),
            1000,
            || {
                let mut bits = BitReader::new(black_box(&bytes));
                black_box(
                    spectral::quantized(&mut bits, black_box(&info), bands, black_box(&mut output))
                        .unwrap(),
                );
            },
        );
    }
    for (label, table_select) in [("zero", [0, 0, 0]), ("zero-middle", [1, 0, 23])] {
        let info = GranuleChannel {
            big_values: 288,
            region0_count: 7,
            region1_count: 7,
            table_select,
            ..Default::default()
        };
        let bands = Bands::for_rate(44100).unwrap();
        let mut output = [0; 576];
        probe(&format!("quantized/{label}"), 1000, || {
            let mut bits = BitReader::new(black_box(&bytes));
            black_box(
                spectral::quantized(&mut bits, black_box(&info), bands, black_box(&mut output))
                    .unwrap(),
            );
        });
    }
    for (kind, mixed) in [(0, false), (2, false), (2, true)] {
        for wide in [false, true] {
            let input = std::array::from_fn(|n| {
                let value = if wide {
                    (n * 37 % 8207) as i16
                } else {
                    (n % 3) as i16
                };
                if n & 1 == 0 { value } else { -value }
            });
            let info = GranuleChannel {
                global_gain: 210,
                block_type: kind,
                mixed_block: mixed,
                ..Default::default()
            };
            let factors = ScaleFactors::default();
            let bands = Bands::for_rate(44100).unwrap();
            let mut output = [0.0; 576];
            probe(&format!("requantize/{kind}/{mixed}/{wide}"), 1000, || {
                spectral::requantize(
                    black_box(&input),
                    black_box(&info),
                    black_box(&factors),
                    bands,
                    black_box(&mut output),
                )
                .unwrap();
            });
        }
    }
    for mode in 1..=3 {
        let source = [signal::<576>(19), signal::<576>(31)];
        let info = [GranuleChannel::default(); 2];
        let factors = [ScaleFactors::default(), ScaleFactors::default()];
        let header = joint(44100, mode);
        let bands = Bands::for_rate(44100).unwrap();
        probe(&format!("stereo/{mode}"), 10_000, || {
            let mut input = source;
            spectral::stereo(
                header,
                black_box(&info),
                black_box(&factors),
                bands,
                black_box(&mut input),
            )
            .unwrap();
        });
    }
    for (rate, kind, mixed) in [(44100, 0, false), (44100, 2, false), (8000, 2, true)] {
        for mode in [1, 3] {
            for live in [0, 384] {
                let mut source = [signal::<576>(19), signal::<576>(31)];
                source[1][live..].fill(0.0);
                let info = [GranuleChannel {
                    block_type: kind,
                    mixed_block: mixed,
                    ..Default::default()
                }; 2];
                let factors = [
                    ScaleFactors {
                        long: [3; 22],
                        short: [[3; 3]; 13],
                        long_width: [3; 22],
                        short_width: [[3; 3]; 13],
                        ..Default::default()
                    },
                    ScaleFactors {
                        long: [3; 22],
                        short: [[3; 3]; 13],
                        long_width: [3; 22],
                        short_width: [[3; 3]; 13],
                        ..Default::default()
                    },
                ];
                let header = joint(rate, mode);
                let bands = Bands::for_rate(rate).unwrap();
                probe(
                    &format!("stereo-shape/{rate}/{kind}/{mixed}/{mode}/{live}"),
                    10_000,
                    || {
                        let mut input = source;
                        spectral::stereo(
                            header,
                            black_box(&info),
                            black_box(&factors),
                            bands,
                            black_box(&mut input),
                        )
                        .unwrap();
                    },
                );
            }
        }
    }
    for path in &args {
        let frames = packets(Path::new(path));
        let label = Path::new(path).file_name().unwrap().to_string_lossy();
        if let Some((data, free)) = frames.iter().find(|(data, free)| {
            let frame = Frame::parse(data, *free, true, Limits::default()).unwrap();
            frame.side_info.main_data_begin == 0
                && frame.side_info.main_data_bits(frame.header) != 0
        }) {
            probe(&format!("decode-first/{label}"), 16, || {
                let mut decoder = FrameDecoder::new();
                let frame = decoder.decode(black_box(data), *free).unwrap();
                for plane in frame.pcm.planes() {
                    black_box(plane);
                }
            });
        }
        let mut decoder = FrameDecoder::new();
        probe(&format!("decode/{label}"), 2, || {
            decoder.reset();
            for (data, free) in &frames {
                let frame = decoder.decode(black_box(data), *free).unwrap();
                for plane in frame.pcm.planes() {
                    black_box(plane);
                }
            }
        });
    }
}
