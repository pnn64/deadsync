extern crate lacing_after;
extern crate lacing_before;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::hint::black_box;
fn main() {
    for segments in [0, 1, 2, 3, 4, 7, 8, 9, 17, 255] {
        let lacing = vec![1; segments];
        let body = vec![37; segments];
        let bytes = lacing_before::encode_page(
            lacing_before::PageHeader {
                flags: 0,
                granule_position: 0,
                serial: 42,
                sequence: 0,
            },
            &lacing,
            &body,
        )
        .unwrap();
        assert_eq!(
            lacing_before::Page::parse(&bytes).unwrap().0.bytes(),
            lacing_after::Page::parse(&bytes).unwrap().0.bytes()
        );
        cpu::compare(&format!("parse-laces/{segments}"), 100_000, |after| {
            if after {
                black_box(lacing_after::Page::parse(black_box(&bytes)).unwrap());
            } else {
                black_box(lacing_before::Page::parse(black_box(&bytes)).unwrap());
            }
        });
    }
}
