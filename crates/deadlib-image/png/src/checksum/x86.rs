use std::arch::x86_64::*;

// Reflect the remainder of x^power modulo the forward IEEE polynomial.
const fn factor(power: usize) -> i64 {
    let mut remainder = 1u64;
    let mut n = 0;
    while n < power {
        remainder <<= 1;
        if remainder & (1 << 32) != 0 {
            remainder ^= 0x104c11db7;
        }
        n += 1;
    }
    (remainder.reverse_bits() >> 31) as i64
}
const K512: (i64, i64) = (factor(544), factor(480));
const K128: (i64, i64) = (factor(160), factor(96));

#[inline]
pub(super) fn update(state: u32, bytes: &[u8]) -> Option<u32> {
    if bytes.len() >= 128
        && std::is_x86_feature_detected!("pclmulqdq")
        && std::is_x86_feature_detected!("sse2")
    {
        // SAFETY: PCLMULQDQ is checked above; SSE2 is baseline on x86-64.
        // All byte reads are through checked, fixed-size arrays.
        Some(unsafe { fold(state, bytes) })
    } else {
        None
    }
}

#[target_feature(enable = "pclmulqdq,sse2")]
unsafe fn fold(state: u32, bytes: &[u8]) -> u32 {
    // Reflected IEEE polynomial folding factors for 512 and 128 input bits.
    // The final 128-bit residue is reduced by the scalar implementation,
    // avoiding a separate Barrett reduction and partial-vector loads.
    let k512 = _mm_set_epi64x(K512.1, K512.0);
    let k128 = _mm_set_epi64x(K128.1, K128.0);
    let (first, rest) = bytes.split_first_chunk::<64>().unwrap();
    let first = first.as_chunks::<16>().0;
    let mut a = _mm_xor_si128(load(&first[0]), _mm_cvtsi32_si128(state as i32));
    let mut b = load(&first[1]);
    let mut c = load(&first[2]);
    let mut d = load(&first[3]);
    let (blocks, rest) = rest.as_chunks::<64>();
    for block in blocks {
        let block = block.as_chunks::<16>().0;
        a = step(a, load(&block[0]), k512);
        b = step(b, load(&block[1]), k512);
        c = step(c, load(&block[2]), k512);
        d = step(d, load(&block[3]), k512);
    }
    a = step(a, b, k128);
    a = step(a, c, k128);
    a = step(a, d, k128);
    let (blocks, tail) = rest.as_chunks::<16>();
    for block in blocks {
        a = step(a, load(block), k128);
    }
    let mut residue = [0; 16];
    residue[..8].copy_from_slice(&_mm_cvtsi128_si64(a).to_le_bytes());
    residue[8..].copy_from_slice(&_mm_cvtsi128_si64(_mm_srli_si128::<8>(a)).to_le_bytes());
    super::update_scalar(super::update_scalar(0, &residue), tail)
}

#[inline]
#[target_feature(enable = "sse2")]
fn load(bytes: &[u8; 16]) -> __m128i {
    // Byte lanes compile to one unaligned load; 64-bit halves added a
    // redundant reload and unpack of the upper half.
    let b = bytes.map(|byte| byte as i8);
    _mm_setr_epi8(
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13],
        b[14], b[15],
    )
}

#[target_feature(enable = "pclmulqdq,sse2")]
#[inline]
fn step(old: __m128i, next: __m128i, factors: __m128i) -> __m128i {
    _mm_xor_si128(
        next,
        _mm_xor_si128(
            _mm_clmulepi64_si128::<0x00>(old, factors),
            _mm_clmulepi64_si128::<0x11>(old, factors),
        ),
    )
}

#[cfg(test)]
#[test]
fn polynomial_folding_matches_scalar_for_offsets_lengths_and_initial_states() {
    if !std::is_x86_feature_detected!("pclmulqdq") {
        return;
    }
    let bytes: Vec<u8> = (0..4608)
        .map(|n| ((n * 137 + n * n * 17) ^ (n >> 3)) as u8)
        .collect();
    for offset in 0..32 {
        for length in (128..400).chain([511, 512, 513, 1023, 1024, 1025, 4095, 4096, 4097]) {
            let input = &bytes[offset..offset + length];
            for state in [0, !0, 0x12345678]
                .into_iter()
                .chain((0..32).map(|bit| 1u32 << bit))
            {
                assert_eq!(
                    update(state, input),
                    Some(super::update_scalar(state, input)),
                    "offset={offset} length={length} state={state:08x}"
                );
            }
        }
    }
}

#[inline]
pub(super) fn adler_update(state: &mut super::Adler32, input: &[u8]) -> bool {
    if input.len() >= 128 && std::is_x86_feature_detected!("ssse3") {
        // SAFETY: SSSE3 support is checked, and every load uses a checked array.
        unsafe {
            adler(state, input);
        }
        true
    } else {
        false
    }
}

#[target_feature(enable = "ssse3")]
unsafe fn adler(state: &mut super::Adler32, input: &[u8]) {
    // Each 32-byte group adds its byte sum, from SAD against zero, and its
    // bytes weighted 32..1. A pair of weighted bytes is at most
    // (32 + 31) * 255 and the two halves' pairs together at most 23970, so
    // the i16 multiply-adds cannot saturate. In a 16 KiB block of G = 512
    // groups, the prefix total is at most 32 * 255 * G * (G - 1) / 2 < 2^31
    // and every other lane total is smaller, so u32 lanes cannot overflow;
    // the block's contribution to `b` is combined in u64.
    let zero = _mm_setzero_si128();
    let ones = _mm_set1_epi16(1);
    let high = _mm_setr_epi8(
        32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17,
    );
    let low = _mm_setr_epi8(16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1);
    for block in input.chunks(16384) {
        let (groups, tail) = block.as_chunks::<32>();
        let mut sums = zero;
        let mut prefixes = zero;
        let mut weighted = zero;
        for bytes in groups {
            let (first, second) = bytes.split_at(16);
            let first = load(first.try_into().unwrap());
            let second = load(second.try_into().unwrap());
            prefixes = _mm_add_epi32(prefixes, sums);
            let sad = _mm_add_epi32(_mm_sad_epu8(first, zero), _mm_sad_epu8(second, zero));
            sums = _mm_add_epi32(sums, sad);
            let pairs = _mm_add_epi16(
                _mm_maddubs_epi16(first, high),
                _mm_maddubs_epi16(second, low),
            );
            weighted = _mm_add_epi32(weighted, _mm_madd_epi16(pairs, ones));
        }
        // Each prior group adds another 32 to every byte's weight.
        let b = u64::from(state.b)
            + u64::from(state.a) * (groups.len() * 32) as u64
            + 32 * u64::from(total(prefixes))
            + u64::from(total(weighted));
        state.b = (b % 65521) as u32;
        state.a += total(sums);
        for &byte in tail {
            state.a += u32::from(byte);
            state.b += state.a;
        }
        state.a %= 65521;
        state.b %= 65521;
    }
}
#[inline]
#[target_feature(enable = "sse2")]
fn total(v: __m128i) -> u32 {
    let v = _mm_add_epi32(v, _mm_srli_si128::<8>(v));
    let v = _mm_add_epi32(v, _mm_srli_si128::<4>(v));
    _mm_cvtsi128_si32(v) as u32
}

#[cfg(test)]
#[test]
fn vector_adler_matches_byte_oracle_for_states_splits_and_modulus_boundaries() {
    if !std::is_x86_feature_detected!("ssse3") {
        return;
    }
    for constant in [false, true] {
        let bytes: Vec<u8> = (0..65600)
            .map(|n| {
                if constant {
                    255
                } else {
                    ((n * 137 + n * n * 17) ^ (n >> 3)) as u8
                }
            })
            .collect();
        for offset in 0..32 {
            let blocks = [5552, 16383, 16384, 16385, 32768, 65535, 65536, 65537];
            for len in (0..140).chain([255, 256, 257]).chain(blocks) {
                let input = &bytes[offset..offset + len];
                for (a, b) in [(0, 0), (1, 0), (65520, 65520), (65520, 0), (0, 65520)] {
                    let (mut x, mut y) = (a, b);
                    for &byte in input {
                        x = (x + u32::from(byte)) % 65521;
                        y = (y + x) % 65521;
                    }
                    let mut actual = super::Adler32 { a, b };
                    // SAFETY: SSSE3 availability was established above.
                    unsafe {
                        adler(&mut actual, input);
                    }
                    assert_eq!(
                        (actual.a, actual.b),
                        (x, y),
                        "offset={offset} len={len} a={a} b={b} constant={constant}"
                    );
                    let mut split = super::Adler32 { a, b };
                    for bytes in input.chunks(193) {
                        split.update(bytes);
                    }
                    assert_eq!(
                        (split.a, split.b),
                        (x, y),
                        "split offset={offset} len={len} a={a} b={b}"
                    );
                }
            }
        }
    }
}
