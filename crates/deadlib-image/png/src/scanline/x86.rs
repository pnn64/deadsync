use std::arch::x86_64::*;

pub(super) fn unfilter<const N: usize, const PAETH: bool>(row: &mut [u8], prev: &[u8]) -> bool {
    assert!(matches!(N, 3 | 4 | 6 | 8));
    if row.len() >= 64 && std::is_x86_feature_detected!("sse4.1") {
        let prev = &prev[..row.len()];
        // SAFETY: SSE4.1 availability is checked above. Byte accesses use
        // checked slices and fixed-size arrays, including the final pixel.
        unsafe { run::<N, PAETH>(row, prev) };
        true
    } else {
        false
    }
}

#[inline]
#[target_feature(enable = "sse4.1")]
fn step<const N: usize, const PAETH: bool>(
    data: u64,
    above: u64,
    left: __m128i,
    upper_left: __m128i,
) -> (__m128i, __m128i, u64) {
    let up_bytes = if N <= 4 {
        _mm_cvtsi32_si128(above as i32)
    } else {
        _mm_cvtsi64_si128(above as i64)
    };
    let data_bytes = if N <= 4 {
        _mm_cvtsi32_si128(data as i32)
    } else {
        _mm_cvtsi64_si128(data as i64)
    };
    if !PAETH {
        // The byte average instruction rounds up. The low bit of a^b is
        // precisely the odd-sum correction needed for PNG's floor average.
        let parity = _mm_and_si128(_mm_xor_si128(left, up_bytes), _mm_set1_epi8(1));
        let predictor = _mm_sub_epi8(_mm_avg_epu8(left, up_bytes), parity);
        let next = _mm_add_epi8(predictor, data_bytes);
        let packed = if N <= 4 {
            u64::from(_mm_cvtsi128_si32(next) as u32)
        } else {
            _mm_cvtsi128_si64(next) as u64
        };
        return (next, up_bytes, packed);
    }
    let up = _mm_cvtepu8_epi16(up_bytes);
    let residual = _mm_cvtepu8_epi16(data_bytes);
    let low = _mm_min_epi16(left, up);
    let high = _mm_max_epi16(left, up);
    // Prepare the preceding-row terms independently of the reconstructed
    // pixel. All values, including 3*c-b-a, fit in signed 16-bit lanes.
    let top = _mm_sub_epi16(
        _mm_add_epi16(_mm_add_epi16(upper_left, upper_left), upper_left),
        up,
    );
    let threshold = _mm_sub_epi16(top, left);
    // Reverse each blend's operands to express inclusive regions with strict
    // comparisons. This preserves Paeth's ties without incrementing bounds.
    let candidate = _mm_blendv_epi8(low, upper_left, _mm_cmpgt_epi16(high, threshold));
    let predictor = _mm_blendv_epi8(high, candidate, _mm_cmpgt_epi16(threshold, low));
    let next = _mm_and_si128(_mm_add_epi16(predictor, residual), _mm_set1_epi16(255));
    let bytes = _mm_packus_epi16(next, _mm_setzero_si128());
    let packed = if N <= 4 {
        u64::from(_mm_cvtsi128_si32(bytes) as u32)
    } else {
        _mm_cvtsi128_si64(bytes) as u64
    };
    (next, up, packed)
}

#[target_feature(enable = "sse4.1")]
unsafe fn run<const N: usize, const PAETH: bool>(row: &mut [u8], prev: &[u8]) {
    let mut left = _mm_setzero_si128();
    let mut upper_left = _mm_setzero_si128();
    let mut pos = 0;
    let load = if N <= 4 { 4 } else { 8 };
    while row.len() - pos >= load {
        let data = if N <= 4 {
            u64::from(u32::from_le_bytes(row[pos..pos + 4].try_into().unwrap()))
        } else {
            u64::from_le_bytes(row[pos..pos + 8].try_into().unwrap())
        };
        let up = if N <= 4 {
            u64::from(u32::from_le_bytes(prev[pos..pos + 4].try_into().unwrap()))
        } else {
            u64::from_le_bytes(prev[pos..pos + 8].try_into().unwrap())
        };
        let (a, c, value) = step::<N, PAETH>(data, up, left, upper_left);
        row[pos..pos + N].copy_from_slice(&value.to_le_bytes()[..N]);
        left = a;
        upper_left = c;
        pos += N;
    }
    while pos < row.len() {
        let n = N.min(row.len() - pos);
        let mut data = [0; 8];
        data[..n].copy_from_slice(&row[pos..pos + n]);
        let mut up = [0; 8];
        up[..n].copy_from_slice(&prev[pos..pos + n]);
        let (a, c, value) = step::<N, PAETH>(
            u64::from_le_bytes(data),
            u64::from_le_bytes(up),
            left,
            upper_left,
        );
        row[pos..pos + n].copy_from_slice(&value.to_le_bytes()[..n]);
        left = a;
        upper_left = c;
        pos += n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(a: u8, b: u8, c: u8) -> u8 {
        let p = i32::from(a) + i32::from(b) - i32::from(c);
        let (pa, pb, pc) = (
            (p - i32::from(a)).abs(),
            (p - i32::from(b)).abs(),
            (p - i32::from(c)).abs(),
        );
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    #[test]
    fn every_vector_paeth_triplet_matches_the_specification() {
        if !std::is_x86_feature_detected!("sse4.1") {
            return;
        }
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                for c in 0..=255u8 {
                    // SAFETY: SSE4.1 availability was established above.
                    let (_, _, actual) = unsafe {
                        step::<8, true>(
                            0,
                            u64::from(b) * 0x0101_0101_0101_0101,
                            _mm_set1_epi16(i16::from(a)),
                            _mm_set1_epi16(i16::from(c)),
                        )
                    };
                    assert_eq!(
                        actual,
                        u64::from(reference(a, b, c)) * 0x0101_0101_0101_0101,
                        "{a} {b} {c}"
                    );
                }
            }
        }
    }

    fn rows<const N: usize, const PAETH: bool>() {
        for offset in 0..32 {
            for len in (0..140).chain([255, 256, 257, 511, 512, 513, 4095, 4096, 4097]) {
                for pattern in 0..4 {
                    let mut actual = vec![0xa5; offset + len + 32];
                    let mut previous = actual.clone();
                    for n in 0..len {
                        actual[offset + n] = match pattern {
                            0 => 0,
                            1 => 255,
                            _ => ((n * 173 + n * n * 47 + 71) ^ (n >> 2)) as u8,
                        };
                        previous[offset + n] = match pattern {
                            0 => 255,
                            1 => 0,
                            _ => (n * 59 + n * n * 113 + 219) as u8,
                        };
                    }
                    let mut expected = actual.clone();
                    for n in 0..len {
                        let a = if n < N { 0 } else { expected[offset + n - N] };
                        let c = if n < N { 0 } else { previous[offset + n - N] };
                        let predictor = if PAETH {
                            reference(a, previous[offset + n], c)
                        } else {
                            ((u16::from(a) + u16::from(previous[offset + n])) / 2) as u8
                        };
                        expected[offset + n] = expected[offset + n].wrapping_add(predictor);
                    }
                    // SAFETY: The caller checks SSE4.1; the two slices have
                    // equal lengths. N is one of the supported pixel widths.
                    unsafe {
                        run::<N, PAETH>(
                            &mut actual[offset..offset + len],
                            &previous[offset..offset + len],
                        );
                    }
                    assert_eq!(
                        actual, expected,
                        "N={N} offset={offset} len={len} pattern={pattern}"
                    );
                }
            }
        }
    }

    #[test]
    fn vector_rows_preserve_channel_history_tails_and_surrounding_bytes() {
        if !std::is_x86_feature_detected!("sse4.1") {
            return;
        }
        rows::<3, true>();
        rows::<4, true>();
        rows::<6, true>();
        rows::<8, true>();
        rows::<3, false>();
        rows::<4, false>();
        rows::<6, false>();
        rows::<8, false>();
    }
    fn averages<const N: usize>() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                for residual in [0u8, 1, 127, 255] {
                    // SAFETY: The caller checks SSE4.1 before entering.
                    let (_, _, actual) = unsafe {
                        step::<N, false>(
                            u64::from(residual) * 0x0101_0101_0101_0101,
                            u64::from(b) * 0x0101_0101_0101_0101,
                            _mm_set1_epi8(a as i8),
                            _mm_setzero_si128(),
                        )
                    };
                    let expected = residual.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8);
                    let repeated = u64::from(expected)
                        * if N <= 4 {
                            0x0101_0101
                        } else {
                            0x0101_0101_0101_0101
                        };
                    assert_eq!(actual, repeated, "N={N} a={a} b={b} residual={residual}");
                }
            }
        }
    }
    #[test]
    fn every_average_pair_and_wrapping_residual_matches_integer_division() {
        if !std::is_x86_feature_detected!("sse4.1") {
            return;
        }
        averages::<4>();
        averages::<8>();
    }
}
