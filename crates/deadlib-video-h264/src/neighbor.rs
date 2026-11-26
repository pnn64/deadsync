//! CABAC/CAVLC neighbor context helpers.
//!
//! Functions for deriving neighbor-dependent context values used in
//! CABAC syntax element decoding and CAVLC nC computation.

use crate::residual::{BLOCK_INDEX_TO_OFFSET, OFFSET_TO_BLOCK};

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn cabac_amvd(
    mvd_store: &[[i16; 2]],
    mb_idx: usize,
    mb_width: usize,
    py: usize,
    px: usize,
    comp: usize, // comp: 0=x, 1=y
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    _mb_field_decoding: &[bool],
) -> u32 {
    // Compute left and above neighbor MB indices (MBAFF-aware)
    let (has_left, left_mb_idx, has_above, above_mb_idx) = if !mbaff {
        let has_left = !mb_idx.is_multiple_of(mb_width);
        let left = if has_left { mb_idx - 1 } else { 0 };
        let has_above = mb_idx >= mb_width;
        let above = if has_above { mb_idx - mb_width } else { 0 };
        (has_left, left, has_above, above)
    } else {
        let pair_addr = mb_idx / 2;
        let has_left = !pair_addr.is_multiple_of(mb_width);
        let left = if has_left {
            (pair_addr - 1) * 2 + (mb_idx % 2)
        } else {
            0
        };
        let is_field = _mb_field_decoding[pair_addr];
        let has_above = if is_field {
            // Field-coded: above is same-field in above pair
            pair_addr >= mb_width
        } else {
            // Frame-coded: above is top of same pair (always available for bottom)
            !mb_idx.is_multiple_of(2) || pair_addr >= mb_width
        };
        let above = if !has_above {
            0
        } else if !mb_idx.is_multiple_of(2) && !is_field {
            mb_idx - 1 // Frame-coded: top of same pair
        } else {
            (pair_addr - mb_width) * 2 + 1
        };
        (has_left, left, has_above, above)
    };

    // Left neighbor
    let left_mvd = if px > 0 {
        // Within MB: block to the left at (py, px-4)
        let blk = OFFSET_TO_BLOCK[py / 4][(px - 4) / 4];
        mvd_store[mb_idx * 16 + blk][comp].unsigned_abs() as u32
    } else if mbaff {
        mbaff_amvd(
            mvd_store,
            mb_idx,
            mb_width,
            -1,
            py as i32,
            comp,
            mb_slice_id,
            cur_slice_id,
            _mb_field_decoding,
        )
    } else if has_left {
        // Left MB: rightmost column, same row
        if mb_slice_id[left_mb_idx] != cur_slice_id {
            0
        } else {
            let blk = OFFSET_TO_BLOCK[py / 4][3];
            mvd_store[left_mb_idx * 16 + blk][comp].unsigned_abs() as u32
        }
    } else {
        0
    };

    // Top neighbor
    let top_mvd = if py > 0 {
        let blk = OFFSET_TO_BLOCK[(py - 4) / 4][px / 4];
        mvd_store[mb_idx * 16 + blk][comp].unsigned_abs() as u32
    } else if mbaff {
        mbaff_amvd(
            mvd_store,
            mb_idx,
            mb_width,
            px as i32,
            -1,
            comp,
            mb_slice_id,
            cur_slice_id,
            _mb_field_decoding,
        )
    } else if has_above {
        if mb_slice_id[above_mb_idx] != cur_slice_id {
            0
        } else {
            let blk = OFFSET_TO_BLOCK[3][px / 4];
            mvd_store[above_mb_idx * 16 + blk][comp].unsigned_abs() as u32
        }
    } else {
        0
    };

    left_mvd + top_mvd
}

/// MBAFF cross-MB |mvd| for the CABAC mvd context, with the vertical
/// component scaled between frame and field MBs (spec 9.3.3.1.1.7).
#[allow(clippy::too_many_arguments)]
#[inline]
fn mbaff_amvd(
    mvd_store: &[[i16; 2]],
    mb_idx: usize,
    mb_width: usize,
    xn: i32,
    yn: i32,
    comp: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mb_field_decoding: &[bool],
) -> u32 {
    let Some((mb_n, blk, cur_field, nbr_field)) = mbaff_luma_neighbor_at(
        mb_idx,
        mb_width,
        mb_field_decoding,
        xn,
        yn,
        mb_slice_id,
        cur_slice_id,
    ) else {
        return 0;
    };
    let v = mvd_store[mb_n * 16 + blk][comp].unsigned_abs() as u32;
    match (comp == 1, cur_field, nbr_field) {
        (true, false, true) => v * 2,
        (true, true, false) => v / 2,
        _ => v,
    }
}

/// MBAFF cross-MB ref_idx for the CABAC ref_idx context; a field
/// neighbour's ref_idx is halved for a frame MB (spec 9.3.3.1.1.6).
#[allow(clippy::too_many_arguments)]
#[inline]
fn mbaff_neighbor_ref(
    ref_idx_store: &[i8],
    mb_idx: usize,
    mb_width: usize,
    xn: i32,
    yn: i32,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    blk_is_direct: &[bool],
    is_b_slice: bool,
    mb_field_decoding: &[bool],
) -> i8 {
    let Some((mb_n, blk, cur_field, nbr_field)) = mbaff_luma_neighbor_at(
        mb_idx,
        mb_width,
        mb_field_decoding,
        xn,
        yn,
        mb_slice_id,
        cur_slice_id,
    ) else {
        return -1;
    };
    if is_b_slice && blk_is_direct[mb_n * 16 + blk] {
        return 0;
    }
    let r = ref_idx_store[mb_n * 16 + blk];
    if !cur_field && nbr_field && r >= 0 {
        r >> 1
    } else {
        r
    }
}

#[allow(clippy::too_many_arguments)]
/// CABAC ref_idx neighbor context for a partition.
#[inline(always)]
/// Returns (left_ref, top_ref) from neighbor blocks.
/// For B-slices, direct-mode neighbors are treated as ref=0 (spec 9.3.3.1.1.4).
pub(crate) fn cabac_neighbor_ref(
    ref_idx_store: &[i8],
    mb_idx: usize,
    mb_width: usize,
    py: usize,
    px: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    _mb_is_direct: &[bool],
    blk_is_direct: &[bool],
    is_b_slice: bool,
    mbaff: bool,
    _mb_field_decoding: &[bool],
) -> (i8, i8) {
    // Compute left and above neighbor MB indices (MBAFF-aware)
    let (has_left, left_mb_idx, has_above, above_mb_idx) = if !mbaff {
        let has_left = !mb_idx.is_multiple_of(mb_width);
        let left = if has_left { mb_idx - 1 } else { 0 };
        let has_above = mb_idx >= mb_width;
        let above = if has_above { mb_idx - mb_width } else { 0 };
        (has_left, left, has_above, above)
    } else {
        let pair_addr = mb_idx / 2;
        let has_left = !pair_addr.is_multiple_of(mb_width);
        let left = if has_left {
            (pair_addr - 1) * 2 + (mb_idx % 2)
        } else {
            0
        };
        let is_field = _mb_field_decoding[pair_addr];
        let has_above = if is_field {
            // Field-coded: above is same-field in above pair
            pair_addr >= mb_width
        } else {
            // Frame-coded: above is top of same pair (always available for bottom)
            !mb_idx.is_multiple_of(2) || pair_addr >= mb_width
        };
        let above = if !has_above {
            0
        } else if !mb_idx.is_multiple_of(2) && !is_field {
            mb_idx - 1 // Frame-coded: top of same pair
        } else {
            (pair_addr - mb_width) * 2 + 1
        };
        (has_left, left, has_above, above)
    };

    let left_ref = if px > 0 {
        let blk = OFFSET_TO_BLOCK[py / 4][(px - 4) / 4];
        if is_b_slice && blk_is_direct[mb_idx * 16 + blk] {
            0
        } else {
            ref_idx_store[mb_idx * 16 + blk]
        }
    } else if mbaff {
        mbaff_neighbor_ref(
            ref_idx_store,
            mb_idx,
            mb_width,
            -1,
            py as i32,
            mb_slice_id,
            cur_slice_id,
            blk_is_direct,
            is_b_slice,
            _mb_field_decoding,
        )
    } else if has_left {
        if mb_slice_id[left_mb_idx] != cur_slice_id {
            -1
        } else {
            let blk = OFFSET_TO_BLOCK[py / 4][3];
            if is_b_slice && blk_is_direct[left_mb_idx * 16 + blk] {
                0
            } else {
                ref_idx_store[left_mb_idx * 16 + blk]
            }
        }
    } else {
        -1
    };

    let top_ref = if py > 0 {
        let blk = OFFSET_TO_BLOCK[(py - 4) / 4][px / 4];
        if is_b_slice && blk_is_direct[mb_idx * 16 + blk] {
            0
        } else {
            ref_idx_store[mb_idx * 16 + blk]
        }
    } else if mbaff {
        mbaff_neighbor_ref(
            ref_idx_store,
            mb_idx,
            mb_width,
            px as i32,
            -1,
            mb_slice_id,
            cur_slice_id,
            blk_is_direct,
            is_b_slice,
            _mb_field_decoding,
        )
    } else if has_above {
        if mb_slice_id[above_mb_idx] != cur_slice_id {
            -1
        } else {
            let blk = OFFSET_TO_BLOCK[3][px / 4];
            if is_b_slice && blk_is_direct[above_mb_idx * 16 + blk] {
                0
            } else {
                ref_idx_store[above_mb_idx * 16 + blk]
            }
        }
    } else {
        -1
    };

    (left_ref, top_ref)
}

/// Neighbouring location in an MBAFF frame (spec 6.4.12.2, Table 6-4).
///
/// For a location `(xn, yn)` relative to the upper-left sample of macroblock
/// `mb_idx` and outside it (left, above, above-left or above-right), returns
/// the macroblock covering it and the location `(xw, yw)` inside that
/// macroblock. `max_w`/`max_h` are the macroblock size in the plane: 16 for
/// luma, 8 for 4:2:0 chroma. Returns `None` when the location is outside
/// the picture or in a macroblock not decoded yet; slice membership is left
/// to the caller.
///
/// Implemented geometrically, which reproduces Table 6-4: the location is
/// mapped to a line of the macroblock pair it falls in (a frame macroblock
/// covers lines 0..16 or 16..32 of its pair, a field macroblock every other
/// line), and that line to whichever macroblock of the neighbouring pair holds
/// it. So a field top MB's above neighbour is the top (same-parity) MB of a
/// field pair above but the bottom MB of a frame pair, and a bottom MB's left
/// neighbour can be the top MB of a left pair coded the other way.
pub(crate) fn mbaff_neighbor_loc(
    mb_idx: usize,
    mb_width: usize,
    mb_field_decoding: &[bool],
    xn: i32,
    yn: i32,
    max_w: i32,
    max_h: i32,
) -> Option<(usize, usize, usize)> {
    let pair = mb_idx / 2;
    let bottom = (mb_idx % 2) as i32;
    let pair_h = 2 * max_h;
    // Line of the current pair holding row yn of this MB (may fall outside)
    let line = if mb_field_decoding[pair] {
        2 * yn + bottom
    } else {
        yn + max_h * bottom
    };
    let (dy, line_in) = if line < 0 {
        (-1, line + pair_h)
    } else if line >= pair_h {
        (1, line - pair_h)
    } else {
        (0, line)
    };
    let dx = if xn < 0 {
        -1
    } else if xn >= max_w {
        1
    } else {
        0
    };
    if !(0..pair_h).contains(&line_in) || dy > 0 {
        return None;
    }
    let col = (pair % mb_width) as i32 + dx;
    let row = (pair / mb_width) as i32 + dy;
    if col < 0 || col >= mb_width as i32 || row < 0 {
        return None;
    }
    let npair = row as usize * mb_width + col as usize;
    let (mb_n, yw) = if mb_field_decoding[npair] {
        (npair * 2 + (line_in % 2) as usize, line_in / 2)
    } else {
        (npair * 2 + (line_in / max_h) as usize, line_in % max_h)
    };
    // Only macroblocks earlier in decoding order are available
    if mb_n >= mb_idx {
        return None;
    }
    let xw = (xn + max_w).rem_euclid(max_w);
    Some((mb_n, xw as usize, yw as usize))
}

/// MBAFF luma neighbour block (left or above) of 4x4 block `blk`, as
/// `(mb, blk)`, or `None` if unavailable (outside the picture or slice).
#[inline]
fn mbaff_luma_neighbor_blk(
    mb_idx: usize,
    mb_width: usize,
    mb_field_decoding: &[bool],
    blk: usize,
    is_left: bool,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
) -> Option<(usize, usize)> {
    let (row, col) = BLOCK_INDEX_TO_OFFSET[blk];
    let (xn, yn) = if is_left {
        (col as i32 - 1, row as i32)
    } else {
        (col as i32, row as i32 - 1)
    };
    let (mb_n, xw, yw) = mbaff_neighbor_loc(mb_idx, mb_width, mb_field_decoding, xn, yn, 16, 16)?;
    if mb_slice_id[mb_n] != cur_slice_id {
        return None;
    }
    Some((mb_n, OFFSET_TO_BLOCK[yw / 4][xw / 4]))
}

/// MBAFF luma neighbour 4x4 block covering pixel `(xn, yn)` (relative to
/// `mb_idx`, outside it) as `(mb, blk, cur_field, nbr_field)`, or `None` if
/// unavailable or in another slice.
#[inline]
fn mbaff_luma_neighbor_at(
    mb_idx: usize,
    mb_width: usize,
    mb_field_decoding: &[bool],
    xn: i32,
    yn: i32,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
) -> Option<(usize, usize, bool, bool)> {
    let (mb_n, xw, yw) = mbaff_neighbor_loc(mb_idx, mb_width, mb_field_decoding, xn, yn, 16, 16)?;
    if mb_slice_id[mb_n] != cur_slice_id {
        return None;
    }
    Some((
        mb_n,
        OFFSET_TO_BLOCK[yw / 4][xw / 4],
        mb_field_decoding[mb_idx / 2],
        mb_field_decoding[mb_n / 2],
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cabac_neighbor_nz_luma(
    nc_luma: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk: usize,
    is_left: bool,
    is_intra: bool,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    _mb_field_decoding: &[bool],
) -> bool {
    // Neighbor block indices: within-MB (>=0) or cross-MB (negative, encoded as -(blk+1))
    #[rustfmt::skip]
    const LEFT: [i8; 16] = [-6, 0, -8, 2, 1, 4, 3, 6, -14, 8, -16, 10, 9, 12, 11, 14];
    #[rustfmt::skip]
    const TOP: [i8; 16] = [-11, -12, 0, 1, -15, -16, 4, 5, 2, 3, 8, 9, 6, 7, 12, 13];

    let neighbor = if is_left { LEFT[blk] } else { TOP[blk] };

    if neighbor >= 0 {
        // Same MB
        nc_luma[mb_idx * 16 + neighbor as usize] > 0
    } else if mbaff {
        match mbaff_luma_neighbor_blk(
            mb_idx,
            mb_width,
            _mb_field_decoding,
            blk,
            is_left,
            mb_slice_id,
            cur_slice_id,
        ) {
            Some((mb_n, blk_n)) => nc_luma[mb_n * 16 + blk_n] > 0,
            None => is_intra,
        }
    } else {
        // Cross-MB: decode the encoded block index
        let neighbor_blk = (-(neighbor + 1)) as usize;
        let neighbor_mb = if is_left {
            let has_left = if !mbaff {
                mb_idx.checked_rem(mb_width) != Some(0)
            } else {
                !(mb_idx / 2).is_multiple_of(mb_width)
            };
            if !has_left {
                return is_intra;
            }
            if !mbaff {
                mb_idx - 1
            } else {
                (mb_idx / 2 - 1) * 2 + (mb_idx % 2)
            }
        } else {
            let has_above = if !mbaff {
                mb_idx >= mb_width
            } else if _mb_field_decoding[mb_idx / 2] {
                (mb_idx / 2) >= mb_width
            } else {
                !mb_idx.is_multiple_of(2) || (mb_idx / 2) >= mb_width
            };
            if !has_above {
                return is_intra;
            }
            if !mbaff {
                mb_idx - mb_width
            } else if !mb_idx.is_multiple_of(2) && !_mb_field_decoding[mb_idx / 2] {
                mb_idx - 1 // Frame-coded: top of same pair
            } else {
                ((mb_idx / 2) - mb_width) * 2 + 1
            }
        };
        if mb_slice_id[neighbor_mb] != cur_slice_id {
            return is_intra;
        }
        nc_luma[neighbor_mb * 16 + neighbor_blk] > 0
    }
}

/// The chroma 4x4 block left of (`is_left`) or above block `blk` of
/// `mb_idx`, as `(mb, blk)`, or `None` if unavailable (outside the picture,
/// not decoded yet, or in another slice). Chroma blocks are numbered in
/// raster order, two per row, over `rows` rows: 2 for 4:2:0, 4 for 4:2:2.
#[allow(clippy::too_many_arguments)]
pub(crate) fn chroma_neighbor_blk(
    mb_idx: usize,
    mb_width: usize,
    blk: usize,
    is_left: bool,
    rows: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> Option<(usize, usize)> {
    let (row, col) = (blk / 2, blk % 2);
    if is_left && col == 1 {
        return Some((mb_idx, blk - 1));
    }
    if !is_left && row > 0 {
        return Some((mb_idx, blk - 2));
    }
    let (mb_n, blk_n) = if mbaff {
        let (xn, yn) = if is_left {
            (-1, (row * 4) as i32)
        } else {
            ((col * 4) as i32, -1)
        };
        let (mb_n, xw, yw) = mbaff_neighbor_loc(
            mb_idx,
            mb_width,
            mb_field_decoding,
            xn,
            yn,
            8,
            4 * rows as i32,
        )?;
        (mb_n, (yw / 4) * 2 + xw / 4)
    } else if is_left {
        if mb_idx.is_multiple_of(mb_width) {
            return None;
        }
        (mb_idx - 1, row * 2 + 1)
    } else {
        if mb_idx < mb_width {
            return None;
        }
        (mb_idx - mb_width, (rows - 1) * 2 + col)
    };
    (mb_slice_id[mb_n] == cur_slice_id).then_some((mb_n, blk_n))
}

/// CABAC coded_block_flag condition of a chroma AC block's neighbour:
/// whether it has coefficients (`nc_chroma`, 8 entries per MB), or for an
/// unavailable neighbour whether the current MB is intra.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cabac_neighbor_nz_chroma(
    nc_chroma: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk: usize,
    is_left: bool,
    is_intra: bool,
    rows: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> bool {
    match chroma_neighbor_blk(
        mb_idx,
        mb_width,
        blk,
        is_left,
        rows,
        mb_slice_id,
        cur_slice_id,
        mbaff,
        mb_field_decoding,
    ) {
        Some((mb_n, blk_n)) => nc_chroma[mb_n * 8 + blk_n] > 0,
        None => is_intra,
    }
}

/// CAVLC nC of chroma AC block `blk` (spec 9.2.1) from its neighbours'
/// total_coeff (`nc_chroma`, 8 entries per MB).
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_nc_chroma(
    nc_chroma: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk: usize,
    rows: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> i32 {
    let n = |is_left: bool| {
        chroma_neighbor_blk(
            mb_idx,
            mb_width,
            blk,
            is_left,
            rows,
            mb_slice_id,
            cur_slice_id,
            mbaff,
            mb_field_decoding,
        )
        .map(|(mb_n, blk_n)| nc_chroma[mb_n * 8 + blk_n])
    };
    match (n(true), n(false)) {
        (Some(a), Some(b)) => (a as i32 + b as i32 + 1) >> 1,
        (Some(v), None) | (None, Some(v)) => v as i32,
        (None, None) => 0,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_i4x4_mode(
    modes: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk_idx: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    intra_avail: &[bool],
    mbaff: bool,
    mb_field_decoding: &[bool],
) -> u8 {
    // None = neighbor unavailable (picture boundary or non-I4x4 neighbor MB that
    // doesn't exist). When either is None, predicted mode defaults to DC (2).
    let mode_a = get_neighbor_i4x4_mode(
        modes,
        mb_idx,
        mb_width,
        blk_idx,
        true,
        mb_slice_id,
        cur_slice_id,
        intra_avail,
        mbaff,
        mb_field_decoding,
    );
    let mode_b = get_neighbor_i4x4_mode(
        modes,
        mb_idx,
        mb_width,
        blk_idx,
        false,
        mb_slice_id,
        cur_slice_id,
        intra_avail,
        mbaff,
        mb_field_decoding,
    );
    match (mode_a, mode_b) {
        (Some(a), Some(b)) => a.min(b),
        _ => 2, // DC when either neighbor is unavailable
    }
}

/// `intra_avail`: per-MB flag, true if the MB is available for intra prediction
/// (always true unless constrained_intra_pred_flag is set and the neighbor is inter).
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_neighbor_i4x4_mode(
    modes: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk_idx: usize,
    is_left: bool,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    intra_avail: &[bool],
    mbaff: bool,
    _mb_field_decoding: &[bool],
) -> Option<u8> {
    // Block layout:  0  1 | 4  5
    //                2  3 | 6  7
    //               ------+------
    //                8  9 |12 13
    //               10 11 |14 15
    if is_left {
        match blk_idx {
            1 | 5 | 9 | 13 => Some(modes[mb_idx * 16 + blk_idx - 1]),
            3 | 7 | 11 | 15 => Some(modes[mb_idx * 16 + blk_idx - 1]),
            4 => Some(modes[mb_idx * 16 + 1]),
            6 => Some(modes[mb_idx * 16 + 3]),
            12 => Some(modes[mb_idx * 16 + 9]),
            14 => Some(modes[mb_idx * 16 + 11]),
            0 | 2 | 8 | 10 if mbaff => mbaff_luma_neighbor_blk(
                mb_idx,
                mb_width,
                _mb_field_decoding,
                blk_idx,
                true,
                mb_slice_id,
                cur_slice_id,
            )
            .filter(|&(n, _)| intra_avail[n])
            .map(|(n, b)| modes[n * 16 + b]),
            0 | 2 | 8 | 10 => {
                // Left edge of MB
                let has_left = if !mbaff {
                    !mb_idx.is_multiple_of(mb_width)
                } else {
                    !(mb_idx / 2).is_multiple_of(mb_width)
                };
                let left_mb = if !has_left {
                    0 // dummy, not used
                } else if !mbaff {
                    mb_idx - 1
                } else {
                    (mb_idx / 2 - 1) * 2 + (mb_idx % 2)
                };
                if has_left && mb_slice_id[left_mb] == cur_slice_id && intra_avail[left_mb] {
                    let left_blk = match blk_idx {
                        0 => 5,
                        2 => 7,
                        8 => 13,
                        10 => 15,
                        _ => unreachable!(),
                    };
                    Some(modes[left_mb * 16 + left_blk])
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        match blk_idx {
            2 | 6 | 10 | 14 => Some(modes[mb_idx * 16 + blk_idx - 2]),
            3 | 7 | 11 | 15 => Some(modes[mb_idx * 16 + blk_idx - 2]),
            8 => Some(modes[mb_idx * 16 + 2]),
            9 => Some(modes[mb_idx * 16 + 3]),
            12 => Some(modes[mb_idx * 16 + 6]),
            13 => Some(modes[mb_idx * 16 + 7]),
            0 | 1 | 4 | 5 if mbaff => mbaff_luma_neighbor_blk(
                mb_idx,
                mb_width,
                _mb_field_decoding,
                blk_idx,
                false,
                mb_slice_id,
                cur_slice_id,
            )
            .filter(|&(n, _)| intra_avail[n])
            .map(|(n, b)| modes[n * 16 + b]),
            0 | 1 | 4 | 5 => {
                // Top edge of MB
                let has_above = if !mbaff {
                    mb_idx >= mb_width
                } else if _mb_field_decoding[mb_idx / 2] {
                    (mb_idx / 2) >= mb_width
                } else {
                    !mb_idx.is_multiple_of(2) || (mb_idx / 2) >= mb_width
                };
                let above_mb = if !has_above {
                    0 // dummy, not used
                } else if !mbaff {
                    mb_idx - mb_width
                } else if !mb_idx.is_multiple_of(2) && !_mb_field_decoding[mb_idx / 2] {
                    mb_idx - 1 // Frame-coded: top of same pair
                } else {
                    ((mb_idx / 2) - mb_width) * 2 + 1
                };
                if has_above && mb_slice_id[above_mb] == cur_slice_id && intra_avail[above_mb] {
                    let above_blk = match blk_idx {
                        0 => 10,
                        1 => 11,
                        4 => 14,
                        5 => 15,
                        _ => unreachable!(),
                    };
                    Some(modes[above_mb * 16 + above_blk])
                } else {
                    None // picture top boundary
                }
            }
            _ => None,
        }
    }
}

/// Compute nC for a 4x4 block from left (A) and above (B) neighbors.
/// H.264 spec 9.2.1: nC = average of neighbor total_coeff values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_nc(
    nc_array: &[u8],
    mb_idx: usize,
    mb_width: usize,
    blk_idx: usize,
    blks_per_mb: usize,
    mb_slice_id: &[u16],
    cur_slice_id: u16,
    mbaff: bool,
    _mb_field_decoding: &[bool],
) -> i32 {
    let (left_blk, left_in_mb) = if blks_per_mb == 16 {
        match blk_idx {
            0 => (5usize, false),
            2 => (7, false),
            8 => (13, false),
            10 => (15, false),
            4 => (1, true),
            6 => (3, true),
            12 => (9, true),
            14 => (11, true),
            1 => (0, true),
            3 => (2, true),
            5 => (4, true),
            7 => (6, true),
            9 => (8, true),
            11 => (10, true),
            13 => (12, true),
            15 => (14, true),
            _ => unreachable!(),
        }
    } else {
        unreachable!("compute_nc is for luma; chroma uses compute_nc_chroma")
    };

    let mbaff_blk = |is_left: bool| {
        mbaff_luma_neighbor_blk(
            mb_idx,
            mb_width,
            _mb_field_decoding,
            blk_idx,
            is_left,
            mb_slice_id,
            cur_slice_id,
        )
    };
    let nc_a: Option<u8> = if left_in_mb {
        Some(nc_array[mb_idx * blks_per_mb + left_blk])
    } else if mbaff {
        mbaff_blk(true).map(|(mb_n, blk_n)| nc_array[mb_n * blks_per_mb + blk_n])
    } else {
        let has_left = if !mbaff {
            !mb_idx.is_multiple_of(mb_width)
        } else {
            !(mb_idx / 2).is_multiple_of(mb_width)
        };
        if has_left {
            let left_nb = if !mbaff {
                mb_idx - 1
            } else {
                (mb_idx / 2 - 1) * 2 + (mb_idx % 2)
            };
            if mb_slice_id[left_nb] == cur_slice_id {
                Some(nc_array[left_nb * blks_per_mb + left_blk])
            } else {
                None
            }
        } else {
            None
        }
    };

    let (above_blk, above_in_mb) = if blks_per_mb == 16 {
        match blk_idx {
            0 => (10usize, false),
            1 => (11, false),
            4 => (14, false),
            5 => (15, false),
            2 => (0, true),
            3 => (1, true),
            6 => (4, true),
            7 => (5, true),
            8 => (2, true),
            9 => (3, true),
            10 => (8, true),
            11 => (9, true),
            12 => (6, true),
            13 => (7, true),
            14 => (12, true),
            15 => (13, true),
            _ => unreachable!(),
        }
    } else {
        unreachable!("compute_nc is for luma; chroma uses compute_nc_chroma")
    };

    let nc_b: Option<u8> = if above_in_mb {
        Some(nc_array[mb_idx * blks_per_mb + above_blk])
    } else if mbaff {
        mbaff_blk(false).map(|(mb_n, blk_n)| nc_array[mb_n * blks_per_mb + blk_n])
    } else {
        let has_above = if !mbaff {
            mb_idx >= mb_width
        } else if _mb_field_decoding[mb_idx / 2] {
            (mb_idx / 2) >= mb_width
        } else {
            !mb_idx.is_multiple_of(2) || (mb_idx / 2) >= mb_width
        };
        if has_above {
            let above_nb = if !mbaff {
                mb_idx - mb_width
            } else if !mb_idx.is_multiple_of(2) && !_mb_field_decoding[mb_idx / 2] {
                mb_idx - 1 // Frame-coded: top of same pair
            } else {
                ((mb_idx / 2) - mb_width) * 2 + 1
            };
            if mb_slice_id[above_nb] == cur_slice_id {
                Some(nc_array[above_nb * blks_per_mb + above_blk])
            } else {
                None
            }
        } else {
            None
        }
    };

    match (nc_a, nc_b) {
        (Some(a), Some(b)) => (a as i32 + b as i32 + 1) >> 1,
        (Some(n), None) | (None, Some(n)) => n as i32,
        (None, None) => 0,
    }
}

/// Dequantize AC coefficients in a 4x4 block in raster order (skip DC at [0][0]).
pub(crate) fn dequant_4x4_ac_raster(block: &mut [i32; 16], qp: i32, scale: &[u8; 16]) {
    // The DC (position 0) is scaled separately; keep it as it is
    let dc = block[0];
    crate::residual::dequant_4x4_full(block, qp, scale);
    block[0] = dc;
}
