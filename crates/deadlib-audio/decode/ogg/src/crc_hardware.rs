#![deny(unsafe_op_in_unsafe_fn)]
//! Direct Ogg polynomial folds using PCLMUL.
//!
//! A state D denotes a raw 128-bit polynomial. Multiplying its low/high halves
//! by x^128/x^192 modulo P gives D*x^128 modulo P; XOR appends the next word.
//! Reduction computes D*x^32 modulo P. A seed is merged at bit 96.

use std::arch::x86_64::*;
/// The only safe entry to the feature-specific implementation.
#[inline]
pub(crate) fn update(seed: u32, bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 32 {
        return None;
    }
    if is_x86_feature_detected!("pclmulqdq") && is_x86_feature_detected!("ssse3") {
        // SAFETY: both target features used by fold are present.
        return Some(unsafe { fold(seed, bytes) });
    }
    None
}

const POLYNOMIAL: u64 = 0x1_04c1_1db7;

const fn power(bits: usize) -> u32 {
    let mut v = 1u32;
    let mut i = 0;
    while i < bits {
        v = (v << 1)
            ^ if v & 0x80000000 != 0 {
                POLYNOMIAL as u32
            } else {
                0
            };
        i += 1;
    }
    v
}
const K128: u32 = power(128);
const K192: u32 = power(192);
const K256: u32 = power(256);
const K320: u32 = power(320);
const K384: u32 = power(384);
const K448: u32 = power(448);
const K512: u32 = power(512);
const K576: u32 = power(576);
const K32: i64 = power(32) as i64;
const K64: i64 = power(64) as i64;
const K96: i64 = power(96) as i64;

const RECIPROCAL: i64 = {
    let mut dividend = 1u128 << 64;
    let mut quotient = 0;
    while dividend >= 1 << 32 {
        let shift = dividend.ilog2() - 32;
        dividend ^= (POLYNOMIAL as u128) << shift;
        quotient |= 1i64 << shift;
    }
    quotient
};

#[inline]
#[target_feature(enable = "ssse3")]
fn reversed_word(bytes: &[u8; 16], reverse: __m128i) -> __m128i {
    let bits = u128::from_ne_bytes(*bytes);
    _mm_shuffle_epi8(_mm_set_epi64x((bits >> 64) as i64, bits as i64), reverse)
}

#[target_feature(enable = "pclmulqdq,ssse3")]
fn fold(seed: u32, bytes: &[u8]) -> u32 {
    let (words, tail) = bytes.as_chunks::<16>();
    let (groups, extra) = words.as_chunks::<4>();
    let reverse = _mm_setr_epi8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
    let k = _mm_set_epi64x(K576 as i64, K512 as i64);
    let k1 = _mm_set_epi64x(K192 as i64, K128 as i64);
    macro_rules! fold {
        ($value:expr,$factor:expr) => {
            _mm_xor_si128(
                _mm_clmulepi64_si128::<0x00>($value, $factor),
                _mm_clmulepi64_si128::<0x11>($value, $factor),
            )
        };
    }
    let (mut state, words) =
        if let Some(([first_a, first_b, first_c, first_d], groups)) = groups.split_first() {
            let mut a = _mm_xor_si128(
                reversed_word(first_a, reverse),
                _mm_set_epi32(seed as i32, 0, 0, 0),
            );
            let mut b = reversed_word(first_b, reverse);
            let mut c = reversed_word(first_c, reverse);
            let mut d = reversed_word(first_d, reverse);
            for [word_a, word_b, word_c, word_d] in groups {
                a = _mm_xor_si128(fold!(a, k), reversed_word(word_a, reverse));
                b = _mm_xor_si128(fold!(b, k), reversed_word(word_b, reverse));
                c = _mm_xor_si128(fold!(c, k), reversed_word(word_c, reverse));
                d = _mm_xor_si128(fold!(d, k), reversed_word(word_d, reverse));
            }
            // Advance each lane by the number of words that follow it.
            let a = fold!(a, _mm_set_epi64x(K448 as i64, K384 as i64));
            let b = fold!(b, _mm_set_epi64x(K320 as i64, K256 as i64));
            let state = _mm_xor_si128(_mm_xor_si128(a, b), _mm_xor_si128(fold!(c, k1), d));
            (state, extra)
        } else if let Some((first, rest)) = words.split_first() {
            let state = _mm_xor_si128(
                reversed_word(first, reverse),
                _mm_set_epi32(seed as i32, 0, 0, 0),
            );
            (state, rest)
        } else {
            return crate::crc::update_scalar(seed, tail);
        };
    for word in words {
        state = _mm_xor_si128(fold!(state, k1), reversed_word(word, reverse));
    }
    // Multiply by x^32 and fold the high coefficients into the low 64 bits.
    let folded = fold!(state, _mm_set_epi64x(K96, K32));
    let high = _mm_srli_si128::<8>(folded);
    let value = _mm_xor_si128(
        folded,
        _mm_clmulepi64_si128::<0x00>(high, _mm_cvtsi64_si128(K64)),
    );
    // For a 64-bit polynomial, the quotient is the high 32 bits times
    // floor(x^64 / P), shifted down by 32. XOR the quotient times P into the
    // value and take the low 32 bits for the remainder.
    let quotient = _mm_srli_epi64::<32>(_mm_clmulepi64_si128::<0x00>(
        _mm_srli_epi64::<32>(value),
        _mm_cvtsi64_si128(RECIPROCAL),
    ));
    let remainder = _mm_xor_si128(
        value,
        _mm_clmulepi64_si128::<0x00>(quotient, _mm_cvtsi64_si128(POLYNOMIAL as i64)),
    );
    let crc = _mm_cvtsi128_si32(remainder) as u32;
    if tail.is_empty() {
        crc
    } else {
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
