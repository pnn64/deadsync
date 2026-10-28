use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};
use std::ops::ControlFlow;

fn wire(target: usize, base: u64) -> Vec<u8> {
    let mut writer = PacketWriter::with_options(
        Vec::new(),
        42,
        WriteOptions {
            target_page_body: target,
        },
    );
    let sizes = [
        1, 0, 1, 254, 255, 256, 510, 511, 8191, 65024, 65025, 130050, 0, 17,
    ];
    for (number, size) in sizes.into_iter().enumerate() {
        let data: Vec<_> = (0..size).map(|i| (i * 37 + number * 11) as u8).collect();
        writer
            .write_packet(&data, base + number as u64 * 7, number == sizes.len() - 1)
            .unwrap();
    }
    writer.into_inner()
}

fn compatibility_wire() -> Vec<u8> {
    let mut out = Vec::new();
    for (sequence, (flags, granule_position, lacing, body)) in [
        (BOS, 100, vec![1], vec![b'h']),
        (0, 0, vec![255], vec![b'a'; 255]),
        (CONTINUED, 150, vec![0, 0, 1], vec![b'b']),
        (EOS, 50, vec![0; 255], vec![]),
    ]
    .into_iter()
    .enumerate()
    {
        out.extend(
            encode_page(
                PageHeader {
                    flags,
                    granule_position,
                    serial: 42,
                    sequence: sequence as u32,
                },
                &lacing,
                &body,
            )
            .unwrap(),
        );
    }
    out
}

fn state(out: &mut impl Write, capture: &PacketCapture) {
    write!(
        out,
        "P{}E{}S{:?};",
        capture.pending_bytes(),
        capture.is_ended(),
        capture.compatibility()
    )
    .unwrap();
}

fn packet(out: &mut impl Write, p: CapturedPacket<'_>) {
    write!(out, "{:?}N{}:", p.info, p.data.len()).unwrap();
    out.write_all(p.data).unwrap();
}

fn main() {
    if std::env::args().any(|a| a == "--layout") {
        println!("PacketCapture {}", std::mem::size_of::<PacketCapture>());
        println!("Demuxer {}", std::mem::size_of::<Demuxer>());
        return;
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    for count in [1u32, 2, 3, 65, 513] {
        let serials: Vec<_> = (0..count)
            .map(|i| i.wrapping_mul(0x9e37_79b9) ^ 0x1234_5678)
            .collect();
        let mut bytes = Vec::new();
        for &serial in &serials {
            bytes.extend(
                encode_page(
                    PageHeader {
                        flags: BOS | EOS,
                        granule_position: 0,
                        serial,
                        sequence: 0,
                    },
                    &[1],
                    &[37],
                )
                .unwrap(),
            );
        }
        let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
        for stream in index.streams() {
            let found = index.stream(stream.serial).unwrap();
            write!(
                out,
                "LOOKUP{count}:{found:?}EQ{};",
                std::ptr::eq(found, stream)
            )
            .unwrap();
        }
        let missing = (0..=u32::MAX).find(|s| !serials.contains(s)).unwrap();
        write!(out, "MISSING{count}:{:?};", index.stream(missing)).unwrap();
    }
    let other = encode_page(
        PageHeader {
            flags: BOS | EOS,
            granule_position: 0,
            serial: 99,
            sequence: 0,
        },
        &[0],
        &[],
    )
    .unwrap();
    let other = Page::parse(&other).unwrap().0;
    let fixtures: Vec<_> = [0, 8192, 65025]
        .into_iter()
        .flat_map(|target| {
            [0, u64::MAX - 100]
                .into_iter()
                .map(move |base| wire(target, base))
        })
        .chain([compatibility_wire()])
        .collect();
    let mut cases = 0;
    for (fixture, bytes) in fixtures.iter().enumerate() {
        for serial in [42, u32::MAX] {
            for target in [
                0,
                1,
                50,
                100,
                200,
                65535,
                u64::MAX - 100,
                u64::MAX - 1,
                u64::MAX,
            ] {
                let mut reader = io::Cursor::new(bytes.as_slice());
                let result = find_granule(&mut reader, 0..bytes.len() as u64, serial, target);
                write!(
                    out,
                    "SEARCH{fixture}/{serial}/{target}:{result:?}POS{};",
                    reader.position()
                )
                .unwrap();
            }
        }
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, length) = Page::parse(&bytes[offset..]).unwrap();
            pages.push((page, offset as u64));
            offset += length;
        }
        let index = OggIndex::read(
            bytes.as_slice(),
            IndexOptions {
                validation: ValidationOptions {
                    allow_zero_span_granules: true,
                    allow_decreasing_eos_granule: true,
                },
                ..IndexOptions::default()
            },
        )
        .unwrap();
        let mut starts = vec![0, 1, pages.len() / 2, pages.len() - 1];
        starts.sort_unstable();
        starts.dedup();
        for start in starts {
            for period in [1, 7, 16] {
                for mixed in [false, true] {
                    for indexed in [false, true] {
                        for limits in [
                            Limits::default(),
                            Limits {
                                max_packet_bytes: 511,
                                max_pending_bytes: 1024,
                                ..Limits::default()
                            },
                            Limits {
                                max_pending_bytes: 0,
                                ..Limits::default()
                            },
                        ] {
                            for policy in 0..if fixture == 6 { 4 } else { 1 } {
                                let validation = ValidationOptions {
                                    allow_zero_span_granules: policy & 1 != 0,
                                    allow_decreasing_eos_granule: policy & 2 != 0,
                                };
                                let mut capture = if indexed {
                                    PacketCapture::at_page_with_validation(
                                        &index.pages(42).unwrap()[start],
                                        limits,
                                        validation,
                                    )
                                } else {
                                    PacketCapture::with_validation(42, limits, validation)
                                };
                                write!(out, "CASE{cases}:{fixture}/{start}/{period}/{mixed}/{indexed}/{limits:?}/{validation:?};").unwrap();
                                cases += 1;
                                'pages: for &(page, offset) in pages.iter().skip(start) {
                                    let mut resumes = 0;
                                    loop {
                                        let mut emitted = 0;
                                        let result = capture.push_page_until(page, offset, |p| {
                                            packet(&mut out, p);
                                            emitted += 1;
                                            if emitted == period {
                                                ControlFlow::Break(())
                                            } else {
                                                ControlFlow::Continue(())
                                            }
                                        });
                                        write!(out, "R{result:?};").unwrap();
                                        state(&mut out, &capture);
                                        match result {
                                            Err(_) => {
                                                write!(
                                                    out,
                                                    "AGAIN{:?};",
                                                    capture.push_page(page, offset, |_| panic!(
                                                        "terminal capture emitted"
                                                    ))
                                                )
                                                .unwrap();
                                                break 'pages;
                                            }
                                            Ok(true) => break,
                                            Ok(false) => {
                                                write!(
                                                    out,
                                                    "OTHER{:?};",
                                                    capture.push_page(other, offset, |_| panic!(
                                                        "wrong serial emitted"
                                                    ))
                                                )
                                                .unwrap();
                                                state(&mut out, &capture);
                                                resumes += 1;
                                                if mixed && resumes % 3 == 0 {
                                                    let result =
                                                        capture.push_page(page, offset, |p| {
                                                            packet(&mut out, p)
                                                        });
                                                    write!(out, "DRAIN{result:?};").unwrap();
                                                    state(&mut out, &capture);
                                                    if result.is_err() {
                                                        break 'pages;
                                                    }
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    if capture.is_ended() {
                                        break;
                                    }
                                }
                                write!(out, "F{:?}F{:?};", capture.finish(), capture.finish())
                                    .unwrap();
                                capture.reset();
                                state(&mut out, &capture);
                                let result =
                                    capture.push_page(pages[0].0, 0, |p| packet(&mut out, p));
                                write!(out, "RESET{result:?};").unwrap();
                                state(&mut out, &capture);
                            }
                        }
                    }
                }
            }
        }
    }
    out.flush().unwrap();
    eprintln!("CAPTURE_CASES {cases}; STREAM_LOOKUPS 584; MISSING_LOOKUPS 5; GRANULE_QUERIES 126");
}
