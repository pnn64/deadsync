use std::arch::x86_64::*;

/// Unfilter `row`, then `next` when given: the following row, also Paeth,
/// with pixels of at most four bytes.
pub(super) fn unfilter<const N: usize, const PAETH: bool>(
    row: &mut [u8],
    prev: &[u8],
    next: Option<&mut [u8]>,
) -> bool {
    assert!(matches!(N, 3 | 4 | 6 | 8));
    assert!(next.is_none() || PAETH && N <= 4);
    if row.len() >= 64 && std::is_x86_feature_detected!("sse4.1") {
        let prev = &prev[..row.len()];
        // SAFETY: SSE4.1 availability is checked above. Byte accesses use
        // checked slices and fixed-size arrays, including the final pixel.
        unsafe {
            match next {
                Some(next) if PAETH && N <= 4 => run_pair::<N>(row, &mut next[..prev.len()], prev),
                _ => run::<N, PAETH>(row, prev),
            }
        };
        true
    } else {
        false
    }
}

/// Sub-unfilter `row`, of pixels of `N` (3 or 4) bytes, where the vector
/// kernel applies.
pub(super) fn sub<const N: usize>(row: &mut [u8]) -> bool {
    assert!(matches!(N, 3 | 4));
    if row.len() >= 64 && std::is_x86_feature_detected!("sse4.1") {
        // SAFETY: SSE4.1 availability is checked above. Byte accesses use
        // checked slices and fixed-size arrays.
        let done = unsafe { sub_run::<N>(row) };
        // The last reconstructed pixel is the left pixel of the rest.
        super::unfilter_first_pixels::<N, false>(&mut row[done - N..]);
        true
    } else {
        false
    }
}

/// Sub reconstruction in groups of four pixels, returning the reconstructed
/// length. Shifted additions sum each group's residuals within its pixels,
/// independently of earlier groups; adding the preceding group's last pixel,
/// spread to every pixel, is the only dependency between groups. Stores write
/// exactly the group, so the next group's load never overlaps a pending store.
#[target_feature(enable = "sse4.1")]
unsafe fn sub_run<const N: usize>(row: &mut [u8]) -> usize {
    let spread = if N == 4 {
        _mm_setr_epi8(
            12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15, 12, 13, 14, 15,
        )
    } else {
        _mm_setr_epi8(9, 10, 11, 9, 10, 11, 9, 10, 11, 9, 10, 11, -1, -1, -1, -1)
    };
    let mut carry = _mm_setzero_si128();
    let mut pos = 0;
    while row.len() - pos >= 16 {
        let bytes: &mut [u8; 16] = (&mut row[pos..pos + 16]).try_into().unwrap();
        let residual = load(bytes);
        let pixels = if N == 4 {
            let sum = _mm_add_epi8(residual, _mm_slli_si128::<4>(residual));
            _mm_add_epi8(_mm_add_epi8(sum, _mm_slli_si128::<8>(sum)), carry)
        } else {
            let sum = _mm_add_epi8(residual, _mm_slli_si128::<3>(residual));
            _mm_add_epi8(_mm_add_epi8(sum, _mm_slli_si128::<6>(sum)), carry)
        };
        carry = _mm_shuffle_epi8(pixels, spread);
        if N == 4 {
            store(bytes, pixels);
        } else {
            bytes[..8].copy_from_slice(&_mm_cvtsi128_si64(pixels).to_le_bytes());
            bytes[8..12].copy_from_slice(&_mm_extract_epi32::<2>(pixels).to_le_bytes());
        }
        pos += 4 * N;
    }
    pos
}

#[inline]
#[target_feature(enable = "sse4.1")]
fn load(bytes: &[u8; 16]) -> __m128i {
    // SAFETY: the array holds the 16 bytes read; the load is unaligned.
    unsafe { _mm_loadu_si128(bytes.as_ptr().cast()) }
}

#[inline]
#[target_feature(enable = "sse4.1")]
fn store(bytes: &mut [u8; 16], value: __m128i) {
    // SAFETY: the array holds the 16 bytes written; the store is unaligned.
    unsafe { _mm_storeu_si128(bytes.as_mut_ptr().cast(), value) }
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
    let next = paeth(_mm_cvtepu8_epi16(data_bytes), up, left, upper_left);
    let bytes = _mm_packus_epi16(next, _mm_setzero_si128());
    let packed = if N <= 4 {
        u64::from(_mm_cvtsi128_si32(bytes) as u32)
    } else {
        _mm_cvtsi128_si64(bytes) as u64
    };
    (next, up, packed)
}

/// One Paeth reconstruction on 16-bit lanes holding bytes.
#[inline]
#[target_feature(enable = "sse4.1")]
fn paeth(residual: __m128i, up: __m128i, left: __m128i, upper_left: __m128i) -> __m128i {
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
    _mm_and_si128(_mm_add_epi16(predictor, residual), _mm_set1_epi16(255))
}

/// The four bytes at `pos`, zero-padded past the end of `bytes`.
#[inline]
fn word(bytes: &[u8], pos: usize) -> u32 {
    if bytes.len() - pos >= 4 {
        u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap())
    } else {
        let mut padded = [0; 4];
        padded[..bytes.len() - pos].copy_from_slice(&bytes[pos..]);
        u32::from_le_bytes(padded)
    }
}

/// Store up to `N` reconstructed bytes at `pos`.
#[inline]
fn put<const N: usize>(bytes: &mut [u8], pos: usize, value: [u8; 4]) {
    if bytes.len() - pos >= N {
        bytes[pos..pos + N].copy_from_slice(&value[..N]);
    } else {
        let n = bytes.len() - pos;
        bytes[pos..].copy_from_slice(&value[..n]);
    }
}

/// Two consecutive Paeth rows of pixels up to four bytes share each register:
/// the first row's pixel in the low four lanes and the second row's, one pixel
/// behind, in the high four. The second row's up pixel is then the first
/// row's previous result, its left pixel is already in the high lanes of the
/// previous result, and its upper-left pixel is the previous up. One Paeth
/// evaluation reconstructs a pixel of each row.
#[target_feature(enable = "sse4.1")]
unsafe fn run_pair<const N: usize>(first: &mut [u8], second: &mut [u8], prev: &[u8]) {
    let steps = first.len().div_ceil(N);
    let mut left = _mm_setzero_si128();
    let mut upper_left = _mm_setzero_si128();
    let mut step = 0;
    while step <= steps {
        let pos = step * N;
        if step != 0 && pos + 4 <= first.len() {
            // Whole pixels: full-width loads, and stores of exactly the pixel
            // so a later load never straddles an earlier store.
            let a = u32::from_le_bytes(first[pos..pos + 4].try_into().unwrap());
            let b = u32::from_le_bytes(second[pos - N..pos - N + 4].try_into().unwrap());
            let p = u32::from_le_bytes(prev[pos..pos + 4].try_into().unwrap());
            let residual = _mm_insert_epi32::<1>(_mm_cvtsi32_si128(a as i32), b as i32);
            let up = _mm_or_si128(
                _mm_cvtepu8_epi16(_mm_cvtsi32_si128(p as i32)),
                _mm_slli_si128::<8>(left),
            );
            let next = paeth(_mm_cvtepu8_epi16(residual), up, left, upper_left);
            let bytes = (_mm_cvtsi128_si64(_mm_packus_epi16(next, next)) as u64).to_le_bytes();
            first[pos..pos + N].copy_from_slice(&bytes[..N]);
            second[pos - N..pos].copy_from_slice(&bytes[4..4 + N]);
            (left, upper_left) = (next, up);
            step += 1;
            continue;
        }
        // The first step, and the row end, where lanes for a row that has not
        // started or has finished hold zeros or values that are never stored.
        let (residual, above) = if step < steps {
            (word(first, pos), word(prev, pos))
        } else {
            (0, 0)
        };
        let below = if step != 0 { word(second, pos - N) } else { 0 };
        let residual = _mm_cvtsi64_si128((u64::from(below) << 32 | u64::from(residual)) as i64);
        let up = _mm_or_si128(
            _mm_cvtepu8_epi16(_mm_cvtsi32_si128(above as i32)),
            _mm_slli_si128::<8>(left),
        );
        let next = paeth(_mm_cvtepu8_epi16(residual), up, left, upper_left);
        let bytes = (_mm_cvtsi128_si64(_mm_packus_epi16(next, next)) as u64).to_le_bytes();
        if step < steps {
            put::<N>(first, pos, [bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        if step != 0 {
            put::<N>(second, pos - N, [bytes[4], bytes[5], bytes[6], bytes[7]]);
        }
        (left, upper_left) = (next, up);
        step += 1;
    }
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

    fn pairs<const N: usize>() {
        let mut state = 0x9e37_79b9u32;
        let mut random = |mask: u8| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8 & mask
        };
        for len in [64, 65, 66, 67, 200, 771, 1024] {
            // Narrow value ranges produce Paeth ties; full ranges cover the rest.
            for mask in [1, 3, 255] {
                let prev: Vec<u8> = (0..len).map(|_| random(mask)).collect();
                let first: Vec<u8> = (0..len).map(|_| random(mask)).collect();
                let second: Vec<u8> = (0..len).map(|_| random(mask)).collect();
                let (mut a, mut b) = (first.clone(), second.clone());
                assert!(unfilter::<N, true>(&mut a, &prev, None));
                assert!(unfilter::<N, true>(&mut b, &a, None));
                let (mut x, mut y) = (first, second);
                assert!(unfilter::<N, true>(&mut x, &prev, Some(&mut y)));
                assert_eq!((x, y), (a, b), "N={N} len={len} mask={mask}");
            }
        }
    }

    fn subs<const N: usize>() {
        for offset in [0, 1, 7] {
            for len in (0..200).chain([255, 256, 257, 4095, 4096, 4097]) {
                let len = len / N * N;
                for pattern in 0..3 {
                    let mut actual = vec![0xa5; offset + len + 32];
                    for n in 0..len {
                        actual[offset + n] = match pattern {
                            0 => 255,
                            1 => n as u8,
                            _ => ((n * 173 + n * n * 47 + 71) ^ (n >> 2)) as u8,
                        };
                    }
                    let mut expected = actual.clone();
                    for n in N..len {
                        expected[offset + n] =
                            expected[offset + n].wrapping_add(expected[offset + n - N]);
                    }
                    let row = &mut actual[offset..offset + len];
                    let mut copy = row.to_vec();
                    assert_eq!(sub::<N>(&mut copy), len >= 64);
                    super::super::unfilter(row, &[], N, 1).unwrap();
                    assert_eq!(
                        actual, expected,
                        "N={N} offset={offset} len={len} pattern={pattern}"
                    );
                }
            }
        }
    }

    #[test]
    fn vector_sub_rows_match_bytewise_left_sums() {
        if !std::is_x86_feature_detected!("sse4.1") {
            return;
        }
        subs::<3>();
        subs::<4>();
    }

    #[test]
    fn paired_paeth_rows_match_one_row_at_a_time() {
        if !std::is_x86_feature_detected!("sse4.1") {
            return;
        }
        pairs::<3>();
        pairs::<4>();
    }
}
