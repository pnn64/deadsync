use core::arch::x86_64::*;

use super::{level, SimdLevel};
use crate::simd::scalar;

// The inputs hold an 8-byte row in each register's low half. Each pair
// of transposed columns occupies the two halves of one result register.
#[inline]
fn transpose8(rows: [__m128i; 8]) -> [__m128i; 4] {
    // SAFETY: SSE2 is available on every x86-64 target.
    unsafe {
        let b01 = _mm_unpacklo_epi8(rows[0], rows[1]);
        let b23 = _mm_unpacklo_epi8(rows[2], rows[3]);
        let b45 = _mm_unpacklo_epi8(rows[4], rows[5]);
        let b67 = _mm_unpacklo_epi8(rows[6], rows[7]);
        let w03lo = _mm_unpacklo_epi16(b01, b23);
        let w03hi = _mm_unpackhi_epi16(b01, b23);
        let w47lo = _mm_unpacklo_epi16(b45, b67);
        let w47hi = _mm_unpackhi_epi16(b45, b67);
        [
            _mm_unpacklo_epi32(w03lo, w47lo),
            _mm_unpackhi_epi32(w03lo, w47lo),
            _mm_unpacklo_epi32(w03hi, w47hi),
            _mm_unpackhi_epi32(w03hi, w47hi),
        ]
    }
}

pub fn load_edge_lines_v(plane: &[u8], base: usize, stride: usize) -> [[u8; 16]; 8] {
    let plane = &plane[base..base + 15 * stride + 8];
    let mut columns = [[0; 16]; 8];
    // SAFETY: Each 8-byte load is within the checked plane window. Each
    // store writes one half of one column in the local output array.
    unsafe {
        for block in [0, 8] {
            let rows = core::array::from_fn(|i| {
                _mm_loadl_epi64(plane.as_ptr().add((i + block) * stride).cast())
            });
            for (pair, lanes) in transpose8(rows).into_iter().enumerate() {
                _mm_storel_epi64(columns[2 * pair].as_mut_ptr().add(block).cast(), lanes);
                _mm_storel_epi64(
                    columns[2 * pair + 1].as_mut_ptr().add(block).cast(),
                    _mm_srli_si128::<8>(lanes),
                );
            }
        }
    }
    columns
}

pub fn store_edge_lines_v(plane: &mut [u8], base: usize, stride: usize, columns: &[[u8; 16]; 8]) {
    let plane = &mut plane[base..base + 15 * stride + 8];
    // SAFETY: Each load reads one half of a column. Each 8-byte store is
    // within the checked plane window.
    unsafe {
        for block in [0, 8] {
            let halves =
                core::array::from_fn(|i| _mm_loadl_epi64(columns[i].as_ptr().add(block).cast()));
            for (pair, lanes) in transpose8(halves).into_iter().enumerate() {
                _mm_storel_epi64(
                    plane.as_mut_ptr().add((block + 2 * pair) * stride).cast(),
                    lanes,
                );
                _mm_storel_epi64(
                    plane
                        .as_mut_ptr()
                        .add((block + 2 * pair + 1) * stride)
                        .cast(),
                    _mm_srli_si128::<8>(lanes),
                );
            }
        }
    }
}

/// Filter eight 8-bit chroma lanes at once. The window starts at p1 and
/// contains the four samples p1, p0, q0 and q1 in each lane.
pub(crate) fn filter_chroma_edge_8<const VERTICAL: bool>(
    plane: &mut [u8],
    p1_start: usize,
    stride: usize,
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    let bytes = if VERTICAL {
        7 * stride + 4
    } else {
        3 * stride + 8
    };
    let plane = &mut plane[p1_start..p1_start + bytes];
    let base = plane.as_mut_ptr();
    // SAFETY: SSE2 is available on x86-64. Each load and store remains in
    // the checked window and stores update only samples at p0 and q0.
    unsafe {
        let zero = _mm_setzero_si128();
        let widen = |v| _mm_unpacklo_epi8(v, zero);
        let [p1, p0, q0, q1] = if VERTICAL {
            let rows = core::array::from_fn(|r| {
                _mm_cvtsi32_si128(base.add(r * stride).cast::<i32>().read_unaligned())
            });
            let [p, q, ..] = transpose8(rows);
            [
                widen(p),
                widen(_mm_srli_si128::<8>(p)),
                widen(q),
                widen(_mm_srli_si128::<8>(q)),
            ]
        } else {
            core::array::from_fn(|r| widen(_mm_loadl_epi64(base.add(r * stride).cast())))
        };
        let bs = _mm_srai_epi16::<8>(_mm_unpacklo_epi8(zero, _mm_loadl_epi64(bs.as_ptr().cast())));
        let alpha = _mm_set1_epi16(alpha as i16);
        let beta = _mm_set1_epi16(beta as i16);
        let below = |a, b, threshold| {
            let d = _mm_sub_epi16(a, b);
            _mm_cmpgt_epi16(threshold, _mm_max_epi16(d, _mm_sub_epi16(zero, d)))
        };
        let mask = _mm_and_si128(
            _mm_and_si128(_mm_cmpgt_epi16(bs, zero), below(p0, q0, alpha)),
            _mm_and_si128(below(p1, p0, beta), below(q1, q0, beta)),
        );
        if _mm_movemask_epi8(mask) == 0 {
            return;
        }

        let tc = _mm_add_epi16(
            widen(_mm_loadl_epi64(tc0.as_ptr().cast())),
            _mm_set1_epi16(1),
        );
        let diff = _mm_add_epi16(
            _mm_slli_epi16::<2>(_mm_sub_epi16(q0, p0)),
            _mm_sub_epi16(p1, q1),
        );
        let delta = _mm_srai_epi16::<3>(_mm_add_epi16(diff, _mm_set1_epi16(4)));
        let delta = _mm_min_epi16(_mm_max_epi16(delta, _mm_sub_epi16(zero, tc)), tc);
        let delta = _mm_and_si128(delta, mask);
        let mut p = _mm_add_epi16(p0, delta);
        let mut q = _mm_sub_epi16(q0, delta);

        let strong = _mm_and_si128(mask, _mm_cmpeq_epi16(bs, _mm_set1_epi16(4)));
        if _mm_movemask_epi8(strong) != 0 {
            let strong_sample = |p0, p1, q1| {
                _mm_srli_epi16::<2>(_mm_add_epi16(
                    _mm_add_epi16(_mm_slli_epi16::<1>(p1), p0),
                    _mm_add_epi16(q1, _mm_set1_epi16(2)),
                ))
            };
            let select = |strong_value, normal| {
                _mm_or_si128(
                    _mm_and_si128(strong, strong_value),
                    _mm_andnot_si128(strong, normal),
                )
            };
            p = select(strong_sample(p0, p1, q1), p);
            q = select(strong_sample(q0, q1, p1), q);
        }

        let pq = _mm_packus_epi16(p, q);
        let q = _mm_srli_si128::<8>(pq);
        if VERTICAL {
            let rows: [u16; 8] = core::mem::transmute(_mm_unpacklo_epi8(pq, q));
            for (r, samples) in rows.into_iter().enumerate() {
                base.add(r * stride + 1)
                    .cast::<u16>()
                    .write_unaligned(samples);
            }
        } else {
            _mm_storel_epi64(base.add(stride).cast(), pq);
            _mm_storel_epi64(base.add(2 * stride).cast(), q);
        }
    }
}

pub fn deblock_luma_lines(
    lines: &mut [[u8; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    // SAFETY: the function is called only if runtime detection found AVX2.
    if level() >= SimdLevel::Avx2 && unsafe { filter_normal::<true>(lines, bs, tc0, alpha, beta) } {
        return;
    }
    scalar::deblock_luma_lines(lines, bs, tc0, alpha, beta);
}

pub fn deblock_chroma_lines(
    lines: &mut [[u8; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    // SAFETY: the function is called only if runtime detection found AVX2.
    if level() >= SimdLevel::Avx2 && unsafe { filter_normal::<false>(lines, bs, tc0, alpha, beta) }
    {
        return;
    }
    scalar::deblock_chroma_lines(lines, bs, tc0, alpha, beta);
}

/// Process the sixteen edge lanes together. If any strong filter is
/// required, leave the input untouched for the scalar path.
#[target_feature(enable = "avx2")]
fn filter_normal<const LUMA: bool>(
    lines: &mut [[u8; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) -> bool {
    unsafe {
        // All loads and stores are exactly 16 bytes. Differences and sums
        // of 8-bit pixels with these weights fit in signed 16-bit lanes.
        let load_samples =
            |row: &[u8; 16]| _mm256_cvtepu8_epi16(_mm_loadu_si128(row.as_ptr().cast()));
        let load_params =
            |row: &[i8; 16]| _mm256_cvtepi8_epi16(_mm_loadu_si128(row.as_ptr().cast()));
        let store = |row: &mut [u8; 16], value| {
            let bytes = _mm_packus_epi16(
                _mm256_castsi256_si128(value),
                _mm256_extracti128_si256::<1>(value),
            );
            _mm_storeu_si128(row.as_mut_ptr().cast(), bytes);
        };
        let below = |a, b, threshold| {
            _mm256_cmpgt_epi16(threshold, _mm256_abs_epi16(_mm256_sub_epi16(a, b)))
        };

        let zero = _mm256_setzero_si256();
        let bs = load_params(bs);
        let alpha = _mm256_set1_epi16(alpha.clamp(0, 256) as i16);
        let beta = _mm256_set1_epi16(beta.clamp(0, 256) as i16);
        let p0 = load_samples(&lines[3]);
        let p1 = load_samples(&lines[2]);
        let q0 = load_samples(&lines[4]);
        let q1 = load_samples(&lines[5]);
        let mask = _mm256_and_si256(
            _mm256_and_si256(_mm256_cmpgt_epi16(bs, zero), below(p0, q0, alpha)),
            _mm256_and_si256(below(p1, p0, beta), below(q1, q0, beta)),
        );
        if _mm256_testz_si256(mask, mask) != 0 {
            return true;
        }
        if _mm256_testz_si256(mask, _mm256_cmpeq_epi16(bs, _mm256_set1_epi16(4))) == 0 {
            return false;
        }

        let t0 = load_params(tc0);
        let one = _mm256_set1_epi16(1);
        let tc = if LUMA {
            let p2 = load_samples(&lines[1]);
            let q2 = load_samples(&lines[6]);
            let ap = below(p2, p0, beta);
            let aq = below(q2, q0, beta);
            let avg = _mm256_avg_epu16(p0, q0);
            let filter_inner = |near, far, side| {
                let mean = _mm256_srli_epi16::<1>(_mm256_add_epi16(far, avg));
                let clipped = _mm256_min_epi16(
                    _mm256_max_epi16(mean, _mm256_sub_epi16(near, t0)),
                    _mm256_add_epi16(near, t0),
                );
                _mm256_blendv_epi8(near, clipped, _mm256_and_si256(mask, side))
            };
            store(&mut lines[2], filter_inner(p1, p2, ap));
            store(&mut lines[5], filter_inner(q1, q2, aq));
            // Comparison results are -1 (true) or 0 (false).
            _mm256_sub_epi16(_mm256_sub_epi16(t0, ap), aq)
        } else {
            _mm256_add_epi16(t0, one)
        };
        let d = _mm256_add_epi16(
            _mm256_slli_epi16::<2>(_mm256_sub_epi16(q0, p0)),
            _mm256_sub_epi16(p1, q1),
        );
        let d = _mm256_srai_epi16::<3>(_mm256_add_epi16(d, _mm256_set1_epi16(4)));
        let d = _mm256_min_epi16(_mm256_max_epi16(d, _mm256_sub_epi16(zero, tc)), tc);
        let d = _mm256_and_si256(d, mask);
        store(&mut lines[3], _mm256_add_epi16(p0, d));
        store(&mut lines[4], _mm256_sub_epi16(q0, d));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simd::scalar;
    use alloc::vec::Vec;

    #[test]
    fn eight_chroma_lanes_match_the_scalar_filter() {
        fn check<const VERTICAL: bool>() {
            let (base, stride) = (3, 19);
            let mut seed = 26u32;
            for i in 0..128 {
                let original: Vec<u8> = (0..base + 8 * stride)
                    .map(|_| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        match i % 3 {
                            0 => (seed >> 24) as u8,
                            1 => (seed >> 28) as u8,
                            _ => 255 - (seed >> 28) as u8,
                        }
                    })
                    .collect();
                let mut actual = original.clone();
                let mut expected = original;
                let mut samples = [[0; 16]; 8];
                let bs = core::array::from_fn(|k| ((i + k) % 6) as i8 - 1);
                let tc0 = core::array::from_fn(|k| ((i + k * 3) % 26) as i8);
                let (alpha, beta) = ([0, 25, 255][i % 3], [0, 18, 7, 18][i % 4]);
                let offset = |line, sample| {
                    base + if VERTICAL {
                        line * stride + sample
                    } else {
                        sample * stride + line
                    }
                };
                for line in 0..8 {
                    for sample in 0..4 {
                        samples[2 + sample][line] = expected[offset(line, sample)];
                    }
                }
                scalar::deblock_chroma_lines(&mut samples, &bs, &tc0, alpha, beta);
                for line in 0..8 {
                    expected[offset(line, 1)] = samples[3][line];
                    expected[offset(line, 2)] = samples[4][line];
                }
                filter_chroma_edge_8::<VERTICAL>(&mut actual, base, stride, &bs, &tc0, alpha, beta);
                assert_eq!(expected, actual, "iteration {i}, vertical={VERTICAL}");
            }
        }
        check::<true>();
        check::<false>();
    }

    #[test]
    fn strided_edge_transpose_matches_scalar() {
        for (base, stride) in [(0, 8), (3, 19), (5, 35)] {
            let len = base + 15 * stride + 8;
            let mut pixels: Vec<u8> = (0..len).map(|i| (i * 13) as u8).collect();
            let mut columns = load_edge_lines_v(&pixels, base, stride);
            assert_eq!(
                columns,
                scalar::load_edge_lines_v::<u8>(&pixels, base, stride)
            );
            for (column, lanes) in columns.iter_mut().enumerate() {
                lanes[column * 2] ^= 0xff;
            }
            let mut expected = pixels.clone();
            scalar::store_edge_lines_v::<u8>(&mut expected, base, stride, &columns);
            store_edge_lines_v(&mut pixels, base, stride, &columns);
            assert_eq!(pixels, expected);
        }
    }
}
