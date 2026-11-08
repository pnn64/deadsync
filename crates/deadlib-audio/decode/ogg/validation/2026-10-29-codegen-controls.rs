extern crate frame_before as before;
extern crate frame_writer as after;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::hint::black_box;
#[inline(never)]
fn old_fragment(page: before::Page<'_>) -> usize {
    let mut result = 0;
    for f in page.fragments() {
        black_box(f);
        result += f.data.len() + f.starts_packet as usize + f.ends_packet as usize;
    }
    result
}
#[inline(never)]
fn new_fragment(page: after::Page<'_>) -> usize {
    let mut result = 0;
    for f in page.fragments() {
        black_box(f);
        result += f.data.len() + f.starts_packet as usize + f.ends_packet as usize;
    }
    result
}
fn main() {
    let old: fn(&[u8]) -> u32 = black_box(before::crc32);
    let new: fn(&[u8]) -> u32 = black_box(after::crc32);
    println!("CRC functions merged: {}", std::ptr::fn_addr_eq(old, new));
    for n in [0, 1, 4, 8, 12, 16, 36, 64, 256, 4096, 65025, 1048576] {
        let bytes: Vec<_> = (0..n).map(|i| (i * 37 + i / 255) as u8).collect();
        assert_eq!(old(&bytes), new(&bytes));
        cpu::compare(
            &format!("crc/{n}"),
            if n >= 65025 {
                32
            } else if n >= 4096 {
                512
            } else {
                65536
            },
            |variant| {
                black_box(if variant {
                    new(black_box(&bytes))
                } else {
                    old(black_box(&bytes))
                });
            },
        );
    }
    for n in [1, 4, 5, 8, 17, 64, 255] {
        for terminated in [false, true] {
            let mut lace = vec![255; n];
            if terminated {
                lace[n - 1] = 17;
            }
            let body = vec![37; lace.iter().map(|&v| v as usize).sum()];
            let bytes = before::encode_page(
                before::PageHeader {
                    flags: before::CONTINUED,
                    granule_position: if terminated { 100 } else { u64::MAX },
                    serial: 42,
                    sequence: 1,
                },
                &lace,
                &body,
            )
            .unwrap();
            let a = before::Page::parse(&bytes).unwrap().0;
            let b = after::Page::parse(&bytes).unwrap().0;
            assert_eq!(old_fragment(a), new_fragment(b));
            cpu::compare(&format!("fragment/{n}/{terminated}"), 65536, |variant| {
                black_box(if variant {
                    new_fragment(black_box(b))
                } else {
                    old_fragment(black_box(a))
                });
            });
        }
    }
}
