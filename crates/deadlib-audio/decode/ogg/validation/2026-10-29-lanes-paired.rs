extern crate lanes_before;
extern crate lanes_counts;
extern crate lanes_short;
extern crate lanes_wide192;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
use std::hint::black_box;
fn frame(lacing: &[u8]) -> Vec<u8> {
    let body = vec![37; lacing.iter().map(|&n| n as usize).sum()];
    lanes_before::encode_page(
        lanes_before::PageHeader {
            flags: lanes_before::CONTINUED,
            granule_position: 0,
            serial: 42,
            sequence: 1,
        },
        lacing,
        &body,
    )
    .unwrap()
}
macro_rules! pair {
    ($label:expr,$a:ident,$b:ident,$bytes:expr,$method:ident) => {{
        let a = $a::Page::parse(&$bytes).unwrap().0;
        let b = $b::Page::parse(&$bytes).unwrap().0;
        cpu::compare($label, 65536, |new| {
            if new {
                black_box(black_box(b).$method());
            } else {
                black_box(black_box(a).$method());
            }
        });
    }};
}
fn main() {
    for length in [0, 1, 4, 7, 8, 9, 15, 16, 17, 31, 32, 127, 128, 254, 255] {
        for pattern in [0, 1, 2] {
            let lacing: Vec<_> = (0..length)
                .map(|i| match pattern {
                    0 => 255,
                    1 => 1,
                    _ => (i * 73 + 127) as u8,
                })
                .collect();
            let bytes = frame(&lacing);
            pair!(
                &format!("count/{length}/{pattern}"),
                lanes_before,
                lanes_counts,
                bytes,
                completed_packets
            );
        }
    }
    macro_rules! fragments {
        ($label:expr,$a:ident,$b:ident,$bytes:expr) => {{
            let a = $a::Page::parse(&$bytes).unwrap().0;
            let b = $b::Page::parse(&$bytes).unwrap().0;
            cpu::compare($label, 65536, |new| {
                let mut sum = 0;
                if new {
                    for f in black_box(b).fragments() {
                        black_box(f);
                        sum += f.data.len() + f.starts_packet as usize + f.ends_packet as usize;
                    }
                } else {
                    for f in black_box(a).fragments() {
                        black_box(f);
                        sum += f.data.len() + f.starts_packet as usize + f.ends_packet as usize;
                    }
                }
                black_box(sum);
            });
        }};
    }
    for length in [
        1, 4, 5, 7, 8, 9, 12, 16, 17, 31, 32, 64, 127, 128, 191, 192, 193, 200, 223, 224, 239, 240,
        254, 255,
    ] {
        for ended in [false, true] {
            let mut lacing = vec![255; length];
            if ended {
                lacing[length - 1] = 17;
            }
            let bytes = frame(&lacing);
            fragments!(
                &format!("short/{length}/{ended}"),
                lanes_counts,
                lanes_short,
                bytes
            );
            fragments!(
                &format!("wide192/{length}/{ended}"),
                lanes_short,
                lanes_wide192,
                bytes
            );
        }
    }
    for length in [4, 16, 36, 64, 4096, 65025] {
        let bytes = vec![37; length];
        cpu::compare(
            &format!("crc/{length}"),
            if length >= 4096 { 512 } else { 65536 },
            |new| {
                black_box(if new {
                    lanes_wide192::crc32(black_box(&bytes))
                } else {
                    lanes_before::crc32(black_box(&bytes))
                });
            },
        );
    }
}
