pub(crate) fn predict<const W: usize>(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    fraction: usize,
    average: bool,
) {
    match (fraction, average) {
        (0, false) => rows::<W, 0, 0, false>(dst, dst_stride, src, src_stride),
        (1, false) => rows::<W, 1, 0, false>(dst, dst_stride, src, src_stride),
        (2, false) => rows::<W, 0, 1, false>(dst, dst_stride, src, src_stride),
        (3, false) => rows::<W, 1, 1, false>(dst, dst_stride, src, src_stride),
        (0, true) => rows::<W, 0, 0, true>(dst, dst_stride, src, src_stride),
        (1, true) => rows::<W, 1, 0, true>(dst, dst_stride, src, src_stride),
        (2, true) => rows::<W, 0, 1, true>(dst, dst_stride, src, src_stride),
        (3, true) => rows::<W, 1, 1, true>(dst, dst_stride, src, src_stride),
        _ => unreachable!(),
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
use x86::rows;

#[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
use scalar_rows as rows;

#[cfg(any(test, not(all(target_arch = "x86_64", target_feature = "sse2"))))]
fn scalar_rows<const W: usize, const X: usize, const Y: usize, const AVG: bool>(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
) {
    for y in 0..W {
        let dst = &mut dst[y * dst_stride..][..W];
        let src = &src[y * src_stride..];
        if X == 0 && Y == 0 && !AVG {
            dst.copy_from_slice(&src[..W]);
            continue;
        }
        for (x, pixel) in dst.iter_mut().enumerate() {
            let a = u16::from(src[x]);
            let predicted = match (X, Y) {
                (0, 0) => a,
                (1, 0) => (a + u16::from(src[x + 1]) + 1) >> 1,
                (0, 1) => (a + u16::from(src[x + src_stride]) + 1) >> 1,
                _ => {
                    (a + u16::from(src[x + 1])
                        + u16::from(src[x + src_stride])
                        + u16::from(src[x + src_stride + 1])
                        + 2)
                        >> 2
                }
            };
            *pixel = if AVG {
                ((predicted + u16::from(*pixel) + 1) >> 1) as u8
            } else {
                predicted as u8
            };
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(unsafe_code)]
mod x86 {
    use core::arch::x86_64::*;

    pub(super) fn rows<const W: usize, const X: usize, const Y: usize, const AVG: bool>(
        dst: &mut [u8],
        dst_stride: usize,
        src: &[u8],
        src_stride: usize,
    ) {
        const { assert!((W == 8 || W == 16) && X <= 1 && Y <= 1) };
        assert!(dst.len() >= W && (dst.len() - W) / (W - 1) >= dst_stride);
        assert!(src.len() >= W + X && (src.len() - W - X) / (W - 1 + Y) >= src_stride);

        // SSE2 is baseline on x86_64. The bounds above include the last
        // sample of every row and the extra samples used by interpolation.
        unsafe {
            for row in 0..W {
                let src = src.as_ptr().add(row * src_stride);
                let dst = dst.as_mut_ptr().add(row * dst_stride);
                let a = load::<W>(src);
                let mut predicted = match (X, Y) {
                    (0, 0) => a,
                    (1, 0) => _mm_avg_epu8(a, load::<W>(src.add(1))),
                    (0, 1) => _mm_avg_epu8(a, load::<W>(src.add(src_stride))),
                    _ => bilinear(
                        a,
                        load::<W>(src.add(1)),
                        load::<W>(src.add(src_stride)),
                        load::<W>(src.add(src_stride + 1)),
                    ),
                };
                if AVG {
                    predicted = _mm_avg_epu8(predicted, load::<W>(dst));
                }
                if W == 8 {
                    _mm_storel_epi64(dst.cast(), predicted);
                } else {
                    _mm_storeu_si128(dst.cast(), predicted);
                }
            }
        }
    }

    #[inline(always)]
    unsafe fn load<const W: usize>(ptr: *const u8) -> __m128i {
        unsafe {
            if W == 8 {
                _mm_loadl_epi64(ptr.cast())
            } else {
                _mm_loadu_si128(ptr.cast())
            }
        }
    }

    #[inline(always)]
    unsafe fn bilinear(a: __m128i, b: __m128i, c: __m128i, d: __m128i) -> __m128i {
        unsafe {
            let ab = _mm_avg_epu8(a, b);
            let cd = _mm_avg_epu8(c, d);
            // Rounded byte averages overcount by one exactly when their sum is
            // odd and at least one original pair sum is odd.
            let odd_pairs = _mm_or_si128(_mm_xor_si128(a, b), _mm_xor_si128(c, d));
            let correction = _mm_and_si128(
                _mm_and_si128(_mm_xor_si128(ab, cd), odd_pairs),
                _mm_set1_epi8(1),
            );
            _mm_sub_epi8(_mm_avg_epu8(ab, cd), correction)
        }
    }
}

#[cfg(all(test, target_arch = "x86_64", target_feature = "sse2"))]
mod tests {
    use super::*;

    #[test]
    fn bilinear_rounding_matches_every_pair_of_byte_sums() {
        let mut src = [0; 17 * 17];
        let mut wide = [0; 16 * 16];
        let mut narrow = [0; 8 * 8];
        // The arithmetic depends only on pair sums and their parity.
        for first in 0..=510u16 {
            for second in 0..=510u16 {
                let a = first.min(255) as u8;
                let b = (first - u16::from(a)) as u8;
                let c = second.min(255) as u8;
                let d = (second - u16::from(c)) as u8;
                let pixels = [[a, b], [c, d]];
                for (y, row) in src.chunks_exact_mut(17).enumerate() {
                    for (x, pixel) in row.iter_mut().enumerate() {
                        *pixel = pixels[y & 1][x & 1];
                    }
                }
                let expected = ((first + second + 2) >> 2) as u8;
                predict::<16>(&mut wide, 16, &src, 17, 3, false);
                predict::<8>(&mut narrow, 8, &src, 17, 3, false);
                assert_eq!(wide, [expected; 16 * 16]);
                assert_eq!(narrow, [expected; 8 * 8]);
            }
        }
    }

    #[test]
    fn vector_prediction_matches_scalar_for_each_mode() {
        fn check<const W: usize, const X: usize, const Y: usize, const AVG: bool>() {
            let mut src = [0; 19 * 17];
            let mut dst = [0; 23 * 16];
            for (i, pixel) in src.iter_mut().enumerate() {
                *pixel = (i * 31 + i / 5) as u8;
            }
            for (i, pixel) in dst.iter_mut().enumerate() {
                *pixel = (i * 9 + i / 11) as u8;
            }
            let mut expected = dst;
            scalar_rows::<W, X, Y, AVG>(&mut expected[1..], 23, &src[1..], 19);
            predict::<W>(&mut dst[1..], 23, &src[1..], 19, X | (Y << 1), AVG);
            assert_eq!(dst, expected);
        }

        fn widths<const W: usize>() {
            check::<W, 0, 0, false>();
            check::<W, 1, 0, false>();
            check::<W, 0, 1, false>();
            check::<W, 1, 1, false>();
            check::<W, 0, 0, true>();
            check::<W, 1, 0, true>();
            check::<W, 0, 1, true>();
            check::<W, 1, 1, true>();
        }
        widths::<8>();
        widths::<16>();
    }
}
