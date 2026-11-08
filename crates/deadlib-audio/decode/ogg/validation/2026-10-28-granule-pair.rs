#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;

use std::{hint::black_box, io::Cursor};
type Location = Option<(u64, u32, u8, u64, u32, u32)>;

fn query_before(bytes: &[u8], target: u64) -> (Location, u64) {
    let mut reader = Cursor::new(bytes);
    let result = before::find_granule(&mut reader, 0..bytes.len() as u64, 42, target)
        .unwrap()
        .map(|p| {
            (
                p.offset,
                p.length,
                p.header.flags,
                p.header.granule_position,
                p.header.serial,
                p.header.sequence,
            )
        });
    (result, reader.position())
}

fn query_after(bytes: &[u8], target: u64) -> (Location, u64) {
    let mut reader = Cursor::new(bytes);
    let result = after::find_granule(&mut reader, 0..bytes.len() as u64, 42, target)
        .unwrap()
        .map(|p| {
            (
                p.offset,
                p.length,
                p.header.flags,
                p.header.granule_position,
                p.header.serial,
                p.header.sequence,
            )
        });
    (result, reader.position())
}

fn main() {
    for size in [0, 1, 17] {
        let mut writer = before::PacketWriter::new(Vec::new(), 42);
        writer.write_packet(b"header", 0, false).unwrap();
        let data = vec![37; size];
        for number in 0..4096 {
            writer.write_packet(&data, number, number == 4095).unwrap();
        }
        let bytes = writer.into_inner();
        let mut offset = 0;
        while offset < bytes.len() {
            let (a, n) = before::Page::parse(&bytes[offset..]).unwrap();
            let (b, m) = after::Page::parse(&bytes[offset..]).unwrap();
            assert_eq!(n, m);
            assert_eq!(
                black_box(a.completed_packets()),
                black_box(b.completed_packets())
            );
            offset += n;
        }
        for number in 0..32 {
            assert_eq!(
                query_before(&bytes, number * 131),
                query_after(&bytes, number * 131)
            );
        }
        let mut number = 0;
        cpu::compare(&format!("granule-pair/{size}"), 256, |after| {
            number = (number + 1) & 31;
            let target = black_box(number * 131);
            black_box(if after {
                query_after(black_box(&bytes), target)
            } else {
                query_before(black_box(&bytes), target)
            });
        });
        let (_, a) = heap::measure(|| query_before(&bytes, 1024));
        let (_, b) = heap::measure(|| query_after(&bytes, 1024));
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        println!("granule-pair/{size}: allocation {a:?}");
    }
}
