extern crate summary_baseline;
extern crate summary_bounded;
extern crate summary_metadata;
extern crate summary_short;

#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::hint::black_box;
use summary_baseline::{BOS, CONTINUED, EOS, PageHeader, UNKNOWN_GRANULE_POSITION, encode_page};

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

macro_rules! index_pair {
    ($label:expr, $before:ident, $after:ident, $bytes:expr) => {
        cpu::compare($label, 32, |after| {
            if after {
                black_box(
                    $after::OggIndex::read(
                        black_box($bytes.as_slice()),
                        $after::IndexOptions::default(),
                    )
                    .unwrap(),
                );
            } else {
                black_box(
                    $before::OggIndex::read(
                        black_box($bytes.as_slice()),
                        $before::IndexOptions::default(),
                    )
                    .unwrap(),
                );
            }
        });
    };
}

fn main() {
    for target in [0, 8192] {
        for size in [0, 1, 17] {
            let mut writer = summary_baseline::PacketWriter::with_options(
                Vec::new(),
                42,
                summary_baseline::WriteOptions {
                    target_page_body: target,
                },
            );
            writer.write_packet(b"header", 0, false).unwrap();
            let packet = vec![37; size];
            for number in 0..4096 {
                writer
                    .write_packet(&packet, number, number == 4095)
                    .unwrap();
            }
            let bytes = writer.into_inner();
            index_pair!(
                &format!("metadata/{target}/{size}"),
                summary_baseline,
                summary_metadata,
                bytes
            );
        }
    }
    for segments in [1, 2, 3, 4, 5, 8, 17, 255] {
        for terminated in [false, true] {
            let mut lacing = vec![255; segments];
            if terminated {
                lacing[segments - 1] = 17;
            }
            let body = vec![37; lacing.iter().map(|&n| n as usize).sum()];
            let mut frame = Vec::new();
            append(
                &mut frame,
                CONTINUED,
                if terminated {
                    0
                } else {
                    UNKNOWN_GRANULE_POSITION
                },
                0,
                &lacing,
                &body,
            );
            let before = summary_metadata::Page::parse(&frame).unwrap().0;
            let after = summary_short::Page::parse(&frame).unwrap().0;
            cpu::compare(
                &format!("fragment/{segments}/{terminated}"),
                131_072,
                |new| {
                    if new {
                        for fragment in black_box(after).fragments() {
                            black_box(fragment);
                        }
                    } else {
                        for fragment in black_box(before).fragments() {
                            black_box(fragment);
                        }
                    }
                },
            );
        }
    }
    for span in [1, 4, 64] {
        let bytes = spanning(span);
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let (page, len) = summary_short::Page::parse(&bytes[offset..]).unwrap();
            before.push((page, offset as u64));
            after.push((
                summary_bounded::Page::parse(&bytes[offset..]).unwrap().0,
                offset as u64,
            ));
            offset += len;
        }
        cpu::compare(&format!("preflight/{span}"), 8, |new| {
            if new {
                let mut demux = summary_bounded::Demuxer::default();
                for &(page, offset) in black_box(&after) {
                    demux
                        .push_page(page, offset, |p| {
                            black_box(p);
                        })
                        .unwrap();
                }
                demux.finish().unwrap();
            } else {
                let mut demux = summary_short::Demuxer::default();
                for &(page, offset) in black_box(&before) {
                    demux
                        .push_page(page, offset, |p| {
                            black_box(p);
                        })
                        .unwrap();
                }
                demux.finish().unwrap();
            }
        });
    }
}
