//! SSE2 weighted prediction for packed 8-bit motion-compensation blocks.

use super::{bi_pred_avg, load8, scalar, store16, store8, widen8, widen8_hi};
use core::arch::x86_64::*;

pub fn weighted_uni(pred: &mut [u8], log2_denom: u32, weight: i32, offset: i32) {
    if log2_denom <= 7 && weight == 1 << log2_denom && offset == 0 {
        return;
    }
    #[cfg(test)]
    if super::level() == super::SimdLevel::Scalar {
        return scalar::weighted_uni(pred, log2_denom, weight, offset);
    }
    if log2_denom > 7 || !(-128..=128).contains(&weight) || !(-128..=127).contains(&offset) {
        return scalar::weighted_uni(pred, log2_denom, weight, offset);
    }

    let mut i = 0;
    // SAFETY: x86-64 guarantees SSE2, and each load/store is bounded by the
    // remaining length. Rounding stays in i16 before the shift. Saturation
    // after adding the offset can only affect values already outside u8.
    unsafe {
        let weights = _mm_set1_epi16(weight as i16);
        let offsets = _mm_set1_epi16(offset as i16);
        let shift = _mm_cvtsi32_si128(log2_denom as i32);
        let round = _mm_set1_epi16((1 << log2_denom) >> 1);
        let apply = |s| {
            let scaled = _mm_add_epi16(_mm_mullo_epi16(s, weights), round);
            _mm_adds_epi16(_mm_sra_epi16(scaled, shift), offsets)
        };
        while i + 16 <= pred.len() {
            let p = _mm_loadu_si128(pred.as_ptr().add(i).cast());
            let pixels = _mm_packus_epi16(apply(widen8(p)), apply(widen8_hi(p)));
            store16(pred, i, pixels);
            i += 16;
        }
        if i + 8 <= pred.len() {
            let p = apply(widen8(load8(pred, i)));
            store8(pred, i, _mm_packus_epi16(p, p));
            i += 8;
        }
    }
    scalar::weighted_uni(&mut pred[i..], log2_denom, weight, offset);
}

pub fn weighted_bi_implicit(a: &[u8], b: &[u8], out: &mut [u8], w0: i32, w1: i32) {
    if w0 == 32 && w1 == 32 {
        return bi_pred_avg(a, b, out);
    }
    #[cfg(test)]
    if super::level() == super::SimdLevel::Scalar {
        return scalar::weighted_bi_implicit(a, b, out, w0, w1);
    }
    if !(-64..=128).contains(&w0) || w1 != 64 - w0 {
        return scalar::weighted_bi_implicit(a, b, out, w0, w1);
    }

    let n = a.len().min(b.len()).min(out.len());
    let mut i = 0;
    // SAFETY: each access lies within n samples. The weight sum is 64, so
    // the combined signed accumulator with rounding is in -16320..=32672.
    unsafe {
        let weight0 = _mm_set1_epi16(w0 as i16);
        let weight1 = _mm_set1_epi16(w1 as i16);
        let round = _mm_set1_epi16(32);
        let apply = |a, b| {
            let sum = _mm_add_epi16(_mm_mullo_epi16(a, weight0), _mm_mullo_epi16(b, weight1));
            _mm_srai_epi16(_mm_add_epi16(sum, round), 6)
        };
        while i + 16 <= n {
            let x = _mm_loadu_si128(a.as_ptr().add(i).cast());
            let y = _mm_loadu_si128(b.as_ptr().add(i).cast());
            let pixels = _mm_packus_epi16(
                apply(widen8(x), widen8(y)),
                apply(widen8_hi(x), widen8_hi(y)),
            );
            store16(out, i, pixels);
            i += 16;
        }
        if i + 8 <= n {
            let pixels = apply(widen8(load8(a, i)), widen8(load8(b, i)));
            store8(out, i, _mm_packus_epi16(pixels, pixels));
            i += 8;
        }
    }
    scalar::weighted_bi_implicit(&a[i..], &b[i..], &mut out[i..], w0, w1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_weights_match_scalar_including_saturation_and_tails() {
        let first = [0, 255, 1, 254, 23, 130, 232, 11].repeat(7);
        let second = [255, 0, 254, 1, 130, 23, 11, 232].repeat(7);
        for len in [0, 4, 8, 15, 16, 17, 24, 55] {
            for w0 in -64..=128 {
                let mut expected = [42; 56];
                let mut actual = expected;
                let w1 = 64 - w0;
                scalar::weighted_bi_implicit(&first[..len], &second, &mut expected, w0, w1);
                weighted_bi_implicit(&first[..len], &second, &mut actual, w0, w1);
                assert_eq!(actual, expected, "len={len}, w0={w0}");
            }
        }
    }

    #[test]
    fn explicit_uni_weights_match_scalar_for_all_denominators() {
        let samples = [0, 255, 1, 254, 127, 128, 31, 213].repeat(7);
        for len in [4, 8, 15, 16, 17, 24, 55] {
            for denom in 0..=7 {
                for weight in [-128, -127, -1, 0, 1, 1 << denom, 127, 128] {
                    for offset in [-128, -7, 0, 3, 127] {
                        let mut expected = samples.clone();
                        let mut actual = samples.clone();
                        scalar::weighted_uni(&mut expected[..len], denom, weight, offset);
                        weighted_uni(&mut actual[..len], denom, weight, offset);
                        assert_eq!(
                            actual, expected,
                            "len={len}, denom={denom}, weight={weight}, offset={offset}"
                        );
                    }
                }
            }
        }
    }
}
