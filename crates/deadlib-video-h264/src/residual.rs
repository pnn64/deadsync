use crate::pixel::Pixel;

/// 4x4 zigzag scan order: maps linear index to (row, col) within a 4x4 block.
pub const ZIGZAG_4X4: [(usize, usize); 16] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (2, 0),
    (1, 1),
    (0, 2),
    (0, 3),
    (1, 2),
    (2, 1),
    (3, 0),
    (3, 1),
    (2, 2),
    (1, 3),
    (2, 3),
    (3, 2),
    (3, 3),
];

/// Inverse 4x4 Hadamard transform for I16x16 luma DC coefficients.
/// Input/output: 16 values arranged as 4x4 in raster order.
pub fn inverse_hadamard_4x4(dc: &mut [i32; 16]) {
    let mut tmp = [0i32; 16];

    // Horizontal transform
    for i in 0..4 {
        let a = dc[i * 4];
        let b = dc[i * 4 + 1];
        let c = dc[i * 4 + 2];
        let d = dc[i * 4 + 3];
        tmp[i * 4] = a + b + c + d;
        tmp[i * 4 + 1] = a + b - c - d;
        tmp[i * 4 + 2] = a - b - c + d;
        tmp[i * 4 + 3] = a - b + c - d;
    }

    // Vertical transform
    for j in 0..4 {
        let a = tmp[j];
        let b = tmp[4 + j];
        let c = tmp[8 + j];
        let d = tmp[12 + j];
        dc[j] = a + b + c + d;
        dc[4 + j] = a + b - c - d;
        dc[8 + j] = a - b - c + d;
        dc[12 + j] = a - b + c - d;
    }
}

/// Inverse 2x2 Hadamard transform for chroma DC coefficients.
pub fn inverse_hadamard_2x2(dc: &mut [i32; 4]) {
    let a = dc[0] + dc[1] + dc[2] + dc[3];
    let b = dc[0] - dc[1] + dc[2] - dc[3];
    let c = dc[0] + dc[1] - dc[2] - dc[3];
    let d = dc[0] - dc[1] - dc[2] + dc[3];
    dc[0] = a;
    dc[1] = b;
    dc[2] = c;
    dc[3] = d;
}

/// Inverse 4x4 integer DCT transform (H.264 spec 8.5.12).
/// Operates in-place on 16 coefficients in raster order.
/// Horizontal pass (rows) first, then vertical pass (columns), per spec 8.5.12.1.
pub fn inverse_dct_4x4(block: &mut [i32; 16]) {
    // Use wrapping arithmetic — malformed coefficients can cause overflow
    use core::num::Wrapping as W;

    block[0] = block[0].wrapping_add(32);

    // First pass: rows
    for i in 0..4 {
        let s = i * 4;
        let z0 = (W(block[s]) + W(block[s + 2])).0;
        let z1 = (W(block[s]) - W(block[s + 2])).0;
        let z2 = (W(block[s + 1] >> 1) - W(block[s + 3])).0;
        let z3 = (W(block[s + 1]) + W(block[s + 3] >> 1)).0;

        block[s] = (W(z0) + W(z3)).0;
        block[s + 1] = (W(z1) + W(z2)).0;
        block[s + 2] = (W(z1) - W(z2)).0;
        block[s + 3] = (W(z0) - W(z3)).0;
    }

    // Second pass: columns, with >> 6 normalization
    for j in 0..4 {
        let z0 = (W(block[j]) + W(block[8 + j])).0;
        let z1 = (W(block[j]) - W(block[8 + j])).0;
        let z2 = (W(block[4 + j] >> 1) - W(block[12 + j])).0;
        let z3 = (W(block[4 + j]) + W(block[12 + j] >> 1)).0;

        block[j] = (W(z0) + W(z3)).0 >> 6;
        block[4 + j] = (W(z1) + W(z2)).0 >> 6;
        block[8 + j] = (W(z1) - W(z2)).0 >> 6;
        block[12 + j] = (W(z0) - W(z3)).0 >> 6;
    }
}

/// LevelScale factors from H.264 Table 8-13.
/// Indexed by [qp_rem][position_category] where position categories are:
/// 0: even row, even col — (0,0),(2,0),(0,2),(2,2)
/// 1: mixed parity — (0,1),(1,0),(0,3),(2,1), etc.
/// 2: odd row, odd col — (1,1),(3,1),(1,3),(3,3)
const LEVEL_SCALE: [[i32; 3]; 6] = [
    [10, 13, 16],
    [11, 14, 18],
    [13, 16, 20],
    [14, 18, 23],
    [16, 20, 25],
    [18, 23, 29],
];

/// Get the position category for a 4x4 block position (row, col).
/// Per spec Table 8-13: 0=even-even, 1=mixed parity, 2=odd-odd.
fn position_category(row: usize, col: usize) -> usize {
    (row & 1) + (col & 1)
}

/// Dequantize a 4x4 AC residual block in-place.
/// `scale` is the 4x4 scaling matrix (in scan order, default all-16 for flat scaling).
pub fn dequant_4x4(block: &mut [i32; 16], qp: i32, scale: &[u8; 16]) {
    let qp_per = qp / 6;
    let qp_rem = (qp % 6) as usize;

    for idx in 0..16 {
        if block[idx] != 0 {
            let (r, c) = ZIGZAG_4X4[idx];
            let v = LEVEL_SCALE[qp_rem][position_category(r, c)] * scale[idx] as i32;
            if qp_per >= 4 {
                block[idx] = block[idx].wrapping_mul(v).wrapping_shl((qp_per - 4) as u32);
            } else {
                block[idx] =
                    (block[idx].wrapping_mul(v).wrapping_add(1 << (3 - qp_per))) >> (4 - qp_per);
            }
        }
    }
}

/// Dequantize I16x16 luma DC coefficients after Hadamard.
/// Per spec 8.5.12.1, DC scaling uses scale[0] (the DC position of the scaling matrix).
pub fn dequant_luma_dc_i16x16(dc: &mut [i32; 16], qp: i32, scale_dc: u8) {
    let qp_per = qp / 6;
    let qp_rem = (qp % 6) as usize;
    let v = LEVEL_SCALE[qp_rem][0] * scale_dc as i32;

    if qp_per >= 6 {
        for d in dc.iter_mut() {
            *d = d.wrapping_mul(v).wrapping_shl((qp_per - 6) as u32);
        }
    } else {
        let round = 1 << (5 - qp_per);
        for d in dc.iter_mut() {
            *d = (d.wrapping_mul(v).wrapping_add(round)) >> (6 - qp_per);
        }
    }
}

/// Dequantize 4:2:0 chroma DC coefficients after the 2x2 Hadamard
/// (spec 8.5.11.2, ChromaArrayType 1):
/// `dcC = ((f * LevelScale4x4(qP % 6, 0, 0)) << (qP / 6)) >> 5`.
///
/// Unlike luma DC and 4:2:2 chroma DC there is no rounding term: the shift
/// floors. With flat scaling matrices `f * LevelScale` is a multiple of 16,
/// so a rounding term would only change the result for chroma QP below 6.
/// Uses scale[0] from the chroma scaling matrix.
pub fn dequant_chroma_dc(dc: &mut [i32; 4], qp: i32, scale_dc: u8) {
    let qp_per = qp / 6;
    let qp_rem = (qp % 6) as usize;
    let v = LEVEL_SCALE[qp_rem][0] * scale_dc as i32;

    if qp_per >= 5 {
        for d in dc.iter_mut() {
            *d = d.wrapping_mul(v).wrapping_shl((qp_per - 5) as u32);
        }
    } else {
        // Arithmetic shift: floors, matching (x << qp_per) >> 5.
        for d in dc.iter_mut() {
            *d = d.wrapping_mul(v) >> (5 - qp_per);
        }
    }
}

/// Inverse transform and scaling of 4:2:2 chroma DC (spec 8.5.11.1-2).
/// `dc` holds the 8 coefficients in parse order; on return it holds the DC
/// of each 4x4 chroma block in raster order (two per row, four rows).
/// Scaling uses QP'c + 3.
pub fn chroma_dc_422(dc: &mut [i32; 8], qp_c: i32, scale_dc: u8) {
    use core::num::Wrapping as W;
    // Parse order to the 4x2 matrix c (spec 8.5.11.1)
    const ORDER: [usize; 8] = [0, 2, 1, 5, 3, 6, 4, 7];
    let mut c = [W(0i32); 8];
    for (i, &o) in ORDER.iter().enumerate() {
        c[i] = W(dc[o]);
    }
    // f = A * c * B, A the 4x4 Hadamard-like matrix, B = [[1, 1], [1, -1]]
    let mut t = [W(0i32); 8];
    for col in 0..2 {
        let (c0, c1, c2, c3) = (c[col], c[2 + col], c[4 + col], c[6 + col]);
        t[col] = c0 + c1 + c2 + c3;
        t[2 + col] = c0 + c1 - c2 - c3;
        t[4 + col] = c0 - c1 - c2 + c3;
        t[6 + col] = c0 - c1 + c2 - c3;
    }
    let qp_dc = qp_c + 3;
    let (qp_per, qp_rem) = (qp_dc / 6, (qp_dc % 6) as usize);
    let v = LEVEL_SCALE[qp_rem][0] * scale_dc as i32;
    for row in 0..4 {
        let (a, b) = (t[row * 2], t[row * 2 + 1]);
        for (k, f) in [(a + b).0, (a - b).0].into_iter().enumerate() {
            dc[row * 2 + k] = if qp_per >= 6 {
                f.wrapping_mul(v).wrapping_shl((qp_per - 6) as u32)
            } else {
                f.wrapping_mul(v).wrapping_add(1 << (5 - qp_per)) >> (6 - qp_per)
            };
        }
    }
}

/// QP_C lookup table from QP_I (H.264 Table 8-15).
/// qPI = clip3(0, 51, QP_Y + chroma_qp_index_offset)
/// QP_C = QPC_TABLE[qPI]
pub const QPC_TABLE: [i32; 52] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 29, 30, 31, 32, 32, 33, 34, 34, 35, 35, 36, 36, 37, 37, 37, 38, 38, 38, 39, 39,
    39, 39,
];

/// Compute chroma QP from luma QP and chroma_qp_index_offset.
pub fn chroma_qp(qp_y: i32, chroma_qp_index_offset: i32) -> i32 {
    chroma_qp_bd(qp_y, chroma_qp_index_offset, 0)
}

/// QP_C for a bit depth with offset QpBdOffsetC (spec 8.5.8): qPI is clipped
/// to -QpBdOffsetC..=51, and below 30 QP_C equals qPI (negative included).
#[inline]
pub fn chroma_qp_bd(qp_y: i32, chroma_qp_index_offset: i32, qp_bd_offset: i32) -> i32 {
    let qpi = (qp_y + chroma_qp_index_offset).clamp(-qp_bd_offset, 51);
    if qpi < 0 {
        qpi
    } else {
        QPC_TABLE[qpi as usize]
    }
}

/// 4x4 field zigzag scan order (spec Table 8-13): used for field pictures and
/// field-coded MBs in MBAFF. Maps linear index to (row, col).
pub const ZIGZAG_4X4_FIELD: [(usize, usize); 16] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (2, 0),
    (3, 0),
    (1, 1),
    (2, 1),
    (3, 1),
    (0, 2),
    (1, 2),
    (2, 2),
    (3, 2),
    (0, 3),
    (1, 3),
    (2, 3),
    (3, 3),
];

/// 8x8 field zigzag scan for CAVLC (spec Table 8-13, rearranged for CAVLC 4-quad decode).
/// Like ZIGZAG_8X8_CAVLC but using field scan order. Each group of 16 corresponds to
/// one 4x4 sub-block (TL, TR, BL, BR), with coefficients ordered by field scan position.
#[rustfmt::skip]
pub const ZIGZAG_8X8_CAVLC_FIELD: [usize; 64] = [
     0,  9,  2, 56, 18, 26, 34, 27,
    35, 28, 36, 29, 45,  7, 54, 39,
     8, 24, 25, 33, 41, 11, 42, 12,
    43, 13, 44, 14, 53, 15, 62, 47,
    16, 32, 40, 10, 49,  4, 50,  5,
    51,  6, 52, 22, 61, 38, 23, 55,
     1, 17, 48,  3, 57, 19, 58, 20,
    59, 21, 60, 37, 30, 46, 31, 63,
];

/// 8x8 field zigzag scan for CABAC (spec Table 8-13, field coding).
/// Value = row*8+col.
#[rustfmt::skip]
pub const ZIGZAG_8X8_CABAC_FIELD: [usize; 64] = [
     0,  8, 16,  1,  9, 24, 32, 17,
     2, 25, 40, 48, 56, 33, 10,  3,
    18, 41, 49, 57, 26, 11,  4, 19,
    34, 42, 50, 58, 27, 12,  5, 20,
    35, 43, 51, 59, 28, 13,  6, 21,
    36, 44, 52, 60, 29, 14, 22, 37,
    45, 53, 61, 30,  7, 15, 38, 46,
    54, 62, 23, 31, 39, 47, 55, 63,
];

/// 8x8 zigzag scan for CAVLC (spec Table 8-12, rearranged for CAVLC 4-quad decode).
/// Structured as 4 groups of 16: each group is one 4x4 sub-block's scan positions
/// within the full 8x8 block. Value = row * 8 + col.
#[rustfmt::skip]
pub const ZIGZAG_8X8_CAVLC: [usize; 64] = [
    // 4 groups of 16: each maps CAVLC scan position → raster position in 8x8
     0,  9, 17, 18, 12, 40, 27,  7, 35, 57, 29, 30, 58, 38, 53, 47,
     1,  2, 24, 11, 19, 48, 20, 14, 42, 50, 22, 37, 59, 31, 60, 55,
     8,  3, 32,  4, 26, 41, 13, 21, 49, 43, 15, 44, 52, 39, 61, 62,
    16, 10, 25,  5, 33, 34,  6, 28, 56, 36, 23, 51, 45, 46, 54, 63,
];

/// 8x8 zigzag scan for CABAC (standard zigzag, value = row*8+col).
#[rustfmt::skip]
pub const ZIGZAG_8X8_CABAC: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// LevelScale factors for 8x8 blocks (H.264 Table 8-14).
/// Indexed by [qp_rem][position_category_8x8].
/// 8x8 blocks have 6 position categories (vs 3 for 4x4).
const LEVEL_SCALE_8X8: [[i32; 6]; 6] = [
    [20, 18, 32, 19, 25, 24],
    [22, 19, 35, 21, 28, 26],
    [26, 23, 42, 24, 33, 31],
    [28, 25, 45, 26, 35, 33],
    [32, 28, 51, 30, 40, 38],
    [36, 32, 58, 34, 46, 43],
];

/// Maps (row%4)*4 + (col%4) to one of 6 position categories for 8x8 dequant.
/// From H.264 spec Table 8-14.
const DEQUANT_8X8_POS_CAT: [usize; 16] = [0, 3, 4, 3, 3, 1, 5, 1, 4, 5, 2, 5, 3, 1, 5, 1];

/// Zigzag scan index of each raster position of an 8x8 block: scaling
/// matrices are coded in (frame) zigzag order, also for field MBs (spec
/// 8.5.6).
const RASTER_TO_ZIGZAG_8X8: [u8; 64] = {
    let mut t = [0u8; 64];
    let mut k = 0;
    while k < 64 {
        t[ZIGZAG_8X8_CABAC[k]] = k as u8;
        k += 1;
    }
    t
};

/// Dequantize an 8x8 residual block in raster order.
/// `block[r*8+c]` contains coefficients in raster positions.
/// `scale` is the 8x8 scaling matrix in zigzag scan order.
pub fn dequant_8x8(block: &mut [i32; 64], qp: i32, scale: &[u8; 64]) {
    let qp_per = qp / 6;
    let ls = LEVEL_SCALE_8X8[(qp % 6) as usize];
    // Branch-free over all 64 positions (a zero coefficient stays zero)
    let v = |idx: usize| {
        let cat = DEQUANT_8X8_POS_CAT[((idx >> 3) & 3) * 4 + (idx & 3)];
        ls[cat] * scale[RASTER_TO_ZIGZAG_8X8[idx] as usize] as i32
    };
    if qp_per >= 6 {
        let sh = (qp_per - 6) as u32;
        for (idx, b) in block.iter_mut().enumerate() {
            *b = b.wrapping_mul(v(idx)).wrapping_shl(sh);
        }
    } else {
        let (round, sh) = (1 << (5 - qp_per), (6 - qp_per) as u32);
        for (idx, b) in block.iter_mut().enumerate() {
            *b = b.wrapping_mul(v(idx)).wrapping_add(round) >> sh;
        }
    }
}

/// Inverse 8x8 integer DCT transform (H.264 spec 8.5.12).
/// Operates in-place on 64 coefficients in raster order (row-major, 8 per row).
/// Row pass first (within each row), then column pass with >> 6 normalization.
pub fn inverse_dct_8x8(block: &mut [i32; 64]) {
    // Use wrapping arithmetic throughout — malformed coefficients can cause overflow
    use core::num::Wrapping as W;

    block[0] = block[0].wrapping_add(32);

    // First pass: rows
    for i in 0..8 {
        let s = i * 8;
        let (b0, b2, b4, b6, b1, b3, b5, b7) = idct8_butterfly(
            block[s],
            block[s + 1],
            block[s + 2],
            block[s + 3],
            block[s + 4],
            block[s + 5],
            block[s + 6],
            block[s + 7],
        );
        block[s] = (W(b0) + W(b7)).0;
        block[s + 1] = (W(b2) + W(b5)).0;
        block[s + 2] = (W(b4) + W(b3)).0;
        block[s + 3] = (W(b6) + W(b1)).0;
        block[s + 4] = (W(b6) - W(b1)).0;
        block[s + 5] = (W(b4) - W(b3)).0;
        block[s + 6] = (W(b2) - W(b5)).0;
        block[s + 7] = (W(b0) - W(b7)).0;
    }

    // Second pass: columns, with >> 6 normalization
    for i in 0..8 {
        let (b0, b2, b4, b6, b1, b3, b5, b7) = idct8_butterfly(
            block[i],
            block[i + 8],
            block[i + 16],
            block[i + 24],
            block[i + 32],
            block[i + 40],
            block[i + 48],
            block[i + 56],
        );
        block[i] = (W(b0) + W(b7)).0 >> 6;
        block[i + 8] = (W(b2) + W(b5)).0 >> 6;
        block[i + 16] = (W(b4) + W(b3)).0 >> 6;
        block[i + 24] = (W(b6) + W(b1)).0 >> 6;
        block[i + 32] = (W(b6) - W(b1)).0 >> 6;
        block[i + 40] = (W(b4) - W(b3)).0 >> 6;
        block[i + 48] = (W(b2) - W(b5)).0 >> 6;
        block[i + 56] = (W(b0) - W(b7)).0 >> 6;
    }
}

/// 8x8 IDCT butterfly using wrapping arithmetic to avoid overflow panics.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn idct8_butterfly(
    x0: i32,
    x1: i32,
    x2: i32,
    x3: i32,
    x4: i32,
    x5: i32,
    x6: i32,
    x7: i32,
) -> (i32, i32, i32, i32, i32, i32, i32, i32) {
    use core::num::Wrapping as W;
    let a0 = (W(x0) + W(x4)).0;
    let a2 = (W(x0) - W(x4)).0;
    let a4 = (W(x2 >> 1) - W(x6)).0;
    let a6 = (W(x6 >> 1) + W(x2)).0;

    let b0 = (W(a0) + W(a6)).0;
    let b2 = (W(a2) + W(a4)).0;
    let b4 = (W(a2) - W(a4)).0;
    let b6 = (W(a0) - W(a6)).0;

    let a1 = (W(0) - W(x3) + W(x5) - W(x7) - W(x7 >> 1)).0;
    let a3 = (W(x1) + W(x7) - W(x3) - W(x3 >> 1)).0;
    let a5 = (W(0) - W(x1) + W(x7) + W(x5) + W(x5 >> 1)).0;
    let a7 = (W(x3) + W(x5) + W(x1) + W(x1 >> 1)).0;

    let b1 = (W(a7 >> 2) + W(a1)).0;
    let b3 = (W(a3) + W(a5 >> 2)).0;
    let b5 = (W(a3 >> 2) - W(a5)).0;
    let b7 = (W(a7) - W(a1 >> 2)).0;

    (b0, b2, b4, b6, b1, b3, b5, b7)
}

/// Raster block index to (mb_row_offset, mb_col_offset) for luma 4x4 blocks.
/// Block ordering within a macroblock: inverse raster scan of 8x8 blocks,
/// then raster scan of 4x4 within each 8x8.
pub const BLOCK_INDEX_TO_OFFSET: [(usize, usize); 16] = [
    (0, 0),
    (0, 4),
    (4, 0),
    (4, 4), // block 0-3 (top-left 8x8)
    (0, 8),
    (0, 12),
    (4, 8),
    (4, 12), // block 4-7 (top-right 8x8)
    (8, 0),
    (8, 4),
    (12, 0),
    (12, 4), // block 8-11 (bottom-left 8x8)
    (8, 8),
    (8, 12),
    (12, 8),
    (12, 12), // block 12-15 (bottom-right 8x8)
];

/// Reverse lookup: `OFFSET_TO_BLOCK[row][col]` gives the block index for the
/// 4x4 block at grid position (row, col) where row/col are in 0..4.
/// Replaces O(16) linear scans of `BLOCK_INDEX_TO_OFFSET.iter().position()`.
pub const OFFSET_TO_BLOCK: [[usize; 4]; 4] = [
    [0, 1, 4, 5],     // row 0
    [2, 3, 6, 7],     // row 1
    [8, 9, 12, 13],   // row 2
    [10, 11, 14, 15], // row 3
];

/// coded_block_pattern mapping for I macroblocks (H.264 Table 9-4).
/// Index is the code_number from ue(v); value is the CBP.
/// Low 4 bits = luma CBP (one bit per 8x8 block), bits 4-5 = chroma CBP (0/1/2).
#[rustfmt::skip]
pub const CBP_INTRA_TABLE: [u8; 48] = [
    47, 31, 15,  0, 23, 27, 29, 30,  7, 11, 13, 14, 39, 43, 45, 46,
    16,  3,  5, 10, 12, 19, 21, 26, 28, 35, 37, 42, 44,  1,  2,  4,
     8, 17, 18, 20, 24,  6,  9, 22, 25, 32, 33, 34, 36, 40, 38, 41,
];

/// coded_block_pattern mapping for Inter macroblocks (H.264 Table 9-4b).
#[rustfmt::skip]
pub const CBP_INTER_TABLE: [u8; 48] = [
     0, 16,  1,  2,  4,  8, 32,  3,  5, 10, 12, 15, 47,  7, 11, 13,
    14,  6,  9, 31, 35, 37, 42, 44, 33, 34, 36, 40, 39, 43, 45, 46,
    17, 18, 20, 24, 19, 21, 26, 28, 23, 27, 29, 30, 22, 25, 38, 41,
];

/// coded_block_pattern mapping for monochrome streams (ChromaArrayType 0,
/// Table 9-4 second column): luma bits only, codeNum 0-15.
#[rustfmt::skip]
const CBP_INTRA_TABLE_GRAY: [u8; 16] = [15, 0, 7, 11, 13, 14, 3, 5, 10, 12, 1, 2, 4, 8, 6, 9];
#[rustfmt::skip]
const CBP_INTER_TABLE_GRAY: [u8; 16] = [0, 1, 2, 4, 8, 3, 5, 10, 12, 15, 7, 11, 13, 14, 6, 9];

/// The coded_block_pattern of me(v) codeNum `code` (spec 9.1.2), or `None`
/// when out of range.
pub fn map_cbp(code: usize, intra: bool, monochrome: bool) -> Option<u8> {
    match (intra, monochrome) {
        (true, false) => CBP_INTRA_TABLE.get(code),
        (false, false) => CBP_INTER_TABLE.get(code),
        (true, true) => CBP_INTRA_TABLE_GRAY.get(code),
        (false, true) => CBP_INTER_TABLE_GRAY.get(code),
    }
    .copied()
}

/// Dequantize a full 4x4 block (including DC at position [0][0]) in raster order.
/// `scale` is the scaling matrix in scan order; mapped to raster via ZIGZAG_4X4.
/// Copy a `w` x `h` block (rows `src_stride` apart) into a plane at `off`
/// (rows `stride` apart), a row at a time. Offsets and strides in samples.
#[inline]
pub(crate) fn put_block<P: Pixel>(
    dst: &mut [u8],
    off: usize,
    stride: usize,
    src: &[P],
    src_stride: usize,
    (w, h): (usize, usize),
) {
    for r in 0..h {
        P::store_row(
            dst,
            off + r * stride,
            &src[r * src_stride..r * src_stride + w],
        );
    }
}

/// `dst = clip(dst + res)` over a `w` x `h` block of a plane at `off` (rows
/// `stride` apart; residual rows `res_stride` apart), a row at a time.
#[inline]
pub(crate) fn add_residual<P: Pixel>(
    dst: &mut [u8],
    off: usize,
    stride: usize,
    res: &[i32],
    res_stride: usize,
    (w, h): (usize, usize),
) {
    for r in 0..h {
        P::add_residual_row(dst, off + r * stride, &res[r * res_stride..][..w]);
    }
}

/// `dst = clip(pred + res)` over a `w` x `h` block of a plane at `off` (rows
/// `stride` apart; prediction rows `pred_stride` apart, residual rows
/// `res_stride` apart), a row at a time.
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn put_pred_residual<P: Pixel>(
    dst: &mut [u8],
    off: usize,
    stride: usize,
    pred: &[P],
    pred_stride: usize,
    res: &[i32],
    res_stride: usize,
    (w, h): (usize, usize),
) {
    for r in 0..h {
        P::put_pred_residual_row(
            dst,
            off + r * stride,
            &pred[r * pred_stride..][..w],
            &res[r * res_stride..][..w],
        );
    }
}

/// Zigzag scan index of each raster position of a 4x4 block (the inverse
/// of `ZIGZAG_4X4`), for looking up scaling lists.
pub(crate) const RASTER_TO_ZIGZAG_4X4: [u8; 16] = {
    let mut t = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        let (r, c) = ZIGZAG_4X4[i];
        t[r * 4 + c] = i as u8;
        i += 1;
    }
    t
};

pub fn dequant_4x4_full(block: &mut [i32; 16], qp: i32, scale: &[u8; 16]) {
    let qp_per = qp / 6;
    let ls = LEVEL_SCALE[(qp % 6) as usize];
    // Branch-free over all 16 positions (a zero coefficient stays zero), so
    // the loop vectorizes
    let v = |idx: usize| {
        ls[((idx >> 2) & 1) + (idx & 1)] * scale[RASTER_TO_ZIGZAG_4X4[idx] as usize] as i32
    };
    if qp_per >= 4 {
        let sh = (qp_per - 4) as u32;
        for (idx, b) in block.iter_mut().enumerate() {
            *b = b.wrapping_mul(v(idx)).wrapping_shl(sh);
        }
    } else {
        let (round, sh) = (1 << (3 - qp_per), (4 - qp_per) as u32);
        for (idx, b) in block.iter_mut().enumerate() {
            *b = b.wrapping_mul(v(idx)).wrapping_add(round) >> sh;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inverse_hadamard_4x4_all_same() {
        // If all DC values are the same, Hadamard should concentrate energy in [0]
        let mut dc = [5i32; 16];
        inverse_hadamard_4x4(&mut dc);
        assert_eq!(dc[0], 80); // 5 * 16
        for &v in &dc[1..] {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn test_dequant_chroma_dc_floors_without_rounding() {
        // qP = 1: LevelScale4x4(1, 0, 0) = 11 * 16 = 176 with a flat matrix.
        // dcC = (f * 176 << 0) >> 5 = floor(f * 5.5): 5 for f = 1, -6 for
        // f = -1, 11 for f = 2. A rounding term would give 6, -5 and 11.
        let mut dc = [1, -1, 2, 0];
        dequant_chroma_dc(&mut dc, 1, 16);
        assert_eq!(dc, [5, -6, 11, 0]);
        // At qP >= 30 the formula is exact either way.
        let mut dc = [1, -1, 0, 0];
        dequant_chroma_dc(&mut dc, 31, 16);
        assert_eq!(dc, [(176 << 5) >> 5, (-176 << 5) >> 5, 0, 0]);
    }

    #[test]
    fn test_inverse_hadamard_2x2() {
        let mut dc = [1, 0, 0, 0];
        inverse_hadamard_2x2(&mut dc);
        assert_eq!(dc, [1, 1, 1, 1]);
    }
}
