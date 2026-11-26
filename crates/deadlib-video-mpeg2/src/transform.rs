// Cosine weights for the 8-bit Simple IDCT, scaled by sqrt(2) * 2^14
// with quantized DC and pi/4 weights. Rows are indexed by output sample.
const BASIS: [[i32; 8]; 8] = [
    [16383, 22725, 21407, 19266, 16383, 12873, 8867, 4520],
    [16383, 19266, 8867, -4520, -16383, -22725, -21407, -12873],
    [16383, 12873, -8867, -22725, -16383, 4520, 21407, 19266],
    [16383, 4520, -21407, -12873, 16383, 19266, -8867, -22725],
    [16383, -4520, -21407, 12873, 16383, -19266, -8867, 22725],
    [16383, -12873, -8867, 22725, -16383, -4520, 21407, -19266],
    [16383, -19266, 8867, 4520, -16383, 22725, -21407, 12873],
    [16383, -22725, 21407, -19266, 16383, -12873, 8867, -4520],
];
const ROW_SHIFT: i32 = 11;
const ROW_BIAS: i32 = 1 << (ROW_SHIFT - 1);
const COLUMN_SHIFT: i32 = 20;
const COLUMN_BIAS: i16 = 32;

pub(crate) fn inverse(coefficients: &[i16; 64]) -> [i16; 64] {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    return x86::inverse(coefficients);
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
    scalar(coefficients)
}

#[cfg(any(test, not(all(target_arch = "x86_64", target_feature = "sse2"))))]
fn scalar(coefficients: &[i16; 64]) -> [i16; 64] {
    if coefficients[1..].iter().all(|&coeff| coeff == 0) {
        let dc = i32::from(coefficients[0]) * 8 + i32::from(COLUMN_BIAS);
        return [((dc * BASIS[0][0]) >> COLUMN_SHIFT) as i16; 64];
    }

    let mut horizontal = [[0i16; 8]; 8];
    for (row, transformed) in coefficients.as_chunks::<8>().0.iter().zip(&mut horizontal) {
        if row[1..].iter().all(|&coeff| coeff == 0) {
            *transformed = [row[0] * 8; 8];
            continue;
        }
        for (output, basis) in transformed.iter_mut().zip(BASIS) {
            let product = row
                .iter()
                .zip(basis)
                .map(|(&coeff, weight)| i32::from(coeff) * weight)
                .sum::<i32>();
            let value = (product + ROW_BIAS) >> ROW_SHIFT;
            *output = value.clamp(i16::MIN.into(), i16::MAX.into()) as i16;
        }
    }
    for dc in &mut horizontal[0] {
        *dc = dc.wrapping_add(COLUMN_BIAS);
    }

    let mut samples = [0; 64];
    for (row, basis) in samples.as_chunks_mut::<8>().0.iter_mut().zip(BASIS) {
        for (x, output) in row.iter_mut().enumerate() {
            let product = horizontal
                .iter()
                .zip(basis)
                .map(|(row, weight)| i32::from(row[x]) * weight)
                .fold(0i32, i32::wrapping_add);
            *output = (product >> COLUMN_SHIFT) as i16;
        }
    }
    samples
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(unsafe_code)]
mod x86 {
    use super::{BASIS, COLUMN_BIAS, COLUMN_SHIFT, ROW_BIAS, ROW_SHIFT};
    use core::arch::x86_64::*;

    const fn weights(a: usize, b: usize) -> [i16; 8] {
        [
            BASIS[0][a] as i16,
            BASIS[0][b] as i16,
            BASIS[1][a] as i16,
            BASIS[1][b] as i16,
            BASIS[2][a] as i16,
            BASIS[2][b] as i16,
            BASIS[3][a] as i16,
            BASIS[3][b] as i16,
        ]
    }

    pub(super) fn inverse(coefficients: &[i16; 64]) -> [i16; 64] {
        // SSE2 is baseline on x86_64. Loads and stores touch eight elements
        // of a complete 8x8 block at a time.
        unsafe {
            let rows: [__m128i; 8] =
                core::array::from_fn(|y| _mm_loadu_si128(coefficients.as_ptr().add(y * 8).cast()));
            let middle = rows[1..7]
                .iter()
                .copied()
                .fold(_mm_setzero_si128(), |a, b| _mm_or_si128(a, b));
            if empty(middle) {
                return outer_rows(rows[0], rows[7]);
            }

            let rows = rows.map(|values| horizontal(values));
            let columns = transpose(rows);
            let dc_bias = _mm_cvtsi32_si128(i32::from(COLUMN_BIAS));
            let columns = columns.map(|column| {
                let biased = _mm_add_epi16(column, dc_bias);
                line::<COLUMN_SHIFT>(biased, _mm_setzero_si128())
            });

            let mut output = [0; 64];
            for (row, values) in output
                .as_chunks_mut::<8>()
                .0
                .iter_mut()
                .zip(transpose(columns))
            {
                _mm_storeu_si128(row.as_mut_ptr().cast(), values);
            }
            output
        }
    }

    #[inline(always)]
    unsafe fn empty(values: __m128i) -> bool {
        unsafe { _mm_movemask_epi8(_mm_cmpeq_epi16(values, _mm_setzero_si128())) == 0xffff }
    }

    #[inline(always)]
    unsafe fn horizontal(values: __m128i) -> __m128i {
        unsafe {
            if empty(_mm_srli_si128::<2>(values)) {
                _mm_set1_epi16(_mm_extract_epi16::<0>(values) as i16 * 8)
            } else {
                line::<ROW_SHIFT>(values, _mm_set1_epi32(ROW_BIAS))
            }
        }
    }

    #[inline]
    unsafe fn outer_rows(first: __m128i, last: __m128i) -> [i16; 64] {
        unsafe {
            let first = _mm_add_epi16(horizontal(first), _mm_set1_epi16(COLUMN_BIAS));
            let last = horizontal(last);
            let low = _mm_unpacklo_epi16(first, last);
            let high = _mm_unpackhi_epi16(first, last);
            let mut output = [0; 64];
            // Mismatch control often leaves a single extra coefficient
            // in the last row. Only these two vertical frequencies contribute.
            for (row, basis) in output.as_chunks_mut::<8>().0.iter_mut().zip(BASIS) {
                let weights = _mm_set1_epi32(BASIS[0][0] | (basis[7] << 16));
                let a = _mm_srai_epi32::<COLUMN_SHIFT>(_mm_madd_epi16(low, weights));
                let b = _mm_srai_epi32::<COLUMN_SHIFT>(_mm_madd_epi16(high, weights));
                _mm_storeu_si128(row.as_mut_ptr().cast(), _mm_packs_epi32(a, b));
            }
            output
        }
    }

    #[inline(always)]
    unsafe fn line<const SHIFT: i32>(values: __m128i, bias: __m128i) -> __m128i {
        const EVEN_LOW: [i16; 8] = weights(0, 2);
        const EVEN_HIGH: [i16; 8] = weights(4, 6);
        const ODD_LOW: [i16; 8] = weights(1, 3);
        const ODD_HIGH: [i16; 8] = weights(5, 7);

        unsafe {
            // Reflecting the output index preserves even terms and negates
            // odd terms. Each multiply-add handles two frequencies at once.
            let values = _mm_shufflehi_epi16::<0xd8>(_mm_shufflelo_epi16::<0xd8>(values));
            let pair02 = _mm_shuffle_epi32::<0x00>(values);
            let pair13 = _mm_shuffle_epi32::<0x55>(values);
            let pair46 = _mm_shuffle_epi32::<0xaa>(values);
            let pair57 = _mm_shuffle_epi32::<0xff>(values);
            let even = _mm_add_epi32(
                _mm_madd_epi16(pair02, _mm_loadu_si128(EVEN_LOW.as_ptr().cast())),
                _mm_madd_epi16(pair46, _mm_loadu_si128(EVEN_HIGH.as_ptr().cast())),
            );
            let odd = _mm_add_epi32(
                _mm_madd_epi16(pair13, _mm_loadu_si128(ODD_LOW.as_ptr().cast())),
                _mm_madd_epi16(pair57, _mm_loadu_si128(ODD_HIGH.as_ptr().cast())),
            );
            let even = _mm_add_epi32(even, bias);
            let low = _mm_srai_epi32::<SHIFT>(_mm_add_epi32(even, odd));
            let high = _mm_srai_epi32::<SHIFT>(_mm_sub_epi32(even, odd));
            _mm_packs_epi32(low, _mm_shuffle_epi32::<0x1b>(high))
        }
    }

    #[inline(always)]
    unsafe fn transpose(rows: [__m128i; 8]) -> [__m128i; 8] {
        unsafe {
            let [a, b, c, d, e, f, g, h] = rows;
            let ab = [_mm_unpacklo_epi16(a, b), _mm_unpackhi_epi16(a, b)];
            let cd = [_mm_unpacklo_epi16(c, d), _mm_unpackhi_epi16(c, d)];
            let ef = [_mm_unpacklo_epi16(e, f), _mm_unpackhi_epi16(e, f)];
            let gh = [_mm_unpacklo_epi16(g, h), _mm_unpackhi_epi16(g, h)];
            let [p, q, r, s] = [
                _mm_unpacklo_epi32(ab[0], cd[0]),
                _mm_unpackhi_epi32(ab[0], cd[0]),
                _mm_unpacklo_epi32(ab[1], cd[1]),
                _mm_unpackhi_epi32(ab[1], cd[1]),
            ];
            let [t, u, v, w] = [
                _mm_unpacklo_epi32(ef[0], gh[0]),
                _mm_unpackhi_epi32(ef[0], gh[0]),
                _mm_unpacklo_epi32(ef[1], gh[1]),
                _mm_unpackhi_epi32(ef[1], gh[1]),
            ];
            [
                _mm_unpacklo_epi64(p, t),
                _mm_unpackhi_epi64(p, t),
                _mm_unpacklo_epi64(q, u),
                _mm_unpackhi_epi64(q, u),
                _mm_unpacklo_epi64(r, v),
                _mm_unpackhi_epi64(r, v),
                _mm_unpacklo_epi64(s, w),
                _mm_unpackhi_epi64(s, w),
            ]
        }
    }
}

#[cfg(all(test, target_arch = "x86_64", target_feature = "sse2"))]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_modes_agree_for_sparse_and_dense_blocks() {
        let mut seed = 0x1234_5678u32;
        for nonzero in 0..=64 {
            for _ in 0..32 {
                let mut block = [0; 64];
                for _ in 0..nonzero {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    block[(seed >> 12) as usize % 64] = ((seed >> 20) as i16) - 2048;
                }
                assert_eq!(inverse(&block), scalar(&block));
            }
        }
        for last in [-2048, -1, 0, 1, 2047] {
            let mut block = [0; 64];
            block[..8].copy_from_slice(&[1231, 221, -192, 4, 0, 0, 0, 0]);
            block[63] = last;
            assert_eq!(inverse(&block), scalar(&block));
        }
    }
}
