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
#[target_feature(enable = "pclmulqdq,ssse3")]
unsafe fn fold(seed: u32, bytes: &[u8]) -> u32 {
    let (words, tail) = bytes.as_chunks::<16>();
    let Some((first, words)) = words.split_first() else {
        return crate::crc::update_scalar(seed, bytes);
    };
    // SAFETY: the caller checks both CPU features. Every load/store is an exact
    // sixteen-byte array; unaligned operations impose no alignment requirement.
    unsafe {
        let reverse = _mm_setr_epi8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
        let k = _mm_set_epi64x(K192 as i64, K128 as i64);
        let mut state = _mm_xor_si128(
            _mm_shuffle_epi8(_mm_loadu_si128(first.as_ptr().cast()), reverse),
            _mm_set_epi32(seed as i32, 0, 0, 0),
        );
        for word in words {
            let lo = _mm_clmulepi64_si128::<0x00>(state, k);
            let hi = _mm_clmulepi64_si128::<0x11>(state, k);
            let input = _mm_shuffle_epi8(_mm_loadu_si128(word.as_ptr().cast()), reverse);
            state = _mm_xor_si128(_mm_xor_si128(lo, hi), input);
        }
        let mut folded = [0u8; 16];
        _mm_storeu_si128(folded.as_mut_ptr().cast(), _mm_shuffle_epi8(state, reverse));
        let crc = crate::crc::update_scalar(0, &folded);
        crate::crc::update_scalar(crc, tail)
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
