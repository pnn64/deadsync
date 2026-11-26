//! SIMD backend dispatch for motion-compensation kernels.
//!
//! Three backends share one function set, selected at compile time:
//! - `neon` (aarch64, default): hand-written NEON for the luma half-pel
//!   filters; everything else falls through to scalar.
//! - `x86` (x86_64, default): SSE2 baseline with runtime-detected SSSE3 /
//!   AVX2 / AVX-512 tiers (detection needs the `std` feature; without it
//!   the SSE2 baseline is used).
//! - `scalar`: the reference implementation and fallback for all other
//!   targets.
//!
//! The native backends must produce the same pixels as the scalar backend.

#[cfg(target_arch = "aarch64")]
mod neon;
#[allow(dead_code)]
pub(crate) mod scalar;
#[cfg(target_arch = "x86_64")]
mod x86;

#[cfg(target_arch = "aarch64")]
pub use neon::*;
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
pub use scalar::*;
#[cfg(target_arch = "x86_64")]
pub use x86::*;

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    /// xorshift64 bytes, biased to the extremes so clipping is exercised.
    fn random_bytes(seed: &mut u64, n: usize) -> Vec<u8> {
        (0..n)
            .map(|_| {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 7;
                *seed ^= *seed << 17;
                match *seed % 8 {
                    0 => 0,
                    1 => 255,
                    _ => (*seed >> 24) as u8,
                }
            })
            .collect()
    }

    /// Every kernel of the active backend matches the scalar reference for
    /// all widths up to 16 (SIMD bodies and scalar tails) on random rows.
    #[test]
    fn kernels_match_scalar() {
        use super::scalar as s;
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        for round in 0..200 {
            for w in 1..=16usize {
                let rows_buf: Vec<Vec<u8>> =
                    (0..6).map(|_| random_bytes(&mut seed, w + 5)).collect();
                let rows: [&[u8]; 6] = core::array::from_fn(|k| &rows_buf[k][..]);
                let rows2_buf: Vec<Vec<u8>> =
                    (0..6).map(|_| random_bytes(&mut seed, w + 5)).collect();
                let rows2: [&[u8]; 6] = core::array::from_fn(|k| &rows2_buf[k][..]);
                let src = random_bytes(&mut seed, w + 5);
                let int_row = random_bytes(&mut seed, w);
                let check = |name: &str, f: &dyn Fn(&mut [u8]), g: &dyn Fn(&mut [u8])| {
                    let (mut a, mut b) = (alloc::vec![0u8; w], alloc::vec![0u8; w]);
                    f(&mut a);
                    g(&mut b);
                    assert_eq!(a, b, "{name} w={w} round={round}");
                };
                check(
                    "row_half_pel_h",
                    &|o| super::row_half_pel_h(&src, o, w),
                    &|o| s::row_half_pel_h(&src, o, w),
                );
                check(
                    "row_half_pel_v",
                    &|o| super::row_half_pel_v(rows, o, w),
                    &|o| s::row_half_pel_v(rows, o, w),
                );
                check(
                    "row_half_pel_hv",
                    &|o| super::row_half_pel_hv(rows, o, w),
                    &|o| s::row_half_pel_hv(rows, o, w),
                );
                check(
                    "row_avg_int_v",
                    &|o| super::row_avg_int_v(&int_row, rows, o, w),
                    &|o| s::row_avg_int_v(&int_row, rows, o, w),
                );
                check(
                    "row_avg_h_v",
                    &|o| super::row_avg_h_v(&src, rows, o, w),
                    &|o| s::row_avg_h_v(&src, rows, o, w),
                );
                check(
                    "row_avg_int_h",
                    &|o| super::row_avg_int_h(&int_row, &src, o, w),
                    &|o| s::row_avg_int_h(&int_row, &src, o, w),
                );
                check(
                    "row_avg_h_int",
                    &|o| super::row_avg_h_int(&src, &int_row, o, w),
                    &|o| s::row_avg_h_int(&src, &int_row, o, w),
                );
                check(
                    "row_avg_h_hv",
                    &|o| super::row_avg_h_hv(&src, rows2, o, w),
                    &|o| s::row_avg_h_hv(&src, rows2, o, w),
                );
                check(
                    "row_avg_v_hv",
                    &|o| super::row_avg_v_hv(rows, rows2, o, w),
                    &|o| s::row_avg_v_hv(rows, rows2, o, w),
                );
                let (a, b) = (random_bytes(&mut seed, w), random_bytes(&mut seed, w));
                check("bi_pred_avg", &|o| super::bi_pred_avg(&a, &b, o), &|o| {
                    s::bi_pred_avg(&a, &b, o)
                });
            }
            // Edge transposes: 16 rows of 8 bytes at a random stride, and
            // back (as a filter would write them)
            let stride = 8 + (seed % 24) as usize;
            let plane = random_bytes(&mut seed, 15 * stride + 8 + 3);
            let base = (seed % 4) as usize;
            let lanes: [[u8; 16]; 8] = super::load_edge_lines_v(&plane, base, stride);
            assert_eq!(
                lanes,
                s::load_edge_lines_v(&plane, base, stride),
                "load round={round}"
            );
            let mut changed = lanes;
            for row in changed.iter_mut() {
                for v in row.iter_mut() {
                    *v = v.wrapping_mul(7).wrapping_add(3);
                }
            }
            let (mut a, mut b) = (plane.clone(), plane.clone());
            super::store_edge_lines_v(&mut a, base, stride, &changed);
            s::store_edge_lines_v(&mut b, base, stride, &changed);
            assert_eq!(a, b, "store round={round}");
            // Deblocking kernels on random lines and parameters
            let lines: [[u8; 16]; 8] =
                core::array::from_fn(|_| random_bytes(&mut seed, 16).try_into().unwrap());
            let bs: [i8; 16] = core::array::from_fn(|_| (random_bytes(&mut seed, 1)[0] % 5) as i8);
            let tc0: [i8; 16] = core::array::from_fn(|i| {
                if bs[i] < 4 {
                    (random_bytes(&mut seed, 1)[0] % 26) as i8
                } else {
                    0
                }
            });
            let alpha = random_bytes(&mut seed, 1)[0] as i32;
            let beta = (random_bytes(&mut seed, 1)[0] % 19) as i32;
            for luma in [true, false] {
                let (mut a, mut b) = (lines, lines);
                if luma {
                    super::deblock_luma_lines(&mut a, &bs, &tc0, alpha, beta);
                    s::deblock_luma_lines(&mut b, &bs, &tc0, alpha, beta);
                } else {
                    super::deblock_chroma_lines(&mut a, &bs, &tc0, alpha, beta);
                    s::deblock_chroma_lines(&mut b, &bs, &tc0, alpha, beta);
                }
                assert_eq!(a, b, "deblock luma={luma} round={round}");
            }
            // Chroma bilinear: every block size, fractions, a plane whose
            // last row ends exactly where the block's reads do
            for &(bw, bh) in &[
                (2usize, 2usize),
                (2, 4),
                (4, 2),
                (4, 4),
                (4, 8),
                (8, 4),
                (8, 8),
                (8, 16),
                (4, 16),
            ] {
                let stride = bw + 1 + (seed % 5) as usize;
                let plane = random_bytes(&mut seed, bh * stride + bw + 1);
                let fx = (seed % 8) as u8;
                let fy = ((seed >> 8) % 8) as u8;
                let c = [(8 - fx) * (8 - fy), fx * (8 - fy), (8 - fx) * fy, fx * fy];
                let (mut a, mut b) = (alloc::vec![0u8; bw * bh], alloc::vec![0u8; bw * bh]);
                super::chroma_bilinear_block(
                    &plane, 0, stride, bw, bh, &mut a, c[0], c[1], c[2], c[3],
                );
                s::chroma_bilinear_block(&plane, 0, stride, bw, bh, &mut b, c[0], c[1], c[2], c[3]);
                assert_eq!(a, b, "chroma_bilinear_block {bw}x{bh} round={round}");
            }
        }
    }
}
