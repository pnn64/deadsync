//! Compare exact streaming checksum implementations, including byte fragments.
use std::hint::black_box;
#[allow(dead_code)]
#[path = "../experiments/bitwords-20261028/baseline-checksum.rs"]
mod baseline;
#[path = "support/cpu.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../src/checksum.rs"]
mod selected;

fn main() {
    for len in [64, 8192] {
        let bytes: Vec<u8> = (0..len)
            .map(|n| (n * 173 + n * n * 47 + 71) as u8)
            .collect();
        let (mut a, mut b) = (1u64, 0u64);
        for &v in &bytes {
            a = (a + u64::from(v)) % 65521;
            b = (b + a) % 65521;
        }
        let expected = ((b << 16) | a) as u32;
        for fragment in [1, 7, 63, 64, 65, 5552] {
            let run = |new| {
                if new {
                    let mut checksum = selected::Adler32::new();
                    for part in black_box(bytes.as_slice()).chunks(fragment) {
                        checksum.update(part);
                    }
                    checksum.finish()
                } else {
                    let mut checksum = baseline::Adler32::new();
                    for part in black_box(bytes.as_slice()).chunks(fragment) {
                        checksum.update(part);
                    }
                    checksum.finish()
                }
            };
            assert_eq!(run(false), expected);
            assert_eq!(run(true), expected);
            cpu::compare(
                &format!("bytes{len}-fragment{fragment}"),
                if len < 4096 { 1024 } else { 32 },
                |new| {
                    black_box(run(new));
                },
            );
        }
    }
}
