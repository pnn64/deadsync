extern crate cl_before;
extern crate cl_four;
extern crate cl_public;
extern crate cl_seeded;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::{
    hint::black_box,
    io::{self, Write},
};
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
fn frame(size: usize) -> Vec<u8> {
    let mut lacing = vec![255; size / 255];
    if lacing.len() < 255 {
        lacing.push((size % 255) as u8);
    }
    cl_before::encode_page(
        cl_before::PageHeader {
            flags: 0,
            granule_position: 0,
            serial: 42,
            sequence: 0,
        },
        &lacing,
        &vec![37; size],
    )
    .unwrap()
}
macro_rules! raw {
    ($label:expr,$a:ident,$b:ident,$bytes:expr,$n:expr) => {{
        let old: fn(&[u8]) -> u32 = black_box($a::crc32);
        let new: fn(&[u8]) -> u32 = black_box($b::crc32);
        assert_eq!(old($bytes), new($bytes));
        cpu::compare($label, $n, |after| {
            black_box(if after {
                new(black_box($bytes))
            } else {
                old(black_box($bytes))
            });
        });
    }};
}
macro_rules! parse {
    ($label:expr,$a:ident,$b:ident,$bytes:expr,$n:expr) => {{
        cpu::compare($label, $n, |after| {
            if after {
                black_box($b::Page::parse(black_box(&$bytes)).unwrap());
            } else {
                black_box($a::Page::parse(black_box(&$bytes)).unwrap());
            }
        });
    }};
}
macro_rules! read {
    ($label:expr,$a:ident,$b:ident,$bytes:expr,$n:expr) => {{
        cpu::compare($label, $n, |after| {
            if after {
                let mut r = $b::PageReader::new(black_box($bytes.as_slice()));
                let mut count = 0;
                while let Some(p) = r.next_page().unwrap() {
                    black_box(p);
                    count += 1;
                }
                assert_eq!(count, 1);
            } else {
                let mut r = $a::PageReader::new(black_box($bytes.as_slice()));
                let mut count = 0;
                while let Some(p) = r.next_page().unwrap() {
                    black_box(p);
                    count += 1;
                }
                assert_eq!(count, 1);
            }
        });
    }};
}
macro_rules! write {
    ($label:expr,$a:ident,$b:ident,$payload:expr,$n:expr) => {{
        cpu::compare($label, $n, |after| {
            if after {
                let mut w = $b::PacketWriter::new(Sink::default(), 42);
                w.write_packet(b"header", 0, false).unwrap();
                for i in 0..8 {
                    w.write_packet(black_box(&$payload), i, i == 7).unwrap();
                }
                black_box(w.into_inner().0);
            } else {
                let mut w = $a::PacketWriter::new(Sink::default(), 42);
                w.write_packet(b"header", 0, false).unwrap();
                for i in 0..8 {
                    w.write_packet(black_box(&$payload), i, i == 7).unwrap();
                }
                black_box(w.into_inner().0);
            }
        });
    }};
}
fn main() {
    let data: Vec<_> = (0usize..1048608)
        .map(|i| (i.wrapping_mul(37) ^ (i >> 3) ^ (i >> 9)) as u8)
        .collect();
    for size in [
        0, 1, 4, 8, 12, 16, 36, 64, 127, 128, 129, 255, 256, 512, 1024, 4096, 65025, 1048576,
    ] {
        let bytes = &data[3..3 + size];
        let n = if size >= 65025 {
            16
        } else if size >= 4096 {
            512
        } else {
            10000
        };
        raw!(&format!("public/{size}"), cl_before, cl_public, bytes, n);
        raw!(&format!("four/{size}"), cl_seeded, cl_four, bytes, n);
    }
    for size in [16, 128, 4096, 65025] {
        raw!(
            &format!("seeded-raw/{size}"),
            cl_public,
            cl_seeded,
            &data[3..3 + size],
            if size >= 4096 { 64 } else { 10000 }
        );
    }
    for size in [0, 17, 100, 128, 255, 256, 512, 4096, 65025] {
        let bytes = frame(size);
        let n = if size >= 4096 { 64 } else { 1000 };
        parse!(
            &format!("seeded-parse/{size}"),
            cl_public,
            cl_seeded,
            bytes,
            n
        );
        parse!(&format!("four-parse/{size}"), cl_seeded, cl_four, bytes, n);
        if [17, 4096, 65025].contains(&size) {
            parse!(
                &format!("public-parse/{size}"),
                cl_before,
                cl_public,
                bytes,
                n
            );
        }
    }
    for size in [17, 4096, 65025] {
        let bytes = frame(size);
        let n = if size >= 4096 { 32 } else { 500 };
        read!(
            &format!("seeded-reader/{size}"),
            cl_public,
            cl_seeded,
            bytes,
            n
        );
        read!(&format!("four-reader/{size}"), cl_seeded, cl_four, bytes, n);
    }
    for size in [17, 255, 4096, 8192, 65025, 130050] {
        let payload = vec![37; size];
        let n = if size >= 65025 {
            2
        } else if size >= 4096 {
            8
        } else {
            500
        };
        write!(
            &format!("seeded-writer/{size}"),
            cl_public,
            cl_seeded,
            payload,
            n
        );
        write!(
            &format!("four-writer/{size}"),
            cl_seeded,
            cl_four,
            payload,
            n
        );
    }
}
