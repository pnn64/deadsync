//! HEVC inverse integer transforms (spec 8.6.4).
//!
//! Implements the inverse 4×4 / 8×8 / 16×16 / 32×32 DCT, the 4×4 luma intra
//! DST, and the DC-only fast paths. Mirrors FFmpeg's `dsp_template.c`
//! `idct_NxN` functions and the underlying butterfly factorization.
//!
//! All transforms operate in-place on a `&mut [i16]` of length `N*N`.
//! The transforms are two-pass (vertical, then horizontal); inside each
//! pass we read a column/row out into a small `i32` buffer, run the 1D
//! inverse transform, scale-and-clip the result, and write back to the
//! same `i16` storage.

/// 32×32 HEVC integer transform matrix (spec 8.6.4.1, FFmpeg `dsp.c`).
/// Each column is a row of the inverse-transform matrix; smaller transforms
/// use a subset of these rows (16-pt = rows 0,2,...,30; 8-pt = rows 0,4,...,28;
/// 4-pt = rows 0,8,16,24).
#[rustfmt::skip]
const TRANSFORM: [[i8; 32]; 32] = [
    [ 64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,
      64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64,  64],
    [ 90,  90,  88,  85,  82,  78,  73,  67,  61,  54,  46,  38,  31,  22,  13,   4,
      -4, -13, -22, -31, -38, -46, -54, -61, -67, -73, -78, -82, -85, -88, -90, -90],
    [ 90,  87,  80,  70,  57,  43,  25,   9,  -9, -25, -43, -57, -70, -80, -87, -90,
     -90, -87, -80, -70, -57, -43, -25,  -9,   9,  25,  43,  57,  70,  80,  87,  90],
    [ 90,  82,  67,  46,  22,  -4, -31, -54, -73, -85, -90, -88, -78, -61, -38, -13,
      13,  38,  61,  78,  88,  90,  85,  73,  54,  31,   4, -22, -46, -67, -82, -90],
    [ 89,  75,  50,  18, -18, -50, -75, -89, -89, -75, -50, -18,  18,  50,  75,  89,
      89,  75,  50,  18, -18, -50, -75, -89, -89, -75, -50, -18,  18,  50,  75,  89],
    [ 88,  67,  31, -13, -54, -82, -90, -78, -46,  -4,  38,  73,  90,  85,  61,  22,
     -22, -61, -85, -90, -73, -38,   4,  46,  78,  90,  82,  54,  13, -31, -67, -88],
    [ 87,  57,   9, -43, -80, -90, -70, -25,  25,  70,  90,  80,  43,  -9, -57, -87,
     -87, -57,  -9,  43,  80,  90,  70,  25, -25, -70, -90, -80, -43,   9,  57,  87],
    [ 85,  46, -13, -67, -90, -73, -22,  38,  82,  88,  54,  -4, -61, -90, -78, -31,
      31,  78,  90,  61,   4, -54, -88, -82, -38,  22,  73,  90,  67,  13, -46, -85],
    [ 83,  36, -36, -83, -83, -36,  36,  83,  83,  36, -36, -83, -83, -36,  36,  83,
      83,  36, -36, -83, -83, -36,  36,  83,  83,  36, -36, -83, -83, -36,  36,  83],
    [ 82,  22, -54, -90, -61,  13,  78,  85,  31, -46, -90, -67,   4,  73,  88,  38,
     -38, -88, -73,  -4,  67,  90,  46, -31, -85, -78, -13,  61,  90,  54, -22, -82],
    [ 80,   9, -70, -87, -25,  57,  90,  43, -43, -90, -57,  25,  87,  70,  -9, -80,
     -80,  -9,  70,  87,  25, -57, -90, -43,  43,  90,  57, -25, -87, -70,   9,  80],
    [ 78,  -4, -82, -73,  13,  85,  67, -22, -88, -61,  31,  90,  54, -38, -90, -46,
      46,  90,  38, -54, -90, -31,  61,  88,  22, -67, -85, -13,  73,  82,   4, -78],
    [ 75, -18, -89, -50,  50,  89,  18, -75, -75,  18,  89,  50, -50, -89, -18,  75,
      75, -18, -89, -50,  50,  89,  18, -75, -75,  18,  89,  50, -50, -89, -18,  75],
    [ 73, -31, -90, -22,  78,  67, -38, -90, -13,  82,  61, -46, -88,  -4,  85,  54,
     -54, -85,   4,  88,  46, -61, -82,  13,  90,  38, -67, -78,  22,  90,  31, -73],
    [ 70, -43, -87,   9,  90,  25, -80, -57,  57,  80, -25, -90,  -9,  87,  43, -70,
     -70,  43,  87,  -9, -90, -25,  80,  57, -57, -80,  25,  90,   9, -87, -43,  70],
    [ 67, -54, -78,  38,  85, -22, -90,   4,  90,  13, -88, -31,  82,  46, -73, -61,
      61,  73, -46, -82,  31,  88, -13, -90,  -4,  90,  22, -85, -38,  78,  54, -67],
    [ 64, -64, -64,  64,  64, -64, -64,  64,  64, -64, -64,  64,  64, -64, -64,  64,
      64, -64, -64,  64,  64, -64, -64,  64,  64, -64, -64,  64,  64, -64, -64,  64],
    [ 61, -73, -46,  82,  31, -88, -13,  90,  -4, -90,  22,  85, -38, -78,  54,  67,
     -67, -54,  78,  38, -85, -22,  90,   4, -90,  13,  88, -31, -82,  46,  73, -61],
    [ 57, -80, -25,  90,  -9, -87,  43,  70, -70, -43,  87,   9, -90,  25,  80, -57,
     -57,  80,  25, -90,   9,  87, -43, -70,  70,  43, -87,  -9,  90, -25, -80,  57],
    [ 54, -85,  -4,  88, -46, -61,  82,  13, -90,  38,  67, -78, -22,  90, -31, -73,
      73,  31, -90,  22,  78, -67, -38,  90, -13, -82,  61,  46, -88,   4,  85, -54],
    [ 50, -89,  18,  75, -75, -18,  89, -50, -50,  89, -18, -75,  75,  18, -89,  50,
      50, -89,  18,  75, -75, -18,  89, -50, -50,  89, -18, -75,  75,  18, -89,  50],
    [ 46, -90,  38,  54, -90,  31,  61, -88,  22,  67, -85,  13,  73, -82,   4,  78,
     -78,  -4,  82, -73, -13,  85, -67, -22,  88, -61, -31,  90, -54, -38,  90, -46],
    [ 43, -90,  57,  25, -87,  70,   9, -80,  80,  -9, -70,  87, -25, -57,  90, -43,
     -43,  90, -57, -25,  87, -70,  -9,  80, -80,   9,  70, -87,  25,  57, -90,  43],
    [ 38, -88,  73,  -4, -67,  90, -46, -31,  85, -78,  13,  61, -90,  54,  22, -82,
      82, -22, -54,  90, -61, -13,  78, -85,  31,  46, -90,  67,   4, -73,  88, -38],
    [ 36, -83,  83, -36, -36,  83, -83,  36,  36, -83,  83, -36, -36,  83, -83,  36,
      36, -83,  83, -36, -36,  83, -83,  36,  36, -83,  83, -36, -36,  83, -83,  36],
    [ 31, -78,  90, -61,   4,  54, -88,  82, -38, -22,  73, -90,  67, -13, -46,  85,
     -85,  46,  13, -67,  90, -73,  22,  38, -82,  88, -54,  -4,  61, -90,  78, -31],
    [ 25, -70,  90, -80,  43,   9, -57,  87, -87,  57,  -9, -43,  80, -90,  70, -25,
     -25,  70, -90,  80, -43,  -9,  57, -87,  87, -57,   9,  43, -80,  90, -70,  25],
    [ 22, -61,  85, -90,  73, -38,  -4,  46, -78,  90, -82,  54, -13, -31,  67, -88,
      88, -67,  31,  13, -54,  82, -90,  78, -46,   4,  38, -73,  90, -85,  61, -22],
    [ 18, -50,  75, -89,  89, -75,  50, -18, -18,  50, -75,  89, -89,  75, -50,  18,
      18, -50,  75, -89,  89, -75,  50, -18, -18,  50, -75,  89, -89,  75, -50,  18],
    [ 13, -38,  61, -78,  88, -90,  85, -73,  54, -31,   4,  22, -46,  67, -82,  90,
     -90,  82, -67,  46, -22,  -4,  31, -54,  73, -85,  90, -88,  78, -61,  38, -13],
    [  9, -25,  43, -57,  70, -80,  87, -90,  90, -87,  80, -70,  57, -43,  25,  -9,
      -9,  25, -43,  57, -70,  80, -87,  90, -90,  87, -80,  70, -57,  43, -25,   9],
    [  4, -13,  22, -31,  38, -46,  54, -61,  67, -73,  78, -82,  85, -88,  90, -90,
      90, -90,  88, -85,  82, -78,  73, -67,  61, -54,  46, -38,  31, -22,  13,  -4],
];

#[inline]
fn clip_i16(v: i32) -> i16 {
    v.clamp(-32768, 32767) as i16
}

// ---- 1D inverse transforms (FFmpeg `TR_4` / `TR_8` / `TR_16` / `TR_32`) ----

/// 4-point inverse DCT butterfly.
fn tr_4(dst: &mut [i32; 4], src: &[i32; 4]) {
    let s0 = src[0];
    let s1 = src[1];
    let s2 = src[2];
    let s3 = src[3];
    let e0 = 64 * s0 + 64 * s2;
    let e1 = 64 * s0 - 64 * s2;
    let o0 = 83 * s1 + 36 * s3;
    let o1 = 36 * s1 - 83 * s3;
    dst[0] = e0 + o0;
    dst[1] = e1 + o1;
    dst[2] = e1 - o1;
    dst[3] = e0 - o0;
}

/// 8-point inverse DCT.
fn tr_8(dst: &mut [i32; 8], src: &[i32; 8]) {
    let mut o = [0i32; 4];
    for (i, slot) in o.iter_mut().enumerate() {
        // j = 1, 3, 5, 7 → use rows 4*j of TRANSFORM (= rows 4, 12, 20, 28).
        for k in 0..4 {
            let j = 2 * k + 1;
            *slot += TRANSFORM[4 * j][i] as i32 * src[j];
        }
    }
    let even_in = [src[0], src[2], src[4], src[6]];
    let mut e = [0i32; 4];
    tr_4(&mut e, &even_in);
    for i in 0..4 {
        dst[i] = e[i] + o[i];
        dst[7 - i] = e[i] - o[i];
    }
}

/// 16-point inverse DCT.
fn tr_16(dst: &mut [i32; 16], src: &[i32; 16]) {
    let mut o = [0i32; 8];
    for (i, slot) in o.iter_mut().enumerate() {
        // j = 1, 3, ..., 15 → rows 2*j (= 2, 6, 10, 14, 18, 22, 26, 30).
        for k in 0..8 {
            let j = 2 * k + 1;
            *slot += TRANSFORM[2 * j][i] as i32 * src[j];
        }
    }
    let even_in = [
        src[0], src[2], src[4], src[6], src[8], src[10], src[12], src[14],
    ];
    let mut e = [0i32; 8];
    tr_8(&mut e, &even_in);
    for i in 0..8 {
        dst[i] = e[i] + o[i];
        dst[15 - i] = e[i] - o[i];
    }
}

/// 32-point inverse DCT.
fn tr_32(dst: &mut [i32; 32], src: &[i32; 32]) {
    let mut o = [0i32; 16];
    for (i, slot) in o.iter_mut().enumerate() {
        // j = 1, 3, ..., 31 → rows j directly.
        for k in 0..16 {
            let j = 2 * k + 1;
            *slot += TRANSFORM[j][i] as i32 * src[j];
        }
    }
    let mut even_in = [0i32; 16];
    for i in 0..16 {
        even_in[i] = src[2 * i];
    }
    let mut e = [0i32; 16];
    tr_16(&mut e, &even_in);
    for i in 0..16 {
        dst[i] = e[i] + o[i];
        dst[31 - i] = e[i] - o[i];
    }
}

// ---- Two-pass inverse DCT for square blocks ---------------------------------

/// Apply the two-pass inverse DCT (vertical then horizontal) of size `N`
/// in-place on a `N×N` `i16` block.
///
/// `bit_depth` controls the second-pass right shift (`20 - bit_depth`).
/// Reads/writes are done in row-major order with stride `N`.
macro_rules! impl_idct {
    ($fn_name:ident, $size:expr, $tr_fn:ident) => {
        fn $fn_name(coeffs: &mut [i16], bit_depth: u32) {
            // Pass 1: vertical (each column of the block).
            for col in 0..$size {
                let mut col_in = [0i32; $size];
                for row in 0..$size {
                    col_in[row] = coeffs[row * $size + col] as i32;
                }
                let mut col_out = [0i32; $size];
                $tr_fn(&mut col_out, &col_in);
                let shift = 7u32;
                let add = 1i32 << (shift - 1);
                for row in 0..$size {
                    coeffs[row * $size + col] = clip_i16((col_out[row] + add) >> shift);
                }
            }
            // Pass 2: horizontal (each row of the block).
            for row in 0..$size {
                let mut row_in = [0i32; $size];
                for col in 0..$size {
                    row_in[col] = coeffs[row * $size + col] as i32;
                }
                let mut row_out = [0i32; $size];
                $tr_fn(&mut row_out, &row_in);
                let shift = 20u32.saturating_sub(bit_depth);
                let add = 1i32 << (shift.max(1) - 1);
                for col in 0..$size {
                    coeffs[row * $size + col] = clip_i16((row_out[col] + add) >> shift);
                }
            }
        }
    };
}

impl_idct!(idct_4x4, 4, tr_4);
// The butterfly versions of the larger sizes are the reference for the
// zero-skipping matrix versions below, which decoding uses.
#[cfg(test)]
impl_idct!(idct_8x8, 8, tr_8);
#[cfg(test)]
impl_idct!(idct_16x16, 16, tr_16);
#[cfg(test)]
impl_idct!(idct_32x32, 32, tr_32);

// ---- Zero-skipping inverse DCT (8x8 to 32x32) -------------------------------

/// The left half of the `N`-point inverse transform matrix:
/// `m[k][i] = TRANSFORM[k * 32 / N][i]`, the weight of input `k` in output
/// `i < N / 2`. Output `N - 1 - i` has the same weights for even `k` and
/// negated ones for odd `k` (the DCT's even/odd symmetry, which the
/// butterflies exploit too).
const fn half_matrix<const N: usize, const H: usize>() -> [[i32; H]; N] {
    let mut m = [[0i32; H]; N];
    let mut k = 0;
    while k < N {
        let mut i = 0;
        while i < H {
            m[k][i] = TRANSFORM[k * (32 / N)][i] as i32;
            i += 1;
        }
        k += 1;
    }
    m
}

const HALF_8: [[i32; 4]; 8] = half_matrix::<8, 4>();
const HALF_16: [[i32; 8]; 16] = half_matrix::<16, 8>();
const HALF_32: [[i32; 16]; 32] = half_matrix::<32, 16>();

/// Two-pass inverse DCT as even/odd matrix products that skip the zero
/// rows and columns outside the coded region (`nz_cols` x `nz_rows`), as
/// FFmpeg's `col_limit` does. Bit-identical to the butterfly version: the
/// 1-D transform is exact integer arithmetic either way, and the rounding
/// and clipping points are the same.
///
/// Both passes run in fixed-width lanes LLVM vectorises: the column pass
/// over 8 adjacent columns at a time (rows of the block are contiguous),
/// the row pass over the `N / 2` outputs of each half of a row.
macro_rules! impl_idct_skip {
    ($fn_name:ident, $n:expr, $h:expr, $m:ident) => {
        // Not inlined: three large kernels would bloat the dispatch.
        #[inline(never)]
        fn $fn_name(coeffs: &mut [i16], bit_depth: u32, nz_cols: usize, nz_rows: usize) {
            const N: usize = $n;
            const H: usize = $h;
            const L: usize = 8;
            let coeffs: &mut [i16; N * N] = (&mut coeffs[..N * N]).try_into().unwrap();
            let (nz_cols, nz_rows) = (nz_cols.clamp(1, N), nz_rows.clamp(1, N));

            // Pass 1: vertical. Columns at or past nz_cols are zero in and
            // out, so only the chunks covering 0..nz_cols run.
            for c0 in (0..nz_cols).step_by(L) {
                let mut inp = [[0i32; L]; N];
                for (k, row) in inp.iter_mut().enumerate().take(nz_rows) {
                    for (l, v) in row.iter_mut().enumerate() {
                        *v = coeffs[k * N + c0 + l] as i32;
                    }
                }
                for i in 0..H {
                    let (mut e, mut o) = ([0i32; L], [0i32; L]);
                    for k in (0..nz_rows).step_by(2) {
                        let (we, wo) = ($m[k][i], $m[k + 1][i]);
                        let (re, ro) = (&inp[k], &inp[k + 1]);
                        for l in 0..L {
                            e[l] += we * re[l];
                            o[l] += wo * ro[l];
                        }
                    }
                    for l in 0..L {
                        coeffs[i * N + c0 + l] = clip_i16((e[l] + o[l] + 64) >> 7);
                        coeffs[(N - 1 - i) * N + c0 + l] = clip_i16((e[l] - o[l] + 64) >> 7);
                    }
                }
            }

            // Pass 2: horizontal, every row; inputs past nz_cols are zero.
            let shift = 20u32.saturating_sub(bit_depth);
            let add = 1i32 << (shift.max(1) - 1);
            for row in coeffs.chunks_exact_mut(N) {
                let (mut e, mut o) = ([0i32; H], [0i32; H]);
                for k in (0..nz_cols).step_by(2) {
                    let (ce, co) = (row[k] as i32, row[k + 1] as i32);
                    let (me, mo) = (&$m[k], &$m[k + 1]);
                    for j in 0..H {
                        e[j] += me[j] * ce;
                        o[j] += mo[j] * co;
                    }
                }
                for j in 0..H {
                    row[j] = clip_i16((e[j] + o[j] + add) >> shift);
                    row[N - 1 - j] = clip_i16((e[j] - o[j] + add) >> shift);
                }
            }
        }
    };
}

impl_idct_skip!(idct_8x8_skip, 8, 4, HALF_8);
impl_idct_skip!(idct_16x16_skip, 16, 8, HALF_16);
impl_idct_skip!(idct_32x32_skip, 32, 16, HALF_32);

// ---- DC-only fast path ------------------------------------------------------

/// Inverse DCT for blocks where only the DC coefficient is non-zero
/// (FFmpeg `idct_NxN_dc`). The fast path skips the matrix multiply entirely.
fn idct_dc(coeffs: &mut [i16], log2_size: u8, bit_depth: u32) {
    let shift = 14u32.saturating_sub(bit_depth);
    let add = 1i32 << (shift.max(1) - 1);
    let dc = (((coeffs[0] as i32 + 1) >> 1) + add) >> shift;
    let dc = clip_i16(dc);
    let n = 1usize << log2_size;
    for c in &mut coeffs[..n * n] {
        *c = dc;
    }
}

// ---- 4×4 luma intra DST (spec 8.6.4.2 / FFmpeg `transform_4x4_luma`) -------

/// 1D inverse 4×4 DST butterfly used for intra luma 4×4 blocks.
fn tr_4x4_luma(dst: &mut [i32; 4], src: &[i32; 4]) {
    let c0 = src[0] + src[2];
    let c1 = src[2] + src[3];
    let c2 = src[0] - src[3];
    let c3 = 74 * src[1];
    dst[2] = 74 * (src[0] - src[2] + src[3]);
    dst[0] = 29 * c0 + 55 * c1 + c3;
    dst[1] = 55 * c2 - 29 * c1 + c3;
    dst[3] = 55 * c0 + 29 * c2 - c3;
}

#[allow(dead_code)]
pub fn transform_4x4_luma(coeffs: &mut [i16], bit_depth: u32) {
    // Pass 1: vertical
    for col in 0..4 {
        let col_in = [
            coeffs[col] as i32,
            coeffs[4 + col] as i32,
            coeffs[8 + col] as i32,
            coeffs[12 + col] as i32,
        ];
        let mut col_out = [0i32; 4];
        tr_4x4_luma(&mut col_out, &col_in);
        let shift = 7u32;
        let add = 1i32 << (shift - 1);
        for row in 0..4 {
            coeffs[row * 4 + col] = clip_i16((col_out[row] + add) >> shift);
        }
    }
    // Pass 2: horizontal
    for row in 0..4 {
        let row_in = [
            coeffs[row * 4] as i32,
            coeffs[row * 4 + 1] as i32,
            coeffs[row * 4 + 2] as i32,
            coeffs[row * 4 + 3] as i32,
        ];
        let mut row_out = [0i32; 4];
        tr_4x4_luma(&mut row_out, &row_in);
        let shift = 20u32.saturating_sub(bit_depth);
        let add = 1i32 << (shift.max(1) - 1);
        for col in 0..4 {
            coeffs[row * 4 + col] = clip_i16((row_out[col] + add) >> shift);
        }
    }
}

// ---- Extended precision -----------------------------------------------------

/// One 1D inverse transform of size `n` (DST for the 4-point intra luma
/// case), on `i32` samples.
fn tr_1d(n: usize, dst_4x4: bool, out: &mut [i32], inp: &[i32]) {
    macro_rules! run {
        ($f:ident, $size:expr) => {{
            let mut i = [0i32; $size];
            i.copy_from_slice(inp);
            let mut o = [0i32; $size];
            $f(&mut o, &i);
            out.copy_from_slice(&o);
        }};
    }
    match n {
        4 if dst_4x4 => run!(tr_4x4_luma, 4),
        4 => run!(tr_4, 4),
        8 => run!(tr_8, 8),
        16 => run!(tr_16, 16),
        _ => run!(tr_32, 32),
    }
}

/// Inverse transform with extended_precision_processing_flag (spec 8.6.4.2):
/// coefficients may exceed 16 bits (`|c| <= 1 << log2_range`), the
/// intermediate after the first stage is clipped to that range, and the
/// second-stage shift is `log2_range + 5 - BitDepth` (`20 - BitDepth`
/// when the range is 15). Returns the residual.
pub fn inverse_transform_ext(
    coeffs: &[i32],
    log2_size: u8,
    bit_depth: u32,
    log2_range: u32,
    dst_4x4: bool,
) -> Vec<i16> {
    let n = 1usize << log2_size;
    let (min, max) = (-(1i64 << log2_range), (1i64 << log2_range) - 1);
    let mut tmp = vec![0i32; n * n];
    let (mut col_in, mut col_out) = (vec![0i32; n], vec![0i32; n]);
    for col in 0..n {
        for row in 0..n {
            col_in[row] = coeffs[row * n + col];
        }
        tr_1d(n, dst_4x4, &mut col_out, &col_in);
        for row in 0..n {
            tmp[row * n + col] = ((col_out[row] as i64 + 64) >> 7).clamp(min, max) as i32;
        }
    }
    let shift = log2_range + 5 - bit_depth;
    let add = 1i64 << (shift - 1);
    let mut out = vec![0i16; n * n];
    let mut row_out = vec![0i32; n];
    for row in 0..n {
        tr_1d(n, dst_4x4, &mut row_out, &tmp[row * n..(row + 1) * n]);
        for col in 0..n {
            out[row * n + col] = ((row_out[col] as i64 + add) >> shift).clamp(-32768, 32767) as i16;
        }
    }
    out
}

// ---- Top-level dispatch -----------------------------------------------------

/// Apply the inverse transform appropriate for `log2_size` and the residual's
/// last-significant-coefficient position. When the only non-zero coefficient
/// is the DC, takes the `idct_dc` fast path.
///
/// `is_luma_intra_4x4` selects the 4×4 intra luma DST instead of the DCT
/// (HEVC spec 8.6.4.2).
///
/// `(nz_cols, nz_rows)` bounds the non-zero coefficients (see
/// `ResidualBlock::nz_cols`); the larger transforms skip what lies outside.
pub fn apply_inverse_transform(
    coeffs: &mut [i16],
    log2_size: u8,
    last_sig_x: u32,
    last_sig_y: u32,
    (nz_cols, nz_rows): (u8, u8),
    bit_depth: u32,
    is_luma_intra_4x4: bool,
) {
    let max_xy = last_sig_x.max(last_sig_y);
    if max_xy == 0 && !is_luma_intra_4x4 {
        idct_dc(coeffs, log2_size, bit_depth);
        return;
    }
    if log2_size == 2 && is_luma_intra_4x4 {
        transform_4x4_luma(coeffs, bit_depth);
        return;
    }
    match log2_size {
        2 => idct_4x4(coeffs, bit_depth),
        3 => idct_8x8_skip(coeffs, bit_depth, nz_cols as usize, nz_rows as usize),
        4 => idct_16x16_skip(coeffs, bit_depth, nz_cols as usize, nz_rows as usize),
        5 => idct_32x32_skip(coeffs, bit_depth, nz_cols as usize, nz_rows as usize),
        _ => panic!("invalid log2_size for inverse transform: {log2_size}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The zero-skipping matrix transforms match the butterflies exactly
    /// for random coefficients confined to random coefficient-group
    /// bounds, at every bit depth (including values that clip).
    #[test]
    fn skip_transforms_match_butterflies() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        type Reference = fn(&mut [i16], u32);
        type Kernel = fn(&mut [i16], u32, usize, usize);
        let cases: [(usize, Reference, Kernel); 3] = [
            (8, idct_8x8, idct_8x8_skip),
            (16, idct_16x16, idct_16x16_skip),
            (32, idct_32x32, idct_32x32_skip),
        ];
        for (n, reference, skip) in cases {
            for iter in 0..400 {
                let cols = 4 * (1 + rnd() as usize % (n / 4));
                let rows = 4 * (1 + rnd() as usize % (n / 4));
                let mut block = vec![0i16; n * n];
                for y in 0..rows {
                    for x in 0..cols {
                        if rnd() % 3 == 0 {
                            // Mostly small levels, sometimes full range.
                            block[y * n + x] = if iter % 4 == 0 {
                                rnd() as i16
                            } else {
                                (rnd() % 512) as i16 - 256
                            };
                        }
                    }
                }
                for bd in [8, 10, 12] {
                    let (mut a, mut b) = (block.clone(), block.clone());
                    reference(&mut a, bd);
                    skip(&mut b, bd, cols, rows);
                    assert_eq!(a, b, "{n}x{n} bounds {cols}x{rows} bit depth {bd}");
                }
            }
        }
    }

    /// Pure DC input → pure DC output.
    #[test]
    fn test_idct_dc_16x16_negative() {
        // From our fixture: dequantized DC = -204, expect -2 per pixel.
        let mut coeffs = vec![0i16; 256];
        coeffs[0] = -204;
        idct_dc(&mut coeffs, 4, 8);
        assert!(coeffs.iter().all(|&c| c == -2));
    }

    /// Round-trip: feeding the full IDCT a block with only a DC coefficient
    /// must produce the same constant value as the DC fast path.
    #[test]
    fn test_idct_full_matches_dc_path_for_dc_only_block() {
        let mut a = vec![0i16; 256];
        let mut b = vec![0i16; 256];
        a[0] = -204;
        b[0] = -204;
        idct_dc(&mut a, 4, 8);
        idct_16x16(&mut b, 8);
        assert_eq!(
            a, b,
            "full 16x16 IDCT should match DC fast path on DC-only input"
        );
    }

    /// Verify 4x4 DST against direct matrix multiplication.
    /// The HEVC 4x4 DST matrix (spec 8.6.4.2) is:
    ///   { 29, 55, 74, 84 }
    ///   { 74, 74,  0,-74 }
    ///   { 84,-29,-74, 55 }
    ///   { 55,-84, 74,-29 }
    /// The inverse DST: x = M^T * C * M (with appropriate scaling).
    #[test]
    fn test_dst_4x4_matrix_verify() {
        // DST matrix rows
        let m: [[i32; 4]; 4] = [
            [29, 55, 74, 84],
            [74, 74, 0, -74],
            [84, -29, -74, 55],
            [55, -84, 74, -29],
        ];

        // Input: single coefficient at (0,0)
        let input_coeff: i16 = 100;
        let mut coeffs = vec![0i16; 16];
        coeffs[0] = input_coeff;

        // Our DST
        let mut our_result = coeffs.clone();
        transform_4x4_luma(&mut our_result, 8);

        // Matrix multiplication: inverse = M^T * C * M / (128 * 4096)
        // Actually, the two-pass inverse DST with shifts 7 and 12 is:
        // Pass 1 (cols): tmp[i][j] = (sum_k M[k][i]*C[k][j] + 64) >> 7
        //   (M transposed applied to columns)
        // Pass 2 (rows): out[i][j] = (sum_k M[k][j]*tmp[i][k] + 2048) >> 12
        //   (M transposed applied to rows)
        let mut tmp = [[0i32; 4]; 4];
        for j in 0..4 {
            for i in 0..4 {
                let mut s = 0i32;
                for k in 0..4 {
                    s += m[k][i] * coeffs[k * 4 + j] as i32;
                }
                tmp[i][j] = (s + 64) >> 7;
            }
        }
        let mut expected = vec![0i16; 16];
        for i in 0..4 {
            for j in 0..4 {
                let mut s = 0i32;
                for k in 0..4 {
                    s += m[k][j] * tmp[i][k];
                }
                expected[i * 4 + j] = ((s + 2048) >> 12).clamp(-32768, 32767) as i16;
            }
        }

        assert_eq!(
            our_result, expected,
            "DST butterfly output doesn't match matrix multiplication\nOurs: {:?}\nExpected: {:?}",
            our_result, expected
        );

        // Also test with coefficient at (1,2) = row 2, col 1
        let mut coeffs2 = vec![0i16; 16];
        coeffs2[2 * 4 + 1] = 200;
        let mut our_result2 = coeffs2.clone();
        transform_4x4_luma(&mut our_result2, 8);

        let mut tmp2 = [[0i32; 4]; 4];
        for j in 0..4 {
            for i in 0..4 {
                let mut s = 0i32;
                for k in 0..4 {
                    s += m[k][i] * coeffs2[k * 4 + j] as i32;
                }
                tmp2[i][j] = (s + 64) >> 7;
            }
        }
        let mut expected2 = vec![0i16; 16];
        for i in 0..4 {
            for j in 0..4 {
                let mut s = 0i32;
                for k in 0..4 {
                    s += m[k][j] * tmp2[i][k];
                }
                expected2[i * 4 + j] = ((s + 2048) >> 12).clamp(-32768, 32767) as i16;
            }
        }
        assert_eq!(
            our_result2, expected2,
            "DST non-DC test failed\nOurs: {:?}\nExpected: {:?}",
            our_result2, expected2
        );
    }
}
