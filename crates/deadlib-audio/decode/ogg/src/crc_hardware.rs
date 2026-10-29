#![deny(unsafe_op_in_unsafe_fn)]
//! Direct Ogg polynomial folds using PCLMUL, with a table reduction of the last word.
//!
//! A state D denotes a raw 128-bit polynomial. Multiplying its low/high halves
//! by x^128/x^192 modulo P gives D*x^128 modulo P; XOR appends the next word.
//! The final scalar CRC computes D*x^32 modulo P. A seed is merged at bit 96.

use std::arch::x86_64::*;
/// The only safe entry to the feature-specific implementation.
#[inline]
pub(crate) fn update(seed: u32, bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 128 {
        return None;
    }
    if is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("ssse3") {
        // SAFETY: both features used by fold are present. Its array-based loads
        // accept every slice alignment and never access the trailing partial word.
        return Some(unsafe { fold(seed, bytes) });
    }
    None
}

const fn power(bits: usize) -> u32 {
    let mut v = 1u32;
    let mut i = 0;
    while i < bits {
        v = (v << 1) ^ if v & 0x80000000 != 0 { 0x04c11db7 } else { 0 };
        i += 1;
    }
    v
}
const K128: u32 = power(128);
const K192: u32 = power(192);
const K512: u32 = power(512);
const K576: u32 = power(576);
#[target_feature(enable = "pclmulqdq,ssse3")]
unsafe fn fold(seed: u32, bytes: &[u8]) -> u32 {
    let (groups, tail) = bytes.as_chunks::<64>();
    let Some((first, groups)) = groups.split_first() else {
        return crate::crc::update_scalar(seed, bytes);
    };
    // SAFETY: feature support is checked by the caller. Each source is a full
    // 64-byte array, every indexed load is exactly sixteen valid bytes, and the
    // destination is a fresh sixteen-byte array. All memory operations are unaligned.
    unsafe {
        let reverse = _mm_setr_epi8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
        let k = _mm_set_epi64x(K576 as i64, K512 as i64);
        let k1 = _mm_set_epi64x(K192 as i64, K128 as i64);
        macro_rules! load {
            ($block:expr,$offset:expr) => {
                _mm_shuffle_epi8(
                    _mm_loadu_si128($block[$offset..$offset + 16].as_ptr().cast()),
                    reverse,
                )
            };
        }
        macro_rules! fold {
            ($value:expr,$factor:expr) => {
                _mm_xor_si128(
                    _mm_clmulepi64_si128::<0x00>($value, $factor),
                    _mm_clmulepi64_si128::<0x11>($value, $factor),
                )
            };
        }
        let mut a = _mm_xor_si128(load!(first, 0), _mm_set_epi32(seed as i32, 0, 0, 0));
        let mut b = load!(first, 16);
        let mut c = load!(first, 32);
        let mut d = load!(first, 48);
        for group in groups {
            a = _mm_xor_si128(fold!(a, k), load!(group, 0));
            b = _mm_xor_si128(fold!(b, k), load!(group, 16));
            c = _mm_xor_si128(fold!(c, k), load!(group, 32));
            d = _mm_xor_si128(fold!(d, k), load!(group, 48));
        }
        let mut state = _mm_xor_si128(fold!(a, k1), b);
        state = _mm_xor_si128(fold!(state, k1), c);
        state = _mm_xor_si128(fold!(state, k1), d);
        let (words, tail) = tail.as_chunks::<16>();
        for word in words {
            state = _mm_xor_si128(
                fold!(state, k1),
                _mm_shuffle_epi8(_mm_loadu_si128(word.as_ptr().cast()), reverse),
            );
        }
        let mut folded = [0u8; 16];
        _mm_storeu_si128(folded.as_mut_ptr().cast(), _mm_shuffle_epi8(state, reverse));
        crate::crc::update_scalar(crate::crc::update_scalar(0, &folded), tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bitwise(mut crc: u32, bytes: &[u8]) -> u32 {
        for &byte in bytes {
            crc ^= (byte as u32) << 24;
            for _ in 0..8 {
                crc = (crc << 1) ^ if crc & 0x80000000 != 0 { 0x04c11db7 } else { 0 };
            }
        }
        crc
    }
    #[cfg(miri)]
    #[test]
    fn hardware_memory_bounds_and_provenance() {
        assert!(is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("ssse3"));
        for length in [
            0, 1, 15, 16, 17, 31, 32, 63, 64, 65, 127, 128, 129, 191, 192, 255, 256, 257, 511, 512,
            513,
        ] {
            for offset in 0..16 {
                // The borrowed input ends at the allocation boundary.
                let data: Vec<_> = (0..offset + length).map(|i| (i * 73 + 127) as u8).collect();
                let input = &data[offset..];
                for seed in [0, 0xfedcba98] {
                    // SAFETY: both required target features were asserted above.
                    assert_eq!(unsafe { fold(seed, input) }, bitwise(seed, input));
                }
            }
        }
    }
    #[test]
    fn hardware_and_scalar_match_independent_crc_across_seeds_alignments_and_tails() {
        if !(is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("ssse3")) {
            eprintln!("CLMUL unavailable; scalar fallback retained");
            return;
        }
        let data: Vec<_> = (0usize..1048608)
            .map(|i| (i.wrapping_mul(37) ^ (i >> 3) ^ (i >> 9)) as u8)
            .collect();
        let mut cases = 0;
        for seed in [0, u32::MAX, 0x01234567, 0xfedcba98] {
            for offset in 0..16 {
                for length in 0..=1024 {
                    let input = &data[offset..offset + length];
                    let expected = bitwise(seed, input);
                    assert_eq!(crate::crc::update_scalar(seed, input), expected);
                    // SAFETY: both required features were checked above; all alignments and lengths are supported.
                    assert_eq!(
                        unsafe { fold(seed, input) },
                        expected,
                        "seed={seed} offset={offset} length={length}"
                    );
                    cases += 1;
                }
            }
            for length in [4096, 65025, 65307, 1048576] {
                let input = &data[3..3 + length];
                let expected = bitwise(seed, input);
                assert_eq!(crate::crc::update_scalar(seed, input), expected);
                // SAFETY: both required features were checked above.
                assert_eq!(unsafe { fold(seed, input) }, expected);
                cases += 1;
                for split in [1, 15, 16, 17, 63, 64, 65, 127, 128, 129, 255, 512, 4096] {
                    let mut crc = seed;
                    for chunk in input.chunks(split) {
                        // SAFETY: both required features were checked above; each chunk is independently bounded.
                        crc = unsafe { fold(crc, chunk) };
                    }
                    assert_eq!(crc, expected);
                    cases += 1;
                }
            }
        }
        eprintln!(
            "CLMUL hardware and scalar matched {cases} independent seeded/alignment/tail/split cases"
        );
    }
}
