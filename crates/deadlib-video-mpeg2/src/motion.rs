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
                    _ => bilinear::<W>(
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
    unsafe fn bilinear<const W: usize>(a: __m128i, b: __m128i, c: __m128i, d: __m128i) -> __m128i {
        unsafe {
            let zero = _mm_setzero_si128();
            let bias = _mm_set1_epi16(2);
            let low = _mm_add_epi16(
                _mm_add_epi16(_mm_unpacklo_epi8(a, zero), _mm_unpacklo_epi8(b, zero)),
                _mm_add_epi16(_mm_unpacklo_epi8(c, zero), _mm_unpacklo_epi8(d, zero)),
            );
            let low = _mm_srli_epi16::<2>(_mm_add_epi16(low, bias));
            let high = if W == 16 {
                let high = _mm_add_epi16(
                    _mm_add_epi16(_mm_unpackhi_epi8(a, zero), _mm_unpackhi_epi8(b, zero)),
                    _mm_add_epi16(_mm_unpackhi_epi8(c, zero), _mm_unpackhi_epi8(d, zero)),
                );
                _mm_srli_epi16::<2>(_mm_add_epi16(high, bias))
            } else {
                zero
            };
            _mm_packus_epi16(low, high)
        }
    }
}

#[cfg(all(test, target_arch = "x86_64", target_feature = "sse2"))]
mod tests {
    use super::*;

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
