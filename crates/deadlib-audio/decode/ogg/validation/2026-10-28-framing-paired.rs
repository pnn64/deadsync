extern crate frame_before;
extern crate frame_empty;
extern crate frame_guards;
extern crate frame_historical;
extern crate frame_writer;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use frame_before::{BOS, CONTINUED, EOS, PageHeader, UNKNOWN_GRANULE_POSITION, encode_page};
use std::{
    hint::black_box,
    io::{self, Write},
};
fn append(bytes: &mut Vec<u8>, flags: u8, granule: u64, sequence: u32, laces: &[u8], body: &[u8]) {
    bytes.extend(
        encode_page(
            PageHeader {
                flags,
                granule_position: granule,
                serial: 42,
                sequence,
            },
            laces,
            body,
        )
        .unwrap(),
    );
}

fn spanning(span: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    append(&mut bytes, BOS, 100, 0, &[1], &[37]);
    let mut sequence = 1;
    for packet in 1..=64 {
        for part in 0..span {
            append(
                &mut bytes,
                if part == 0 { 0 } else { CONTINUED },
                UNKNOWN_GRANULE_POSITION,
                sequence,
                &[255],
                &[37; 255],
            );
            sequence += 1;
        }
        append(
            &mut bytes,
            CONTINUED | if packet == 64 { EOS } else { 0 },
            100 + packet * 100,
            sequence,
            &[0],
            &[],
        );
        sequence += 1;
    }
    bytes
}

fn nil_pages() -> Vec<u8> {
    let mut bytes = Vec::new();
    append(&mut bytes, BOS, 100, 0, &[1], &[37]);
    let mut sequence = 1;
    for packet in 1..=64 {
        append(
            &mut bytes,
            0,
            UNKNOWN_GRANULE_POSITION,
            sequence,
            &[255],
            &[37; 255],
        );
        sequence += 1;
        for _ in 0..32 {
            append(
                &mut bytes,
                CONTINUED,
                UNKNOWN_GRANULE_POSITION,
                sequence,
                &[],
                &[],
            );
            sequence += 1;
        }
        append(
            &mut bytes,
            CONTINUED | if packet == 64 { EOS } else { 0 },
            100 + packet * 100,
            sequence,
            &[0],
            &[],
        );
        sequence += 1;
    }
    bytes
}
fn chained(history: u32) -> Vec<u8> {
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
    bytes
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
fn consume_before(p: frame_before::Packet<'_>) {
    black_box(p);
}
fn consume_empty(p: frame_empty::Packet<'_>) {
    black_box(p);
}
fn consume_guards(p: frame_guards::Packet<'_>) {
    black_box(p);
}
fn consume_historical(p: frame_historical::Packet<'_>) {
    black_box(p);
}
macro_rules! demux_pair {
    ($label:expr,$before:ident,$after:ident,$bytes:expr,$old:path,$new:path) => {{
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut offset = 0;
        while offset < $bytes.len() {
            let (page, len) = $before::Page::parse(&$bytes[offset..]).unwrap();
            before.push((page, offset as u64));
            after.push((
                $after::Page::parse(&$bytes[offset..]).unwrap().0,
                offset as u64,
            ));
            offset += len;
        }
        let old = black_box($old as for<'a> fn($before::Packet<'a>));
        let new = black_box($new as for<'a> fn($after::Packet<'a>));
        cpu::compare($label, 8, |after_variant| {
            if after_variant {
                let mut stream = $after::Demuxer::default();
                for &(page, offset) in black_box(&after) {
                    stream.push_page(page, offset, new).unwrap();
                }
                stream.finish().unwrap();
            } else {
                let mut stream = $before::Demuxer::default();
                for &(page, offset) in black_box(&before) {
                    stream.push_page(page, offset, old).unwrap();
                }
                stream.finish().unwrap();
            }
        });
    }};
}
fn main() {
    for history in [0, 64, 512] {
        let bytes = chained(history);
        demux_pair!(
            &format!("historical-before/{history}"),
            frame_historical,
            frame_before,
            bytes,
            consume_historical,
            consume_before
        );
        demux_pair!(
            &format!("historical-final/{history}"),
            frame_historical,
            frame_guards,
            bytes,
            consume_historical,
            consume_guards
        );
    }
    for span in [1, 4, 64] {
        let bytes = spanning(span);
        demux_pair!(
            &format!("empty/{span}"),
            frame_before,
            frame_empty,
            bytes,
            consume_before,
            consume_empty
        );
    }
    let bytes = nil_pages();
    demux_pair!(
        "empty/nil",
        frame_before,
        frame_empty,
        bytes,
        consume_before,
        consume_empty
    );
    let bytes = chained(64);
    demux_pair!(
        "empty/completed",
        frame_before,
        frame_empty,
        bytes,
        consume_before,
        consume_empty
    );
    for target in [0, 8192] {
        for size in [0, 1, 17, 255, 8192, 65025, 130050] {
            let payload = vec![37; size];
            let iterations = if size >= 65025 {
                2
            } else if size >= 8192 {
                8
            } else {
                500
            };
            cpu::compare(&format!("writer/{target}/{size}"), iterations, |new| {
                if new {
                    let mut writer = frame_writer::PacketWriter::with_options(
                        Sink::default(),
                        42,
                        frame_writer::WriteOptions {
                            target_page_body: target,
                        },
                    );
                    writer.write_packet(b"header", 0, false).unwrap();
                    for i in 0..8 {
                        writer.write_packet(black_box(&payload), i, i == 7).unwrap();
                    }
                    black_box(writer.into_inner().0);
                } else {
                    let mut writer = frame_empty::PacketWriter::with_options(
                        Sink::default(),
                        42,
                        frame_empty::WriteOptions {
                            target_page_body: target,
                        },
                    );
                    writer.write_packet(b"header", 0, false).unwrap();
                    for i in 0..8 {
                        writer.write_packet(black_box(&payload), i, i == 7).unwrap();
                    }
                    black_box(writer.into_inner().0);
                }
            });
        }
    }
    for target in [0, 8192] {
        for size in [0, 1, 17, 255, 8192, 65025, 130050] {
            let payload = vec![37; size];
            let iterations = if size >= 65025 {
                2
            } else if size >= 8192 {
                8
            } else {
                500
            };
            cpu::compare(&format!("guards/{target}/{size}"), iterations, |new| {
                if new {
                    let mut writer = frame_guards::PacketWriter::with_options(
                        Sink::default(),
                        42,
                        frame_guards::WriteOptions {
                            target_page_body: target,
                        },
                    );
                    writer.write_packet(b"header", 0, false).unwrap();
                    for i in 0..8 {
                        writer.write_packet(black_box(&payload), i, i == 7).unwrap();
                    }
                    black_box(writer.into_inner().0);
                } else {
                    let mut writer = frame_writer::PacketWriter::with_options(
                        Sink::default(),
                        42,
                        frame_writer::WriteOptions {
                            target_page_body: target,
                        },
                    );
                    writer.write_packet(b"header", 0, false).unwrap();
                    for i in 0..8 {
                        writer.write_packet(black_box(&payload), i, i == 7).unwrap();
                    }
                    black_box(writer.into_inner().0);
                }
            });
        }
    }
    for size in [0, 1, 17] {
        let payload = vec![37; size];
        let mut before = frame_writer::PacketWriter::new(Sink::default(), 42);
        let mut after = frame_guards::PacketWriter::new(Sink::default(), 42);
        before.write_packet(b"header", 0, false).unwrap();
        after.write_packet(b"header", 0, false).unwrap();
        for _ in 0..255 {
            before.write_packet(&payload, 100, false).unwrap();
            after.write_packet(&payload, 100, false).unwrap();
        }
        before.flush_page().unwrap();
        after.flush_page().unwrap();
        cpu::compare(&format!("guard-steady/{size}"), 131_072, |new| {
            if new {
                after
                    .write_packet(black_box(&payload), black_box(100), false)
                    .unwrap();
            } else {
                before
                    .write_packet(black_box(&payload), black_box(100), false)
                    .unwrap();
            }
        });
    }
}
