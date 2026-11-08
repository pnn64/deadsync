//! Resident-input CPU/allocation probes; setup and byte verification are untimed.
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;

use deadlib_audio_decode_ogg::*;
use std::{
    hint::black_box,
    io::{self, Read, Write},
};

fn probe(label: &str, iterations: usize, mut work: impl FnMut()) {
    cpu::measure(label, iterations, &mut work);
    let (_, counts) = heap::measure(work);
    println!("{label}: allocation {counts:?}");
}

#[derive(Default)]
struct Sink(usize);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        black_box(bytes);
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Capped<'a> {
    input: &'a [u8],
    limit: usize,
}
impl Read for Capped<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let length = output.len().min(self.limit);
        self.input.read(&mut output[..length])
    }
}

fn wire(target: usize) -> Vec<u8> {
    let mut writer = PacketWriter::with_options(
        Vec::new(),
        42,
        WriteOptions {
            target_page_body: target,
        },
    );
    writer.write_packet(b"header", 0, false).unwrap();
    for (i, n) in [0, 1, 254, 255, 256, 510, 8191, 65024, 65025, 130050, 0, 99]
        .into_iter()
        .enumerate()
    {
        let bytes: Vec<_> = (0..n).map(|v| (v * 31 + i * 17) as u8).collect();
        writer.write_packet(&bytes, i as u64, false).unwrap();
        if i % 3 == 0 {
            writer.flush_page().unwrap();
        }
    }
    writer.finish().unwrap();
    writer.into_inner()
}

fn nil_continuations() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut sequence = 0;
    let mut emit = |flags, granule_position, lacing: &[u8], body: &[u8]| {
        bytes.extend(
            encode_page(
                PageHeader {
                    flags,
                    granule_position,
                    serial: 42,
                    sequence,
                },
                lacing,
                body,
            )
            .unwrap(),
        );
        sequence += 1;
    };
    emit(BOS, 100, &[1], &[37]);
    for packet in 1..=64 {
        emit(0, UNKNOWN_GRANULE_POSITION, &[255], &[37; 255]);
        for _ in 0..32 {
            emit(CONTINUED, UNKNOWN_GRANULE_POSITION, &[], &[]);
        }
        emit(
            CONTINUED | if packet == 64 { EOS } else { 0 },
            100 + packet * 100,
            &[0],
            &[],
        );
    }
    bytes
}

fn main() {
    if std::env::args().any(|arg| arg == "verify") {
        let mut output = io::stdout().lock();
        for target in [0, 1, 255, 8192, 65025, usize::MAX] {
            output.write_all(&wire(target)).unwrap();
        }
        return;
    }
    let bytes = nil_continuations();
    let mut pages = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let (page, length) = Page::parse(&bytes[offset..]).unwrap();
        pages.push((page, offset as u64));
        offset += length;
    }
    probe("nil-demux", 8, || {
        let mut demux = Demuxer::default();
        for &(page, offset) in black_box(&pages) {
            demux
                .push_page(page, offset, |packet| {
                    black_box(packet);
                })
                .unwrap();
        }
        demux.finish().unwrap();
    });
    probe("nil-capture", 8, || {
        let mut capture = PacketCapture::new(42, Limits::default());
        for &(page, offset) in black_box(&pages) {
            capture
                .push_page(page, offset, |packet| {
                    black_box(packet);
                })
                .unwrap();
        }
        capture.finish().unwrap();
    });
    for span in [1, 4, 64] {
        let mut bytes = encode_page(
            PageHeader {
                flags: BOS,
                granule_position: 100,
                serial: 42,
                sequence: 0,
            },
            &[1],
            &[37],
        )
        .unwrap();
        let mut sequence = 1;
        for packet in 1..=64 {
            for part in 0..span {
                bytes.extend(
                    encode_page(
                        PageHeader {
                            flags: if part == 0 { 0 } else { CONTINUED },
                            granule_position: UNKNOWN_GRANULE_POSITION,
                            serial: 42,
                            sequence,
                        },
                        &[255],
                        &[37; 255],
                    )
                    .unwrap(),
                );
                sequence += 1;
            }
            bytes.extend(
                encode_page(
                    PageHeader {
                        flags: CONTINUED | if packet == 64 { EOS } else { 0 },
                        granule_position: 100 + packet * 100,
                        serial: 42,
                        sequence,
                    },
                    &[0],
                    &[],
                )
                .unwrap(),
            );
            sequence += 1;
        }
        let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, length) = Page::parse(&bytes[offset..]).unwrap();
            pages.push((page, offset as u64));
            offset += length;
        }
        probe(&format!("span-demux/{span}"), 32, || {
            let mut demux = Demuxer::default();
            for &(page, offset) in black_box(&pages) {
                demux
                    .push_page(page, offset, |packet| {
                        black_box(packet);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
        });
        probe(&format!("span-capture/{span}"), 32, || {
            let mut capture = PacketCapture::new(42, Limits::default());
            for &(page, offset) in black_box(&pages) {
                capture
                    .push_page(page, offset, |packet| {
                        black_box(packet);
                    })
                    .unwrap();
            }
            capture.finish().unwrap();
        });
        let mut cursor = 0;
        probe(&format!("span-query/{span}/granule"), 10_000, || {
            cursor = (cursor + 17) & 8191;
            black_box(index.granule_at_or_before(black_box(42), black_box(cursor)));
        });
        let mut cursor = 0;
        let mask = bytes.len().next_power_of_two() as u64 - 1;
        probe(&format!("span-query/{span}/offset"), 10_000, || {
            cursor = (cursor + 31337) & mask;
            black_box(index.page_at_or_before(black_box(42), black_box(cursor)));
        });
        let mut cursor = 0;
        probe(&format!("span-query/{span}/packet"), 10_000, || {
            cursor = (cursor + 17) & 63;
            black_box(index.packet_start_page(black_box(42), black_box(cursor)));
        });
    }
    for segments in [1, 2, 3, 4, 5, 8, 17, 255] {
        for terminated in [false, true] {
            let mut lacing = vec![255; segments];
            if terminated {
                lacing[segments - 1] = 17;
            }
            let body = vec![37; lacing.iter().map(|&n| n as usize).sum()];
            let frame = encode_page(
                PageHeader {
                    flags: CONTINUED,
                    granule_position: if terminated {
                        0
                    } else {
                        UNKNOWN_GRANULE_POSITION
                    },
                    serial: 42,
                    sequence: 0,
                },
                &lacing,
                &body,
            )
            .unwrap();
            let page = Page::parse(&frame).unwrap().0;
            probe(
                &format!("fragment-prefix/{segments}/{terminated}"),
                10_000,
                || {
                    for fragment in black_box(page).fragments() {
                        black_box(fragment);
                    }
                },
            );
        }
    }
    let crc_input: Vec<_> = (0usize..4112)
        .map(|i| (i.wrapping_mul(37) ^ (i >> 3)) as u8)
        .collect();
    for length in [
        63, 64, 65, 127, 128, 129, 255, 256, 257, 511, 512, 513, 4096,
    ] {
        for offset in [0, 1, 15] {
            let input = &crc_input[offset..offset + length];
            probe(
                &format!("crc-boundary/{length}/{offset}"),
                if length >= 4096 { 512 } else { 10_000 },
                || {
                    black_box(crc32(black_box(input)));
                },
            );
        }
    }
    for segments in [1, 17, 128, 255] {
        let frame = encode_page(
            PageHeader {
                flags: 0,
                granule_position: UNKNOWN_GRANULE_POSITION,
                serial: 42,
                sequence: 0,
            },
            &vec![255; segments],
            &vec![37; segments * 255],
        )
        .unwrap();
        let header = &frame[..27 + segments];
        for chunk in [1, 7, 27] {
            let mut decoder = PageDecoder::new();
            decoder.push(header).unwrap();
            probe(&format!("decoder-header/{segments}/{chunk}"), 1000, || {
                decoder.reset();
                for bytes in black_box(header).chunks(chunk) {
                    let progress = decoder.push(bytes).unwrap();
                    assert_eq!(progress.consumed, bytes.len());
                    assert!(progress.page.is_none());
                }
            });
        }
    }
    for streams in [2u32, 16, 128] {
        for shuffled in [false, true] {
            let mut bytes = Vec::new();
            for sequence in 0..64 {
                for i in 0..streams {
                    let serial = if shuffled {
                        (i * 17 + sequence * 13) & (streams - 1)
                    } else {
                        i
                    };
                    bytes.extend(
                        encode_page(
                            PageHeader {
                                flags: if sequence == 0 {
                                    BOS
                                } else if sequence == 63 {
                                    EOS
                                } else {
                                    0
                                },
                                granule_position: sequence as u64,
                                serial,
                                sequence,
                            },
                            &[1],
                            &[37],
                        )
                        .unwrap(),
                    );
                }
            }
            probe(
                &format!("mux-index/{streams}/{shuffled}"),
                if streams == 128 { 1 } else { 8 },
                || {
                    black_box(
                        OggIndex::read(black_box(bytes.as_slice()), IndexOptions::default())
                            .unwrap(),
                    );
                },
            );
        }
    }
    for size in [65025, 130050] {
        let mut writer = PacketWriter::new(Vec::new(), 42);
        writer.write_packet(b"header", 0, false).unwrap();
        let data = vec![37; size];
        for i in 0..16 {
            writer.write_packet(&data, i, i == 15).unwrap();
        }
        let bytes = writer.into_inner();
        probe(&format!("span-index/{size}"), 1, || {
            black_box(
                OggIndex::read(black_box(bytes.as_slice()), IndexOptions::default()).unwrap(),
            );
        });
    }
    for segments in [8, 17, 128, 255] {
        let frame = encode_page(
            PageHeader {
                flags: 0,
                granule_position: UNKNOWN_GRANULE_POSITION,
                serial: 42,
                sequence: 0,
            },
            &vec![255; segments],
            &vec![37; segments * 255],
        )
        .unwrap();
        let header = &frame[..27 + segments];
        let mut sync = SyncBuffer::with_limit(MAX_PAGE_SIZE);
        sync.push(header).unwrap();
        probe(&format!("lacing-header/{segments}"), 10_000, || {
            sync.reset();
            sync.push(black_box(header)).unwrap();
            assert!(matches!(sync.page_seek().unwrap(), PageSeek::NeedMore));
            black_box(sync.buffered_bytes());
        });
    }
    for n in [0, 17, 4096] {
        let mut lacing = vec![255; n / 255];
        lacing.push((n % 255) as u8);
        let frame = encode_page(
            PageHeader {
                flags: 0,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &vec![37; n],
        )
        .unwrap();
        for window in [8, 128] {
            let warm = frame.repeat(window * 2);
            probe(&format!("sync-window/{n}/{window}"), 1, || {
                let mut sync = SyncBuffer::with_limit(warm.len());
                sync.push(black_box(&warm)).unwrap();
                let capacity = sync.storage_bytes();
                for _ in 0..window {
                    let PageSeek::Page(page) = sync.page_seek().unwrap() else {
                        panic!("warm page missing");
                    };
                    black_box(page);
                }
                for _ in 0..1024 {
                    let PageSeek::Page(page) = sync.page_seek().unwrap() else {
                        panic!("window page missing");
                    };
                    black_box(page);
                    sync.push(black_box(&frame)).unwrap();
                }
                let mut tail = 0;
                while let PageSeek::Page(page) = sync.page_seek().unwrap() {
                    black_box(page);
                    tail += 1;
                }
                assert_eq!(tail, window);
                assert_eq!(sync.position(), ((window * 2 + 1024) * frame.len()) as u64);
                assert_eq!(sync.buffered_bytes(), 0);
                assert_eq!(sync.storage_bytes(), capacity);
            });
        }
    }
    for n in [4096, 1048576] {
        for stride in [0, 31, 1, 2, 3] {
            let mut bytes = vec![37; n];
            if stride == 2 || stride == 3 {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = if i % stride == 0 { b'O' } else { b'g' };
                }
            } else if stride != 0 {
                for i in (0..n).step_by(stride) {
                    bytes[i] = b'O';
                }
            }
            let valid = wire(8192);
            let mut expected = 0;
            let mut remaining = valid.as_slice();
            while !remaining.is_empty() {
                let (_, length) = Page::parse(remaining).unwrap();
                remaining = &remaining[length..];
                expected += 1;
            }
            bytes.extend(valid);
            for recover in [false, true] {
                let label = format!(
                    "noise-{}/{n}/{stride}",
                    if recover { "reader" } else { "sync" }
                );
                probe(&label, if n >= 1048576 { 8 } else { 64 }, || {
                    let mut pages = 0;
                    if recover {
                        let mut reader =
                            PageReader::with_mode(black_box(bytes.as_slice()), SyncMode::Recover);
                        while let Some(page) = reader.next_page().unwrap() {
                            black_box(page);
                            pages += 1;
                        }
                        assert_eq!(reader.discarded_bytes(), n as u64);
                    } else {
                        let mut sync = SyncBuffer::with_limit(bytes.len());
                        sync.push(black_box(&bytes)).unwrap();
                        loop {
                            match sync.page_out().unwrap() {
                                PageOut::NeedMore => break,
                                PageOut::Hole => (),
                                PageOut::Page(page) => {
                                    black_box(page);
                                    pages += 1;
                                }
                            }
                        }
                        assert_eq!(sync.buffered_bytes(), 0);
                    }
                    assert_eq!(pages, expected);
                });
            }
        }
    }
    for n in [4, 5, 6, 7, 12, 13, 14, 15] {
        let bytes: Vec<_> = (0..n).map(|i| (i * 37 + i / 255) as u8).collect();
        probe(&format!("crc-tail/{n}"), 10_000, || {
            black_box(crc32(black_box(&bytes)));
        });
    }
    for n in [64, 256, 4096, 65025, 1048576] {
        let bytes: Vec<_> = (0..n).map(|i| (i * 37 + i / 255) as u8).collect();
        probe(
            &format!("crc/{n}"),
            if n >= 65025 { 1 } else { 1000 },
            || {
                black_box(crc32(black_box(&bytes)));
            },
        );
    }
    for n in [0, 36, 228, 4096, 65025] {
        let body = vec![37; n];
        let mut lacing = vec![255; n / 255];
        if lacing.len() < 255 {
            lacing.push((n % 255) as u8);
        }
        let bytes = encode_page(
            PageHeader {
                flags: 0,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &body,
        )
        .unwrap();
        let fields = Page::parse(&bytes).unwrap().0.header();
        probe(
            &format!("encode/{n}"),
            if n >= 4096 { 100 } else { 10_000 },
            || {
                black_box(
                    encode_page(black_box(fields), black_box(&lacing), black_box(&body)).unwrap(),
                );
            },
        );
        probe(
            &format!("parse/{n}"),
            if n >= 4096 { 100 } else { 10_000 },
            || {
                black_box(Page::parse(black_box(&bytes)).unwrap());
            },
        );
    }
    for n in [17, 65025] {
        let mut lacing = vec![255; n / 255];
        if lacing.len() < 255 {
            lacing.push((n % 255) as u8);
        }
        let bytes = encode_page(
            PageHeader {
                flags: 0,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &vec![37; n],
        )
        .unwrap();
        for limit in [1, 7, 4096, usize::MAX] {
            probe(
                &format!("capped-reader/{n}/{limit}"),
                if n >= 4096 { 16 } else { 1000 },
                || {
                    let mut reader = PageReader::new(Capped {
                        input: black_box(&bytes),
                        limit,
                    });
                    let mut pages = 0;
                    while let Some(page) = reader.next_page().unwrap() {
                        black_box(page);
                        pages += 1;
                    }
                    assert_eq!(pages, 1);
                },
            );
        }
        for chunk in [1, 7, 4096, usize::MAX] {
            probe(
                &format!("incremental/{n}/{chunk}"),
                if n >= 4096 { 16 } else { 1000 },
                || {
                    let mut decoder = PageDecoder::new();
                    let mut ready = 0;
                    for input in black_box(&bytes).chunks(chunk) {
                        let progress = decoder.push(input).unwrap();
                        assert_eq!(progress.consumed, input.len());
                        ready += usize::from(progress.page.is_some());
                        black_box(progress.page);
                    }
                    decoder.finish().unwrap();
                    assert_eq!(ready, 1);
                },
            );
        }
    }
    for n in [0, 4096, 65025] {
        let mut lacing = vec![255; n / 255];
        if lacing.len() < 255 {
            lacing.push((n % 255) as u8);
        }
        let bytes = encode_page(
            PageHeader {
                flags: 0,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &vec![37; n],
        )
        .unwrap();
        let chunks: &[usize] = if n == 65025 {
            &[1, 7, 4096, usize::MAX]
        } else {
            &[7, 4096, usize::MAX]
        };
        for &chunk in chunks {
            probe(
                &format!("sync/{n}/{chunk}"),
                if n >= 4096 { 1 } else { 1000 },
                || {
                    let mut sync = SyncBuffer::new();
                    let mut pages = 0;
                    for input in black_box(&bytes).chunks(chunk) {
                        sync.push(input).unwrap();
                        loop {
                            match sync.page_out().unwrap() {
                                PageOut::NeedMore => break,
                                PageOut::Hole => panic!("valid fixture lost synchronization"),
                                PageOut::Page(page) => {
                                    pages += 1;
                                    black_box(page.page.bytes());
                                }
                            }
                        }
                    }
                    assert_eq!(pages, 1);
                    assert_eq!(sync.buffered_bytes(), 0);
                },
            );
        }
    }
    for n in [0, 1, 17] {
        let mut writer = PacketWriter::new(Vec::new(), 42);
        writer.write_packet(b"header", 0, false).unwrap();
        for i in 0..4096 {
            writer.write_packet(&vec![37; n], i, i == 4095).unwrap();
        }
        let bytes = writer.into_inner();
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, len) = Page::parse(&bytes[offset..]).unwrap();
            pages.push((page, offset as u64));
            offset += len;
        }
        probe(&format!("dense-demux/{n}"), 32, || {
            let mut demux = Demuxer::default();
            for &(page, offset) in black_box(&pages) {
                demux
                    .push_page(page, offset, |p| {
                        black_box(p);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
        });
        probe(&format!("dense-capture/{n}"), 32, || {
            let mut capture = PacketCapture::new(42, Limits::default());
            for &(page, offset) in black_box(&pages) {
                capture
                    .push_page(page, offset, |p| {
                        black_box(p);
                    })
                    .unwrap();
            }
            capture.finish().unwrap();
        });
        probe(&format!("paused-capture/{n}"), 8, || {
            let mut capture = PacketCapture::new(42, Limits::default());
            for &(page, offset) in black_box(&pages) {
                while !capture
                    .push_page_until(page, offset, |p| {
                        black_box(p);
                        std::ops::ControlFlow::Break(())
                    })
                    .unwrap()
                {}
            }
            capture.finish().unwrap();
        });
        probe(&format!("batched-capture/{n}"), 8, || {
            let mut capture = PacketCapture::new(42, Limits::default());
            for &(page, offset) in black_box(&pages) {
                loop {
                    let mut emitted = 0;
                    if capture
                        .push_page_until(page, offset, |p| {
                            black_box(p);
                            emitted += 1;
                            if emitted == 16 {
                                std::ops::ControlFlow::Break(())
                            } else {
                                std::ops::ControlFlow::Continue(())
                            }
                        })
                        .unwrap()
                    {
                        break;
                    }
                }
            }
            capture.finish().unwrap();
        });
        probe(&format!("dense-index/{n}"), 16, || {
            black_box(
                OggIndex::read(black_box(bytes.as_slice()), IndexOptions::default()).unwrap(),
            );
        });
        let mut target = 0;
        probe(&format!("granule-search/{n}"), 32, || {
            target = (target + 131) & 4095;
            let mut reader = io::Cursor::new(black_box(bytes.as_slice()));
            black_box(
                find_granule(
                    &mut reader,
                    0..bytes.len() as u64,
                    black_box(42),
                    black_box(target),
                )
                .unwrap(),
            );
        });
    }
    for history in [0, 64, 512] {
        let mut bytes = Vec::new();
        for serial in 0..history {
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
        for sequence in 0..2048 {
            bytes.extend(
                encode_page(
                    PageHeader {
                        flags: if sequence == 0 {
                            BOS
                        } else if sequence == 2047 {
                            EOS
                        } else {
                            0
                        },
                        granule_position: sequence as u64,
                        serial: history,
                        sequence,
                    },
                    &[1],
                    &[37],
                )
                .unwrap(),
            );
        }
        let mut pages = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, len) = Page::parse(&bytes[offset..]).unwrap();
            pages.push((page, offset as u64));
            offset += len;
        }
        let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
        for (name, serial) in [
            ("first", 0),
            ("middle", history / 2),
            ("last", history),
            ("missing", u32::MAX),
        ] {
            probe(&format!("stream/{history}/{name}"), 10_000, || {
                black_box(index.stream(black_box(serial)));
            });
        }
        let mut number = 0;
        probe(&format!("packet-start/{history}"), 10_000, || {
            number = (number + 17) & 2047;
            black_box(
                index
                    .packet_start_page(black_box(history), black_box(number))
                    .unwrap(),
            );
        });
        probe(&format!("chained-demux/{history}"), 8, || {
            let mut demux = Demuxer::default();
            for &(page, offset) in black_box(&pages) {
                demux
                    .push_page(page, offset, |p| {
                        black_box(p);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
        });
        probe(&format!("runtime-stats-demux/{history}"), 8, || {
            let validation = black_box(ValidationOptions {
                allow_zero_span_granules: true,
                allow_decreasing_eos_granule: true,
            });
            let mut demux = Demuxer::with_validation(Limits::default(), validation);
            for &(page, offset) in black_box(&pages) {
                demux
                    .push_page(page, offset, |p| {
                        black_box(p);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
            black_box(demux.compatibility());
        });
        probe(&format!("runtime-stats-capture/{history}"), 8, || {
            let validation = black_box(ValidationOptions {
                allow_zero_span_granules: true,
                allow_decreasing_eos_granule: true,
            });
            let mut capture =
                PacketCapture::with_validation(history, Limits::default(), validation);
            for &(page, offset) in black_box(&pages) {
                capture
                    .push_page(page, offset, |p| {
                        black_box(p);
                    })
                    .unwrap();
            }
            capture.finish().unwrap();
            black_box(capture.compatibility());
        });
        probe(&format!("chained-index/{history}"), 4, || {
            black_box(
                OggIndex::read(black_box(bytes.as_slice()), IndexOptions::default()).unwrap(),
            );
        });
    }
    for (name, lacing, flags) in [
        ("tiny", vec![1; 255], 0),
        (
            "mixed",
            (0..255).map(|i| [0, 1, 17, 255, 254][i % 5]).collect(),
            0,
        ),
        ("255", [255, 0].repeat(127), 0),
        ("three", [255, 255, 0].repeat(85), 0),
        ("four", [255, 255, 255, 0].repeat(63), 0),
        (
            "eight",
            [255, 255, 255, 255, 255, 255, 255, 0].repeat(31),
            0,
        ),
        ("bulk", [vec![255; 128], vec![128]].concat(), 0),
        ("continued", vec![255; 255], CONTINUED),
    ] {
        let body = vec![37; lacing.iter().map(|&n| n as usize).sum()];
        let bytes = encode_page(
            PageHeader {
                flags,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &body,
        )
        .unwrap();
        let page = Page::parse(&bytes).unwrap().0;
        probe(&format!("completed/{name}"), 10_000, || {
            black_box(black_box(page).completed_packets());
        });
        probe(&format!("fragments/{name}"), 10_000, || {
            let mut sum = 0;
            for f in black_box(page).fragments() {
                sum += f.data.len() + usize::from(f.starts_packet) + usize::from(f.ends_packet);
                black_box(f.data);
            }
            black_box(sum);
        });
    }
    for n in [0, 17, 255, 8192, 65024, 130050] {
        let data = vec![37; n];
        probe(
            &format!("writer/{n}"),
            if n >= 8192 { 32 } else { 500 },
            || {
                let mut writer = PacketWriter::new(Sink::default(), 42);
                writer.write_packet(b"header", 0, false).unwrap();
                for i in 0..8 {
                    writer.write_packet(black_box(&data), i, i == 7).unwrap();
                }
                black_box(writer.into_inner().0);
            },
        );
    }
    for n in [1, 17, 255, 8192] {
        let mut writer = PacketWriter::new(Vec::new(), 42);
        writer.write_packet(b"header", 0, false).unwrap();
        for i in 0..2048 {
            writer.write_packet(&vec![37; n], i, i == 2047).unwrap();
        }
        let bytes = writer.into_inner();
        for demux in [false, true] {
            let label = format!("{}/{n}", if demux { "demux" } else { "reader" });
            probe(&label, if n >= 8192 { 1 } else { 50 }, || {
                let mut reader = PageReader::new(black_box(bytes.as_slice()));
                let mut packets = Demuxer::default();
                while let Some(page) = reader.next_page().unwrap() {
                    if demux {
                        packets
                            .push_page(page.page, page.offset, |p| {
                                black_box(p);
                            })
                            .unwrap();
                    } else {
                        black_box(page);
                    }
                }
                if demux {
                    packets.finish().unwrap();
                }
                black_box(reader.position());
            });
        }
    }
}
