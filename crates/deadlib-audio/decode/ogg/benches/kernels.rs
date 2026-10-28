//! Resident-input CPU/allocation probes; setup and byte verification are untimed.
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;

use deadlib_audio_decode_ogg::*;
use std::{
    hint::black_box,
    io::{self, Write},
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

fn main() {
    if std::env::args().any(|arg| arg == "verify") {
        let mut output = io::stdout().lock();
        for target in [0, 1, 255, 8192, 65025, usize::MAX] {
            output.write_all(&wire(target)).unwrap();
        }
        return;
    }
    for n in [0, 36, 228, 4096, 65025] {
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
        for chunk in [1, 7, 4096, usize::MAX] {
            probe(
                &format!("incremental/{n}/{chunk}"),
                if n >= 4096 { 1 } else { 1000 },
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
        probe(&format!("dense-index/{n}"), 16, || {
            black_box(
                OggIndex::read(black_box(bytes.as_slice()), IndexOptions::default()).unwrap(),
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
