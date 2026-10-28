//! Standalone exact-byte/output/error comparison with the original LZW codec.
//! Copy this file and git snapshots named original-lzw.rs / current-lzw.rs
//! into a scratch directory, then rustc --edition=2024 -O lzw-parity.rs.
#![allow(dead_code)]

#[derive(Debug, PartialEq)]
pub enum Error {
    Invalid(&'static str),
    Allocation,
}
fn reserve<T>(vec: &mut Vec<T>, extra: usize) -> Result<(), Error> {
    vec.try_reserve(extra).map_err(|_| Error::Allocation)
}
#[path = "current-lzw.rs"]
mod current;
#[path = "original-lzw.rs"]
mod original;

fn main() {
    let mut cases = 0;
    for minimum in 2..=8 {
        let mask = ((1u16 << minimum) - 1) as u8;
        for length in [0, 1, 2, 3, 7, 31, 255, 256, 4095, 4096, 16384, 262144] {
            for pattern in 0..4 {
                let mut random = 1327u32;
                let input: Vec<_> = (0..length)
                    .map(|i| {
                        random ^= random << 13;
                        random ^= random >> 17;
                        random ^= random << 5;
                        match pattern {
                            0 => 0,
                            1 => mask,
                            2 => i as u8 & mask,
                            _ => random as u8 & mask,
                        }
                    })
                    .collect();
                let before = original::encode(&input, minimum).unwrap();
                let after = current::encode(&input, minimum).unwrap();
                assert_eq!(
                    before, after,
                    "minimum={minimum} length={length} pattern={pattern}"
                );
                assert_eq!(current::decode(&after, minimum, length).unwrap(), input);
                assert_eq!(original::decode(&before, minimum, length).unwrap(), input);
                cases += 1;
            }
        }
    }
    println!("{cases} exact encoded-byte comparisons and decoder roundtrips passed");
    let mut streams = 0;
    for minimum in 2..=8 {
        for word in 0u32..=65535 {
            let input = (word as u16).to_le_bytes();
            for limit in [0, 1, 8] {
                assert_eq!(
                    original::decode(&input, minimum, limit),
                    current::decode(&input, minimum, limit),
                    "minimum={minimum} input={input:?} limit={limit}",
                );
                streams += 1;
            }
            if word % 17 == 0 {
                for chunk in [1, 2] {
                    let mut before = original::Decoder::new();
                    let mut after = current::Decoder::new();
                    before.reset(minimum).unwrap();
                    after.reset(minimum).unwrap();
                    // Public update may append to an existing output prefix.
                    let mut a = vec![1, 2, 3];
                    let mut b = a.clone();
                    for part in input.chunks(chunk) {
                        let old_result = before.update(part, &mut a, 16);
                        let new_result = after.update(part, &mut b, 16);
                        assert_eq!(old_result, new_result);
                        assert_eq!(a, b);
                        assert_eq!(before.ended(), after.ended());
                        if old_result.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
    println!(
        "{streams} exhaustive two-byte output/error comparisons passed, plus fragmented-prefix checks"
    );
}
