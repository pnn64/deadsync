extern crate cl_before;
extern crate cl_four;
extern crate cl_public;
extern crate cl_seeded;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::hint::black_box;
macro_rules! consume {
    ($name:ident,$api:ident) => {
        fn $name(bytes: &[u8]) -> (u64, u64) {
            let mut reader = $api::PageReader::new(bytes);
            let mut demux = $api::Demuxer::default();
            let (mut packets, mut size) = (0, 0);
            while let Some(p) = reader.next_page().unwrap() {
                demux
                    .push_page(p.page, p.offset, |packet| {
                        packets += 1;
                        size += packet.data.len() as u64;
                        black_box(packet);
                    })
                    .unwrap();
            }
            demux.finish().unwrap();
            (packets, size)
        }
    };
}
consume!(before, cl_before);
consume!(public, cl_public);
consume!(seeded, cl_seeded);
consume!(four, cl_four);
fn main() {
    for (index, path) in std::env::args().skip(1).enumerate() {
        let bytes = std::fs::read(path).unwrap();
        let expected = before(&bytes);
        assert_eq!(public(&bytes), expected);
        assert_eq!(seeded(&bytes), expected);
        assert_eq!(four(&bytes), expected);
        println!(
            "file/{index}: packets {}; payload bytes {}",
            expected.0, expected.1
        );
        for (label, old, new) in [
            (
                "public",
                before as fn(&[u8]) -> (u64, u64),
                public as fn(&[u8]) -> (u64, u64),
            ),
            ("seeded", public, seeded),
            ("four", seeded, four),
        ] {
            let old = black_box(old);
            let new = black_box(new);
            cpu::compare(&format!("{label}/{index}"), 2, |after| {
                black_box(if after {
                    new(black_box(&bytes))
                } else {
                    old(black_box(&bytes))
                });
            });
        }
    }
}
